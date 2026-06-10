use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::convert;
use crate::dto::LsmDto;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LsmParams {
    /// Render the tree as of this manifest instead of the latest.
    manifest_id: Option<u64>,
}

pub async fn lsm(
    State(state): State<Arc<AppState>>,
    Query(params): Query<LsmParams>,
) -> Result<Json<LsmDto>, ApiError> {
    let dto = match params.manifest_id {
        Some(id) => {
            let m = state.manifest_by_id(id).await?.ok_or_else(|| {
                ApiError::NotFound(format!("manifest {id} not found (possibly GC'd)"))
            })?;
            convert::manifest_dto(&m)
        }
        None => {
            let manifest = state.latest_manifest().await?;
            let Some(m) = manifest.as_ref() else {
                return Err(ApiError::NotFound(format!(
                    "no manifest found at '{}'",
                    state.db_path
                )));
            };
            convert::manifest_dto(m)
        }
    };
    Ok(Json(LsmDto {
        manifest_id: dto.id,
        tree: dto.tree,
        segments: dto.segments,
        segment_extractor_name: dto.segment_extractor_name,
    }))
}
