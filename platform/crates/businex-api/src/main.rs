//! Businex platform API server.

use businex_api::{init_tracing, router, AppConfig, AppState};
use businex_events::{Relay, RelayOptions};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing("info");

    let host = std::env::var("BUSINEX_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port: u16 = std::env::var("BUSINEX_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8788);
    // Runtime credentials: the least-privilege application role. Migrations
    // need separate admin credentials and are never run with this URL.
    let database_url = std::env::var("BUSINEX_DATABASE_URL")
        .expect("BUSINEX_DATABASE_URL is required (application role)");
    let max_conn: u32 = std::env::var("BUSINEX_DB_POOL_MAX")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(10);

    let pool = businex_db::connect(&database_url, max_conn).await?;
    // Fail closed: verify the runtime role is an ordinary RLS-subject role.
    let attrs = businex_db::current_role_attributes(&pool).await?;
    if let Err(reason) = businex_db::ensure_api_role_safe(&attrs) {
        tracing::error!(role = %attrs.role, "{}", reason);
        return Err(reason.into());
    }
    tracing::info!(role = %attrs.role, "database role verified");
    match std::env::var("BUSINEX_DATABASE_ADMIN_URL") {
        Ok(admin_url) => {
            let admin = businex_db::connect(&admin_url, 2).await?;
            businex_db::run_migrations(&admin).await?;
            admin.close().await;
            tracing::info!("migrations applied with admin credentials");
        }
        Err(_) => {
            tracing::warn!(
                "BUSINEX_DATABASE_ADMIN_URL not set; skipping migrations. \
                 Run the businex-migrate binary with admin credentials."
            );
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

    let config = AppConfig {
        session_ttl_secs: std::env::var("BUSINEX_SESSION_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(86_400),
        // Secure cookies are the default; local development over plain HTTP
        // must opt out explicitly.
        cookie_secure: std::env::var("BUSINEX_COOKIE_SECURE")
            .map(|v| v != "false")
            .unwrap_or(true),
        // Development login/registration stays off unless explicitly enabled.
        dev_login_enabled: std::env::var("BUSINEX_DEV_LOGIN")
            .map(|v| v == "true")
            .unwrap_or(false),
    };

    let app = router(AppState {
        pool: pool.clone(),
        relay: relay.clone(),
        started_at: Instant::now(),
        config,
    });

    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(addr = %addr, "businex api listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutdown signal received");
        })
        .await?;

    relay.stop();
    pool.close().await;
    Ok(())
}
