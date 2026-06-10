use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use slatedb::seq_tracker::FindOption;

use crate::convert;
use crate::dto::{HealthDto, OverviewDto};
use crate::error::ApiError;
use crate::state::AppState;
use crate::warnings::{compute_warnings, WarningInputs};

pub async fn health(State(state): State<Arc<AppState>>) -> Json<HealthDto> {
    Json(HealthDto {
        status: "ok",
        db_path: state.db_path.clone(),
        provider: state.provider.clone(),
    })
}

pub async fn overview(
    State(state): State<Arc<AppState>>,
) -> Result<Json<OverviewDto>, ApiError> {
    let manifest = state.latest_manifest().await?;
    let Some(m) = manifest.as_ref() else {
        return Err(ApiError::NotFound(format!(
            "no manifest found at '{}' — is this a SlateDB root?",
            state.db_path
        )));
    };

    let dto = convert::manifest_dto(m);
    let (l0_count, sorted_run_count, sst_count, l0_bytes, est_total_bytes) =
        convert::manifest_totals(&dto);

    let entries = state.manifest_entries().await?;
    // RoundDown: the latest tracked timestamp at or before the last L0 seq.
    // RoundUp would return None at the tail of the tracker.
    let last_l0_approx_time = m
        .sequence_tracker()
        .find_ts(m.last_l0_seq(), FindOption::RoundDown);

    let live_manifest_ids: HashSet<u64> = entries.iter().map(|e| e.id).collect();
    let latest_manifest_written_at = entries.last().map(|e| e.last_modified);
    let warnings = compute_warnings(&WarningInputs {
        manifest: &dto,
        live_manifest_ids: &live_manifest_ids,
        latest_manifest_written_at,
        now: chrono::Utc::now(),
    });

    Ok(Json(OverviewDto {
        db_path: state.db_path.clone(),
        provider: state.provider.clone(),
        manifest_id: dto.id,
        initialized: dto.initialized,
        writer_epoch: dto.writer_epoch,
        compactor_epoch: dto.compactor_epoch,
        l0_count,
        sorted_run_count,
        sst_count,
        l0_bytes,
        est_total_bytes,
        segment_count: dto.segments.len(),
        next_wal_sst_id: dto.next_wal_sst_id,
        replay_after_wal_id: dto.replay_after_wal_id,
        last_l0_seq: dto.last_l0_seq,
        last_l0_approx_time,
        recent_snapshot_min_seq: dto.recent_snapshot_min_seq,
        checkpoint_count: dto.checkpoints.len(),
        clone_count: dto.external_dbs.len(),
        wal_object_store_uri: dto.wal_object_store_uri.clone(),
        manifest_count: entries.len(),
        oldest_manifest_id: entries.first().map(|e| e.id),
        latest_manifest_written_at,
        warnings,
    }))
}
