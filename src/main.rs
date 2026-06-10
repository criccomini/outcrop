mod assets;
mod cache;
mod convert;
mod diff;
mod dto;
mod error;
mod routes;
mod state;

use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use crate::state::AppState;

/// Read-only web dashboard for SlateDB.
///
/// The object store is configured through environment variables (or an env
/// file), exactly like slatedb-cli: set CLOUD_PROVIDER to local | memory |
/// aws | azure | opendal plus the provider-specific variables (e.g.
/// LOCAL_PATH for local).
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Path to the database root within the object store
    #[arg(short, long)]
    path: String,

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "slatedb_dashboard=info,tower_http=info".into()),
        )
        .init();

    let args = Args::parse();

    let object_store = slatedb::admin::load_object_store_from_env(args.env_file.clone())
        .map_err(|e| anyhow::anyhow!("failed to load object store from env: {e}"))?;
    let provider = std::env::var("CLOUD_PROVIDER").unwrap_or_else(|_| "unknown".to_string());

    let state = Arc::new(AppState::new(
        args.path.clone(),
        provider,
        object_store,
        Duration::from_secs(args.cache_ttl_secs),
    ));

    // Startup probe: warn loudly if there's no DB here, but start anyway —
    // the DB may be created later.
    match state.admin.read_manifest(None).await {
        Ok(Some(m)) => info!(
            "found SlateDB at '{}' (manifest id {})",
            args.path,
            m.id()
        ),
        Ok(None) => warn!(
            "no manifest found at '{}' — is this a SlateDB root?",
            args.path
        ),
        Err(e) => warn!("could not read manifest at '{}': {e}", args.path),
    }

    let app = routes::api_router(state)
        .fallback(assets::static_handler)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&args.listen).await?;
    info!("slatedb-dashboard listening on http://{}", args.listen);
    axum::serve(listener, app).await?;
    Ok(())
}
