//! Integration tests for the Redis live-event relay against a real Redis.
//!
//! Set BUSINEX_TEST_REDIS_URL to a disposable Redis instance.

use businex_events::{Event, Relay, RelayOptions, Status};
use serde_json::json;
use std::time::{Duration, Instant};

fn redis_url() -> String {
    std::env::var("BUSINEX_TEST_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:16390".into())
}

fn relay(prefix: &str) -> Relay {
    Relay::new(RelayOptions {
        url: Some(redis_url()),
        password: None,
        prefix: prefix.into(),
    })
}

async fn wait_connected(r: &Relay, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if r.status() == Status::Connected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("relay did not reach connected state in {:?}", timeout);
}

fn recv_within(rx: &mut tokio::sync::broadcast::Receiver<Event>, timeout: Duration) -> Option<Event> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(ev) = rx.try_recv() {
            return Some(ev);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

#[tokio::test(flavor = "multi_thread")]
async fn remote_delivery_is_exactly_once() {
    let prefix = format!("businex-test-{}", uuid::Uuid::new_v4());
    let a = relay(&prefix);
    let b = relay(&prefix);
    a.start().await;
    b.start().await;
    wait_connected(&a, Duration::from_secs(10)).await;
    wait_connected(&b, Duration::from_secs(10)).await;

    let mut local_rx = a.subscribe();
    let mut remote_rx = b.subscribe();

    a.publish(Event::new("record.changed", "w1", json!({"id": 7})));

    let local = recv_within(&mut local_rx, Duration::from_secs(2)).expect("local delivery");
    let remote = recv_within(&mut remote_rx, Duration::from_secs(5)).expect("remote delivery");
    assert_eq!(local.kind, "record.changed");
    assert_eq!(remote.kind, "record.changed");
    assert_eq!(remote.payload, json!({"id": 7}));

    // No duplicates: neither side receives a second copy.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(local_rx.try_recv().is_err(), "local duplicate delivery");
    assert!(remote_rx.try_recv().is_err(), "remote duplicate delivery");

    // Sender never receives its own remote echo.
    a.stop();
    b.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn terminal_events_stay_in_one_process() {
    let prefix = format!("businex-test-{}", uuid::Uuid::new_v4());
    let a = relay(&prefix);
    let b = relay(&prefix);
    a.start().await;
    b.start().await;
    wait_connected(&a, Duration::from_secs(10)).await;
    wait_connected(&b, Duration::from_secs(10)).await;

    let mut local_rx = a.subscribe();
    let mut remote_rx = b.subscribe();
    a.publish(Event::new("terminal.output", "w1", json!({"chunk": "ls\n"})));

    assert!(recv_within(&mut local_rx, Duration::from_secs(1)).is_some(), "local delivery");
    assert!(
        recv_within(&mut remote_rx, Duration::from_secs(1)).is_none(),
        "terminal events must not cross processes"
    );
    a.stop();
    b.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn oversized_events_do_not_cross_processes() {
    let prefix = format!("businex-test-{}", uuid::Uuid::new_v4());
    let a = relay(&prefix);
    let b = relay(&prefix);
    a.start().await;
    b.start().await;
    wait_connected(&a, Duration::from_secs(10)).await;
    wait_connected(&b, Duration::from_secs(10)).await;

    let mut local_rx = a.subscribe();
    let mut remote_rx = b.subscribe();
    let huge = "x".repeat(2 * 1024 * 1024);
    a.publish(Event::new("document.blob", "w1", json!({"data": huge})));

    assert!(recv_within(&mut local_rx, Duration::from_secs(1)).is_some());
    assert!(
        recv_within(&mut remote_rx, Duration::from_secs(1)).is_none(),
        "oversized messages must not be published"
    );
    a.stop();
    b.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn prefixes_isolate_installations() {
    let a = relay(&format!("businex-test-a-{}", uuid::Uuid::new_v4()));
    let b = relay(&format!("businex-test-b-{}", uuid::Uuid::new_v4()));
    a.start().await;
    b.start().await;
    wait_connected(&a, Duration::from_secs(10)).await;
    wait_connected(&b, Duration::from_secs(10)).await;

    let mut remote_rx = b.subscribe();
    a.publish(Event::new("record.changed", "w1", json!({})));
    assert!(
        recv_within(&mut remote_rx, Duration::from_secs(1)).is_none(),
        "different prefixes must not see each other"
    );
    a.stop();
    b.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn status_reports_disabled_and_connected() {
    let local_only = Relay::new(RelayOptions::default());
    assert_eq!(local_only.status(), Status::Disabled);

    let live = relay(&format!("businex-test-{}", uuid::Uuid::new_v4()));
    live.start().await;
    wait_connected(&live, Duration::from_secs(10)).await;
    assert_eq!(live.status(), Status::Connected);
    live.stop();
    assert_eq!(live.status(), Status::Disabled);
}
