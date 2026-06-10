mod activity;
mod checkpoints;
mod compactions;
mod dbs;
mod garbage;
mod lsm;
mod manifests;
mod metrics;
mod overview;
mod search;
mod ssts;
mod wal;

use std::sync::Arc;

use axum::routing::{any, get};
use axum::Router;

use crate::registry::Registry;
use crate::state::AppState;

/// Routes for one DB. Mounted per discovered DB behind the dispatcher,
/// which rewrites `/api/dbs/{db}/…` to the `/api/…` paths used here.
pub fn api_router(state: Arc<AppState>) -> Router {
    Router::new()
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
        .route("/api/search", get(search::search))
        .with_state(state)
}

/// Top-level API: discovery, global health, per-DB dispatch, and the
/// all-DBs Prometheus endpoint (root-level by convention).
pub fn root_router(registry: Arc<Registry>) -> Router {
    Router::new()
        .route("/api/health", get(dbs::health))
        .route("/api/dbs", get(dbs::list))
        .route("/api/dbs/{db}/{*rest}", any(dbs::dispatch))
        .route("/metrics", get(metrics::metrics))
        .with_state(registry)
}
