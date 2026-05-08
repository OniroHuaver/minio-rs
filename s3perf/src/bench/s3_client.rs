//! S3 客户端工厂 — aws-sdk-s3 + 可选自定义 TLS（自签名 / 额外 CA）与多 endpoint。

use aws_credential_types::Credentials;
use aws_sdk_s3::config::{BehaviorVersion, Region};
use aws_sdk_s3::Client;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct S3Config {
    pub host: String,
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
    pub tls: bool,
    pub insecure: bool,
    pub no_verify_ssl: bool,
    /// 额外信任的 PEM（整文件内容），与系统根证书合并
    pub ca_pem: Option<Vec<u8>>,
}

impl S3Config {
    pub fn endpoint_url(&self) -> String {
        let scheme = if self.tls { "https" } else { "http" };
        format!("{}://{}", scheme, self.host)
    }

    pub fn create_client(&self) -> Client {
        let creds = Credentials::new(
            &self.access_key,
            &self.secret_key,
            None,
            None,
            "s3perf-static",
        );

        let mut config_builder = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .credentials_provider(creds)
            .region(Region::new(self.region.clone()))
            .endpoint_url(self.endpoint_url())
            .force_path_style(true);

        let need_custom_https =
            self.tls && (self.insecure || self.no_verify_ssl || self.ca_pem.is_some());

        if need_custom_https {
            let insecure = self.insecure || self.no_verify_ssl;
            match crate::bench::http_transport::shared_https_client(insecure, self.ca_pem.as_deref())
            {
                Ok(http) => {
                    config_builder = config_builder.http_client(http);
                }
                Err(e) => {
                    tracing::warn!("custom TLS hyper client unavailable, falling back: {e}");
                }
            }
        }

        let config = config_builder.build();
        Client::from_conf(config)
    }
}

/// `host_idx` 为 `hosts` 中的下标（由 `Common::pick_host_index` 等产生）。
pub type ClientFactory = Arc<dyn Fn(usize) -> Client + Send + Sync>;

pub fn client_factory(config: S3Config, hosts: Vec<String>) -> ClientFactory {
    let hosts = if hosts.is_empty() {
        vec![config.host.clone()]
    } else {
        hosts
    };
    Arc::new(move |idx: usize| {
        let mut c = config.clone();
        c.host = hosts[idx % hosts.len()].clone();
        c.create_client()
    })
}
