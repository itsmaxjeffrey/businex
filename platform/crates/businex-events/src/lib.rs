//! Transient live events over Redis Pub/Sub.
//!
//! This carries the semantics of the Businex workspace event relay: events are
//! notifications that clients refetch records for after reconnecting. Redis
//! Pub/Sub is NOT a durable queue and never carries durable work: jobs, agent
//! runs and schedules live in PostgreSQL (see businex-queue).
//!
//! Wire format stays compatible with the existing relay: one JSON envelope
//! with a sender id and the event. Senders ignore their own messages, terminal
//! events (PTY-style session streams) stay process-local, oversized messages
//! are dropped, and installation prefixes isolate environments.

use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use uuid::Uuid;

const MAX_MESSAGE_BYTES: usize = 1_048_576;
const WARN_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "workspaceId")]
    pub workspace_id: String,
    pub payload: Value,
    pub at: DateTime<Utc>,
}

impl Event {
    pub fn new(kind: impl Into<String>, workspace_id: impl Into<String>, payload: Value) -> Self {
        Event {
            kind: kind.into(),
            workspace_id: workspace_id.into(),
            payload,
            at: Utc::now(),
        }
    }

    /// Terminal events stream process-local data (PTY output, live cursors) and
    /// never cross process boundaries.
    pub fn is_terminal(&self) -> bool {
        self.kind.starts_with("terminal.")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Envelope {
    sender: String,
    event: Event,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Disabled,
    Connected,
    Degraded,
}

#[derive(Debug, Clone)]
pub struct RelayOptions {
    pub url: Option<String>,
    pub password: Option<String>,
    pub prefix: String,
}

impl Default for RelayOptions {
    fn default() -> Self {
        RelayOptions {
            url: None,
            password: None,
            prefix: "businex".into(),
        }
    }
}

struct Inner {
    sender_id: String,
    channel: String,
    opts: RelayOptions,
    local: broadcast::Sender<Event>,
    status: AtomicU8,
    last_warn: Mutex<Option<Instant>>,
    running: Mutex<Vec<JoinHandle<()>>>,
}

#[derive(Clone)]
pub struct Relay {
    inner: Arc<Inner>,
}

impl Relay {
    pub fn new(opts: RelayOptions) -> Relay {
        let prefix = opts.prefix.clone();
        let (local, _) = broadcast::channel(1024);
        Relay {
            inner: Arc::new(Inner {
                sender_id: Uuid::new_v4().to_string(),
                channel: format!("{}:workspace-events:v1", prefix),
                opts,
                local,
                status: AtomicU8::new(0),
                last_warn: Mutex::new(None),
                running: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn status(&self) -> Status {
        match self.inner.status.load(Ordering::Relaxed) {
            1 => Status::Connected,
            2 => Status::Degraded,
            _ => Status::Disabled,
        }
    }

    /// Subscribe to all events visible to this process (local and remote).
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.local.subscribe()
    }

    /// Publish an event: delivered to local subscribers immediately and
    /// relayed to other processes unless it is terminal or oversized.
    pub fn publish(&self, event: Event) {
        let _ = self.inner.local.send(event.clone());
        if event.is_terminal() {
            return;
        }
        let client = match self.client() {
            Some(c) => c,
            None => return,
        };
        let envelope = Envelope {
            sender: self.inner.sender_id.clone(),
            event,
        };
        let message = match serde_json::to_string(&envelope) {
            Ok(m) => m,
            Err(_) => return,
        };
        if message.len() > MAX_MESSAGE_BYTES {
            self.warn();
            return;
        }
        let channel = self.inner.channel.clone();
        let password = self.inner.opts.password.clone();
        tokio::spawn(async move {
            if let Ok(mut conn) = connect(&client, password.as_deref()).await {
                let _: Result<i64, _> = redis::cmd("PUBLISH")
                    .arg(channel)
                    .arg(message)
                    .query_async(&mut conn)
                    .await;
            }
        });
    }

    /// Start the background publisher/subscriber tasks. Without a Redis URL
    /// the relay is local-only and reports Disabled.
    pub async fn start(&self) {
        let client = match self.client() {
            Some(c) => c,
            None => {
                self.inner.status.store(0, Ordering::Relaxed);
                return;
            }
        };

        let sub_self = self.clone();
        let sub_client = client.clone();
        let sub_password = self.inner.opts.password.clone();
        let sub_channel = self.inner.channel.clone();
        let subscriber = tokio::spawn(async move {
            let mut backoff = Duration::from_millis(250);
            loop {
                match sub_client.get_async_pubsub().await {
                    Ok(mut pubsub) => {
                        if pubsub.subscribe(&sub_channel).await.is_err() {
                            sub_self.inner.status.store(2, Ordering::Relaxed);
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(Duration::from_secs(8));
                            continue;
                        }
                        sub_self.inner.status.store(1, Ordering::Relaxed);
                        backoff = Duration::from_millis(250);
                        let mut messages = pubsub.on_message();
                        while let Some(msg) = messages.next().await {
                            let payload: String = match msg.get_payload() {
                                Ok(p) => p,
                                Err(_) => continue,
                            };
                            if payload.len() > MAX_MESSAGE_BYTES {
                                continue;
                            }
                            let envelope: Envelope = match serde_json::from_str(&payload) {
                                Ok(e) => e,
                                Err(_) => continue,
                            };
                            if envelope.sender == sub_self.inner.sender_id
                                || envelope.event.is_terminal()
                            {
                                continue;
                            }
                            // Remote deliveries are local-only: never republished.
                            let _ = sub_self.inner.local.send(envelope.event);
                        }
                        sub_self.inner.status.store(2, Ordering::Relaxed);
                        sub_self.warn();
                    }
                    Err(_) => {
                        sub_self.inner.status.store(2, Ordering::Relaxed);
                        sub_self.warn();
                    }
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(8));
            }
        });
        self.inner.running.lock().unwrap().push(subscriber);
        let _ = connect(&client, sub_password.as_deref()).await;
    }

    /// Stop the background tasks and close connections.
    pub fn stop(&self) {
        let mut running = self.inner.running.lock().unwrap();
        for handle in running.drain(..) {
            handle.abort();
        }
        self.inner.status.store(0, Ordering::Relaxed);
    }

    fn client(&self) -> Option<redis::Client> {
        self.inner.opts.url.as_ref().and_then(|url| {
            redis::Client::open(build_connection_info(
                url,
                self.inner.opts.password.as_deref(),
            ))
            .ok()
        })
    }

    /// Throttled warning without connection details: Redis errors can include
    /// URLs or credentials and must never reach the logs.
    fn warn(&self) {
        let mut last = self.inner.last_warn.lock().unwrap();
        if last.map(|t| t.elapsed() >= WARN_INTERVAL).unwrap_or(true) {
            *last = Some(Instant::now());
            tracing::warn!("redis unavailable; live updates are local to this process");
        }
    }
}

fn build_connection_info(url: &str, password: Option<&str>) -> redis::ConnectionInfo {
    let mut info: redis::ConnectionInfo = url.parse().expect("valid redis url in configuration");
    if let Some(password) = password {
        info.redis.password = Some(password.to_string());
    }
    info
}

async fn connect(
    client: &redis::Client,
    _password: Option<&str>,
) -> Result<redis::aio::MultiplexedConnection, redis::RedisError> {
    client.get_multiplexed_async_connection().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_events_stay_local() {
        assert!(Event::new("terminal.output", "w1", json_null()).is_terminal());
        assert!(!Event::new("record.changed", "w1", json_null()).is_terminal());
    }

    #[test]
    fn local_only_without_url() {
        let relay = Relay::new(RelayOptions::default());
        assert_eq!(relay.status(), Status::Disabled);
        let mut rx = relay.subscribe();
        relay.publish(Event::new("record.changed", "w1", json_null()));
        let got = rx.try_recv().expect("local delivery works without redis");
        assert_eq!(got.kind, "record.changed");
    }

    fn json_null() -> Value {
        Value::Null
    }
}
