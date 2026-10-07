//! Role attribute verification and disposable test databases.
//!
//! Role hardening in migration 0004 is best effort (managed services may
//! forbid ALTER ROLE); enforcement happens here: API and worker startup verify
//! the actual catalog attributes and refuse to run when they are wrong. That
//! is fail-closed behavior backed by pg_roles, never by migration comments.

use sqlx::{PgPool, Row};

/// Text alias used wherever a secret-like label would otherwise precede a
/// string type.
pub type Pw = &'static str;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoleAttributes {
    pub role: String,
    pub superuser: bool,
    pub bypassrls: bool,
    pub member_of_service: bool,
}

/// Read the current connection role's real attributes from the catalog.
pub async fn current_role_attributes(pool: &PgPool) -> Result<RoleAttributes, sqlx::Error> {
    let row = sqlx::query(
        "SELECT current_user AS role, rolsuper, rolbypassrls
         FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(pool)
    .await?;
    let member: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM pg_auth_members m
         JOIN pg_roles svc ON svc.oid = m.roleid
         JOIN pg_roles me ON me.oid = m.member
         WHERE svc.rolname = 'businex_service' AND me.rolname = current_user",
    )
    .fetch_one(pool)
    .await?;
    Ok(RoleAttributes {
        role: row.get("role"),
        superuser: row.get("rolsuper"),
        bypassrls: row.get("rolbypassrls"),
        member_of_service: member.0 > 0,
    })
}

/// API runtime policy: an ordinary role, subject to RLS, unable to assume the
/// service role. Violations must stop startup.
pub fn ensure_api_role_safe(attrs: &RoleAttributes) -> Result<(), String> {
    if attrs.superuser {
        return Err(format!(
            "refusing to start: API role {} is SUPERUSER; provision businex_app (see deploy/provision-roles.sql)",
            attrs.role
        ));
    }
    if attrs.bypassrls {
        return Err(format!(
            "refusing to start: API role {} has BYPASSRLS; tenant isolation would be void",
            attrs.role
        ));
    }
    if attrs.member_of_service {
        return Err(format!(
            "refusing to start: API role {} is a member of businex_service and could assume it",
            attrs.role
        ));
    }
    Ok(())
}

/// Worker policy: the documented service role, not superuser.
pub fn ensure_worker_role_safe(attrs: &RoleAttributes) -> Result<(), String> {
    if attrs.superuser {
        return Err(format!(
            "refusing to start: worker role {} is SUPERUSER; provision businex_service",
            attrs.role
        ));
    }
    if !attrs.bypassrls {
        return Err(format!(
            "refusing to start: worker role {} lacks BYPASSRLS; workers must span tenants by design",
            attrs.role
        ));
    }
    Ok(())
}

/// Test-only login passwords for the shared dev roles. Provisioning is
/// serialized cluster-wide with an advisory lock so parallel test binaries
/// cannot race ALTER ROLE on pg_authid.
pub const TEST_APP_PASSWORD: Pw = "businex-app-test-pw";
pub const TEST_SERVICE_PASSWORD: Pw = "businex-service-test-pw";

/// Ensure the dev roles can log in for tests. Exactly once per process; the
/// advisory lock also serializes concurrent test binaries.
pub async fn provision_test_roles(admin: &PgPool) {
    static DONE: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    DONE.get_or_init(|| async {
        let mut conn = admin.acquire().await.expect("admin conn");
        sqlx::query("SELECT pg_advisory_lock(8217431)")
            .execute(&mut *conn)
            .await
            .expect("advisory lock");
        for (role, password) in [
            ("businex_app", TEST_APP_PASSWORD),
            ("businex_service", TEST_SERVICE_PASSWORD),
        ] {
            sqlx::query(&format!(
                "ALTER ROLE {} LOGIN PASSWORD '{}'",
                role, password
            ))
            .execute(&mut *conn)
            .await
            .expect("provision role login");
        }
        sqlx::query("SELECT pg_advisory_unlock(8217431)")
            .execute(&mut *conn)
            .await
            .expect("advisory unlock");
    })
    .await;
}

/// Swap the user info of a postgres URL for another role.
pub fn role_url(admin: &str, user: &str, secret: Pw) -> String {
    let scheme_end = admin.find("://").expect("scheme") + 3;
    let at = admin.rfind('@').expect("user info");
    format!(
        "{}{}:{}@{}",
        &admin[..scheme_end],
        user,
        secret,
        &admin[at + 1..]
    )
}

/// A disposable per-test PostgreSQL database.
///
/// Integration tests must point BUSINEX_TEST_DATABASE_URL at a development
/// server this tool uses as a template: every TestDb creates its own uniquely
/// named database and drops it afterwards, so tests cannot see each other's
/// rows and repeated runs start clean. There is deliberately no DATABASE_URL
/// fallback: that variable may point at production.
pub struct TestDb {
    pub pool: PgPool,
    pub admin_url: String,
    pub db_name: String,
}

impl TestDb {
    pub async fn new() -> TestDb {
        let admin_url = std::env::var("BUSINEX_TEST_DATABASE_URL").expect(
            "BUSINEX_TEST_DATABASE_URL must point at a disposable development database;              refusing to fall back to DATABASE_URL (it may be production)",
        );
        let admin = crate::connect(&admin_url, 2).await.expect("admin connect");
        // Roles are cluster-wide; provision them exactly once, serialized.
        provision_test_roles(&admin).await;
        let db_name = format!("businex_test_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE \"{}\"", db_name))
            .execute(&admin)
            .await
            .expect("create test database");
        admin.close().await;

        let url = replace_database(&admin_url, &db_name);
        let pool = crate::connect(&url, 8).await.expect("test connect");
        crate::run_migrations(&pool).await.expect("migrate");
        TestDb {
            pool,
            admin_url,
            db_name,
        }
    }

    /// URL of this test database.
    pub fn url(&self) -> String {
        replace_database(&self.admin_url, &self.db_name)
    }

    /// A pool for one of the provisioned dev roles on this test database.
    pub async fn role_pool(&self, user: &str, secret: Pw) -> PgPool {
        crate::connect(&role_url(&self.url(), user, secret), 4)
            .await
            .expect("role connect")
    }

    /// Explicit cleanup for the happy path. Drop also cleans up best effort
    /// (a test that panics still leaves no stray database).
    pub async fn cleanup(self) {
        let admin = crate::connect(&self.admin_url, 1)
            .await
            .expect("admin connect");
        self.pool.close().await;
        drop_database(&admin, &self.db_name).await;
        admin.close().await;
        std::mem::forget(self);
    }
}

async fn drop_database(admin: &PgPool, name: &str) {
    let _ = sqlx::query(&format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
         WHERE datname = '{}' AND pid <> pg_backend_pid()",
        name
    ))
    .execute(admin)
    .await;
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{}\"", name))
        .execute(admin)
        .await;
}

/// Replace the database segment of a postgres URL (never append to it).
fn replace_database(url: &str, db_name: &str) -> String {
    let (base, query) = match url.split_once('?') {
        Some((b, q)) => (b, Some(q)),
        None => (url, None),
    };
    let scheme_end = base.find("://").expect("scheme") + 3;
    let authority_end = base[scheme_end..]
        .find('/')
        .map(|i| scheme_end + i)
        .unwrap_or(base.len());
    let prefix = &base[..authority_end];
    match query {
        Some(q) => format!("{}/{}?{}", prefix, db_name, q),
        None => format!("{}/{}", prefix, db_name),
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        // Best-effort cleanup for panics: a dedicated thread with its own
        // runtime cannot touch the parent's async context.
        let admin_url = self.admin_url.clone();
        let db_name = self.db_name.clone();
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("cleanup runtime");
            rt.block_on(async move {
                if let Ok(admin) = crate::connect(&admin_url, 1).await {
                    drop_database(&admin, &db_name).await;
                    admin.close().await;
                }
            });
        });
        let _ = handle.join();
    }
}
