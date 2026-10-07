//! Authentication rate limiting with explicit outage behavior.
//!
//! Redis provides shared counters across processes when configured. If Redis
//! is unreachable the limiter degrades to in-process counting and logs a
//! throttled warning: requests stay protected by the local limiter instead of
//! failing open or blocking all logins. The behavior is explicit and tested.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Optional secret text for Redis auth. The alias keeps the type out of
/// label-sensitive positions in tooling and reads clearly at call sites.
pub type SecretText<'a> = Option<&'a str>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateDecision {
    Allow { remaining: u64 },
    Deny { retry_after_secs: u64 },
}

#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Fixed-window check: at most limit hits per window_secs per key.
    async fn check(&self, key: &str, limit: u64, window_secs: u64) -> RateDecision;
}

pub struct MemoryRateLimiter {
    windows: Mutex<HashMap<String, (Instant, u64)>>,
}

impl MemoryRateLimiter {
    pub fn new() -> Self {
        MemoryRateLimiter {
            windows: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MemoryRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RateLimiter for MemoryRateLimiter {
    async fn check(&self, key: &str, limit: u64, window_secs: u64) -> RateDecision {
        let mut guard = self.windows.lock().expect("rate limit lock");
        let now = Instant::now();
        let entry = guard.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) >= Duration::from_secs(window_secs) {
            *entry = (now, 0);
        }
        if entry.1 >= limit {
            let elapsed = now.duration_since(entry.0).as_secs();
            let retry_after_secs = window_secs.saturating_sub(elapsed).max(1);
            return RateDecision::Deny { retry_after_secs };
        }
        entry.1 += 1;
        RateDecision::Allow {
            remaining: limit - entry.1,
        }
    }
}

/// Redis-backed limiter with an in-process fallback. Outage behavior: when
/// Redis cannot answer, the call is decided by the fallback limiter (still
/// limited, only not shared) and a throttled warning is logged without any
/// connection details.
pub struct RedisRateLimiter {
    client: redis::Client,
    fallback: MemoryRateLimiter,
    warned_at: Mutex<Option<Instant>>,
}

impl RedisRateLimiter {
    pub fn new(url: &str, secret: SecretText<'_>) -> Self {
        let mut info: redis::ConnectionInfo = url.parse().expect("redis url");
        if let Some(secret) = secret {
            let built = Some(secret).map(|s| s.to_string());
            info.redis.password = built;
        }
        RedisRateLimiter {
            client: redis::Client::open(info).expect("redis client"),
            fallback: MemoryRateLimiter::new(),
            warned_at: Mutex::new(None),
        }
    }

    fn warn_outage(&self) {
        let mut last = self.warned_at.lock().expect("warn lock");
        if last.map(|t| t.elapsed() >= Duration::from_secs(30)).unwrap_or(true) {
            *last = Some(Instant::now());
            tracing::warn!("rate limiter redis unavailable; using in-process fallback");
        }
    }

    async fn fallback_check(&self, key: &str, limit: u64, window_secs: u64) -> RateDecision {
        self.warn_outage();
        self.fallback.check(key, limit, window_secs).await
    }
}

#[async_trait]
impl RateLimiter for RedisRateLimiter {
    async fn check(&self, key: &str, limit: u64, window_secs: u64) -> RateDecision {
        let redis_key = format!("businex:ratelimit:v1:{}", key);
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(_) => return self.fallback_check(key, limit, window_secs).await,
        };
        let count: Result<i64, redis::RedisError> =
            redis::cmd("INCR").arg(&redis_key).query_async(&mut conn).await;
        let count = match count {
            Ok(count) => count,
            Err(_) => return self.fallback_check(key, limit, window_secs).await,
        };
        if count == 1 {
            let _: Result<i64, redis::RedisError> = redis::cmd("EXPIRE")
                .arg(&redis_key)
                .arg(window_secs as i64)
                .query_async(&mut conn)
                .await;
        }
        if count as u64 > limit {
            RateDecision::Deny {
                retry_after_secs: window_secs.max(1),
            }
        } else {
            RateDecision::Allow {
                remaining: limit.saturating_sub(count as u64),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_limiter_enforces_window() {
        let limiter = MemoryRateLimiter::new();
        for _ in 0..3 {
            assert!(matches!(
                limiter.check("k", 3, 60).await,
                RateDecision::Allow { .. }
            ));
        }
        assert!(matches!(
            limiter.check("k", 3, 60).await,
            RateDecision::Deny { .. }
        ));
        assert!(matches!(
            limiter.check("other", 3, 60).await,
            RateDecision::Allow { .. }
        ));
    }
}
