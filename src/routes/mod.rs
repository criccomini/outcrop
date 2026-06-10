mod activity;
mod checkpoints;
mod compactions;
mod garbage;
mod lsm;
mod manifests;
mod metrics;
mod overview;
mod ssts;
mod wal;

use std::sync::Arc;

use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub fn api_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(overview::health))
        .route("/api/overview", get(overview::overview))
        .route("/api/activity", get(activity::list))
        .route("/api/lsm", get(lsm::lsm))
        .route("/api/wal", get(wal::wal))
        .route("/api/manifests", get(manifests::list))
        .route("/api/manifests/ids", get(manifests::ids))
        .route("/api/manifests/diff", get(manifests::diff))
        .route("/api/manifests/{id}", get(manifests::get_one))
        .route("/api/ssts/{ulid}", get(ssts::get_one))
        .route("/api/compactor/state", get(compactions::state))
        .route("/api/compactions", get(compactions::list))
        .route("/api/compactions/{ulid}", get(compactions::get_one))
        .route("/api/checkpoints", get(checkpoints::list))
        .route("/api/clones", get(checkpoints::clones))
        .route("/api/garbage", get(garbage::garbage))
        // Root-level by Prometheus convention, intentionally not under /api.
        .route("/metrics", get(metrics::metrics))
        .with_state(state)
}
