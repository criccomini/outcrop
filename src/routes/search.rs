use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use slatedb::manifest::{SsTableId, SortedRun, SsTableView, VersionedManifest};
use ulid::Ulid;

use crate::dto::{
    SearchCheckpointHitDto, SearchCompactionHitDto, SearchDto, SearchManifestHitDto,
    SearchSstObjectDto,
};
use crate::error::ApiError;
use crate::state::AppState;

/// Newest manifests scanned per search; each cold manifest is one GET
/// (warmed via the immutable LRU afterwards).
const MANIFEST_SCAN_LIMIT: usize = 80;
/// Manifest hits returned before truncating.
const MANIFEST_HIT_LIMIT: usize = 20;
/// Recent .compactions versions swept.
const COMPACTIONS_SCAN_LIMIT: u64 = 30;

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchParams {
    q: String,
}

fn scan_views<'a>(
    label: &str,
    l0: impl Iterator<Item = &'a SsTableView>,
    runs: &[SortedRun],
    ulid: &Ulid,
    places: &mut Vec<String>,
) {
    let sep = if label.is_empty() { "" } else { " " };
    for view in l0 {
        if matches!(view.sst.id, SsTableId::Compacted(u) if u == *ulid) {
            places.push(format!("SST in{sep}{label} L0"));
        }
        if view.id == *ulid {
            places.push(format!("view id in{sep}{label} L0"));
        }
    }
    for run in runs {
        for view in &run.sst_views {
            if matches!(view.sst.id, SsTableId::Compacted(u) if u == *ulid) {
                places.push(format!("SST in{sep}{label} SR {}", run.id));
            }
            if view.id == *ulid {
                places.push(format!("view id in{sep}{label} SR {}", run.id));
            }
        }
    }
}

fn manifest_places(m: &VersionedManifest, ulid: &Ulid) -> Vec<String> {
    let mut places = Vec::new();
    scan_views("", m.l0().iter(), m.compacted(), ulid, &mut places);
    for seg in m.segments() {
        let label = format!("segment {}", String::from_utf8_lossy(seg.prefix()));
        scan_views(&label, seg.l0().iter(), seg.compacted(), ulid, &mut places);
    }
    places
}

/// ULID search across the DB: the SST object itself, manifests whose trees
/// reference it (as an SST id or an L0 view id), compactor jobs (by job id
/// or output SST), and — when the query parses as a UUID — checkpoints.
#[utoipa::path(get, path = "/api/dbs/{db}/search", tag = "search", params(crate::routes::DbPathParam, SearchParams), responses(
    (status = 200, description = "Matches across manifests, checkpoints, SSTs and compactions", body = SearchDto),
    (status = 404, description = "Unknown database or missing resource", body = crate::dto::ErrorDto),
))]
pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchDto>, ApiError> {
    let query = params.q.trim().to_string();
    let ulid = Ulid::from_string(&query).ok();
    let uuid = uuid::Uuid::parse_str(&query).ok();
    if ulid.is_none() && uuid.is_none() {
        return Err(ApiError::BadRequest(format!(
            "'{query}' is not a ULID (or checkpoint UUID)"
        )));
    }

    let mut out = SearchDto {
        query: query.clone(),
        sst_object: None,
        manifests: Vec::new(),
        manifests_scanned: 0,
        manifests_total: 0,
        compactions: Vec::new(),
        checkpoints: Vec::new(),
    };

    if let Some(ulid) = ulid {
        // The object itself.
        if let Some(entry) = state
            .compacted_entries()
            .await?
            .iter()
            .find(|e| e.ulid == ulid)
        {
            out.sst_object = Some(SearchSstObjectDto {
                location: format!("{}/compacted/{ulid}.sst", state.db_path),
                size_bytes: entry.size_bytes,
                last_modified: entry.last_modified,
            });
        }

        // Manifests referencing it, newest first.
        let entries = state.manifest_entries().await?;
        out.manifests_total = entries.len();
        for entry in entries.iter().rev().take(MANIFEST_SCAN_LIMIT) {
            out.manifests_scanned += 1;
            let Some(m) = state.manifest_by_id(entry.id).await? else {
                continue;
            };
            let places = manifest_places(&m, &ulid);
            if !places.is_empty() {
                out.manifests.push(SearchManifestHitDto {
                    id: entry.id,
                    places,
                });
                if out.manifests.len() >= MANIFEST_HIT_LIMIT {
                    break;
                }
            }
        }

        // Compactor jobs, newest version first; one hit per (job, role).
        if let Some(latest) = state.admin.read_compactions(None).await? {
            let end = latest.id();
            let start = end.saturating_sub(COMPACTIONS_SCAN_LIMIT - 1);
            let versions = state.admin.list_compactions(start..=end).await?;
            for vc in versions.iter().rev() {
                for c in vc.recent_compactions() {
                    let mut push = |role: &'static str| {
                        if !out
                            .compactions
                            .iter()
                            .any(|h| h.job_id == c.id().to_string() && h.role == role)
                        {
                            out.compactions.push(SearchCompactionHitDto {
                                version: vc.id(),
                                job_id: c.id().to_string(),
                                role,
                            });
                        }
                    };
                    if c.id() == ulid {
                        push("job");
                    }
                    if c.output_ssts()
                        .iter()
                        .any(|h| matches!(h.id, SsTableId::Compacted(u) if u == ulid))
                    {
                        push("output");
                    }
                }
            }
        }
    }

    if let Some(uuid) = uuid {
        let manifest = state.latest_manifest().await?;
        if let Some(m) = manifest.as_ref() {
            for c in m.checkpoints() {
                if c.id == uuid {
                    out.checkpoints.push(SearchCheckpointHitDto {
                        id: c.id.to_string(),
                        name: c.name.clone(),
                        manifest_id: c.manifest_id,
                    });
                }
            }
        }
    }

    Ok(Json(out))
}
