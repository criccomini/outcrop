mod activity;
mod checkpoints;
mod compactions;
mod dbs;
mod garbage;
mod lsm;
mod manifests;
mod metrics;
mod openapi;
mod overview;
mod search;
mod ssts;
mod wal;

use std::sync::Arc;

use axum::routing::{any, get};
use axum::Router;

use crate::registry::Registry;
use crate::state::AppState;

/// Path parameter shared by every per-DB route — documentation only; the
/// dispatcher extracts it before requests reach the per-DB routers.
#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Path)]
#[allow(dead_code)]
pub struct DbPathParam {
    /// Database id `{store}:{path}` as a single path segment, e.g.
    /// `default:demo-db-1`. The colon may be sent raw; percent-encode any
    /// slashes in the DB path (`default:teams%2Fa%2Fdb1`) or the route
    /// won't match.
    pub db: String,
}

/// Routes for one DB. Mounted per discovered DB behind the dispatcher,
/// which rewrites `/api/dbs/{db}/…` to the `/api/…` paths used here.
pub fn api_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/overview", get(overview::overview))
        .route("/api/activity", get(activity::list))
        .route("/api/lsm", get(lsm::lsm))
        .route("/api/lsm/summary", get(lsm::lsm_summary))
        .route("/api/lsm/level", get(lsm::level_slice))
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
        .route("/api/garbage/events", get(garbage::gc_events))
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
        .route("/api/openapi.json", get(openapi::spec))
        .route("/api/docs", get(openapi::docs))
        .route("/metrics", get(metrics::metrics))
        .with_state(registry)
}
