//! OpenAPI document for the REST API, served at `/api/openapi.json` with a
//! reference UI at `/api/docs`. Handlers are annotated with their PUBLIC
//! paths (`/api/dbs/{db}/…`); the registry's internal URI rewrite into the
//! per-DB routers is invisible to clients and to this spec.
//!
//! Generate clients with any OpenAPI 3.1 generator, e.g.
//! `npx openapi-typescript http://127.0.0.1:8333/api/openapi.json`.

use axum::response::Html;
use axum::Json;
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "SlateDB Dashboard API",
        description = "Read-only REST API for inspecting SlateDB databases directly from \
                       object storage. Databases are auto-discovered and addressed by id \
                       `{store}:{path}`, URL-encoded as a single path segment. The dashboard \
                       performs zero writes.",
        license(name = "Apache-2.0"),
    ),
    paths(
        super::dbs::health,
        super::dbs::list,
        super::metrics::metrics,
        super::overview::overview,
        super::activity::list,
        super::lsm::lsm,
        super::lsm::lsm_summary,
        super::lsm::level_slice,
        super::wal::wal,
        super::manifests::ids,
        super::manifests::list,
        super::manifests::get_one,
        super::manifests::diff,
        super::ssts::get_one,
        super::compactions::state,
        super::compactions::list,
        super::compactions::get_one,
        super::checkpoints::list,
        super::checkpoints::clones,
        super::garbage::garbage,
        super::garbage::gc_events,
        super::search::search,
    ),
    tags(
        (name = "fleet", description = "Discovery, health and Prometheus metrics"),
        (name = "overview", description = "Per-DB headline stats and warnings"),
        (name = "activity", description = "Manifest transition feed"),
        (name = "lsm", description = "LSM tree structure"),
        (name = "wal", description = "Write-ahead log"),
        (name = "manifests", description = "Manifest versions and diffs"),
        (name = "ssts", description = "SST detail"),
        (name = "compactions", description = "Compactor state and history"),
        (name = "checkpoints", description = "Checkpoints and clones"),
        (name = "garbage", description = "Space amplification and GC observations"),
        (name = "search", description = "Cross-resource search"),
    )
)]
pub struct ApiDoc;

pub async fn spec() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

/// Interactive API reference: a static page that renders the spec with
/// Scalar. The viewer script comes from a CDN, so the page (unlike the
/// spec itself) needs internet access — same trade-off as the SPA's fonts.
pub async fn docs() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html>
  <head>
    <title>SlateDB Dashboard API</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
  </head>
  <body>
    <script id="api-reference" data-url="/api/openapi.json"></script>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
  </body>
</html>"#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_documents_every_route_and_serializes() {
        let doc = ApiDoc::openapi();
        let json = serde_json::to_value(&doc).unwrap();
        let paths = json["paths"].as_object().unwrap();
        for p in [
            "/api/health",
            "/api/dbs",
            "/metrics",
            "/api/dbs/{db}/overview",
            "/api/dbs/{db}/activity",
            "/api/dbs/{db}/lsm",
            "/api/dbs/{db}/lsm/summary",
            "/api/dbs/{db}/lsm/level",
            "/api/dbs/{db}/wal",
            "/api/dbs/{db}/manifests",
            "/api/dbs/{db}/manifests/ids",
            "/api/dbs/{db}/manifests/diff",
            "/api/dbs/{db}/manifests/{id}",
            "/api/dbs/{db}/ssts/{ulid}",
            "/api/dbs/{db}/compactor/state",
            "/api/dbs/{db}/compactions",
            "/api/dbs/{db}/compactions/{ulid}",
            "/api/dbs/{db}/checkpoints",
            "/api/dbs/{db}/clones",
            "/api/dbs/{db}/garbage",
            "/api/dbs/{db}/garbage/events",
            "/api/dbs/{db}/search",
        ] {
            assert!(paths.contains_key(p), "spec missing path {p}");
        }
        // Schemas referenced from responses must have been collected.
        let schemas = json["components"]["schemas"].as_object().unwrap();
        for s in ["OverviewDto", "LsmSummaryDto", "ManifestDto", "ErrorDto", "KeyDto"] {
            assert!(schemas.contains_key(s), "spec missing schema {s}");
        }
    }
}
