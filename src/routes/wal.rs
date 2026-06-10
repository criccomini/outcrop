use std::sync::Arc;

use axum::extract::State;
use axum::Json;

use crate::dto::{WalDto, WalSstDto};
use crate::error::ApiError;
use crate::state::AppState;

pub async fn wal(State(state): State<Arc<AppState>>) -> Result<Json<WalDto>, ApiError> {
    let manifest = state.latest_manifest().await?;
    let Some(m) = manifest.as_ref() else {
        return Err(ApiError::NotFound(format!(
            "no manifest found at '{}'",
            state.db_path
        )));
    };

    let entries = state.wal_entries().await?;
    let total_bytes = entries.iter().map(|e| e.size_bytes).sum();
    let mut ssts: Vec<WalSstDto> = entries
        .iter()
        .map(|e| WalSstDto {
            id: e.id,
            size_bytes: e.size_bytes,
            last_modified: e.last_modified,
        })
        .collect();
    ssts.reverse();

    Ok(Json(WalDto {
        next_wal_sst_id: m.next_wal_sst_id(),
        replay_after_wal_id: m.replay_after_wal_id(),
        total_bytes,
        wal_object_store_uri: m.wal_object_store_uri().map(|s| s.to_string()),
        entries: ssts,
    }))
}
