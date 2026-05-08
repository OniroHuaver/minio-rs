//! 指数退避 + jitter 重试。

use rand::Rng;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub base_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 4,
            base_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(60),
        }
    }
}

/// 判断是否可重试的错误
pub fn is_retryable(err_str: &str) -> bool {
    let s = err_str.to_lowercase();
    s.contains("conflict")
        || s.contains("internal server error")
        || s.contains("429")
        || s.contains("too many requests")
}

/// 指数退避重试执行
pub async fn retry_with_backoff<F, Fut, T, E>(
    config: &RetryConfig,
    ctx: &CancellationToken,
    mut f: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut backoff = config.base_backoff;

    for attempt in 0..=config.max_retries {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                let retryable = is_retryable(&e.to_string());
                if !retryable || attempt == config.max_retries {
                    return Err(e);
                }

                let jitter = {
                    let mut rng = rand::thread_rng();
                    rng.gen::<f64>() * 0.5 + 0.75
                };
                let sleep = backoff.mul_f64(jitter).min(config.max_backoff);

                tokio::select! {
                    _ = tokio::time::sleep(sleep) => {},
                    _ = ctx.cancelled() => {
                        return Err(e);
                    }
                }
                backoff = (backoff * 2).min(config.max_backoff);
            }
        }
    }

    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable() {
        assert!(is_retryable("Conflict: version mismatch"));
        assert!(is_retryable("Internal Server Error"));
        assert!(is_retryable("429 Too Many Requests"));
        assert!(!is_retryable("InvalidArgument"));
        assert!(!is_retryable("Not Found"));
    }
}
