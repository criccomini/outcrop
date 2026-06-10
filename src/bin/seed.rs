//! Seeds a throwaway local-filesystem SlateDB so the dashboard has real data
//! to show. This tool WRITES to the demo database — the dashboard itself
//! never does.
//!
//! Two modes:
//! - default: one-shot seed of a deterministic LSM shape (waves of writes,
//!   one explicit compaction pass, checkpoints).
//! - `--traffic`: run forever, simulating live traffic at a slowly varying
//!   rate with the embedded compactor and GC enabled, so the dashboard can
//!   be watched while the DB evolves. Works on a fresh dir or on top of an
//!   existing demo DB.

use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use slatedb::admin::AdminBuilder;
use slatedb::config::{CheckpointOptions, PutOptions, Settings, WriteOptions};
use slatedb::object_store::local::LocalFileSystem;
use slatedb::object_store::ObjectStore;
use slatedb::Db;
use tokio_util::sync::CancellationToken;

#[derive(Parser, Debug)]
#[command(about = "Generate demo data for slatedb-dashboard")]
struct Args {
    /// Directory backing the local object store
    #[arg(long, default_value = "./demo-data")]
    dir: String,

    /// DB root path within the object store
    #[arg(long, default_value = "demo-db")]
    path: String,

    /// Number of write waves; each wave ends with a flush (≈ one L0 SST)
    #[arg(long, default_value_t = 12)]
    waves: usize,

    /// Keys written per wave
    #[arg(long, default_value_t = 3000)]
    keys_per_wave: usize,

    /// Seconds to run the compactor between the write phases (0 to skip)
    #[arg(long, default_value_t = 10)]
    compact_secs: u64,

    /// Seed on top of an existing demo DB instead of refusing
    #[arg(long)]
    force: bool,

    /// Run forever, simulating live traffic until Ctrl-C
    #[arg(long)]
    traffic: bool,

    /// Average operations per second in traffic mode; the actual rate
    /// swings between 20% and 100% of this on a slow cycle
    #[arg(long, default_value_t = 150)]
    rate: u64,

    /// Seconds between short-lived checkpoints in traffic mode (0 disables)
    #[arg(long, default_value_t = 120)]
    checkpoint_secs: u64,
}

// Deterministic pseudo-random stream so runs are reproducible.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 16
    }
}

async fn open_db(args: &Args, store: Arc<dyn ObjectStore>) -> anyhow::Result<Db> {
    let settings = Settings {
        // Small L0 SSTs so a modest amount of data produces a real tree.
        l0_sst_size_bytes: 128 * 1024,
        // Filter every SST regardless of key count so the dashboard's SST
        // drawer has bloom filters to show.
        min_filter_keys: 0,
        // The default Db runs an embedded compactor and GC. Disable both:
        // compaction here is run explicitly between the write phases, so the
        // final LSM shape (fresh L0 SSTs on top of sorted runs) is
        // deterministic instead of racing the embedded compactor, and
        // close() doesn't have to wind down mid-flight background work.
        compactor_options: None,
        garbage_collector_options: None,
        // With no compactor draining L0, the default cap (l0_max_ssts: 8)
        // would stall flushes — including the final flush inside close() —
        // as soon as L0 fills, hanging the seeder. Lift it well above what
        // the write phases can produce.
        l0_max_ssts: 256,
        l0_max_ssts_per_key: 256,
        ..Settings::default()
    };
    Ok(Db::builder(args.path.clone(), store)
        .with_settings(settings)
        .build()
        .await?)
}

async fn open_traffic_db(args: &Args, store: Arc<dyn ObjectStore>) -> anyhow::Result<Db> {
    let settings = Settings {
        // Small L0 SSTs so the tree visibly evolves within seconds, and
        // bloom filters on every SST. Unlike the one-shot seed, keep the
        // embedded compactor and GC at their defaults: watching them work
        // (GC sweeps every minute, deleting objects older than 5 minutes)
        // is the point of traffic mode.
        l0_sst_size_bytes: 128 * 1024,
        min_filter_keys: 0,
        // One WAL SST per second instead of ten per second; keeps the WAL
        // listing (and the dashboard's WAL page) a few hundred entries at
        // GC steady state.
        flush_interval: Some(Duration::from_secs(1)),
        ..Settings::default()
    };
    Ok(Db::builder(args.path.clone(), store)
        .with_settings(settings)
        .build()
        .await?)
}

async fn write_waves(
    db: &Db,
    waves: std::ops::Range<usize>,
    args: &Args,
    rng: &mut Lcg,
    written: &mut u64,
    deleted: &mut u64,
) -> anyhow::Result<()> {
    let key_space = (args.waves * args.keys_per_wave) as u64;
    // Don't await durability per write — the flush at the end of each wave
    // makes everything durable, and serial durable puts would crawl at the
    // WAL flush interval.
    let write_opts = WriteOptions {
        await_durable: false,
        ..Default::default()
    };
    for wave in waves {
        for _ in 0..args.keys_per_wave {
            let k = rng.next() % key_space;
            let key = format!("user:{k:08}");
            if rng.next() % 10 == 0 && *written > 0 {
                db.delete_with_options(&key, &write_opts).await?;
                *deleted += 1;
            } else {
                let len = 64 + (rng.next() % 512) as usize;
                let value = format!("v{wave}:{key}:")
                    .into_bytes()
                    .into_iter()
                    .cycle()
                    .take(len)
                    .collect::<Vec<u8>>();
                db.put_with_options(&key, &value, &PutOptions::default(), &write_opts)
                    .await?;
                *written += 1;
            }
        }
        db.flush().await?;
        println!("  wave {}/{} flushed", wave + 1, args.waves);
    }
    Ok(())
}

async fn create_checkpoint(
    args: &Args,
    store: Arc<dyn ObjectStore>,
    name: &str,
    lifetime: Option<Duration>,
) -> anyhow::Result<()> {
    let admin = AdminBuilder::new(args.path.clone(), store).build();
    let result = admin
        .create_detached_checkpoint(&CheckpointOptions {
            lifetime,
            source: None,
            name: Some(name.to_string()),
        })
        .await?;
    println!("created checkpoint '{name}' ({})", result.id);
    Ok(())
}

/// Write puts/deletes forever at a slowly swinging rate, with short-lived
/// checkpoints so expiry/GC dynamics stay visible. Returns on Ctrl-C.
async fn run_traffic(args: &Args, store: Arc<dyn ObjectStore>) -> anyhow::Result<()> {
    let db = open_traffic_db(args, store.clone()).await?;
    // Churn the keyspace the one-shot seed uses, extend past it on inserts.
    let mut key_high = (args.waves * args.keys_per_wave) as u64;
    let mut rng = Lcg(0xC0FFEE);
    let write_opts = WriteOptions {
        await_durable: false,
        ..Default::default()
    };

    println!(
        "simulating ~{} ops/s against {}/{} (rate swings 20–100% on a 4m cycle) — Ctrl-C to stop",
        args.rate, args.dir, args.path
    );

    let started = Instant::now();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let mut puts = 0u64;
    let mut deletes = 0u64;
    let mut ops_since_report = 0u64;
    let mut last_report = Instant::now();
    let mut last_checkpoint = Instant::now();
    let mut checkpoint_n = 0u64;
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            _ = tick.tick() => {}
        }

        // Sine-modulated rate, full cycle every 4 minutes, floor at 20%.
        let phase = started.elapsed().as_secs_f64() / 240.0 * std::f64::consts::TAU;
        let mult = 0.2 + 0.8 * (0.5 * (1.0 + phase.sin()));
        let ops = ((args.rate as f64) * mult / 10.0).round() as u64;

        for _ in 0..ops {
            let roll = rng.next() % 10;
            if roll == 0 && puts > 0 {
                // 10% deletes of a random existing key.
                let k = rng.next() % key_high;
                db.delete_with_options(format!("user:{k:08}"), &write_opts)
                    .await?;
                deletes += 1;
            } else {
                // 20% inserts of fresh keys, 70% updates of existing ones.
                let k = if roll <= 2 {
                    key_high += 1;
                    key_high - 1
                } else {
                    rng.next() % key_high
                };
                let key = format!("user:{k:08}");
                let len = 64 + (rng.next() % 512) as usize;
                let value = format!("t:{key}:")
                    .into_bytes()
                    .into_iter()
                    .cycle()
                    .take(len)
                    .collect::<Vec<u8>>();
                db.put_with_options(&key, &value, &PutOptions::default(), &write_opts)
                    .await?;
                puts += 1;
            }
        }
        ops_since_report += ops;

        if args.checkpoint_secs > 0
            && last_checkpoint.elapsed() >= Duration::from_secs(args.checkpoint_secs)
        {
            last_checkpoint = Instant::now();
            checkpoint_n += 1;
            let name = format!("traffic-{checkpoint_n}");
            // Best-effort: a manifest CAS race with the writer or compactor
            // shouldn't kill the simulation.
            if let Err(e) =
                create_checkpoint(args, store.clone(), &name, Some(Duration::from_secs(300)))
                    .await
            {
                println!("checkpoint '{name}' failed (continuing): {e}");
            }
        }

        if last_report.elapsed() >= Duration::from_secs(10) {
            let actual = ops_since_report as f64 / last_report.elapsed().as_secs_f64();
            println!(
                "[t+{:>4}s] {actual:.0} ops/s (target {:.0}) · {puts} puts · {deletes} deletes · keyspace {key_high}",
                started.elapsed().as_secs(),
                args.rate as f64 * mult,
            );
            last_report = Instant::now();
            ops_since_report = 0;
        }
    }

    println!("shutting down (flushing) ...");
    db.close().await?;
    println!("wrote {puts} puts, {deletes} deletes");
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Surface slatedb's internal logs (flush/close/GC progress); raise with
    // e.g. RUST_LOG=slatedb=debug when diagnosing.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "slatedb=info".into()),
        )
        .init();

    let args = Args::parse();

    let db_dir = std::path::Path::new(&args.dir).join(&args.path);
    // Traffic mode is additive by design — it runs on a fresh dir or on top
    // of an existing demo DB.
    if !args.traffic && db_dir.exists() && !args.force {
        anyhow::bail!(
            "{} already contains a demo DB; remove it first (rm -rf {}) or pass --force to seed on top of it",
            db_dir.display(),
            args.dir
        );
    }

    std::fs::create_dir_all(&args.dir)?;
    let store: Arc<dyn ObjectStore> = Arc::new(LocalFileSystem::new_with_prefix(&args.dir)?);

    if args.traffic {
        return run_traffic(&args, store).await;
    }

    let mut rng = Lcg(42);
    let mut written: u64 = 0;
    let mut deleted: u64 = 0;
    let phase1_waves = (args.waves * 2) / 3;

    println!(
        "seeding {} waves x {} keys into {}/{} ...",
        args.waves, args.keys_per_wave, args.dir, args.path
    );

    // Phase 1: bulk of the data, then a named checkpoint.
    let db = open_db(&args, store.clone()).await?;
    write_waves(&db, 0..phase1_waves, &args, &mut rng, &mut written, &mut deleted).await?;
    db.close().await?;
    create_checkpoint(
        &args,
        store.clone(),
        "demo-midway",
        Some(Duration::from_secs(7 * 24 * 3600)),
    )
    .await?;

    // Phase 2: compact phase-1 L0 into sorted runs. Run between the write
    // phases so phase 3's L0 SSTs survive; the writer is closed first since
    // a second compactor would fence an open Db's embedded one (and with it
    // the whole Db handle).
    if args.compact_secs > 0 {
        println!("running compactor for {}s ...", args.compact_secs);
        let admin = AdminBuilder::new(args.path.clone(), store.clone()).build();
        let token = CancellationToken::new();
        let stop = token.clone();
        let handle = tokio::spawn(async move { admin.run_compactor(token).await });
        tokio::time::sleep(Duration::from_secs(args.compact_secs)).await;
        stop.cancel();
        match handle.await? {
            Ok(()) => println!("compactor finished"),
            Err(e) => println!("compactor exited with: {e}"),
        }
    }

    // Phase 3: reopen (bumps writer epoch) and leave fresh L0 SSTs stacked
    // on top of the sorted runs.
    let db = open_db(&args, store.clone()).await?;
    write_waves(
        &db,
        phase1_waves..args.waves,
        &args,
        &mut rng,
        &mut written,
        &mut deleted,
    )
    .await?;
    db.close().await?;
    println!("wrote {written} puts, {deleted} deletes");

    create_checkpoint(&args, store.clone(), "demo-final", None).await?;

    println!("done. start the dashboard with:");
    println!(
        "  CLOUD_PROVIDER=local LOCAL_PATH={} cargo run -- --path {}",
        args.dir, args.path
    );
    Ok(())
}
