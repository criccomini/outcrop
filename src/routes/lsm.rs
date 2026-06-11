use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use slatedb::manifest::VersionedManifest;

use crate::convert;
use crate::dto::{LevelSliceDto, LsmDto, LsmSummaryDto};
use crate::error::ApiError;
use crate::state::AppState;
use crate::summary;

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

#[derive(Deserialize)]
pub struct LsmSummaryParams {
    /// Summarize as of this manifest instead of the latest.
    manifest_id: Option<u64>,
    /// Segment index to view; omitted = root tree (falling through to
    /// segment 0 when the root is empty in a segmented DB).
    segment: Option<usize>,
}

/// Summary-first LSM view. Cheap for any tree size: full conversion runs
/// at most once per (manifest, segment) thanks to the immutable cache.
pub async fn lsm_summary(
    State(state): State<Arc<AppState>>,
    Query(params): Query<LsmSummaryParams>,
) -> Result<Json<Arc<LsmSummaryDto>>, ApiError> {
    // try_from, not `as`: usize::MAX as i64 is -1, which would alias the
    // root sentinel and serve the cached root summary for a bogus index.
    let seg_key = match params.segment {
        Some(s) => i64::try_from(s)
            .map_err(|_| ApiError::BadRequest(format!("segment {s} out of range")))?,
        None => -1,
    };
    if let Some(id) = params.manifest_id {
        // Explicit ids can hit the cache without touching the store.
        if let Some(s) = state.lsm_summaries.get(&(id, seg_key)) {
            return Ok(Json(s));
        }
        let m = state.manifest_by_id(id).await?.ok_or_else(|| {
            ApiError::NotFound(format!("manifest {id} not found (possibly GC'd)"))
        })?;
        return build_summary(&state, &m, params.segment, seg_key);
    }
    let latest = state.latest_manifest().await?;
    let Some(m) = latest.as_ref() else {
        return Err(ApiError::NotFound(format!(
            "no manifest found at '{}'",
            state.db_path
        )));
    };
    if let Some(s) = state.lsm_summaries.get(&(m.id(), seg_key)) {
        return Ok(Json(s));
    }
    build_summary(&state, m, params.segment, seg_key)
}

#[derive(Deserialize)]
pub struct LevelSliceParams {
    /// Slice as of this manifest instead of the latest.
    manifest_id: Option<u64>,
    /// Segment index; omitted = root tree (no auto-fallback — pass the
    /// effective segment the summary reported).
    segment: Option<usize>,
    /// Sorted-run id; omitted = L0.
    run: Option<u32>,
    /// Hex-encoded inclusive key bounds (as in KeyDto.hex); omitted =
    /// unbounded on that side.
    start: Option<String>,
    end: Option<String>,
    limit: Option<usize>,
}

/// On-demand per-SST drill-down for one level restricted to a key range —
/// how the UI reaches SSTs in levels too large for summary detail. Pure
/// CPU over the cached manifest; no object-store requests.
pub async fn level_slice(
    State(state): State<Arc<AppState>>,
    Query(params): Query<LevelSliceParams>,
) -> Result<Json<LevelSliceDto>, ApiError> {
    let limit = params.limit.unwrap_or(64).min(200).max(1);
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
    let tree = match params.segment {
        None => &dto.tree,
        Some(i) => {
            &dto.segments
                .get(i)
                .ok_or_else(|| {
                    ApiError::BadRequest(format!(
                        "segment {i} out of range ({} segments)",
                        dto.segments.len()
                    ))
                })?
                .tree
        }
    };
    // Stored hex is lowercase; accept either case from the client.
    let start = params.start.unwrap_or_default().to_ascii_lowercase();
    let end = params.end.map(|e| e.to_ascii_lowercase());
    let (total, ssts) = summary::slice_level(tree, params.run, &start, end.as_deref(), limit)
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "sorted run {} not in this manifest (possibly compacted away)",
                params.run.unwrap_or_default()
            ))
        })?;
    Ok(Json(LevelSliceDto {
        total,
        truncated: total > ssts.len(),
        ssts,
    }))
}

fn build_summary(
    state: &AppState,
    m: &VersionedManifest,
    segment: Option<usize>,
    seg_key: i64,
) -> Result<Json<Arc<LsmSummaryDto>>, ApiError> {
    let dto = convert::manifest_dto(m);
    let summary = summary::summarize(&dto, segment).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "segment {} out of range ({} segments)",
            segment.unwrap_or(0),
            dto.segments.len()
        ))
    })?;
    let arc = Arc::new(summary);
    state.lsm_summaries.put((dto.id, seg_key), arc.clone());
    Ok(Json(arc))
}
