//! Object operation orchestration layer
//!
//! Core business logic of the storage system:
//! - `ObjectAPI` trait: object-level operations (PUT/GET/DELETE/LIST)
//! - `set` module: ErasureSet — Set/Disk coordination layer
//! - `erasure_objects` module: ErasureObjects — Storage API layer implementation

pub mod erasure_objects;
pub mod ns_lock;
pub mod object_api;
pub mod set;
pub mod standalone_objects;

pub use erasure_objects::ErasureObjects;
pub use object_api::{
    CompletedPart, DeleteObjectsResult, ListObjectsResult, MetadataDirective, MultipartInfo,
    ObjectAPI, ObjectInfo, VersioningConfig, VersioningStatus,
};
pub use standalone_objects::StandaloneObjects;

// ---- Test modules ----
// All test functions are tagged with #[ignore], enabled when corresponding types are ready
#[cfg(test)]
mod tests {
    //! Object operation test suite
    //!
    //! Module organization:
    //! - `object_api`: Core ObjectAPI operations (PUT/GET/DELETE/LIST/Multipart)
    //! - `utils`:      Utility functions (validation, metadata, compression)
    //! - `handlers`:   HTTP handler layer tests
    //! - `bucket`:     Bucket-level operations (handler/policy/encryption/replication)
    //! - `lifecycle`:  Lifecycle config parsing and evaluation
    //! - `replication`: Replication config parsing
    //! - `encryption`:  Encryption (SSE-C/SSE-S3/ETag decryption/range reads)
    //! - `lock`:        Namespace locks and local locks
    //! - `object_lock`: Object locking (retention/legal hold)
    //! - `batch`:       Batch jobs (expiration/replication/rotation)
    //! - `data_usage`:  Data usage scanning and caching
    //! - `copy_part`:   Copy part range parsing
    //! - `lambda`:      Object Lambda handler
    //! - `versioning`:  Versioning configuration
    //! - `bandwidth`:   Replication bandwidth monitoring
}
