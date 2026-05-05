//! ErasureSet — Set/Disk coordination layer
//!
//! Responsibilities:
//! - Manage a group of EC disks (N = M + parity)
//! - Parallel disk I/O with read/write quorum decision
//! - Multi-disk xl.meta signature alignment and version selection
//! - Disk shuffle for even distribution

use std::collections::HashMap;
use std::sync::Arc;

use crate::base::error::{MinioError, MinioResult};
use crate::base::erasure::ErasureParams;
use crate::base::format::{ObjectPart, XlMeta, XlMetaEntry, XlMetaVersionHeader};
use crate::erasure::bitrot::BitrotDetector;
use crate::erasure::Erasure;
use futures::future::join_all;
use sha2::{Digest, Sha256};
use crate::storage::StorageAPI;
use tracing::{debug, warn};

/// ErasureSet — a group of disks + EC engine
///
/// Manages all disks in one EC set.
pub struct ErasureSet {
    disks: Vec<Arc<dyn StorageAPI>>,
    erasure: Erasure,
}

impl ErasureSet {
    /// Creates an ErasureSet
    ///
    /// Automatically selects parity based on disk count (<=5 -> 2, 6-7 -> 3, >=8 -> 4)
    ///
    /// # Errors
    /// Returns `MinioError::Internal` if fewer than 3 disks are provided.
    pub fn new(disks: Vec<Arc<dyn StorageAPI>>) -> MinioResult<Self> {
        let total = disks.len();
        if total < 3 {
            return Err(MinioError::Internal(format!(
                "at least 3 disks are required, got {total}",
            )));
        }
        let erasure = Erasure::with_default_parity(total)?;
        Ok(Self { disks, erasure })
    }

    /// Creates with custom EC parameters
    ///
    /// # Errors
    /// Returns `MinioError::Internal` if fewer than 3 disks are provided or `data_blocks < 1`.
    pub fn with_params(disks: Vec<Arc<dyn StorageAPI>>, data_blocks: usize, parity_blocks: usize) -> MinioResult<Self> {
        if disks.len() < 3 {
            return Err(MinioError::Internal(format!(
                "at least 3 disks are required, got {}",
                disks.len()
            )));
        }
        if data_blocks < 1 {
            return Err(MinioError::Internal(
                "data_blocks must be >= 1".into(),
            ));
        }
        let erasure = Erasure::new(data_blocks, parity_blocks)?;
        Ok(Self { disks, erasure })
    }

    // ---- properties ----

    pub fn params(&self) -> &ErasureParams {
        self.erasure.params()
    }

    pub fn erasure(&self) -> &Erasure {
        &self.erasure
    }

    pub fn disk_count(&self) -> usize {
        self.disks.len()
    }

    /// Returns list of online disks
    pub async fn online_disks(&self) -> Vec<&dyn StorageAPI> {
        let futures: Vec<_> = self
            .disks
            .iter()
            .map(|disk| disk.as_ref().is_online())
            .collect();
        let results = join_all(futures).await;
        self.disks
            .iter()
            .enumerate()
            .filter(|&(i, _)| results[i])
            .map(|(_, disk)| disk.as_ref())
            .collect()
    }

    /// Whether online disk count satisfies write quorum
    pub async fn has_write_quorum(&self) -> bool {
        let online = self.online_disks().await.len();
        online >= self.params().write_quorum()
    }

    /// Whether online disk count satisfies read quorum
    pub async fn has_read_quorum(&self) -> bool {
        let online = self.online_disks().await.len();
        online >= self.params().read_quorum()
    }

    // ---- write path ----

    /// Writes object: EC encode -> Bitrot wrap -> parallel disk write
    ///
    /// Returns the number of successful writes; caller checks write quorum.
    pub async fn write_shards(
        &self,
        volume: &str,
        path: &str,
        version_id: &str,
        shards: &[Vec<u8>],
    ) -> MinioResult<usize> {
        if shards.len() != self.disks.len() {
            return Err(MinioError::Internal(format!(
                "shard count mismatch: expected {}, got {}",
                self.disks.len(),
                shards.len()
            )));
        }

        let futures: Vec<_> = self
            .disks
            .iter()
            .enumerate()
            .map(|(i, disk)| {
                let volume = volume.to_string();
                let shard_path = format!("{path}/{version_id}/part.{i}");
                let wrapped = BitrotDetector::wrap(&shards[i]);
                async move {
                    match disk.write_all(&volume, &shard_path, &wrapped).await {
                        Ok(()) => {
                            debug!("shard {i} write success: {volume}/{shard_path}");
                            true
                        }
                        Err(e) => {
                            warn!("shard {i} write failed: {e}");
                            false
                        }
                    }
                }
            })
            .collect();

        let results = join_all(futures).await;
        let successes = results.iter().filter(|&&s| s).count();
        Ok(successes)
    }

    /// Writes xl.meta to all disks
    pub async fn write_xl_meta(
        &self,
        volume: &str,
        path: &str,
        meta: &XlMeta,
    ) -> MinioResult<usize> {
        let meta_bytes = Arc::from(meta.to_bytes()?.into_boxed_slice());
        let futures: Vec<_> = self
            .disks
            .iter()
            .enumerate()
            .map(|(i, disk)| {
                let volume = volume.to_string();
                let obj_path = format!("{path}/xl.meta");
                let data = Arc::clone(&meta_bytes);
                async move {
                    match disk.write_all(&volume, &obj_path, &data).await {
                        Ok(()) => {
                            debug!("xl.meta disk {i} write success");
                            true
                        }
                        Err(e) => {
                            warn!("xl.meta disk {i} write failed: {e}");
                            false
                        }
                    }
                }
            })
            .collect();

        let results = join_all(futures).await;
        Ok(results.iter().filter(|&&s| s).count())
    }

    /// Builds an XlMeta version entry
    pub fn build_version_header(
        &self,
        version_id: &str,
        data: &[u8],
        metadata: &[(String, String)],
    ) -> MinioResult<XlMetaVersionHeader> {
        let mod_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;

        // Single part object (full data as one part)
        let etag = {
            let mut hasher = Sha256::new();
            hasher.update(data);
            format!("{:x}", hasher.finalize())
        };
        let part = ObjectPart {
            number: 1,
            etag,
            size: data.len() as i64,
            actual_size: data.len() as i64,
            index: 0,
        };

        let params = self.params();
        let mut header = XlMetaVersionHeader::new(version_id.to_string());
        header.mod_time = mod_time;
        header.erasure_algorithm = 0; // 0 = ReedSolomon
        header.erasure_m = params.data_blocks as u16;
        header.erasure_n = (params.data_blocks + params.parity_blocks) as u16;
        header.erasure_block_size = params.block_size;
        header.erasure_dist = vec![0u8; params.total_shards()]; // even distribution
        header.parts = vec![part];
        // Split metadata into system (Content-Type) and user (x-amz-meta-*)
        let mut meta_sys = Vec::new();
        let mut meta_user = Vec::new();
        for (k, v) in metadata {
            if k.eq_ignore_ascii_case("Content-Type") {
                meta_sys.push(("content-type".to_string(), v.as_bytes().to_vec()));
            } else {
                meta_user.push((k.clone(), v.as_bytes().to_vec()));
            }
        }
        header.meta_sys = meta_sys;
        header.meta_user = meta_user;
        // Compute cross-disk consistency signature
        header.signature = header.compute_signature()?;

        Ok(header)
    }

    // ---- read path ----

    /// Reads xl.meta from multiple disks, aligns by signature, returns majority result
    ///
    /// Flow:
    /// 1. Read xl.meta from all disks in parallel
    /// 2. Compute signature for each meta
    /// 3. Select the majority group with consistent signatures (>= read_quorum)
    /// 4. Return the XlMeta from that group
    pub async fn read_xl_meta_quorum(
        &self,
        volume: &str,
        path: &str,
    ) -> MinioResult<XlMeta> {
        let meta_path = format!("{path}/xl.meta");
        let futures: Vec<_> = self
            .disks
            .iter()
            .enumerate()
            .map(|(i, disk)| {
                let volume = volume.to_string();
                let meta_path = meta_path.clone();
                async move {
                    match disk.read_all(&volume, &meta_path).await {
                        Ok(bytes) => match XlMeta::from_bytes(&bytes) {
                            Ok(meta) => (i, Some(meta)),
                            Err(e) => {
                                warn!("xl.meta parse failed on disk {i}: {e}");
                                (i, None)
                            }
                        },
                        Err(e) => {
                            debug!("xl.meta read failed on disk {i}: {e}");
                            (i, None)
                        }
                    }
                }
            })
            .collect();

        let results = join_all(futures).await;
        let read_quorum = self.params().read_quorum();

        // Group by composite version signature, select largest group
        let mut sig_groups: HashMap<Vec<u8>, Vec<XlMeta>> = HashMap::new();
        let mut seen_any = false;

        for (_, meta_opt) in results {
            if let Some(meta) = meta_opt {
                seen_any = true;
                // Build composite signature key from all version entries
                let mut key = Vec::new();
                for entry in &meta.versions {
                    match entry {
                        XlMetaEntry::Object { header, .. } => {
                            key.extend_from_slice(&header.signature);
                        }
                        XlMetaEntry::Delete { signature, .. } => {
                            key.extend_from_slice(signature);
                        }
                        XlMetaEntry::Legacy => {
                            key.push(0u8);
                        }
                    }
                }
                sig_groups.entry(key).or_default().push(meta);
            }
        }

        if !seen_any {
            return Err(MinioError::ObjectNotFound(format!(
                "xl.meta not found on any disk: {volume}/{meta_path}"
            )));
        }

        // Pick the largest group that meets read quorum
        let best_group = sig_groups
            .into_values()
            .max_by_key(|group| group.len())
            .filter(|group| group.len() >= read_quorum);

        match best_group {
            Some(group) => Ok(group.into_iter().next().unwrap()),
            None => Err(MinioError::InsufficientReadQuorum {
                required: read_quorum,
                actual: seen_any as usize,
            }),
        }
    }

    /// Reads shards from multiple disks, returns `[Option<Vec<u8>>; N]`
    ///
    /// Bitrot verification is built into unwrap: corrupted shards are automatically None
    pub async fn read_shards(
        &self,
        volume: &str,
        path: &str,
        version_id: &str,
    ) -> MinioResult<Vec<Option<Vec<u8>>>> {
        let total = self.disks.len();
        let futures: Vec<_> = self
            .disks
            .iter()
            .enumerate()
            .map(|(i, disk)| {
                let volume = volume.to_string();
                let shard_path = format!("{path}/{version_id}/part.{i}");
                async move {
                    match disk.read_all(&volume, &shard_path).await {
                        Ok(bytes) => BitrotDetector::unwrap(&bytes),
                        Err(e) => {
                            debug!("shard {i} read failed: {e}");
                            None
                        }
                    }
                }
            })
            .collect();

        let results = join_all(futures).await;

        // Pad to total length
        let mut shards = results;
        shards.resize(total, None);

        Ok(shards)
    }

    /// Deletes object path (all disks)
    pub async fn delete_path(&self, volume: &str, path: &str) -> MinioResult<usize> {
        let futures: Vec<_> = self
            .disks
            .iter()
            .map(|disk| {
                let volume = volume.to_string();
                let path = path.to_string();
                async move {
                    match disk.delete(&volume, &path).await {
                        Ok(()) => true,
                        Err(e) => {
                            warn!("delete failed: {volume}/{path}: {e}");
                            false
                        }
                    }
                }
            })
            .collect();

        let results = join_all(futures).await;
        Ok(results.iter().filter(|&&s| s).count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    /// Minimal mock disk for construction tests
    struct MockDisk;

    #[async_trait]
    impl StorageAPI for MockDisk {
        async fn disk_info(&self) -> MinioResult<crate::storage::DiskInfo> {
            unimplemented!()
        }
        async fn is_online(&self) -> bool {
            true
        }
        fn endpoint(&self) -> &str {
            "mock"
        }
        async fn read_all(&self, _: &str, _: &str) -> MinioResult<Vec<u8>> {
            unimplemented!()
        }
        async fn read_range(&self, _: &str, _: &str, _: i64, _: i64) -> MinioResult<Vec<u8>> {
            unimplemented!()
        }
        async fn write_all(&self, _: &str, _: &str, _: &[u8]) -> MinioResult<()> {
            unimplemented!()
        }
        async fn append_file(&self, _: &str, _: &str, _: &[u8]) -> MinioResult<()> {
            unimplemented!()
        }
        async fn delete(&self, _: &str, _: &str) -> MinioResult<()> {
            unimplemented!()
        }
        async fn rename(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: &str,
        ) -> MinioResult<()> {
            unimplemented!()
        }
        async fn list_dir(&self, _: &str, _: &str, _: usize) -> MinioResult<Vec<String>> {
            unimplemented!()
        }
        async fn make_volume(&self, _: &str) -> MinioResult<()> {
            unimplemented!()
        }
        async fn delete_volume(&self, _: &str) -> MinioResult<()> {
            unimplemented!()
        }
        async fn stat_file(&self, _: &str, _: &str) -> MinioResult<crate::storage::FileStat> {
            unimplemented!()
        }
        async fn file_exists(&self, _: &str, _: &str) -> MinioResult<bool> {
            unimplemented!()
        }
    }

    #[test]
    fn test_erasure_set_creation_rejects_few_disks() {
        let d = Arc::new(MockDisk) as Arc<dyn StorageAPI>;

        // 0 disks — should error
        assert!(ErasureSet::new(vec![]).is_err());

        // 1 disk — should error
        assert!(ErasureSet::new(vec![d.clone()]).is_err());

        // 2 disks — should error
        assert!(ErasureSet::new(vec![d.clone(), d.clone()]).is_err());

        // 3 disks — should succeed (M=1, N=2)
        assert!(ErasureSet::new(vec![d.clone(), d.clone(), d.clone()]).is_ok());
    }

    #[test]
    fn test_erasure_set_with_params_rejects_bad_data_blocks() {
        let d = Arc::new(MockDisk) as Arc<dyn StorageAPI>;
        let disks = vec![d.clone(), d.clone(), d.clone()];

        // data_blocks = 0 should error
        assert!(ErasureSet::with_params(disks.clone(), 0, 2).is_err());

        // data_blocks = 1, parity_blocks = 2 should succeed
        assert!(ErasureSet::with_params(disks, 1, 2).is_ok());
    }
}
