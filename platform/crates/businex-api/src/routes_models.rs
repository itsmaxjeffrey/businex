//! Company model keys, model budgets and the budget-aware app generator.
//!
//! Key material is sealed with AES-256-GCM before it touches the database and
//! unsealed only for the duration of one model call; API responses carry key
//! metadata only. Generation reserves budget capacity before the call
//! dispatches, settles observed usage afterwards and releases the reservation
//! when the call fails, so spend is accounted exactly once per attempt.

use crate::builder::{
    estimate_tokens, extract_manifest_json, GenerateRequest, PROVIDERS,
};
use crate::routes_apps::{db_err, insert_app_with_manifest, tx_err};
use crate::routes_identity::{audit, authorize, current_user};
use crate::{error_response, AppState};
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use businex_core::app_manifest::AppManifest;
use businex_core::{Error, Permission};
use businex_models::keys::SealedKey;
use businex_models::store::{self, DenyReason, StoreError};
use businex_models::ModelError;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/companies/{id}/model-keys",
            get(list_keys).post(create_key),
        )
        .route("/api/companies/{id}/model-keys/{key}", delete(revoke_key))
        .route(
            "/api/companies/{id}/model-budget",
            get(get_budget).put(set_budget),
        )
        .route("/api/companies/{id}/apps/generate", post(generate_app))
}

fn unavailable(message: &str) -> Response {
    error_response(&Error::Unavailable {
        message: message.into(),
    })
}

fn store_err(e: StoreError) -> Response {
    match e {
        StoreError::Denied(reason) => error_response(&Error::Forbidden {
            action: "model.generate".into(),
            reason: match reason {
                DenyReason::Tokens { .. } => "token budget exhausted".into(),
                DenyReason::Cost { .. } => "cost budget exhausted".into(),
                DenyReason::UnknownCost => "policy forbids calls with unknown cost".into(),
            },
        }),
        other => {
            tracing::error!(error = ?other, "model accounting error");
            error_response(&Error::Internal("database error".into()))
        }
    }
}

fn model_err(e: ModelError) -> Response {
    match e {
        ModelError::Config(message) => {
            tracing::error!(message = %message, "model configuration error");
            error_response(&Error::Internal("model configuration error".into()))
        }
        other => {
            tracing::warn!(error = %other, "model generation failed");
            error_response(&Error::Upstream {
                message: "the model call failed".into(),
            })
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewKey {
    provider: String,
    label: String,
    key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetBody {
    #[serde(default)]
    max_tokens_per_period: Option<i64>,
    #[serde(default)]
    max_cost_micros: Option<i64>,
    #[serde(default)]
    deny_when_cost_unknown: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GenerateBody {
    description: String,
    provider: String,
    model: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
}

/// Register a provider key for this company. The plaintext appears once in
/// the request and never again.
async fn create_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(company_id): Path<Uuid>,
    Json(body): Json<NewKey>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(
        &state,
        &user,
        company_id,
        Permission::ModelKeysManage,
        "*",
    )
    .await?;
    let master = state
        .master_key
        .as_ref()
        .ok_or_else(|| unavailable("model key storage is not configured"))?;
    if !PROVIDERS.contains(&body.provider.as_str()) {
        return Err(error_response(&Error::Invalid {
            message: "unknown provider".into(),
        }));
    }
    if body.label.is_empty() || body.label.len() > 64 {
        return Err(error_response(&Error::Invalid {
            message: "label must be 1-64 characters".into(),
        }));
    }
    if body.key.is_empty() || body.key.len() > 512 {
        return Err(error_response(&Error::Invalid {
            message: "key must be 1-512 characters".into(),
        }));
    }
    let sealed = master
        .seal(
            company_id.to_string(),
            body.provider.clone(),
            body.key.clone(),
        )
        .map_err(|_| error_response(&Error::Internal("key sealing failed".into())))?;

    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let key_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO model_keys (id, company_id, provider, label, sealed_nonce, sealed_bytes, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(key_id)
    .bind(company_id)
    .bind(body.provider.as_str())
    .bind(body.label.as_str())
    .bind(sealed.nonce.as_slice())
    .bind(sealed.ciphertext.as_slice())
    .bind(user.id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if matches!(&e, sqlx::Error::Database(db) if db.constraint().is_some()) {
            error_response(&Error::Conflict {
                message: "a key with this provider and label already exists".into(),
            })
        } else {
            db_err(e)
        }
    })?;
    tx.commit().await.map_err(tx_err)?;

    audit(
        &state,
        &auth,
        "model_key.create",
        "model_key",
        Some(key_id),
        json!({"provider": body.provider, "label": body.label}),
    )
    .await;
    Ok(Json(json!({"id": key_id, "provider": body.provider, "label": body.label})).into_response())
}

/// Key metadata only: never sealed bytes, never plaintext.
async fn list_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(company_id): Path<Uuid>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    authorize(&state, &user, company_id, Permission::ModelKeysManage, "*").await?;
    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let rows: Vec<(Uuid, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, provider, label, created_at::text, revoked_at::text
         FROM model_keys WHERE company_id = $1 ORDER BY created_at DESC",
    )
    .bind(company_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(tx_err)?;
    let keys: Vec<_> = rows
        .into_iter()
        .map(|(id, provider, label, created_at, revoked_at)| {
            json!({"id": id, "provider": provider, "label": label,
                   "created_at": created_at, "revoked_at": revoked_at})
        })
        .collect();
    Ok(Json(json!({"keys": keys})).into_response())
}

/// Revoke a key. Generation can no longer use it; the row stays for audit.
async fn revoke_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((company_id, key_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(
        &state,
        &user,
        company_id,
        Permission::ModelKeysManage,
        "*",
    )
    .await?;
    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let result = sqlx::query(
        "UPDATE model_keys SET revoked_at = now()
         WHERE id = $1 AND company_id = $2 AND revoked_at IS NULL",
    )
    .bind(key_id)
    .bind(company_id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(tx_err)?;
    if result.rows_affected() == 0 {
        return Err(error_response(&Error::NotFound {
            entity: "model key".into(),
        }));
    }
    audit(
        &state,
        &auth,
        "model_key.revoke",
        "model_key",
        Some(key_id),
        json!({}),
    )
    .await;
    Ok(Json(json!({"id": key_id, "revoked": true})).into_response())
}

async fn get_budget(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(company_id): Path<Uuid>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    authorize(
        &state,
        &user,
        company_id,
        Permission::SettingsManage,
        "*",
    )
    .await?;
    let budget = store::get_budget(&state.pool, company_id)
        .await
        .map_err(store_err)?;
    Ok(Json(json!({"budget": budget})).into_response())
}

async fn set_budget(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(company_id): Path<Uuid>,
    Json(body): Json<BudgetBody>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(
        &state,
        &user,
        company_id,
        Permission::SettingsManage,
        "*",
    )
    .await?;
    if let Some(max) = body.max_tokens_per_period {
        if max <= 0 {
            return Err(error_response(&Error::Invalid {
                message: "max_tokens_per_period must be positive".into(),
            }));
        }
    }
    if let Some(max) = body.max_cost_micros {
        if max < 0 {
            return Err(error_response(&Error::Invalid {
                message: "max_cost_micros must not be negative".into(),
            }));
        }
    }
    let budget = store::set_budget(
        &state.pool,
        company_id,
        body.max_tokens_per_period,
        body.max_cost_micros,
        body.deny_when_cost_unknown,
    )
    .await
    .map_err(store_err)?;
    audit(
        &state,
        &auth,
        "model_budget.set",
        "company",
        Some(company_id),
        json!({
            "max_tokens_per_period": body.max_tokens_per_period,
            "max_cost_micros": body.max_cost_micros,
            "deny_when_cost_unknown": body.deny_when_cost_unknown,
        }),
    )
    .await;
    Ok(Json(json!({"budget": budget})).into_response())
}

fn validate_generate(body: &GenerateBody) -> Result<(), Response> {
    let desc_len = body.description.trim().chars().count();
    if desc_len == 0 || desc_len > 8000 {
        return Err(error_response(&Error::Invalid {
            message: "description must be 1-8000 characters".into(),
        }));
    }
    if !PROVIDERS.contains(&body.provider.as_str()) {
        return Err(error_response(&Error::Invalid {
            message: "unknown provider".into(),
        }));
    }
    if body.model.trim().is_empty() || body.model.len() > 120 {
        return Err(error_response(&Error::Invalid {
            message: "model must be 1-120 characters".into(),
        }));
    }
    if let Some(label) = &body.label {
        if label.is_empty() || label.len() > 64 {
            return Err(error_response(&Error::Invalid {
                message: "label must be 1-64 characters".into(),
            }));
        }
    }
    if body.base_url.as_deref().unwrap_or("").len() > 512 {
        return Err(error_response(&Error::Invalid {
            message: "base_url is too long".into(),
        }));
    }
    if matches!(body.provider.as_str(), "openai-compatible" | "xiaomi")
        && body.base_url.is_none()
    {
        return Err(error_response(&Error::Invalid {
            message: "base_url is required for this provider".into(),
        }));
    }
    Ok(())
}

/// Budget-aware generation: describe an app, get back a validated manifest
/// and a created app at version 1. The reservation wraps the model call and
/// nothing else.
async fn generate_app(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(company_id): Path<Uuid>,
    Json(body): Json<GenerateBody>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(&state, &user, company_id, Permission::AppsManage, "*").await?;
    validate_generate(&body)?;
    let master = state
        .master_key
        .as_ref()
        .ok_or_else(|| unavailable("model key storage is not configured"))?;

    // Find an active key for the provider under the tenant pin.
    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let row: Option<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT sealed_nonce, sealed_bytes FROM model_keys
         WHERE company_id = $1 AND provider = $2 AND revoked_at IS NULL
           AND ($3::text IS NULL OR label = $3)
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(body.provider.as_str())
    .bind(body.label.as_deref())
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(tx_err)?;
    let (nonce, ciphertext) = row.ok_or_else(|| {
        error_response(&Error::NotFound {
            entity: "model key".into(),
        })
    })?;
    let api_key = master
        .open(
            company_id.to_string(),
            body.provider.clone(),
            &SealedKey { nonce, ciphertext },
        )
        .map_err(|_| error_response(&Error::Internal("model key cannot be opened".into())))?;

    // Reserve capacity before dispatch. A denial means the model is never
    // called.
    let reservation = store::reserve(&state.pool, company_id, estimate_tokens(&body.description))
        .await
        .map_err(store_err)?;
    let outcome = match state
        .generator
        .generate(GenerateRequest {
            provider: body.provider.clone(),
            model: body.model.clone(),
            api_key,
            base_url: body.base_url.clone(),
            description: body.description.clone(),
        })
        .await
    {
        Ok(outcome) => outcome,
        Err(e) => {
            if let Err(release_err) = store::release(&state.pool, company_id, reservation).await {
                tracing::error!(error = ?release_err, "model reservation release failed");
            }
            return Err(model_err(e));
        }
    };

    // The call happened: settle with observed usage and cost, even when the
    // output turns out to be unusable. Unknown cost stays unknown.
    store::settle(
        &state.pool,
        company_id,
        reservation,
        body.provider.as_str(),
        body.model.as_str(),
        outcome.usage,
        outcome.cost,
    )
    .await
    .map_err(store_err)?;

    let manifest = match serde_json::from_str::<AppManifest>(extract_manifest_json(&outcome.text)) {
        Ok(manifest) if manifest.validate().is_ok() => manifest,
        _ => {
            return Err(error_response(&Error::Upstream {
                message: "the model did not return a valid app manifest".into(),
            }));
        }
    };

    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let app_id = insert_app_with_manifest(&mut tx, company_id, user.id, &manifest).await?;
    tx.commit().await.map_err(tx_err)?;

    audit(
        &state,
        &auth,
        "app.generate",
        "app",
        Some(app_id),
        json!({"provider": body.provider, "model": body.model,
               "slug": manifest.slug, "usage_known": outcome.usage.is_some()}),
    )
    .await;
    Ok(Json(json!({
        "app": {"id": app_id, "slug": manifest.slug, "version": 1},
        "manifest": manifest,
        "usage": outcome.usage,
        "cost": outcome.cost,
    }))
    .into_response())
}
