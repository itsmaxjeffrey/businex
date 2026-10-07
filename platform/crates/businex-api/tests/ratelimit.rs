//! Redis rate limiter tests: shared counters and explicit outage fallback.
//! Uses the disposable test Redis from the protected dev environment.

use businex_api::ratelimit::{RateDecision, RateLimiter, RedisRateLimiter};
use uuid::Uuid;

fn redis_url() -> String {
    std::env::var("BUSINEX_TEST_REDIS_URL")
        .expect("BUSINEX_TEST_REDIS_URL must point at a disposable development Redis")
}

#[tokio::test(flavor = "multi_thread")]
async fn counters_are_shared_across_instances() {
    let key = format!("test-shared-{}", Uuid::new_v4());
    let a = RedisRateLimiter::new(&redis_url(), None);
    let b = RedisRateLimiter::new(&redis_url(), None);
    for i in 0..3 {
        assert!(
            matches!(a.check(&key, 3, 60).await, RateDecision::Allow { remaining } if remaining == 2 - i),
            "instance A allows up to the limit"
        );
    }
    // A different instance sees the same counter: no bypass by reconnecting.
    assert!(matches!(
        b.check(&key, 3, 60).await,
        RateDecision::Deny { retry_after_secs } if retry_after_secs >= 1
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn outage_falls_back_to_in_process_limiting() {
    // Unreachable Redis: explicit behavior is local limiting, not failure-open.
    let key = format!("test-outage-{}", Uuid::new_v4());
    let limiter = RedisRateLimiter::new("redis://127.0.0.1:1", None);
    for _ in 0..2 {
        assert!(matches!(
            limiter.check(&key, 2, 60).await,
            RateDecision::Allow { .. }
        ));
    }
    assert!(
        matches!(limiter.check(&key, 2, 60).await, RateDecision::Deny { .. }),
        "fallback limiter still enforces the cap during an outage"
    );
}
