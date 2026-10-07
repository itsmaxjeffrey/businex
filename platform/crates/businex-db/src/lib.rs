//! PostgreSQL access for the Businex platform: pool setup, migrations and the
//! per-transaction tenant context that drives row level security.
//!
//! Authorization has two independent layers: application checks (roles,
//! permissions, scoped grants in businex-core) and PostgreSQL RLS policies.
//! Every tenant query runs inside a transaction created with
//! begin_company_tx, which pins the company id the policies compare against.

use sqlx::postgres::PgPoolOptions;
use sqlx::{Connection as _, PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Application runtime role; RLS policies apply to it.
pub const APP_ROLE: &str = "businex_app";

/// Service role for workers and maintenance; it bypasses RLS on purpose
/// because jobs may belong to any company. Never use it for user requests.
pub const SERVICE_ROLE: &str = "businex_service";

/// Build a pool with sensible limits. The URL must use a role allowed to
/// access the schema; production connects as businex_app.
pub async fn connect(url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(url)
        .await
}

/// Apply all pending migrations. Safe to run repeatedly.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// Who the current transaction acts for. The actor id is recorded in the
/// session settings for audit triggers and debugging.
#[derive(Debug, Clone, Copy)]
pub struct CompanyContext {
    pub company_id: Uuid,
    pub actor_id: Option<Uuid>,
}

impl CompanyContext {
    pub fn new(company_id: Uuid) -> Self {
        CompanyContext {
            company_id,
            actor_id: None,
        }
    }

    pub fn with_actor(company_id: Uuid, actor_id: Uuid) -> Self {
        CompanyContext {
            company_id,
            actor_id: Some(actor_id),
        }
    }
}

/// Begin a transaction scoped to one company as the application role.
/// Row level security allows exactly this company's rows: reads of other
/// tenants return nothing and writes to other tenants are rejected.
pub async fn begin_company_tx<'a>(
    pool: &'a PgPool,
    ctx: CompanyContext,
) -> Result<Transaction<'a, Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(&format!("SET LOCAL ROLE {}", APP_ROLE))
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT set_config('businex.company_id', $1, true)")
        .bind(ctx.company_id.to_string())
        .execute(&mut *tx)
        .await?;
    match ctx.actor_id {
        Some(actor) => {
            sqlx::query("SELECT set_config('businex.actor_id', $1, true)")
                .bind(actor.to_string())
                .execute(&mut *tx)
                .await?;
        }
        None => {
            sqlx::query("SELECT set_config('businex.actor_id', '', true)")
                .execute(&mut *tx)
                .await?;
        }
    }
    Ok(tx)
}

/// Begin a company-scoped transaction on an existing connection. Used when a
/// pooled connection must be reused across transactions: the company context
/// is transaction-local (SET LOCAL), so it cannot leak into the next
/// transaction on the same connection.
pub async fn begin_company_tx_on<'c>(
    conn: &'c mut sqlx::PgConnection,
    ctx: CompanyContext,
) -> Result<Transaction<'c, Postgres>, sqlx::Error> {
    let mut tx = conn.begin().await?;
    sqlx::query(&format!("SET LOCAL ROLE {}", APP_ROLE))
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT set_config('businex.company_id', $1, true)")
        .bind(ctx.company_id.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT set_config('businex.actor_id', $1, true)")
        .bind(match ctx.actor_id {
            Some(a) => a.to_string(),
            None => String::new(),
        })
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

/// Begin a service transaction as the service role (bypasses RLS). Used by
/// workers that process jobs for any company and by migration tooling.
pub async fn begin_service_tx<'a>(pool: &'a PgPool) -> Result<Transaction<'a, Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(&format!("SET LOCAL ROLE {}", SERVICE_ROLE))
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

/// Role attribute verification (fail closed) and disposable test databases.
pub mod verify_roles;
pub use verify_roles::{
    current_role_attributes, ensure_api_role_safe, ensure_worker_role_safe,
    provision_test_roles, role_url, RoleAttributes, TestDb, TEST_APP_PASSWORD,
    TEST_SERVICE_PASSWORD,
};

/// Readiness probe: a real round trip to the database.
pub async fn ping(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}
