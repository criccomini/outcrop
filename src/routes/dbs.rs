use std::sync::Arc;

use axum::extract::{Path, Query, Request, State};
use axum::http::Uri;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use tower::util::ServiceExt;

use crate::dto::{DbInfoDto, DbsDto, HealthDto};
use crate::error::ApiError;
use crate::registry::Registry;

pub async fn health(State(registry): State<Arc<Registry>>) -> Json<HealthDto> {
    Json(HealthDto {
        status: "ok",
        store_count: registry.store_count(),
        db_count: registry.peek_db_count(),
    })
}

#[derive(Deserialize)]
pub struct DbsParams {
    /// Present (any value) to bypass the scan cache.
    rescan: Option<String>,
}

pub async fn list(
    State(registry): State<Arc<Registry>>,
    Query(params): Query<DbsParams>,
) -> Result<Json<DbsDto>, ApiError> {
    let (scanned_at, dbs) = registry.scan(params.rescan.is_some()).await?;
    Ok(Json(DbsDto {
        scanned_at,
        dbs: dbs
            .into_iter()
            .map(|d| DbInfoDto {
                id: d.id,
                store: d.store,
                path: d.path,
            })
            .collect(),
    }))
}

/// Forwards `/api/dbs/{db}/{*rest}` into the per-DB router as `/api/{rest}`,
/// so every existing per-DB route file works unchanged.
pub async fn dispatch(
    State(registry): State<Arc<Registry>>,
    Path((db, rest)): Path<(String, String)>,
    mut req: Request,
) -> Response {
    let handle = match registry.resolve(&db).await {
        Ok(h) => h,
        Err(e) => return e.into_response(),
    };
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();
    let uri: Uri = match format!("/api/{rest}{query}").parse() {
        Ok(u) => u,
        Err(_) => {
            return ApiError::NotFound(format!("invalid path: {rest}")).into_response();
        }
    };
    *req.uri_mut() = uri;
    // Drop this route's captured path params (and any other extensions):
    // they'd otherwise pile onto the inner router's own captures and break
    // its Path extractors.
    let (mut parts, body) = req.into_parts();
    parts.extensions.clear();
    let req = Request::from_parts(parts, body);
    match handle.router.clone().oneshot(req).await {
        Ok(resp) => resp,
        Err(never) => match never {},
    }
}
