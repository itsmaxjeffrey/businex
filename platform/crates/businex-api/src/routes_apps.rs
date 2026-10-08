//! App platform: versioned manifests, per-company installs and the
//! schema-driven record store for generated apps.
//!
//! Every handler runs inside the company transaction, so RLS confines the
//! data to one tenant before any application check, and every mutation is
//! audited. Manifests are immutable once published (DB trigger enforces it);
//! upgrade and rollback simply point the install at a different immutable
//! version and record the transition. Uninstall keeps records in place.

use crate::routes_identity::{audit, authorize, current_user};
use crate::{error_response, AppState};
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use businex_core::app_manifest::{validate_record, AppManifest};
use businex_core::{Error, Permission};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/companies/{id}/apps", get(list_apps).post(create_app))
        .route(
            "/api/companies/{id}/apps/{app}/versions",
            post(publish_version),
        )
        .route(
            "/api/companies/{id}/apps/{app}/install",
            post(install).delete(uninstall),
        )
        .route(
            "/api/companies/{id}/apps/{app}/history",
            get(install_history),
        )
        .route(
            "/api/companies/{id}/apps/{app}/records/{entity}",
            get(list_records).post(create_record),
        )
        .route(
            "/api/companies/{id}/apps/{app}/records/{entity}/{record}",
            get(get_record).patch(update_record).delete(delete_record),
        )
}

fn db_err(e: sqlx::Error) -> Response {
    // Logged server-side for diagnosis; the client response stays a fixed
    // sanitized string.
    tracing::error!(error = ?e, "app platform database error");
    match &e {
        sqlx::Error::Database(db) if db.constraint().is_some() => {
            error_response(&Error::Conflict {
                message: "app slug or version already exists".into(),
            })
        }
        _ => error_response(&Error::Internal("database error".into())),
    }
}

fn tx_err(e: sqlx::Error) -> Response {
    tracing::error!(error = ?e, "app platform transaction error");
    error_response(&Error::Internal("database error".into()))
}

#[derive(Deserialize)]
struct NewApp {
    manifest: AppManifest,
}

#[derive(Deserialize)]
struct InstallBody {
    version: i32,
}

fn manifest_hash(manifest: &AppManifest) -> Result<String, Response> {
    let canonical = serde_json::to_string(manifest)
        .map_err(|_| error_response(&Error::Internal("manifest serialization failed".into())))?;
    Ok(hex::encode(Sha256::digest(canonical.as_bytes())))
}

/// Create a new app in the company library together with its first manifest
/// version. The version row is immutable from here on.
async fn create_app(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(company_id): Path<Uuid>,
    Json(input): Json<NewApp>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(&state, &user, company_id, Permission::AppsManage, "*").await?;
    let manifest = input.manifest;
    manifest.validate().map_err(|e| error_response(&e))?;
    let bundle_hash = manifest_hash(&manifest)?;
    let manifest_json = serde_json::to_value(&manifest)
        .map_err(|_| error_response(&Error::Internal("manifest serialization failed".into())))?;

    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let app_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO apps (id, company_id, slug, name, kind, created_by)\n         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(app_id)
    .bind(company_id)
    .bind(manifest.slug.as_str())
    .bind(manifest.name.as_str())
    .bind(manifest.kind.as_str())
    .bind(user.id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;
    sqlx::query(
        "INSERT INTO app_versions (id, app_id, company_id, version, manifest, bundle_hash, created_by)\n         VALUES ($1, $2, $3, 1, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind(app_id)
    .bind(company_id)
    .bind(manifest_json)
    .bind(bundle_hash)
    .bind(user.id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(tx_err)?;

    audit(
        &state,
        &auth,
        "app.create",
        "app",
        Some(app_id),
        json!({"slug": manifest.slug, "kind": manifest.kind.as_str(), "version": 1}),
    )
    .await;
    Ok(Json(json!({"id": app_id, "slug": manifest.slug, "version": 1})).into_response())
}

/// Publish the next immutable manifest version for an existing app.
async fn publish_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((company_id, app_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<NewApp>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(&state, &user, company_id, Permission::AppsManage, "*").await?;
    let manifest = input.manifest;
    manifest.validate().map_err(|e| error_response(&e))?;
    let bundle_hash = manifest_hash(&manifest)?;
    let manifest_json = serde_json::to_value(&manifest)
        .map_err(|_| error_response(&Error::Internal("manifest serialization failed".into())))?;

    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let slug: Option<(String,)> =
        sqlx::query_as("SELECT slug FROM apps WHERE id = $1 AND company_id = $2")
            .bind(app_id)
            .bind(company_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err)?;
    let (slug,) = slug.ok_or_else(|| {
        error_response(&Error::NotFound {
            entity: "app".into(),
        })
    })?;
    if slug != manifest.slug {
        return Err(error_response(&Error::Invalid {
            message: "manifest slug does not match the app".into(),
        }));
    }
    let version: i32 = sqlx::query_as::<_, (i32,)>(
        "SELECT coalesce(max(version), 0) + 1 FROM app_versions WHERE app_id = $1",
    )
    .bind(app_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_err)?
    .0;
    sqlx::query(
        "INSERT INTO app_versions (id, app_id, company_id, version, manifest, bundle_hash, created_by)\n         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(app_id)
    .bind(company_id)
    .bind(version)
    .bind(manifest_json)
    .bind(bundle_hash)
    .bind(user.id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(tx_err)?;

    audit(
        &state,
        &auth,
        "app.version.publish",
        "app",
        Some(app_id),
        json!({"version": version}),
    )
    .await;
    Ok(Json(json!({"id": app_id, "version": version})).into_response())
}

/// Point the company install at a manifest version. A first activation is an
/// install, a move to a higher version is an upgrade, a move to a lower one a
/// rollback; the transition is recorded in the history table.
async fn install(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((company_id, app_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<InstallBody>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(&state, &user, company_id, Permission::AppsInstall, "*").await?;

    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let target: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM app_versions WHERE app_id = $1 AND version = $2")
            .bind(app_id)
            .bind(input.version)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err)?;
    let (target_id,) = target.ok_or_else(|| {
        error_response(&Error::NotFound {
            entity: "app version".into(),
        })
    })?;
    let current: Option<(Uuid, i32, String)> = sqlx::query_as(
        "SELECT i.id, v.version, i.status FROM app_installs i\n         JOIN app_versions v ON v.id = i.version_id\n         WHERE i.company_id = $1 AND i.app_id = $2",
    )
    .bind(company_id)
    .bind(app_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_err)?;

    let (install_id, action, from_version) = match &current {
        None => (Uuid::new_v4(), "install", None),
        Some((_, version, status)) if status == "uninstalled" => {
            (Uuid::new_v4(), "install", Some(*version))
        }
        Some((_, version, _)) if input.version > *version => {
            (Uuid::new_v4(), "upgrade", Some(*version))
        }
        Some((_, version, _)) if input.version < *version => {
            (Uuid::new_v4(), "rollback", Some(*version))
        }
        Some(_) => {
            return Err(error_response(&Error::Conflict {
                message: "that version is already installed".into(),
            }));
        }
    };

    match &current {
        Some((row_id, _, _)) => {
            sqlx::query(
                "UPDATE app_installs SET version_id = $1, status = 'active', updated_at = now()\n                 WHERE id = $2",
            )
            .bind(target_id)
            .bind(row_id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }
        None => {
            sqlx::query(
                "INSERT INTO app_installs (id, company_id, app_id, version_id, status, installed_by)\n                 VALUES ($1, $2, $3, $4, 'active', $5)",
            )
            .bind(install_id)
            .bind(company_id)
            .bind(app_id)
            .bind(target_id)
            .bind(user.id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }
    }
    sqlx::query(
        "INSERT INTO app_install_history (id, company_id, install_id, action, from_version, to_version, actor_id)\n         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind(current.as_ref().map(|(id, _, _)| *id).unwrap_or(install_id))
    .bind(action)
    .bind(from_version)
    .bind(input.version)
    .bind(user.id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(tx_err)?;

    audit(
        &state,
        &auth,
        "app.install",
        "app",
        Some(app_id),
        json!({"action": action, "version": input.version}),
    )
    .await;
    Ok(Json(json!({"action": action, "version": input.version})).into_response())
}

/// Uninstall stops the app but preserves every record it owns.
async fn uninstall(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((company_id, app_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(&state, &user, company_id, Permission::AppsInstall, "*").await?;

    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let current: Option<(Uuid, i32, String)> = sqlx::query_as(
        "SELECT i.id, v.version, i.status FROM app_installs i\n         JOIN app_versions v ON v.id = i.version_id\n         WHERE i.company_id = $1 AND i.app_id = $2",
    )
    .bind(company_id)
    .bind(app_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_err)?;
    let (install_id, version, status) = current.ok_or_else(|| {
        error_response(&Error::NotFound {
            entity: "install".into(),
        })
    })?;
    if status == "uninstalled" {
        return Err(error_response(&Error::Conflict {
            message: "app is not installed".into(),
        }));
    }
    sqlx::query("UPDATE app_installs SET status = 'uninstalled', updated_at = now() WHERE id = $1")
        .bind(install_id)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
    sqlx::query(
        "INSERT INTO app_install_history (id, company_id, install_id, action, from_version, to_version, actor_id)\n         VALUES ($1, $2, $3, 'uninstall', $4, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind(install_id)
    .bind(version)
    .bind(user.id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(tx_err)?;

    audit(
        &state,
        &auth,
        "app.uninstall",
        "app",
        Some(app_id),
        json!({"version": version}),
    )
    .await;
    Ok(Json(json!({"uninstalled": true, "version": version})).into_response())
}

/// Install history: who moved the install when, and between which versions.
async fn install_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((company_id, app_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    authorize(&state, &user, company_id, Permission::AppsUse, "*").await?;
    let ctx = businex_db::CompanyContext::new(company_id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let rows: Vec<(Uuid, String, Option<i32>, i32)> = sqlx::query_as(
        "SELECT h.install_id, h.action, h.from_version, h.to_version\n         FROM app_install_history h\n         JOIN app_installs i ON i.id = h.install_id\n         WHERE i.company_id = $1 AND i.app_id = $2\n         ORDER BY h.created_at",
    )
    .bind(company_id)
    .bind(app_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(db_err)?;
    let events = rows
        .into_iter()
        .map(|(install_id, action, from, to)| {
            json!({"install_id": install_id, "action": action, "from_version": from, "to_version": to})
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({"history": events})).into_response())
}

/// Apps in the company library with their current install state.
async fn list_apps(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(company_id): Path<Uuid>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    authorize(&state, &user, company_id, Permission::AppsUse, "*").await?;
    let ctx = businex_db::CompanyContext::new(company_id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let rows: Vec<(Uuid, String, String, String, Option<String>, Option<i32>)> = sqlx::query_as(
        "SELECT a.id, a.slug, a.name, a.kind, i.status, v.version\n         FROM apps a\n         LEFT JOIN app_installs i ON i.app_id = a.id AND i.company_id = a.company_id\n         LEFT JOIN app_versions v ON v.id = i.version_id\n         WHERE a.company_id = $1\n         ORDER BY a.slug",
    )
    .bind(company_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(db_err)?;
    let apps = rows
        .into_iter()
        .map(|(id, slug, name, kind, status, version)| {
            json!({"id": id, "slug": slug, "name": name, "kind": kind,
                   "install": status.map(|s| json!({"status": s, "version": version}))})
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({"apps": apps})).into_response())
}

/// The active install's manifest — the schema every record is validated
/// against. Uninstalled apps have no active manifest and no record access.
async fn active_manifest(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    company_id: Uuid,
    app_id: Uuid,
) -> Result<AppManifest, Response> {
    let row: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT v.manifest FROM app_installs i\n         JOIN app_versions v ON v.id = i.version_id\n         WHERE i.company_id = $1 AND i.app_id = $2 AND i.status <> 'uninstalled'",
    )
    .bind(company_id)
    .bind(app_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_err)?;
    let (value,) = row.ok_or_else(|| {
        error_response(&Error::Conflict {
            message: "app is not installed".into(),
        })
    })?;
    serde_json::from_value(value)
        .map_err(|_| error_response(&Error::Internal("stored manifest is invalid".into())))
}

fn entity_or_404<'a>(
    manifest: &'a AppManifest,
    entity: &str,
) -> Result<&'a businex_core::app_manifest::EntityDef, Response> {
    manifest.entity(entity).ok_or_else(|| {
        error_response(&Error::NotFound {
            entity: "entity".into(),
        })
    })
}

/// Enforce manifest uniqueness declarations before writing a record.
async fn ensure_unique(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    company_id: Uuid,
    app_id: Uuid,
    entity: &str,
    manifest_entity: &businex_core::app_manifest::EntityDef,
    data: &serde_json::Value,
    exclude: Uuid,
) -> Result<(), Response> {
    for field in manifest_entity.fields.iter().filter(|f| f.unique) {
        let value = match data.get(&field.name) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) if !other.is_null() => other.to_string(),
            _ => continue,
        };
        let clash: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM app_records\n             WHERE company_id = $1 AND app_id = $2 AND entity = $3\n               AND deleted_at IS NULL AND data->>$4 = $5 AND id <> $6",
        )
        .bind(company_id)
        .bind(app_id)
        .bind(entity)
        .bind(field.name.as_str())
        .bind(value)
        .bind(exclude)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_err)?;
        if clash.is_some() {
            return Err(error_response(&Error::Conflict {
                message: format!("{} must be unique", field.name),
            }));
        }
    }
    Ok(())
}

async fn list_records(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((company_id, app_id, entity)): Path<(Uuid, Uuid, String)>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    authorize(&state, &user, company_id, Permission::RecordsRead, "*").await?;
    let ctx = businex_db::CompanyContext::new(company_id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let manifest = active_manifest(&mut tx, company_id, app_id).await?;
    entity_or_404(&manifest, &entity)?;
    let rows: Vec<(Uuid, serde_json::Value)> = sqlx::query_as(
        "SELECT id, data FROM app_records\n         WHERE company_id = $1 AND app_id = $2 AND entity = $3 AND deleted_at IS NULL\n         ORDER BY created_at",
    )
    .bind(company_id)
    .bind(app_id)
    .bind(&entity)
    .fetch_all(&mut *tx)
    .await
    .map_err(db_err)?;
    let records = rows
        .into_iter()
        .map(|(id, data)| json!({"id": id, "data": data}))
        .collect::<Vec<_>>();
    Ok(Json(json!({"records": records})).into_response())
}

#[derive(Deserialize)]
struct RecordBody {
    data: serde_json::Value,
}

async fn create_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((company_id, app_id, entity)): Path<(Uuid, Uuid, String)>,
    Json(input): Json<RecordBody>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(&state, &user, company_id, Permission::RecordsWrite, "*").await?;
    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let manifest = active_manifest(&mut tx, company_id, app_id).await?;
    let schema = entity_or_404(&manifest, &entity)?;
    validate_record(schema, &input.data).map_err(|e| error_response(&e))?;
    ensure_unique(
        &mut tx,
        company_id,
        app_id,
        &entity,
        schema,
        &input.data,
        Uuid::nil(),
    )
    .await?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO app_records (id, company_id, app_id, entity, data, created_by)\n         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(company_id)
    .bind(app_id)
    .bind(&entity)
    .bind(&input.data)
    .bind(user.id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(tx_err)?;

    audit(
        &state,
        &auth,
        "record.create",
        "app_record",
        Some(id),
        json!({"app_id": app_id, "entity": entity}),
    )
    .await;
    Ok(Json(json!({"id": id, "data": input.data})).into_response())
}

async fn get_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((company_id, app_id, entity, record_id)): Path<(Uuid, Uuid, String, Uuid)>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    authorize(&state, &user, company_id, Permission::RecordsRead, "*").await?;
    let ctx = businex_db::CompanyContext::new(company_id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let manifest = active_manifest(&mut tx, company_id, app_id).await?;
    entity_or_404(&manifest, &entity)?;
    let row: Option<(Uuid, serde_json::Value)> = sqlx::query_as(
        "SELECT id, data FROM app_records\n         WHERE company_id = $1 AND app_id = $2 AND entity = $3 AND id = $4 AND deleted_at IS NULL",
    )
    .bind(company_id)
    .bind(app_id)
    .bind(&entity)
    .bind(record_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_err)?;
    let (id, data) = row.ok_or_else(|| {
        error_response(&Error::NotFound {
            entity: "record".into(),
        })
    })?;
    Ok(Json(json!({"id": id, "data": data})).into_response())
}

/// Replace the record document with a new one validated against the schema.
async fn update_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((company_id, app_id, entity, record_id)): Path<(Uuid, Uuid, String, Uuid)>,
    Json(input): Json<RecordBody>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(&state, &user, company_id, Permission::RecordsWrite, "*").await?;
    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let manifest = active_manifest(&mut tx, company_id, app_id).await?;
    let schema = entity_or_404(&manifest, &entity)?;
    validate_record(schema, &input.data).map_err(|e| error_response(&e))?;
    ensure_unique(
        &mut tx,
        company_id,
        app_id,
        &entity,
        schema,
        &input.data,
        record_id,
    )
    .await?;
    let updated = sqlx::query(
        "UPDATE app_records SET data = $1, updated_at = now()\n         WHERE company_id = $2 AND app_id = $3 AND entity = $4 AND id = $5 AND deleted_at IS NULL",
    )
    .bind(&input.data)
    .bind(company_id)
    .bind(app_id)
    .bind(&entity)
    .bind(record_id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;
    if updated.rows_affected() == 0 {
        return Err(error_response(&Error::NotFound {
            entity: "record".into(),
        }));
    }
    tx.commit().await.map_err(tx_err)?;

    audit(
        &state,
        &auth,
        "record.update",
        "app_record",
        Some(record_id),
        json!({"app_id": app_id, "entity": entity}),
    )
    .await;
    Ok(Json(json!({"id": record_id, "data": input.data})).into_response())
}

/// Soft delete: the row stays on disk with a deleted_at stamp.
async fn delete_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((company_id, app_id, entity, record_id)): Path<(Uuid, Uuid, String, Uuid)>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(&state, &user, company_id, Permission::RecordsDelete, "*").await?;
    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(tx_err)?;
    let updated = sqlx::query(
        "UPDATE app_records SET deleted_at = now()\n         WHERE company_id = $1 AND app_id = $2 AND entity = $3 AND id = $4 AND deleted_at IS NULL",
    )
    .bind(company_id)
    .bind(app_id)
    .bind(&entity)
    .bind(record_id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;
    if updated.rows_affected() == 0 {
        return Err(error_response(&Error::NotFound {
            entity: "record".into(),
        }));
    }
    tx.commit().await.map_err(tx_err)?;

    audit(
        &state,
        &auth,
        "record.delete",
        "app_record",
        Some(record_id),
        json!({"app_id": app_id, "entity": entity}),
    )
    .await;
    Ok(Json(json!({"deleted": true})).into_response())
}
