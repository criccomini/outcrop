use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use chrono::{DateTime, Utc};
use slatedb::manifest::{SsTableId, VersionedManifest};
use ulid::Ulid;

use crate::dto::{GarbageDto, GcEventsDto};
use crate::error::ApiError;
use crate::garbage::{compute_garbage, CheckpointPin, GarbageInputs, ManifestRefs};
use crate::state::AppState;

/// Compacted-SST ULIDs referenced by a manifest's root tree and segments,
/// plus the WAL window it needs for replay.
fn manifest_refs(m: &VersionedManifest) -> ManifestRefs {
    let mut compacted: HashSet<Ulid> = HashSet::new();
    let mut collect = |l0: &std::collections::VecDeque<slatedb::manifest::SsTableView>,
                       runs: &[slatedb::manifest::SortedRun]| {
        let views = l0.iter().chain(runs.iter().flat_map(|r| r.sst_views.iter()));
        for view in views {
            if let SsTableId::Compacted(ulid) = view.sst.id {
                compacted.insert(ulid);
            }
        }
    };
    collect(m.l0(), m.compacted());
    for seg in m.segments() {
        collect(seg.l0(), seg.compacted());
    }
    ManifestRefs {
        id: m.id(),
        compacted,
        replay_after_wal_id: m.replay_after_wal_id(),
        next_wal_sst_id: m.next_wal_sst_id(),
    }
}

/// Newest L0 timestamp across the manifest's trees (root + segments),
/// falling back to the last-compacted L0 view id when a tree's L0 is
/// empty. Mirrors the GC's `newest_l0_dt`: SSTs newer than this may be L0
/// flushes the manifest hasn't caught up with, so the GC won't touch them.
fn newest_l0_dt(m: &VersionedManifest) -> DateTime<Utc> {
    let tree_dt = |l0: &std::collections::VecDeque<slatedb::manifest::SsTableView>,
                   last_compacted: Option<Ulid>| {
        if l0.is_empty() {
            last_compacted.map(|u| DateTime::<Utc>::from(u.datetime()))
        } else {
            l0.iter()
                .filter_map(|v| match v.sst.id {
                    SsTableId::Compacted(u) => Some(DateTime::<Utc>::from(u.datetime())),
                    SsTableId::Wal(_) => None,
                })
                .max()
        }
    };
    let mut newest = tree_dt(m.l0(), m.last_compacted_l0_sst_view_id());
    for seg in m.segments() {
        newest = newest.max(tree_dt(seg.l0(), seg.last_compacted_l0_sst_view_id()));
    }
    newest.unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}

/// The instant past which the GC deletes nothing from `compacted/`
/// regardless of references: the min start time across the compactions
/// file (in-flight outputs land before the manifest references them; a
/// missing/empty file disables compaction-state-based deletion entirely),
/// capped by the newest-L0 barrier.
async fn gc_cutoff(
    state: &Arc<AppState>,
    m: &VersionedManifest,
) -> Result<DateTime<Utc>, ApiError> {
    let compactor = state.compactor_state_dto().await?;
    let watermark = compactor
        .compactions
        .as_ref()
        .map(|vc| {
            vc.compactions
                .iter()
                .filter_map(|c| Ulid::from_string(&c.id).ok())
                .map(|u| DateTime::<Utc>::from(u.datetime()))
                .min()
                .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
        })
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    Ok(watermark.min(newest_l0_dt(m)))
}

/// Deletions observed by diffing consecutive listing refreshes (the GC
/// itself leaves no record). Refreshes the listings first so the feed is
/// as current as a poll can make it.
#[utoipa::path(get, path = "/api/dbs/{db}/garbage/events", tag = "garbage", params(crate::routes::DbPathParam), responses(
    (status = 200, description = "Deletions observed by diffing consecutive listings (per-process, in-memory)", body = GcEventsDto),
    (status = 404, description = "Unknown database or missing resource", body = crate::dto::ErrorDto),
))]
pub async fn gc_events(
    State(state): State<Arc<AppState>>,
) -> Result<Json<GcEventsDto>, ApiError> {
    state.compacted_entries().await?;
    state.wal_entries().await?;
    state.manifest_entries().await?;
    let obs = state.gc_observer.lock().unwrap();
    Ok(Json(GcEventsDto {
        observing_since: obs.started_at(),
        events: obs.events().cloned().collect(),
    }))
}

#[utoipa::path(get, path = "/api/dbs/{db}/garbage", tag = "garbage", params(crate::routes::DbPathParam), responses(
    (status = 200, description = "Space-amplification report: live / pinned / reclaimable per object class", body = GarbageDto),
    (status = 404, description = "Unknown database or missing resource", body = crate::dto::ErrorDto),
))]
pub async fn garbage(State(state): State<Arc<AppState>>) -> Result<Json<GarbageDto>, ApiError> {
    let manifest = state.latest_manifest().await?;
    let Some(m) = manifest.as_ref() else {
        return Err(ApiError::NotFound(format!(
            "no manifest found at '{}'",
            state.db_path
        )));
    };

    let now = chrono::Utc::now();
    let mut live_checkpoint_count = 0;
    let mut expired_checkpoint_count = 0;
    let mut pinned_checkpoints: Vec<CheckpointPin> = Vec::new();
    let mut pinned_ids: BTreeSet<u64> = BTreeSet::new();
    for c in m.checkpoints() {
        if c.expire_time.is_some_and(|t| t <= now) {
            expired_checkpoint_count += 1;
            continue;
        }
        live_checkpoint_count += 1;
        pinned_checkpoints.push(CheckpointPin {
            id: c.id.to_string(),
            name: c.name.clone(),
            manifest_id: c.manifest_id,
            expire_time: c.expire_time,
        });
        if c.manifest_id != m.id() {
            pinned_ids.insert(c.manifest_id);
        }
    }

    let mut pinned = Vec::with_capacity(pinned_ids.len());
    let mut dangling_checkpoint_count = 0;
    for id in pinned_ids {
        match state.manifest_by_id(id).await? {
            Some(pm) => pinned.push(manifest_refs(&pm)),
            None => dangling_checkpoint_count += 1,
        }
    }

    let gc_cutoff = gc_cutoff(&state, m).await?;
    let compacted_listing = state.compacted_entries().await?;
    let wal_listing = state.wal_entries().await?;
    let manifest_listing = state.manifest_entries().await?;

    Ok(Json(compute_garbage(&GarbageInputs {
        latest: manifest_refs(m),
        pinned,
        pinned_checkpoints,
        live_checkpoint_count,
        expired_checkpoint_count,
        dangling_checkpoint_count,
        compacted_listing: &compacted_listing,
        wal_listing: &wal_listing,
        manifest_listing: &manifest_listing,
        gc_cutoff,
    })))
}
