use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::convert;
use crate::diff::diff_manifests;
use crate::dto::ActivityDto;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ListParams {
    limit: Option<usize>,
}

/// Recent manifest transitions, newest first: each item diffs a manifest
/// against its predecessor. Each transition costs one manifest GET on a cold
/// cache (plus one for the oldest predecessor), so the range is capped.
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<ActivityDto>>, ApiError> {
    let limit = params.limit.unwrap_or(20).min(100).max(1);
    let entries = state.manifest_entries().await?;

    // The newest `limit` transitions need `limit + 1` manifests.
    let tail: Vec<_> = entries
        .iter()
        .rev()
        .take(limit + 1)
        .rev()
        .cloned()
        .collect();

    // Fetch and convert each manifest once; a manifest may have been GC'd
    // between the LIST and this read.
    let mut dtos = Vec::with_capacity(tail.len());
    for entry in &tail {
        let dto = state
            .manifest_by_id(entry.id)
            .await?
            .map(|m| convert::manifest_dto(&m));
        dtos.push(dto);
    }

    let mut out = Vec::new();
    for i in 1..tail.len() {
        let (Some(a), Some(b)) = (&dtos[i - 1], &dtos[i]) else {
            continue;
        };
        out.push(ActivityDto {
            a: a.id,
            b: b.id,
            at: tail[i].last_modified,
            diff: diff_manifests(a, b),
        });
    }
    out.reverse();
    Ok(Json(out))
}
