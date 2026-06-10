use std::sync::Arc;

use axum::extract::State;
use axum::Json;

use crate::convert;
use crate::dto::LsmDto;
use crate::error::ApiError;
use crate::state::AppState;

pub async fn lsm(State(state): State<Arc<AppState>>) -> Result<Json<LsmDto>, ApiError> {
    let manifest = state.latest_manifest().await?;
    let Some(m) = manifest.as_ref() else {
        return Err(ApiError::NotFound(format!(
            "no manifest found at '{}'",
            state.db_path
        )));
    };
    let dto = convert::manifest_dto(m);
    Ok(Json(LsmDto {
        manifest_id: dto.id,
        tree: dto.tree,
        segments: dto.segments,
        segment_extractor_name: dto.segment_extractor_name,
    }))
}
