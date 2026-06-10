use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::convert;
use crate::diff::diff_manifests;
use crate::dto::{ManifestDiffDto, ManifestDto, ManifestSummaryDto};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ListParams {
    start: Option<u64>,
    end: Option<u64>,
    limit: Option<usize>,
}

/// Manifest summaries, newest first. Each summary costs one manifest GET on
/// a cold cache, so the range is capped.
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<ManifestSummaryDto>>, ApiError> {
    let entries = state.manifest_entries().await?;
    let limit = params.limit.unwrap_or(50).min(500);
    let mut selected: Vec<_> = entries
        .iter()
        .filter(|e| {
            params.start.is_none_or(|s| e.id >= s) && params.end.is_none_or(|x| e.id <= x)
        })
        .collect();
    selected.reverse();
    selected.truncate(limit);

    let mut out = Vec::with_capacity(selected.len());
    for entry in selected {
        // The manifest may have been GC'd between the LIST and this read.
        let Some(m) = state.manifest_by_id(entry.id).await? else {
            continue;
        };
        let dto = convert::manifest_dto(&m);
        let (l0_count, sorted_run_count, sst_count, _, est_total_bytes) =
            convert::manifest_totals(&dto);
        out.push(ManifestSummaryDto {
            id: entry.id,
            last_modified: Some(entry.last_modified),
            writer_epoch: dto.writer_epoch,
            compactor_epoch: dto.compactor_epoch,
            l0_count,
            sorted_run_count,
            sst_count,
            est_total_bytes,
            checkpoint_count: dto.checkpoints.len(),
        });
    }
    Ok(Json(out))
}

pub async fn get_one(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ManifestDto>, ApiError> {
    if id == "latest" {
        let manifest = state.latest_manifest().await?;
        return match manifest.as_ref() {
            Some(m) => Ok(Json(convert::manifest_dto(m))),
            None => Err(ApiError::NotFound(format!(
                "no manifest found at '{}'",
                state.db_path
            ))),
        };
    }
    let id: u64 = id
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("invalid manifest id '{id}'")))?;
    match state.manifest_by_id(id).await? {
        Some(m) => Ok(Json(convert::manifest_dto(&m))),
        None => Err(ApiError::NotFound(format!(
            "manifest {id} not found (possibly GC'd)"
        ))),
    }
}

#[derive(Deserialize)]
pub struct DiffParams {
    a: u64,
    b: u64,
}

pub async fn diff(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DiffParams>,
) -> Result<Json<ManifestDiffDto>, ApiError> {
    let ma = state.manifest_by_id(params.a).await?.ok_or_else(|| {
        ApiError::NotFound(format!("manifest {} not found (possibly GC'd)", params.a))
    })?;
    let mb = state.manifest_by_id(params.b).await?.ok_or_else(|| {
        ApiError::NotFound(format!("manifest {} not found (possibly GC'd)", params.b))
    })?;
    Ok(Json(diff_manifests(
        &convert::manifest_dto(&ma),
        &convert::manifest_dto(&mb),
    )))
}
