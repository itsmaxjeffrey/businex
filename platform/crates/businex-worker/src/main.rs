//! Businex durable worker: processes queued jobs and schedules.

use businex_events::{Relay, RelayOptions};
use businex_worker::Worker;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    businex_api_tracing();

    // Worker credentials: the service role (cross-tenant job processing).
    // Runtime roles never run migrations and never hold admin credentials.
    let database_url = std::env::var("BUSINEX_WORKER_DATABASE_URL")
        .expect("BUSINEX_WORKER_DATABASE_URL is required (service role)");
    let pool = businex_db::connect(&database_url, 5).await?;
    match std::env::var("BUSINEX_DATABASE_ADMIN_URL") {
        Ok(admin_url) => {
            let admin = businex_db::connect(&admin_url, 2).await?;
            businex_db::run_migrations(&admin).await?;
            admin.close().await;
        }
        Err(_) => {
            tracing::warn!("BUSINEX_DATABASE_ADMIN_URL not set; run businex-migrate before workers");
        }
    }

    let relay = Relay::new(RelayOptions {
        url: std::env::var("BUSINEX_REDIS_URL").ok(),
        password: std::env::var("BUSINEX_REDIS_PASSWORD_FILE")
            .ok()
            .map(|path| std::fs::read_to_string(path).map(|s| s.trim().to_string()))
            .transpose()?
            .filter(|s| !s.is_empty()),
        prefix: std::env::var("BUSINEX_REDIS_PREFIX").unwrap_or_else(|_| "businex".into()),
    });
    relay.start().await;

    let worker = Worker::new(pool.clone(), relay.clone()).with_worker_id(format!(
        "worker-{}",
        &uuid::Uuid::new_v4().to_string()[..8]
    ));

    // Handlers are registered by the platform as features land; until then
    // the worker still maintains schedules and reclaims expired leases.
    let (tx, rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = tx.send(true);
    });
    worker.run(rx).await;

    relay.stop();
    pool.close().await;
    Ok(())
}

fn businex_api_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .with_current_span(false)
        .flatten_event(true)
        .init();
}
