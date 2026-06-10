//! Seeds a throwaway local-filesystem SlateDB so the dashboard has real data
//! to show. This tool WRITES to the demo database — the dashboard itself
//! never does.

use std::sync::Arc;
use std::time::Duration;

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    std::fs::create_dir_all(&args.dir)?;
    let store: Arc<dyn ObjectStore> = Arc::new(LocalFileSystem::new_with_prefix(&args.dir)?);

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

    // Phase 2: compact phase-1 L0 into sorted runs. The writer must be
    // closed first — an in-process compactor fences an open writer.
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
