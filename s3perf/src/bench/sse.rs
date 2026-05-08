//! SSE 配置 — S3 服务端加密（Server-Side Encryption）支持。
//!
//! 支持三种模式：
//! - `None`：不加密
//! - `SseS3`：服务端管理密钥（`x-amz-server-side-encryption: AES256`）
//! - `SseC`：客户端管理密钥（32-byte 随机密钥，通过请求头发送）

use aws_sdk_s3::operation::get_object::builders::GetObjectFluentBuilder;
use aws_sdk_s3::operation::put_object::builders::PutObjectFluentBuilder;
use aws_sdk_s3::types::ServerSideEncryption;

/// SSE 配置枚举。
#[derive(Debug, Clone)]
pub enum SseConfig {
    /// 不加密。
    None,
    /// SSE-C：客户端生成并管理 32-byte 随机密钥。
    SseC {
        /// 32-byte 密钥。
        key: [u8; 32],
    },
    /// SSE-S3：服务端管理密钥（AES256）。
    SseS3,
}

impl Default for SseConfig {
    fn default() -> Self {
        Self::None
    }
}

impl SseConfig {
    /// 生成随机 32-byte 密钥的 SSE-C 配置。
    /// 每次调用生成新的随机密钥。
    pub fn random_ssec() -> Self {
        use rand::Rng;
        let mut key = [0u8; 32];
        rand::thread_rng().fill(&mut key);
        Self::SseC { key }
    }

    /// 将 SSE 配置应用到 [`PutObjectInputBuilder`]。
    ///
    /// - `None`：直接返回，无变化
    /// - `SseS3`：添加 `x-amz-server-side-encryption: AES256` 请求头
    /// - `SseC`：添加 SSE-C 请求头（algorithm + key）
    pub fn apply_to_put_request(
        &self,
        builder: PutObjectFluentBuilder,
    ) -> PutObjectFluentBuilder {
        match self {
            Self::None => builder,
            Self::SseS3 => {
                builder.server_side_encryption(ServerSideEncryption::Aes256)
            }
            Self::SseC { key } => {
                use base64::Engine;
                let engine = base64::engine::general_purpose::STANDARD;
                let key_b64 = engine.encode(key);
                builder
                    .sse_customer_algorithm("AES256")
                    .sse_customer_key(key_b64)
                // 注意：sse_customer_key_md5 需要 MD5 哈希，
                // 因 md-5 crate 未在依赖中，暂不设置。
            }
        }
    }

    /// 将 SSE 配置应用到 [`GetObjectFluentBuilder`]。
    ///
    /// - `None` / `SseS3`：无变化（SSE-S3 加密的对象可直接读取）
    /// - `SseC`：添加 SSE-C 解密所需的请求头
    pub fn apply_to_get_request(
        &self,
        builder: GetObjectFluentBuilder,
    ) -> GetObjectFluentBuilder {
        match self {
            Self::None => builder,
            Self::SseS3 => builder,
            Self::SseC { key } => {
                use base64::Engine;
                let engine = base64::engine::general_purpose::STANDARD;
                let key_b64 = engine.encode(key);
                builder
                    .sse_customer_algorithm("AES256")
                    .sse_customer_key(key_b64)
            }
        }
    }
}
