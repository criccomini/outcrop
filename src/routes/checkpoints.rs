use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;

use crate::convert;
use crate::dto::{CheckpointStatusDto, ExternalDbDto};
use crate::error::ApiError;
use crate::state::AppState;

#[utoipa::path(get, path = "/api/dbs/{db}/checkpoints", tag = "checkpoints", params(crate::routes::DbPathParam), responses(
    (status = 200, description = "Checkpoints in the latest manifest with target availability", body = Vec<CheckpointStatusDto>),
    (status = 404, description = "Unknown database or missing resource", body = crate::dto::ErrorDto),
))]
pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<CheckpointStatusDto>>, ApiError> {
    let manifest = state.latest_manifest().await?;
    let Some(m) = manifest.as_ref() else {
        return Err(ApiError::NotFound(format!(
            "no manifest found at '{}'",
            state.db_path
        )));
    };
    let entries = state.manifest_entries().await?;
    let live_ids: HashSet<u64> = entries.iter().map(|e| e.id).collect();
    let out = m
        .checkpoints()
        .iter()
        .map(|c| CheckpointStatusDto {
            checkpoint: convert::checkpoint_dto(c),
            manifest_available: live_ids.contains(&c.manifest_id),
        })
        .collect();
    Ok(Json(out))
}

#[utoipa::path(get, path = "/api/dbs/{db}/clones", tag = "checkpoints", params(crate::routes::DbPathParam), responses(
    (status = 200, description = "External DBs cloned from this one", body = Vec<ExternalDbDto>),
    (status = 404, description = "Unknown database or missing resource", body = crate::dto::ErrorDto),
))]
pub async fn clones(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ExternalDbDto>>, ApiError> {
    let manifest = state.latest_manifest().await?;
    let Some(m) = manifest.as_ref() else {
        return Err(ApiError::NotFound(format!(
            "no manifest found at '{}'",
            state.db_path
        )));
    };
    Ok(Json(convert::manifest_dto(m).external_dbs))
}
