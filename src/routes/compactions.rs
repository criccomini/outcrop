use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use ulid::Ulid;

use crate::convert;
use crate::dto::{CompactionDto, CompactorStateDto, VersionedCompactionsDto};
use crate::error::ApiError;
use crate::state::AppState;

pub async fn state(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CompactorStateDto>, ApiError> {
    let dto = state.compactor_state_dto().await?;
    Ok(Json((*dto).clone()))
}

#[derive(Deserialize)]
pub struct ListParams {
    start: Option<u64>,
    end: Option<u64>,
    limit: Option<u64>,
}

/// History of `.compactions` file versions, newest first. Each version is
/// one GET, so the default range is anchored to the latest id.
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<VersionedCompactionsDto>>, ApiError> {
    let limit = params.limit.unwrap_or(20).min(200).max(1);
    let range = match (params.start, params.end) {
        // Anchor to `end` so the newest `limit` versions of the requested
        // range are returned; each version is one object-store GET, so an
        // explicit range must not bypass the cap.
        (Some(s), Some(e)) => e.saturating_sub(limit - 1).max(s)..=e,
        (Some(s), None) => s..=s.saturating_add(limit - 1),
        (None, end) => {
            let end = match end {
                Some(e) => e,
                None => match state.admin.read_compactions(None).await? {
                    Some(latest) => latest.id(),
                    None => return Ok(Json(vec![])),
                },
            };
            end.saturating_sub(limit - 1)..=end
        }
    };
    let list = state.admin.list_compactions(range).await?;
    let mut out: Vec<VersionedCompactionsDto> = list
        .iter()
        .map(convert::versioned_compactions_dto)
        .collect();
    out.reverse();
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct GetParams {
    version: Option<u64>,
}

pub async fn get_one(
    State(state): State<Arc<AppState>>,
    Path(ulid_str): Path<String>,
    Query(params): Query<GetParams>,
) -> Result<Json<CompactionDto>, ApiError> {
    let ulid = Ulid::from_string(&ulid_str)
        .map_err(|_| ApiError::BadRequest(format!("invalid compaction ULID '{ulid_str}'")))?;
    match state.admin.read_compaction(ulid, params.version).await? {
        Some(c) => Ok(Json(convert::compaction_dto(&c))),
        None => Err(ApiError::NotFound(format!("compaction {ulid} not found"))),
    }
}
