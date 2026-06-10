use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use slatedb::manifest::{SsTableId, VersionedManifest};
use ulid::Ulid;

use crate::dto::GarbageDto;
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
    })))
}
