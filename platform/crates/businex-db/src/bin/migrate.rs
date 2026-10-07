//! Schema migration tool. Runs with dedicated admin credentials.
//!
//! Usage: set BUSINEX_DATABASE_ADMIN_URL to a migration/admin role URL and run
//! businex-migrate. API and worker runtimes never run migrations and never
//! receive admin credentials.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let admin_url = match std::env::var("BUSINEX_DATABASE_ADMIN_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("BUSINEX_DATABASE_ADMIN_URL is required for migrations");
            return ExitCode::from(2);
        }
    };
    let pool = match businex_db::connect(&admin_url, 2).await {
        Ok(pool) => pool,
        Err(err) => {
            eprintln!("database connection failed: {}", err);
            return ExitCode::from(3);
        }
    };
    match businex_db::run_migrations(&pool).await {
        Ok(()) => {
            println!("migrations applied");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("migration failed: {}", err);
            ExitCode::from(4)
        }
    }
}
