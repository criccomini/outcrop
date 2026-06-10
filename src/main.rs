mod assets;
mod cache;
mod convert;
mod demo;
mod diff;
mod dto;
mod error;
mod garbage;
mod routes;
mod state;
mod warnings;

use std::sync::Arc;
use std::time::Duration;

use axum::http::{HeaderValue, Method, Uri};
use axum::response::{IntoResponse, Response};
use axum::Router;
use clap::{Parser, Subcommand};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use crate::error::ApiError;
use crate::state::AppState;

/// Read-only web dashboard for SlateDB.
///
/// With no subcommand this serves the dashboard (UI + API), exactly like
/// `serve`. The object store is configured through environment variables
/// (or an env file), exactly like slatedb-cli: set CLOUD_PROVIDER to
/// local | memory | aws | azure | opendal plus the provider-specific
/// variables (e.g. LOCAL_PATH for local).
#[derive(Parser, Debug)]
#[command(
    version,
    about,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    serve: ServeArgs,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Serve the dashboard: UI + API (default), --api-only, or --ui-only
    Serve(ServeArgs),
    /// Seed the local demo DB if missing, then simulate live traffic
    /// against it until Ctrl-C
    Traffic(demo::TrafficArgs),
}

#[derive(clap::Args, Debug)]
struct ServeArgs {
    /// Path to the database root within the object store
    #[arg(short, long, required_unless_present = "ui_only")]
    path: Option<String>,

    /// Address to listen on
    #[arg(short, long, default_value = "127.0.0.1:8333")]
    listen: String,

    /// Env file to load object store configuration from
    #[arg(short, long)]
    env_file: Option<String>,

    /// TTL in seconds for cached reads of mutable state (latest manifest,
    /// manifest listing, compactor state)
    #[arg(long, default_value_t = 5)]
    cache_ttl_secs: u64,

    /// Serve only the REST API (and /metrics) — no embedded UI
    #[arg(long, conflicts_with = "ui_only")]
    api_only: bool,

    /// Serve only the UI; the browser calls a remote API directly
    #[arg(long)]
    ui_only: bool,

    /// API base URL baked into the UI (required with --ui-only),
    /// e.g. http://api-host:8333
    #[arg(long, required_if_eq("ui_only", "true"))]
    api_url: Option<String>,

    /// Origin allowed for cross-origin API requests (repeatable; '*' for
    /// any). Defaults to '*' under --api-only so a --ui-only instance can
    /// reach it; the API is read-only and unauthenticated either way.
    #[arg(long)]
    cors_allow_origin: Vec<String>,
}

fn init_tracing(default_filter: &str) {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        // Surface slatedb's internal logs (flush/close/GC progress); raise
        // with e.g. RUST_LOG=slatedb=debug when diagnosing.
        Some(Command::Traffic(args)) => {
            init_tracing("slatedb=info");
            demo::run_traffic(args).await
        }
        Some(Command::Serve(args)) => {
            init_tracing("slatedb_dashboard=info,tower_http=info");
            serve(args).await
        }
        None => {
            init_tracing("slatedb_dashboard=info,tower_http=info");
            serve(cli.serve).await
        }
    }
}

/// JSON 404 for non-API paths in --api-only mode, mirroring how the static
/// handler treats unknown /api paths.
async fn api_only_fallback(uri: Uri) -> Response {
    ApiError::NotFound(format!(
        "no such endpoint: {} (this server is API-only)",
        uri.path()
    ))
    .into_response()
}

fn cors_layer(origins: &[String]) -> anyhow::Result<CorsLayer> {
    let layer = CorsLayer::new()
        .allow_methods([Method::GET])
        .allow_headers(Any);
    if origins.iter().any(|o| o == "*") {
        return Ok(layer.allow_origin(Any));
    }
    let parsed = origins
        .iter()
        .map(|o| {
            o.parse::<HeaderValue>()
                .map_err(|e| anyhow::anyhow!("invalid --cors-allow-origin '{o}': {e}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(layer.allow_origin(parsed))
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let app: Router = if args.ui_only {
        // No object store, no API state — just the embedded SPA with the
        // remote API base injected into index.html.
        let api_base = args
            .api_url
            .clone()
            .expect("clap requires --api-url with --ui-only")
            .trim_end_matches('/')
            .to_string();
        info!("ui-only mode; the browser will call the API at {api_base}");
        Router::new().fallback(move |uri: Uri| assets::static_handler(uri, Some(api_base.clone())))
    } else {
        let path = args
            .path
            .clone()
            .expect("clap requires --path unless --ui-only");
        let object_store = slatedb::admin::load_object_store_from_env(args.env_file.clone())
            .map_err(|e| anyhow::anyhow!("failed to load object store from env: {e}"))?;
        let provider =
            std::env::var("CLOUD_PROVIDER").unwrap_or_else(|_| "unknown".to_string());

        let state = Arc::new(AppState::new(
            path.clone(),
            provider,
            object_store,
            Duration::from_secs(args.cache_ttl_secs),
        ));

        // Startup probe: warn loudly if there's no DB here, but start
        // anyway — the DB may be created later.
        match state.admin.read_manifest(None).await {
            Ok(Some(m)) => info!("found SlateDB at '{}' (manifest id {})", path, m.id()),
            Ok(None) => warn!("no manifest found at '{}' — is this a SlateDB root?", path),
            Err(e) => warn!("could not read manifest at '{}': {e}", path),
        }

        let mut app = routes::api_router(state);
        app = if args.api_only {
            info!("api-only mode; not serving the UI");
            app.fallback(api_only_fallback)
        } else {
            app.fallback(|uri: Uri| assets::static_handler(uri, None))
        };

        // CORS so a --ui-only instance elsewhere can call this API from
        // the browser.
        let origins = if args.cors_allow_origin.is_empty() && args.api_only {
            vec!["*".to_string()]
        } else {
            args.cors_allow_origin.clone()
        };
        if origins.is_empty() {
            app
        } else {
            app.layer(cors_layer(&origins)?)
        }
    };

    let app = app.layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&args.listen).await?;
    info!("slatedb-dashboard listening on http://{}", args.listen);
    axum::serve(listener, app).await?;
    Ok(())
}
