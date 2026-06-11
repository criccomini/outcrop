use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::dto::{WalDto, WalSstDto};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct WalParams {
    limit: Option<usize>,
}

#[utoipa::path(get, path = "/api/dbs/{db}/wal", tag = "wal", params(crate::routes::DbPathParam, WalParams), responses(
    (status = 200, description = "WAL SST listing and replay window", body = WalDto),
    (status = 404, description = "Unknown database or missing resource", body = crate::dto::ErrorDto),
))]
pub async fn wal(
    State(state): State<Arc<AppState>>,
    Query(params): Query<WalParams>,
) -> Result<Json<WalDto>, ApiError> {
    // The listing is already cached, so the limit only bounds the payload —
    // a GC-less DB can accumulate WAL SSTs without bound.
    let limit = params.limit.unwrap_or(200).min(2000).max(1);
    let manifest = state.latest_manifest().await?;
    let Some(m) = manifest.as_ref() else {
        return Err(ApiError::NotFound(format!(
            "no manifest found at '{}'",
            state.db_path
        )));
    };

    let entries = state.wal_entries().await?;
    let total_bytes = entries.iter().map(|e| e.size_bytes).sum();
    let ssts: Vec<WalSstDto> = entries
        .iter()
        .rev()
        .take(limit)
        .map(|e| WalSstDto {
            id: e.id,
            size_bytes: e.size_bytes,
            last_modified: e.last_modified,
        })
        .collect();

    Ok(Json(WalDto {
        next_wal_sst_id: m.next_wal_sst_id(),
        replay_after_wal_id: m.replay_after_wal_id(),
        total_bytes,
        total_count: entries.len(),
        wal_object_store_uri: m.wal_object_store_uri().map(|s| s.to_string()),
        entries: ssts,
    }))
}
