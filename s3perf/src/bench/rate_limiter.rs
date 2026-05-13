//! Token Bucket 速率限制器。
//!
//! 全局 RPS 上限：所有 worker 共享一个 [`RateLimiter`]（`Arc`），
//! 在每次 S3 请求前调用 [`RateLimiter::wait`]。

use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration, Instant};

/// 基于固定间隔算法的简单速率限制器。
///
/// 记录上次允许请求的时刻，计算下一个允许的时刻，
/// 如果还没到则 sleep 等待，从而达到限制每秒请求数（RPS）的目的。
///
/// # 不限流
///
/// 当构造时传入的 `rps <= 0` 时，`wait()` 立即返回，不做任何等待。
pub struct RateLimiter {
    /// 两次请求之间的最小间隔（秒）。`None` 表示不限流。
    interval: Option<f64>,
    /// 上一次允许请求的时刻。`None` 表示尚未有过请求。
    last: Mutex<Option<Instant>>,
}

/// 由 CLI `rps_limit` 构造共享限速器；无效或非正数时不限流。
pub fn opt_rps_limiter(rps: Option<f64>) -> Option<Arc<RateLimiter>> {
    rps.filter(|x| x.is_finite() && *x > 0.0)
        .map(|r| Arc::new(RateLimiter::new(r)))
}

impl RateLimiter {
    /// 创建一个限速器。
    ///
    /// `rps` 为每秒允许的最大请求数。
    /// 当 `rps <= 0` 时，限速器不生效，`wait()` 立即返回。
    pub fn new(rps: f64) -> Self {
        let interval = if rps > 0.0 { Some(1.0 / rps) } else { None };
        Self {
            interval,
            last: Mutex::new(None),
        }
    }

    /// 异步等待直到可以发送下一个请求。
    ///
    /// 计算 `next_allowed = last + interval`：
    /// - 如果当前时间已过 `next_allowed`，则立即返回；
    /// - 否则 sleep `next_allowed - now`，维持固定速率。
    ///
    /// 不限流时立即返回，不做任何等待。
    pub async fn wait(&self) {
        let interval = match self.interval {
            Some(i) => i,
            None => return, // rps <= 0，不限流
        };

        let (sleep_for, stamp_after) = {
            let mut g = self.last.lock().unwrap();
            let now = Instant::now();
            match *g {
                None => {
                    *g = Some(now);
                    return;
                }
                Some(last) => {
                    let next_allowed = last + Duration::from_secs_f64(interval);
                    if now < next_allowed {
                        (Some(next_allowed - now), Some(next_allowed))
                    } else {
                        *g = Some(now);
                        return;
                    }
                }
            }
        };

        if let Some(d) = sleep_for {
            sleep(d).await;
            if let Some(ts) = stamp_after {
                *self.last.lock().unwrap() = Some(ts);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration as StdDuration;

    #[tokio::test]
    async fn test_no_limit() {
        // rps <= 0 bypasses pacing
        let limiter = RateLimiter::new(0.0);
        let start = Instant::now();
        limiter.wait().await;
        assert!(start.elapsed() < StdDuration::from_millis(100));

        let limiter = RateLimiter::new(-1.0);
        let start = Instant::now();
        limiter.wait().await;
        assert!(start.elapsed() < StdDuration::from_millis(100));
    }

    #[tokio::test]
    async fn test_basic_rate_limit() {
        let rps = 100.0;
        let interval = 1.0 / rps; // 10ms
        let limiter = RateLimiter::new(rps);

        let start = Instant::now();
        // first acquire should succeed immediately
        limiter.wait().await;
        let elapsed = start.elapsed();
        assert!(
            elapsed < StdDuration::from_millis(5),
            "unexpected wait on first token: {:?}",
            elapsed
        );

        // second acquire observes ~interval spacing
        limiter.wait().await;
        let elapsed = start.elapsed();
        assert!(
            elapsed >= StdDuration::from_secs_f64(interval * 0.8),
            "second wait should throttle ~{interval}s spacing: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_burst_catchup() {
        // after a long idle period the limiter resets
        let limiter = RateLimiter::new(1000.0); // 1ms 间隔
        limiter.wait().await; // t=0
        tokio::time::sleep(StdDuration::from_millis(100)).await;
        // 100ms idle >> default 1ms spacing
        let start = Instant::now();
        limiter.wait().await;
        assert!(
            start.elapsed() < StdDuration::from_millis(5),
            "burst path should unblock immediately after idle sleep: {:?}",
            start.elapsed()
        );
    }
}
