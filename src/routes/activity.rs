use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::convert;
use crate::diff::{diff_manifests, summarize_diff};
use crate::dto::{ActivityDto, ManifestDto};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListParams {
    limit: Option<usize>,
}

/// Recent manifest transitions, newest first: each item diffs a manifest
/// against its predecessor. Transitions are immutable, so each (a, b) pair
/// is computed once and served from the LRU after that — a steady-state
/// poll only pays for pairs that appeared since the last one.
#[utoipa::path(get, path = "/api/dbs/{db}/activity", tag = "activity", params(crate::routes::DbPathParam, ListParams), responses(
    (status = 200, description = "Recent manifest transitions, newest first", body = Vec<ActivityDto>),
    (status = 404, description = "Unknown database or missing resource", body = crate::dto::ErrorDto),
))]
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<ActivityDto>>, ApiError> {
    // The cap must stay below the manifest LRU (256) so a cold fetch still
    // warms instead of thrashing it.
    let limit = params.limit.unwrap_or(20).min(200).max(1);
    let entries = state.manifest_entries().await?;

    // The newest `limit` transitions need `limit + 1` manifests.
    let tail: Vec<_> = entries
        .iter()
        .rev()
        .take(limit + 1)
        .rev()
        .cloned()
        .collect();

    let mut out = Vec::new();
    // On a cache miss both manifests convert; the carry reuses pair i's
    // `b` conversion as pair i+1's `a` during cold stretches.
    let mut carry: Option<(u64, ManifestDto)> = None;
    for i in 1..tail.len() {
        let key = (tail[i - 1].id, tail[i].id);
        if let Some(cached) = state.activity_cache.get(&key) {
            out.push(ActivityDto::clone(&cached));
            carry = None;
            continue;
        }
        // A manifest may have been GC'd between the LIST and this read.
        let a = match carry.take() {
            Some((id, dto)) if id == key.0 => Some(dto),
            _ => state
                .manifest_by_id(key.0)
                .await?
                .map(|m| convert::manifest_dto(&m)),
        };
        let b = state
            .manifest_by_id(key.1)
            .await?
            .map(|m| convert::manifest_dto(&m));
        let (Some(a), Some(b)) = (a, b) else {
            carry = None;
            continue;
        };
        let dto = ActivityDto {
            a: key.0,
            b: key.1,
            at: tail[i].last_modified,
            diff: summarize_diff(&diff_manifests(&a, &b)),
        };
        state.activity_cache.put(key, Arc::new(dto.clone()));
        out.push(dto);
        carry = Some((key.1, b));
    }
    out.reverse();
    Ok(Json(out))
}
