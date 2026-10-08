//! Durable budget accounting tests against real PostgreSQL.
//!
//! These cover the model-review requirements that in-memory tests cannot:
//! simultaneous reservations from independent connections cannot double-spend,
//! totals survive process restarts (fresh pool), and unknown cost is never
//! counted as zero.

use businex_db::TestDb;
use businex_models::store::{self, DenyReason, StoreError};
use businex_models::{Cost, Usage};
use uuid::Uuid;

async fn setup_company(db: &TestDb) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, slug) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(format!("Budget Co {}", id))
        .bind(format!("budget-co-{}", id))
        .execute(&db.pool)
        .await
        .expect("insert company");
    id
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_reservations_cannot_double_spend() {
    let db = TestDb::new().await;
    let company = setup_company(&db).await;
    store::set_budget(&db.pool, company, Some(1000), None, None)
        .await
        .expect("budget");

    // Eight independent connections race for 600 tokens each; only one fits.
    let mut handles = Vec::new();
    for _ in 0..8 {
        let pool = db.pool.clone();
        handles.push(tokio::spawn(async move {
            store::reserve(&pool, company, 600, None).await
        }));
    }
    let mut wins = 0;
    for handle in handles {
        if handle.await.expect("join").is_ok() {
            wins += 1;
        }
    }
    assert_eq!(wins, 1, "only one 600-token call fits in 1000 tokens");
    let budget = store::get_budget(&db.pool, company)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(budget.reserved_tokens, 600);
    assert_eq!(budget.settled_tokens, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn totals_survive_process_restart() {
    let db = TestDb::new().await;
    let company = setup_company(&db).await;
    store::set_budget(&db.pool, company, Some(10_000), None, None)
        .await
        .expect("budget");
    let reservation = store::reserve(&db.pool, company, 500, None)
        .await
        .expect("reserve");
    store::settle(
        &db.pool,
        company,
        reservation,
        "test",
        "test-model",
        Some(Usage::of(300, 150)),
        Cost::Unknown,
    )
    .await
    .expect("settle");

    // A fresh pool is what a restarted process sees: totals must persist.
    let restarted = businex_db::connect(&db.url(), 4).await.expect("reconnect");
    let budget = store::get_budget(&restarted, company)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(budget.settled_tokens, 450, "observed usage persisted");
    assert_eq!(budget.reserved_tokens, 0, "reservation closed");
    assert_eq!(
        budget.unknown_cost_calls, 1,
        "unknown cost recorded as unknown"
    );

    // And the cap still applies after restart.
    let denied = store::reserve(&restarted, company, 9_600, None).await;
    assert!(
        matches!(denied, Err(StoreError::Denied(DenyReason::Tokens { .. }))),
        "cap survives restart: {:?}",
        denied.err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_usage_charges_estimate_and_unknown_cost_never_zeroes() {
    let db = TestDb::new().await;
    let company = setup_company(&db).await;
    let reservation = store::reserve(&db.pool, company, 50, None)
        .await
        .expect("reserve");
    // Stream died with no usage and no cost: charge the estimate, mark unknown.
    store::settle(&db.pool, company, reservation, "test", "test-model", None, Cost::Unknown)
        .await
        .expect("settle");
    let budget = store::get_budget(&db.pool, company)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(budget.settled_tokens, 50);
    assert_eq!(budget.settled_cost_micros, 0);
    assert_eq!(budget.unknown_cost_calls, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn ambiguous_outcome_is_charged_and_recorded_for_reconciliation() {
    let db = TestDb::new().await;
    let company = setup_company(&db).await;
    let reservation = store::reserve(&db.pool, company, 50, None)
        .await
        .expect("reserve");
    // The call was dispatched but its outcome is unknown (timeout after
    // dispatch). The estimate stays charged and the row records the unknown
    // outcome for reconciliation instead of refunding the capacity.
    store::settle_ambiguous(&db.pool, company, reservation, "test", "test-model")
        .await
        .expect("settle ambiguous");
    let budget = store::get_budget(&db.pool, company)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(budget.settled_tokens, 50, "the estimate stays charged");
    assert_eq!(budget.reserved_tokens, 0, "the reservation is closed");
    assert_eq!(budget.unknown_cost_calls, 1, "unknown cost never zeroes");

    let mut conn = db.pool.acquire().await.expect("connection");
    sqlx::query("SELECT set_config('businex.company_id', $1, false)")
        .bind(company.to_string())
        .execute(&mut *conn)
        .await
        .expect("tenant pin");
    let outcome: (String,) =
        sqlx::query_as("SELECT outcome FROM model_usage WHERE company_id = $1")
            .bind(company)
            .fetch_one(&mut *conn)
            .await
            .expect("usage row");
    assert_eq!(outcome.0, "ambiguous", "unknown outcome persisted");

    // The closed reservation can neither be released as unspent nor settled
    // a second time.
    let closed = store::release(&db.pool, company, reservation).await;
    assert!(
        matches!(closed, Err(StoreError::ReservationClosed)),
        "{:?}",
        closed
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_priced_reservations_cannot_exceed_the_cost_cap() {
    let db = TestDb::new().await;
    let company = setup_company(&db).await;
    store::set_budget(&db.pool, company, None, Some(1_000_000), Some(false))
        .await
        .expect("budget");

    // Eight independent connections race for 600_000 micros each. Money is
    // reserved like tokens, so only one call fits the same remaining balance.
    let mut handles = Vec::new();
    for _ in 0..8 {
        let pool = db.pool.clone();
        handles.push(tokio::spawn(async move {
            store::reserve(&pool, company, 10, Some(600_000)).await
        }));
    }
    let mut wins = 0;
    for handle in handles {
        if handle.await.expect("join").is_ok() {
            wins += 1;
        }
    }
    assert_eq!(wins, 1, "only one 600k-micro call fits in a 1M-micro cap");
    let budget = store::get_budget(&db.pool, company)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(budget.reserved_cost_micros, 600_000, "money stays reserved");
    assert_eq!(budget.settled_cost_micros, 0);
    assert_eq!(budget.reserved_tokens, 10);
}

#[tokio::test(flavor = "multi_thread")]
async fn deny_unknown_cost_applies_without_a_cost_cap() {
    let db = TestDb::new().await;
    let company = setup_company(&db).await;
    // Policy configured with no monetary cap at all: unknown-price calls are
    // still refused.
    store::set_budget(&db.pool, company, None, None, Some(true))
        .await
        .expect("budget");
    let denied = store::reserve(&db.pool, company, 10, None).await;
    assert!(
        matches!(denied, Err(StoreError::Denied(DenyReason::UnknownCost))),
        "policy binds without a cap: {:?}",
        denied.err()
    );
    // A priced call is unaffected by the unknown-cost policy.
    store::reserve(&db.pool, company, 10, Some(1_000))
        .await
        .expect("priced call admitted");
}

#[tokio::test(flavor = "multi_thread")]
async fn observed_state_stays_separate_from_the_conservative_charge() {
    let db = TestDb::new().await;
    let company = setup_company(&db).await;
    let reservation = store::reserve(&db.pool, company, 50, Some(300))
        .await
        .expect("reserve");
    // The provider reported nothing: the budget charge is the conservative
    // estimate while the observed columns keep saying nothing was observed.
    store::settle(
        &db.pool,
        company,
        reservation,
        "test",
        "test-model",
        None,
        Cost::Unknown,
    )
    .await
    .expect("settle");
    let budget = store::get_budget(&db.pool, company)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(budget.settled_tokens, 50, "conservative charge");
    assert_eq!(budget.observed_tokens, 0, "nothing was observed");
    assert_eq!(budget.unknown_usage_calls, 1);
    assert_eq!(budget.settled_cost_micros, 300, "cost estimate charged, not zero");
    assert_eq!(budget.unknown_cost_calls, 1, "cost still recorded as unknown");

    // A later call with real numbers moves both sides.
    let reservation = store::reserve(&db.pool, company, 50, Some(300))
        .await
        .expect("reserve");
    store::settle(
        &db.pool,
        company,
        reservation,
        "test",
        "test-model",
        Some(Usage::of(20, 10)),
        Cost::Known { micros: 150 },
    )
    .await
    .expect("settle");
    let budget = store::get_budget(&db.pool, company)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(budget.settled_tokens, 80, "50 estimate + 30 observed");
    assert_eq!(budget.observed_tokens, 30, "only the observed half");
    assert_eq!(budget.unknown_usage_calls, 1, "second call was observed");
    assert_eq!(budget.settled_cost_micros, 450, "300 estimate + 150 observed");
}

#[tokio::test(flavor = "multi_thread")]
async fn cost_budget_denies_when_exhausted() {
    let db = TestDb::new().await;
    let company = setup_company(&db).await;
    store::set_budget(&db.pool, company, None, Some(5_000_000), Some(false))
        .await
        .expect("budget");
    let reservation = store::reserve(&db.pool, company, 10, None)
        .await
        .expect("reserve");
    store::settle(
        &db.pool,
        company,
        reservation,
        "test",
        "test-model",
        Some(Usage::of(5, 5)),
        Cost::Known { micros: 5_000_000 },
    )
    .await
    .expect("settle");
    let denied = store::reserve(&db.pool, company, 1, None).await;
    assert!(
        matches!(denied, Err(StoreError::Denied(DenyReason::Cost { .. }))),
        "exhausted cost budget denies: {:?}",
        denied.err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn deny_unknown_cost_policy_applies_before_dispatch() {
    let db = TestDb::new().await;
    let company = setup_company(&db).await;
    store::set_budget(&db.pool, company, None, Some(1_000_000), Some(true))
        .await
        .expect("budget");
    let denied = store::reserve(&db.pool, company, 1, None).await;
    assert!(
        matches!(denied, Err(StoreError::Denied(DenyReason::UnknownCost))),
        "policy denies before the call: {:?}",
        denied.err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn release_returns_capacity() {
    let db = TestDb::new().await;
    let company = setup_company(&db).await;
    store::set_budget(&db.pool, company, Some(100), None, None)
        .await
        .expect("budget");
    let reservation = store::reserve(&db.pool, company, 100, None)
        .await
        .expect("reserve");
    store::release(&db.pool, company, reservation)
        .await
        .expect("release");
    let budget = store::get_budget(&db.pool, company)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(budget.reserved_tokens, 0);
    store::reserve(&db.pool, company, 100, None)
        .await
        .expect("capacity returned");
}

#[tokio::test(flavor = "multi_thread")]
async fn settled_reservation_cannot_be_settled_or_released_twice() {
    let db = TestDb::new().await;
    let company = setup_company(&db).await;
    let reservation = store::reserve(&db.pool, company, 10, None)
        .await
        .expect("reserve");
    store::settle(
        &db.pool,
        company,
        reservation,
        "test",
        "test-model",
        Some(Usage::of(4, 6)),
        Cost::Unknown,
    )
    .await
    .expect("settle");
    // The same reservation id cannot be replayed.
    assert!(matches!(
        store::settle(
            &db.pool,
            company,
            reservation,
            "test",
            "test-model",
            Some(Usage::of(1, 1)),
            Cost::Unknown,
        )
        .await,
        Err(StoreError::ReservationClosed)
    ));
    assert!(matches!(
        store::release(&db.pool, company, reservation).await,
        Err(StoreError::ReservationClosed)
    ));
    let budget = store::get_budget(&db.pool, company)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(budget.settled_tokens, 10, "usage counted exactly once");
}

#[tokio::test(flavor = "multi_thread")]
async fn settle_and_release_refuse_another_companys_reservation() {
    let db = TestDb::new().await;
    let company_a = setup_company(&db).await;
    let company_b = setup_company(&db).await;
    let reservation = store::reserve(&db.pool, company_a, 10, None)
        .await
        .expect("reserve");

    // Company B may learn the reservation id but can do nothing with it.
    assert!(matches!(
        store::settle(
            &db.pool,
            company_b,
            reservation,
            "test",
            "test-model",
            Some(Usage::of(1, 1)),
            Cost::Unknown,
        )
        .await,
        Err(StoreError::ReservationClosed)
    ));
    assert!(matches!(
        store::release(&db.pool, company_b, reservation).await,
        Err(StoreError::ReservationClosed)
    ));

    // The rightful owner still settles it exactly once.
    store::settle(
        &db.pool,
        company_a,
        reservation,
        "test",
        "test-model",
        Some(Usage::of(4, 6)),
        Cost::Unknown,
    )
    .await
    .expect("owner settle");
    let budget = store::get_budget(&db.pool, company_a)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(budget.settled_tokens, 10);
}

#[tokio::test(flavor = "multi_thread")]
async fn store_works_under_the_application_role_with_tenant_pin() {
    let db = TestDb::new().await;
    let company_a = setup_company(&db).await;
    let company_b = setup_company(&db).await;
    store::set_budget(&db.pool, company_b, Some(7), None, None)
        .await
        .expect("budget b");

    // Production connects as businex_app and the model tables are FORCE ROW
    // LEVEL SECURITY. The old store ran bare transactions with no tenant
    // pin: every statement saw no rows and failed. The pin each store
    // transaction sets is what makes the accounting usable at all here.
    let app = db
        .role_pool(businex_db::APP_ROLE, businex_db::TEST_APP_PASSWORD)
        .await;
    store::set_budget(&app, company_a, Some(1000), None, None)
        .await
        .expect("budget via app role");
    let reservation = store::reserve(&app, company_a, 400, None)
        .await
        .expect("reserve via app role");
    store::settle(
        &app,
        company_a,
        reservation,
        "test",
        "test-model",
        Some(Usage::of(300, 100)),
        Cost::Unknown,
    )
    .await
    .expect("settle via app role");
    let budget = store::get_budget(&app, company_a)
        .await
        .expect("get via app role")
        .expect("row");
    assert_eq!(budget.settled_tokens, 400);

    // A pinned transaction sees exactly one tenant's rows, never both.
    let mut tx = app.begin().await.expect("tx");
    sqlx::query("SELECT set_config('businex.company_id', $1, true)")
        .bind(company_a.to_string())
        .execute(&mut *tx)
        .await
        .expect("pin");
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM model_budgets")
        .fetch_one(&mut *tx)
        .await
        .expect("count");
    assert_eq!(count.0, 1, "pinned transaction sees exactly one budget row");
    tx.rollback().await.expect("rollback");

    // And with no pin at all the same connection sees nothing.
    let bare: (i64,) = sqlx::query_as("SELECT count(*) FROM model_budgets")
        .fetch_one(&app)
        .await
        .expect("bare count");
    assert_eq!(bare.0, 0, "unpinned app-role query leaks nothing");
}
