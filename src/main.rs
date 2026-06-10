mod assets;
mod cache;
mod config;
mod convert;
mod demo;
mod diff;
mod discovery;
mod dto;
mod error;
mod garbage;
mod registry;
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

use crate::discovery::ScanLimits;
use crate::error::ApiError;
use crate::registry::{Registry, Store};

/// Read-only web dashboard for SlateDB.
///
/// With no subcommand this serves the dashboard (UI + API), exactly like
/// `serve`. DBs are auto-discovered by walking the configured object
/// store(s) for prefixes with a `manifest/` directory. The single-store
/// case is configured through ambient environment variables, exactly like
/// slatedb-cli (CLOUD_PROVIDER plus provider-specific variables); multiple
/// stores via --config.
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
    /// Seed local demo DBs if missing, then simulate live traffic against
    /// them until Ctrl-C
    Traffic(demo::TrafficArgs),
}

#[derive(clap::Args, Debug)]
struct ServeArgs {
    /// TOML file describing multiple object stores and their scan roots
    /// (see README); without it, the single ambient-env store is scanned
    #[arg(short, long, conflicts_with = "root")]
    config: Option<String>,

    /// Root prefix to scan for DBs on the ambient-env store (repeatable;
    /// default: the store root)
    #[arg(long)]
    root: Vec<String>,

    /// Address to listen on
    #[arg(short, long, default_value = "127.0.0.1:8333")]
    listen: String,

    /// TTL in seconds for cached reads of mutable per-DB state (latest
    /// manifest, listings, compactor state)
    #[arg(long, default_value_t = 5)]
    cache_ttl_secs: u64,

    /// How many "directory" levels below each root discovery descends
    #[arg(long, default_value_t = 4)]
    scan_depth: usize,

    /// Seconds discovery results are cached before rescanning
    #[arg(long, default_value_t = 60)]
    scan_ttl_secs: u64,

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

fn runtime() -> anyhow::Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?)
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        // Surface slatedb's internal logs (flush/close/GC progress); raise
        // with e.g. RUST_LOG=slatedb=debug when diagnosing.
        Some(Command::Traffic(args)) => {
            init_tracing("slatedb=info");
            runtime()?.block_on(demo::run_traffic(args))
        }
        Some(Command::Serve(args)) => run_serve(args),
        None => run_serve(cli.serve),
    }
}

fn run_serve(args: ServeArgs) -> anyhow::Result<()> {
    init_tracing("slatedb_dashboard=info,tower_http=info");
    // Stores are built BEFORE the tokio runtime exists: building stages
    // provider settings as env vars (slatedb's loaders only read the
    // process env), which is only sound while single-threaded.
    let stores = if args.ui_only {
        Vec::new()
    } else {
        build_stores(&args)?
    };
    runtime()?.block_on(serve(args, stores))
}

fn build_stores(args: &ServeArgs) -> anyhow::Result<Vec<Store>> {
    if let Some(path) = &args.config {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("reading config '{path}': {e}"))?;
        let stores = config::build_stores(&text)?;
        Ok(stores
            .into_iter()
            .map(|b| Store {
                name: b.name,
                provider: b.provider,
                object_store: b.object_store,
                roots: b.roots,
            })
            .collect())
    } else {
        let object_store = slatedb::admin::load_object_store_from_env(None)
            .map_err(|e| anyhow::anyhow!("failed to load object store from env: {e}"))?;
        let provider =
            std::env::var("CLOUD_PROVIDER").unwrap_or_else(|_| "unknown".to_string());
        let roots = if args.root.is_empty() {
            vec![String::new()]
        } else {
            args.root.clone()
        };
        Ok(vec![Store {
            name: "default".to_string(),
            provider,
            object_store,
            roots,
        }])
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

async fn serve(args: ServeArgs, stores: Vec<Store>) -> anyhow::Result<()> {
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
        let registry = Arc::new(Registry::new(
            stores,
            ScanLimits {
                max_depth: args.scan_depth,
                ..ScanLimits::default()
            },
            Duration::from_secs(args.scan_ttl_secs),
            Duration::from_secs(args.cache_ttl_secs),
        ));

        // Startup scan: informational only — DBs may appear later.
        match registry.scan(false).await {
            Ok((_, dbs)) if dbs.is_empty() => {
                warn!("no SlateDBs discovered — are the roots right? (rescans every {}s)", args.scan_ttl_secs)
            }
            Ok((_, dbs)) => {
                let ids: Vec<&str> = dbs.iter().map(|d| d.id.as_str()).collect();
                info!("discovered {} database(s): {}", dbs.len(), ids.join(", "))
            }
            Err(e) => warn!("initial discovery failed: {e}"),
        }

        let mut app = routes::root_router(registry);
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
