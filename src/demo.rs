//! Demo traffic tool: the only code path in this crate that WRITES — the
//! dashboard itself never does.
//!
//! `traffic` seeds a deterministic base LSM shape into a local-filesystem
//! SlateDB when the demo DB doesn't exist yet (waves of writes, one explicit
//! compaction pass, checkpoints), then simulates live traffic forever at a
//! slowly varying rate with the embedded compactor and GC enabled, so the
//! dashboard can be watched while the DB evolves.
//!
//! `--target-size` switches seeding to bulk mode: batched unthrottled
//! writes with the embedded compactor + GC running concurrently, looping
//! until the manifest's live bytes reach the target. Bulk seeding is
//! resumable — progress is measured from the store, so re-running (with the
//! same flags) tops the DB up rather than starting over.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use slatedb::admin::{Admin, AdminBuilder};
use slatedb::config::{
    CheckpointOptions, CompactorOptions, FlushOptions, FlushType, GarbageCollectorDirectoryOptions,
    GarbageCollectorOptions, PutOptions, Settings, WriteOptions,
};
use slatedb::manifest::{SortedRun, SsTableView};
use slatedb::object_store::local::LocalFileSystem;
use slatedb::object_store::ObjectStore;
use slatedb::prefix_extractor::{PrefixExtractor, PrefixTarget};
use slatedb::{Db, WriteBatch};
use tokio_util::sync::CancellationToken;

#[derive(clap::Args, Debug)]
pub struct TrafficArgs {
    /// Directory backing the local object store
    #[arg(long, default_value = "./demo-data")]
    dir: String,

    /// Base DB root path; DBs are named {path}-1..N (just {path} when
    /// --dbs 1)
    #[arg(long, default_value = "demo-db")]
    path: String,

    /// How many DBs to seed and churn concurrently [default: 3, or 1 when
    /// --target-size is set]
    #[arg(long)]
    dbs: Option<usize>,

    /// Seed waves when creating a fresh demo DB; each wave ends with a
    /// flush (≈ one L0 SST)
    #[arg(long, default_value_t = 12)]
    waves: usize,

    /// Keys written per seed wave
    #[arg(long, default_value_t = 3000)]
    keys_per_wave: usize,

    /// Seconds to run the compactor between the seed write phases (0 to skip)
    #[arg(long, default_value_t = 10)]
    compact_secs: u64,

    /// Average operations per second for the busiest DB; the others run at
    /// a fraction of this, and every rate swings 20–100% on a slow cycle
    #[arg(long, default_value_t = 150)]
    rate: u64,

    /// Seconds between short-lived checkpoints (0 disables)
    #[arg(long, default_value_t = 120)]
    checkpoint_secs: u64,

    /// Delete the entire --dir first, so the demo starts from scratch
    #[arg(long)]
    clean: bool,

    /// Partition keys into N segments (RFC-0024) via a first-'/' prefix
    /// extractor; keys become "t{seg}/user:{id}". Unset: each DB decides
    /// randomly (but stably, by name) whether it's segmented, so the fleet
    /// shows both shapes. 0 forces unsegmented everywhere. The extractor is
    /// fixed at DB creation, so changes need fresh DBs — pair with --clean
    #[arg(long)]
    segments: Option<usize>,

    /// Seed each DB up to this much live data (e.g. 50GiB; binary units,
    /// KB == KiB) instead of the small demo shape. Bulk seeding writes
    /// unthrottled batches with the embedded compactor + GC running, and
    /// resumes: re-running with the same flags tops the DB up to the target
    #[arg(long, value_parser = parse_size)]
    target_size: Option<u64>,

    /// Value size, fixed ("512") or a range ("4KiB..64KiB"). Default:
    /// 64..575 for the demo shape and the traffic phase, 4KiB..64KiB for
    /// bulk seeding
    #[arg(long, value_parser = parse_byte_range)]
    value_bytes: Option<ByteRange>,

    /// Target SST size: the L0 SST size while seeding and the compactor's
    /// max output SST size. target-size / sst-bytes ≈ final SST count.
    /// Default: 128KiB for the demo shape, 32MiB for bulk seeding
    #[arg(long, value_parser = parse_size)]
    sst_bytes: Option<u64>,

    /// Exit after seeding instead of simulating live traffic
    #[arg(long)]
    seed_only: bool,

    /// Seed without a WAL, halving bytes written (seeding only — the
    /// traffic phase keeps its WAL so the dashboard's WAL page has data)
    #[arg(long)]
    no_wal: bool,

    /// Bulk seeding: pause writing whenever non-live bytes in the store
    /// exceed this budget, and GC until they drain (the compactor's
    /// internal 15-minute checkpoints pin replaced SSTs, so full-speed
    /// seeding otherwise accumulates transient garbage far past the
    /// target). Peak disk per DB ≈ target-size + this. Bigger = faster
    #[arg(long, value_parser = parse_size, default_value = "32GiB")]
    max_garbage: u64,
}

/// Size like "1024", "128KiB" or "50gb" in bytes. Units are binary
/// (KB == KiB); the "i" and "B" suffix parts are optional, any case.
fn parse_size(s: &str) -> Result<u64, String> {
    let t = s.trim();
    let digits = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let (num, unit) = t.split_at(digits);
    let n: u64 = num.parse().map_err(|_| format!("invalid size '{s}'"))?;
    let unit = unit.trim().to_ascii_lowercase();
    let mult: u64 = match unit.trim_end_matches('b').trim_end_matches('i') {
        "" => 1,
        "k" => 1 << 10,
        "m" => 1 << 20,
        "g" => 1 << 30,
        "t" => 1 << 40,
        _ => return Err(format!("invalid unit in size '{s}'")),
    };
    n.checked_mul(mult)
        .ok_or_else(|| format!("size '{s}' overflows u64"))
}

/// Inclusive byte-size range, parsed from "512" or "4KiB..64KiB".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ByteRange {
    min: u64,
    max: u64,
}

impl ByteRange {
    fn sample(&self, rng: &mut Lcg) -> usize {
        (self.min + rng.next() % (self.max - self.min + 1)) as usize
    }
}

fn parse_byte_range(s: &str) -> Result<ByteRange, String> {
    let (lo, hi) = match s.split_once("..") {
        Some((lo, hi)) => (lo, hi),
        None => (s, s),
    };
    let (min, max) = (parse_size(lo)?, parse_size(hi)?);
    if min == 0 {
        return Err("value size must be at least 1 byte".to_string());
    }
    if max < min {
        return Err(format!("invalid range '{s}': max below min"));
    }
    Ok(ByteRange { min, max })
}

/// The original demo distribution (64 + r % 512).
const DEMO_VALUES: ByteRange = ByteRange { min: 64, max: 575 };
const BULK_VALUES: ByteRange = ByteRange {
    min: 4 << 10,
    max: 64 << 10,
};
const DEMO_SST_BYTES: u64 = 128 * 1024;
const BULK_SST_BYTES: u64 = 32 << 20;

/// Per-DB write-shape parameters, derived once from the flags. Every run
/// against the same DB must use the same flags: the key space and widths
/// are not persisted, so changing them mid-DB writes a disjoint key set.
struct Shape {
    segments: usize,
    /// Distinct logical key ids the seeder draws from.
    key_space: u64,
    /// Zero-pad width of the numeric key part; floored at 8 so demo DBs
    /// keep their historical "user:00000042" format.
    key_width: usize,
    /// Power-of-two-minus-one domain for the churn loop's insert
    /// bijection; covers the seeded key space with room above it.
    scatter_mask: u64,
    /// Seed-phase value sizes (the traffic phase resolves its own).
    values: ByteRange,
    sst_bytes: u64,
}

fn shape_for(args: &TrafficArgs, db_path: &str) -> Shape {
    let bulk = args.target_size.is_some();
    let values = args
        .value_bytes
        .unwrap_or(if bulk { BULK_VALUES } else { DEMO_VALUES });
    let sst_bytes = args
        .sst_bytes
        .unwrap_or(if bulk { BULK_SST_BYTES } else { DEMO_SST_BYTES });
    let key_space = match args.target_size {
        // 2× headroom over the keys the target needs, so random draws keep
        // finding fresh keys and live size can actually reach the target.
        Some(target) => {
            let avg = ((values.min + values.max) / 2).max(1);
            (target / avg).saturating_mul(2).max(1)
        }
        None => ((args.waves * args.keys_per_wave) as u64).max(1),
    };
    let key_width = (key_space.saturating_sub(1).max(1).ilog10() as usize + 1).max(8);
    let scatter_mask = key_space
        .saturating_mul(2)
        .checked_next_power_of_two()
        .unwrap_or(u64::MAX)
        .max(1 << 26)
        - 1;
    Shape {
        segments: db_segments(args, db_path),
        key_space,
        key_width,
        scatter_mask,
        values,
        sst_bytes,
    }
}

/// Segment count for one DB: the explicit flag, or a stable pseudo-random
/// per-name choice (~half unsegmented, the rest 2–6 segments). Stability
/// across restarts matters because a DB's extractor is fixed at creation
/// and every reopen must make the same choice.
fn db_segments(args: &TrafficArgs, db_path: &str) -> usize {
    if let Some(n) = args.segments {
        return n;
    }
    let h = db_path
        .bytes()
        .fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ b as u64).wrapping_mul(0x100000001b3)
        });
    if h % 2 == 0 {
        0
    } else {
        2 + ((h >> 8) % 5) as usize
    }
}

/// Segments keys at the first '/': "t03/user:00000042" → segment "t03/".
/// First-delimiter extraction is extension-safe, so Point and Prefix
/// targets answer identically; keys without a '/' have no segment.
struct FirstSlashExtractor;

impl PrefixExtractor for FirstSlashExtractor {
    fn name(&self) -> &str {
        "first-slash"
    }

    fn prefix_len(&self, target: &PrefixTarget) -> Option<usize> {
        let bytes = match target {
            PrefixTarget::Point(b) | PrefixTarget::Prefix(b) => b,
        };
        bytes.iter().position(|&c| c == b'/').map(|i| i + 1)
    }
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

/// Key for logical id `k`: plain, or segment-prefixed when --segments is on.
fn key_for(segments: usize, width: usize, k: u64) -> String {
    if segments == 0 {
        format!("user:{k:0width$}")
    } else {
        format!("t{:02}/user:{k:0width$}", k % segments as u64)
    }
}

fn apply_segments(builder: slatedb::DbBuilder<String>, segments: usize) -> slatedb::DbBuilder<String> {
    if segments == 0 {
        builder
    } else {
        builder.with_segment_extractor(Arc::new(FirstSlashExtractor))
    }
}

async fn open_seed_db(
    path: &str,
    shape: &Shape,
    no_wal: bool,
    store: Arc<dyn ObjectStore>,
) -> anyhow::Result<Db> {
    let settings = Settings {
        // Small L0 SSTs so a modest amount of data produces a real tree.
        l0_sst_size_bytes: shape.sst_bytes as usize,
        wal_enabled: !no_wal,
        // Filter every SST regardless of key count so the dashboard's SST
        // drawer has bloom filters to show.
        min_filter_keys: 0,
        // The default Db runs an embedded compactor and GC. Disable both:
        // seed compaction is run explicitly between the write phases, so the
        // base LSM shape (fresh L0 SSTs on top of sorted runs) is
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
    let builder = Db::builder(path.to_string(), store).with_settings(settings);
    Ok(apply_segments(builder, shape.segments).build().await?)
}

/// Bulk-seed handle: real-ish SST sizes, and — unlike the demo seed — the
/// embedded compactor and GC run concurrently. The l0_max_ssts cap doubles
/// as backpressure: when the compactor falls behind, flushes (and with
/// them the writer) stall until it catches up. It's raised well above the
/// default 8, and the compactor polls fast, because with the default
/// window the writer spends most of its time stalled waiting for the next
/// 5-second compactor poll.
async fn open_bulk_db(
    path: &str,
    shape: &Shape,
    no_wal: bool,
    store: Arc<dyn ObjectStore>,
) -> anyhow::Result<Db> {
    let settings = Settings {
        l0_sst_size_bytes: shape.sst_bytes as usize,
        min_filter_keys: 0,
        flush_interval: Some(Duration::from_secs(1)),
        wal_enabled: !no_wal,
        l0_max_ssts: 64,
        l0_max_ssts_per_key: 64,
        compactor_options: Some(CompactorOptions {
            poll_interval: Duration::from_millis(500),
            max_sst_size: shape.sst_bytes as usize,
            ..CompactorOptions::default()
        }),
        ..Settings::default()
    };
    let builder = Db::builder(path.to_string(), store).with_settings(settings);
    Ok(apply_segments(builder, shape.segments).build().await?)
}

async fn open_traffic_db(
    path: &str,
    segments: usize,
    sst_bytes: Option<u64>,
    store: Arc<dyn ObjectStore>,
) -> anyhow::Result<Db> {
    let settings = Settings {
        // Small L0 SSTs so the tree visibly evolves within seconds, and
        // bloom filters on every SST. Unlike the seed phase, keep the
        // embedded compactor and GC at their defaults: watching them work
        // (GC sweeps every minute, deleting objects older than 5 minutes)
        // is the point of traffic mode.
        l0_sst_size_bytes: 128 * 1024,
        min_filter_keys: 0,
        // One WAL SST per second instead of ten per second; keeps the WAL
        // listing (and the dashboard's WAL page) a few hundred entries at
        // GC steady state.
        flush_interval: Some(Duration::from_secs(1)),
        // Honor --sst-bytes so churn doesn't re-merge a bulk-seeded DB's
        // many SSTs into default-sized (256MiB) ones.
        compactor_options: Some(CompactorOptions {
            max_sst_size: sst_bytes
                .map(|b| b as usize)
                .unwrap_or_else(|| CompactorOptions::default().max_sst_size),
            ..CompactorOptions::default()
        }),
        ..Settings::default()
    };
    let builder = Db::builder(path.to_string(), store).with_settings(settings);
    Ok(apply_segments(builder, segments).build().await?)
}

async fn write_waves(
    db: &Db,
    tag: &str,
    waves: std::ops::Range<usize>,
    shape: &Shape,
    args: &TrafficArgs,
    rng: &mut Lcg,
    written: &mut u64,
    deleted: &mut u64,
) -> anyhow::Result<()> {
    // Don't await durability per write — the flush at the end of each wave
    // makes everything durable, and serial durable puts would crawl at the
    // WAL flush interval.
    let write_opts = WriteOptions {
        await_durable: false,
        ..Default::default()
    };
    for wave in waves {
        for _ in 0..args.keys_per_wave {
            let k = rng.next() % shape.key_space;
            let key = key_for(shape.segments, shape.key_width, k);
            if rng.next() % 10 == 0 && *written > 0 {
                db.delete_with_options(&key, &write_opts).await?;
                *deleted += 1;
            } else {
                let len = shape.values.sample(rng);
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
        println!("[{tag}] wave {}/{} flushed", wave + 1, args.waves);
    }
    Ok(())
}

async fn create_checkpoint(
    path: &str,
    store: Arc<dyn ObjectStore>,
    name: &str,
    lifetime: Option<Duration>,
) -> anyhow::Result<()> {
    let admin = AdminBuilder::new(path.to_string(), store).build();
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

/// One-shot deterministic base: two write phases around an explicit
/// compaction pass, with named checkpoints.
async fn seed_base(
    args: &TrafficArgs,
    db_path: &str,
    shape: &Shape,
    store: Arc<dyn ObjectStore>,
) -> anyhow::Result<()> {
    let mut rng = Lcg(42);
    let mut written: u64 = 0;
    let mut deleted: u64 = 0;
    let phase1_waves = (args.waves * 2) / 3;

    println!(
        "[{db_path}] seeding {} waves x {} keys into {}/{db_path} ...",
        args.waves, args.keys_per_wave, args.dir
    );

    // Phase 1: bulk of the data, then a named checkpoint.
    let db = open_seed_db(db_path, shape, args.no_wal, store.clone()).await?;
    write_waves(&db, db_path, 0..phase1_waves, shape, args, &mut rng, &mut written, &mut deleted)
        .await?;
    db.close().await?;
    create_checkpoint(
        db_path,
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
        println!("[{db_path}] running compactor for {}s ...", args.compact_secs);
        let admin = AdminBuilder::new(db_path.to_string(), store.clone()).build();
        let token = CancellationToken::new();
        let stop = token.clone();
        let handle = tokio::spawn(async move { admin.run_compactor(token).await });
        tokio::time::sleep(Duration::from_secs(args.compact_secs)).await;
        stop.cancel();
        match handle.await? {
            Ok(()) => println!("[{db_path}] compactor finished"),
            Err(e) => println!("[{db_path}] compactor exited with: {e}"),
        }
    }

    // Phase 3: reopen (bumps writer epoch) and leave fresh L0 SSTs stacked
    // on top of the sorted runs.
    let db = open_seed_db(db_path, shape, args.no_wal, store.clone()).await?;
    write_waves(
        &db,
        db_path,
        phase1_waves..args.waves,
        shape,
        args,
        &mut rng,
        &mut written,
        &mut deleted,
    )
    .await?;
    db.close().await?;
    println!("[{db_path}] seeded {written} puts, {deleted} deletes");

    create_checkpoint(db_path, store.clone(), "demo-final", None).await?;
    Ok(())
}

/// Bytes written between live-size re-measurements and progress lines.
const BULK_TRANCHE_BYTES: u64 = 256 << 20;
/// WriteBatch granularity: large enough to amortize the write channel,
/// small enough to stay responsive to Ctrl-C and backpressure.
const BULK_BATCH_BYTES: usize = 4 << 20;
const BULK_BATCH_MAX_ENTRIES: usize = 4096;

/// Live data bytes per the latest manifest: every L0 and sorted-run view
/// across the root tree and all segments. This is what bulk seeding
/// measures progress against — garbage awaiting GC doesn't inflate it, so
/// resuming after an interruption (or a re-run with a higher target) tops
/// the DB up correctly.
async fn live_bytes(admin: &Admin) -> anyhow::Result<u64> {
    let Some(m) = admin.read_manifest(None).await? else {
        return Ok(0);
    };
    let mut total = 0u64;
    let mut add = |l0: &VecDeque<SsTableView>, runs: &[SortedRun]| {
        total += l0.iter().map(|v| v.estimate_size()).sum::<u64>();
        total += runs.iter().map(|r| r.estimate_size()).sum::<u64>();
    };
    add(m.l0(), m.compacted());
    for seg in m.segments() {
        add(seg.l0(), seg.compacted());
    }
    Ok(total)
}

/// Total bytes stored under the DB's prefix — live data plus WAL plus
/// not-yet-collected garbage. One LIST; cheap on the local stores bulk
/// seeding targets.
async fn stored_bytes(store: &dyn ObjectStore, db_path: &str) -> anyhow::Result<u64> {
    use futures::TryStreamExt;
    let prefix = slatedb::object_store::path::Path::from(db_path);
    let mut stream = store.list(Some(&prefix));
    let mut total = 0u64;
    while let Some(meta) = stream.try_next().await? {
        total += meta.size;
    }
    Ok(total)
}

fn fmt_bytes(b: u64) -> String {
    if b < 1024 {
        return format!("{b}B");
    }
    for (name, size) in [
        ("TiB", 1u64 << 40),
        ("GiB", 1 << 30),
        ("MiB", 1 << 20),
        ("KiB", 1 << 10),
    ] {
        if b >= size {
            return format!("{:.1}{name}", b as f64 / size as f64);
        }
    }
    unreachable!()
}

/// Bulk value: tagged with the key for provenance in the SST drawer, then
/// LCG-filled so sizes stay honest even if compression is ever enabled.
fn bulk_value(rng: &mut Lcg, key: &str, len: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(len + 8);
    v.extend_from_slice(format!("t:{key}:").as_bytes());
    v.truncate(len);
    while v.len() < len {
        v.extend_from_slice(&rng.next().to_le_bytes());
    }
    v.truncate(len);
    v
}

/// Unthrottled batched writes until the manifest's live bytes reach
/// `target`, with the embedded compactor and GC keeping pace. Put-only:
/// the traffic phase adds deletes/tombstones over time.
async fn bulk_seed(
    args: &TrafficArgs,
    db_path: &str,
    shape: &Shape,
    target: u64,
    store: Arc<dyn ObjectStore>,
    token: &CancellationToken,
) -> anyhow::Result<()> {
    let admin = AdminBuilder::new(db_path.to_string(), store.clone()).build();
    let mut live = live_bytes(&admin).await?;
    if live >= target {
        println!(
            "[{db_path}] bulk target already met ({} live ≥ {})",
            fmt_bytes(live),
            fmt_bytes(target)
        );
        // Still worth a pass: seed garbage pinned by the compactor's
        // 15-minute internal checkpoints becomes collectible on re-runs.
        println!("[{db_path}] collecting leftover seed garbage (one GC pass) ...");
        collect_seed_garbage(db_path, store).await?;
        return Ok(());
    }
    println!(
        "[{db_path}] bulk-seeding to {} (values {}..{}, SSTs ~{}, {} keys, wal {}) — \
         compaction and the WAL need transient extra disk until GC catches up",
        fmt_bytes(target),
        fmt_bytes(shape.values.min),
        fmt_bytes(shape.values.max),
        fmt_bytes(shape.sst_bytes),
        shape.key_space,
        if args.no_wal { "off" } else { "on" },
    );

    let db = open_bulk_db(db_path, shape, args.no_wal, store.clone()).await?;
    let write_opts = WriteOptions {
        await_durable: false,
        ..Default::default()
    };
    let mut rng = Lcg(42);
    let started = Instant::now();
    let mut written = 0u64;
    let mut interrupted = false;
    'seeding: while live < target {
        let tranche_cap = BULK_TRANCHE_BYTES.min(target - live);
        let mut tranche = 0u64;
        while tranche < tranche_cap {
            if token.is_cancelled() {
                interrupted = true;
                break 'seeding;
            }
            let mut batch = WriteBatch::new();
            let mut batch_bytes = 0usize;
            let mut entries = 0usize;
            while batch_bytes < BULK_BATCH_BYTES && entries < BULK_BATCH_MAX_ENTRIES {
                let k = rng.next() % shape.key_space;
                let key = key_for(shape.segments, shape.key_width, k);
                let len = shape.values.sample(&mut rng);
                let value = bulk_value(&mut rng, &key, len);
                batch_bytes += key.len() + value.len();
                batch.put(key, value);
                entries += 1;
            }
            db.write_with_options(batch, &write_opts).await?;
            tranche += batch_bytes as u64;
        }
        written += tranche;
        // Flush the MEMTABLE, not the WAL: live size is measured from the
        // manifest, which only advances when memtables land in L0. With
        // the default WAL flush, up to max_unflushed_bytes of accepted
        // writes would be invisible to the measurement and the loop would
        // overshoot the target by that much.
        db.flush_with_options(FlushOptions {
            flush_type: FlushType::MemTable,
        })
        .await?;
        live = live_bytes(&admin).await?;
        let garbage = stored_bytes(store.as_ref(), db_path)
            .await?
            .saturating_sub(live);
        let mb_s = written as f64 / 1e6 / started.elapsed().as_secs_f64().max(0.001);
        println!(
            "[{db_path}] bulk: {} / {} live · {} written · {} garbage · {mb_s:.0} MB/s",
            fmt_bytes(live),
            fmt_bytes(target),
            fmt_bytes(written),
            fmt_bytes(garbage),
        );

        // Chunked seeding: replaced SSTs stay pinned by the compactor's
        // internal 15-minute checkpoints, so an unthrottled seed
        // accumulates transient garbage at the compaction-rewrite rate —
        // several times the target. Once the budget is exceeded, stop
        // writing and GC until enough pins expire; resume at half the
        // budget so pauses don't thrash.
        if garbage > args.max_garbage {
            println!(
                "[{db_path}] pausing writes: {} garbage > {} budget — collecting as the \
                 compactor's checkpoints expire (≤15m), resuming below {}",
                fmt_bytes(garbage),
                fmt_bytes(args.max_garbage),
                fmt_bytes(args.max_garbage / 2),
            );
            loop {
                if token.is_cancelled() {
                    interrupted = true;
                    break 'seeding;
                }
                collect_seed_garbage(db_path, store.clone()).await?;
                let garbage = stored_bytes(store.as_ref(), db_path)
                    .await?
                    .saturating_sub(live_bytes(&admin).await?);
                if garbage <= args.max_garbage / 2 {
                    println!(
                        "[{db_path}] resuming writes ({} garbage)",
                        fmt_bytes(garbage)
                    );
                    break;
                }
                println!(
                    "[{db_path}] draining: {} garbage (resume ≤ {})",
                    fmt_bytes(garbage),
                    fmt_bytes(args.max_garbage / 2),
                );
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        }
    }
    println!("[{db_path}] closing (final flush, compactor wind-down) ...");
    db.close().await?;
    if interrupted {
        println!(
            "[{db_path}] bulk seed interrupted at {} / {} — re-run to resume",
            fmt_bytes(live),
            fmt_bytes(target)
        );
        return Ok(());
    }
    if written > 0 {
        create_checkpoint(
            db_path,
            store.clone(),
            "bulk-final",
            Some(Duration::from_secs(24 * 3600)),
        )
        .await?;
    }
    println!("[{db_path}] collecting seed garbage (one GC pass) ...");
    collect_seed_garbage(db_path, store).await?;
    println!(
        "[{db_path}] bulk seed complete: {} live (replaced SSTs stay pinned by the \
         compactor's 15-minute internal checkpoints; they're GC'd during traffic \
         mode, or by re-running after the checkpoints expire)",
        fmt_bytes(live)
    );
    Ok(())
}

/// One aggressive GC pass. A finished seed leaves the WAL plus replaced
/// SSTs from compaction on disk — several times the target — and with
/// --seed-only no process sticks around to collect them. Zero min-age is
/// safe: the GC's compaction-watermark and newest-L0 barriers still
/// protect anything in flight, and checkpoint pins are honored (the
/// compactor's internal 15-minute checkpoints keep some garbage alive
/// until they expire).
async fn collect_seed_garbage(
    db_path: &str,
    store: Arc<dyn ObjectStore>,
) -> anyhow::Result<()> {
    let dir_opts = |min_age| {
        Some(GarbageCollectorDirectoryOptions {
            interval: None,
            min_age,
        })
    };
    let admin = AdminBuilder::new(db_path.to_string(), store).build();
    admin
        .run_gc_once(GarbageCollectorOptions {
            manifest_options: dir_opts(Duration::from_secs(300)),
            wal_options: dir_opts(Duration::ZERO),
            compacted_options: dir_opts(Duration::ZERO),
            compactions_options: dir_opts(Duration::from_secs(300)),
            detach_options: None,
        })
        .await?;
    Ok(())
}

/// Per-DB rate factors so the fleet looks heterogeneous in the dashboard.
const RATE_FACTORS: [f64; 3] = [1.0, 0.45, 0.2];

/// Traffic inserts scatter across the shape's power-of-two key domain via
/// a multiplicative bijection (odd constant, power-of-two modulus) instead
/// of appending ever-increasing ids — otherwise newer SSTs always cover
/// higher key ranges and the dashboard's key-range view degenerates into a
/// recency staircase. The bijection keeps inserts collision-free while
/// letting updates re-derive any existing key from its insertion index.
fn scatter(i: u64, mask: u64) -> u64 {
    i.wrapping_mul(0x9E37_79B1) & mask
}

/// Seed one DB when missing (or below its bulk target), then write
/// puts/deletes until cancelled.
async fn run_one(
    args: Arc<TrafficArgs>,
    db_path: String,
    idx: usize,
    store: Arc<dyn ObjectStore>,
    token: CancellationToken,
) -> anyhow::Result<()> {
    let shape = shape_for(&args, &db_path);
    let segments = shape.segments;
    println!(
        "[{db_path}] {}",
        if segments == 0 {
            "unsegmented".to_string()
        } else {
            format!("segmented into {segments} (t00/..t{:02}/)", segments - 1)
        }
    );

    match args.target_size {
        // Bulk mode self-checks against the target, so it always runs:
        // that's what makes interrupted or grown targets resumable.
        Some(target) => bulk_seed(&args, &db_path, &shape, target, store.clone(), &token).await?,
        None => {
            let db_dir = std::path::Path::new(&args.dir).join(&db_path);
            if db_dir.exists() {
                println!("[{db_path}] found existing demo DB — skipping seed");
            } else {
                seed_base(&args, &db_path, &shape, store.clone()).await?;
            }
        }
    }
    if args.seed_only || token.is_cancelled() {
        return Ok(());
    }

    let rate = (args.rate as f64) * RATE_FACTORS[idx % RATE_FACTORS.len()];
    // Stagger the sine phase per DB so the fleet doesn't move in lockstep.
    let phase_offset = (idx as f64) * 60.0;

    let db = open_traffic_db(&db_path, segments, args.sst_bytes, store.clone()).await?;
    // The seed drew from dense ids [0, seeded); traffic inserts scatter
    // from index `seeded` upward so they rarely collide with it.
    let seeded = shape.key_space;
    let churn_values = args.value_bytes.unwrap_or(DEMO_VALUES);
    let mut inserted: u64 = 0;
    let mut rng = Lcg(0xC0FFEE + idx as u64);
    let write_opts = WriteOptions {
        await_durable: false,
        ..Default::default()
    };

    println!(
        "[{db_path}] simulating ~{rate:.0} ops/s (swings 20–100% on a 4m cycle)"
    );

    let started = Instant::now();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let mut puts = 0u64;
    let mut deletes = 0u64;
    let mut ops_since_report = 0u64;
    let mut last_report = Instant::now();
    let mut last_checkpoint = Instant::now();
    let mut checkpoint_n = 0u64;

    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            _ = tick.tick() => {}
        }

        // Sine-modulated rate, full cycle every 4 minutes, floor at 20%.
        let phase =
            (started.elapsed().as_secs_f64() + phase_offset) / 240.0 * std::f64::consts::TAU;
        let mult = 0.2 + 0.8 * (0.5 * (1.0 + phase.sin()));
        let ops = (rate * mult / 10.0).round() as u64;

        // Any existing key, by index: dense seeded lows, scattered inserts
        // above.
        let existing = |r: u64, inserted: u64| {
            let j = r % (seeded + inserted);
            if j < seeded {
                j
            } else {
                scatter(j, shape.scatter_mask)
            }
        };
        for _ in 0..ops {
            let roll = rng.next() % 10;
            if roll == 0 && puts > 0 {
                // 10% deletes of a random existing key.
                let k = existing(rng.next(), inserted);
                db.delete_with_options(key_for(segments, shape.key_width, k), &write_opts)
                    .await?;
                deletes += 1;
            } else {
                // 20% inserts of fresh keys, 70% updates of existing ones.
                // No keys yet (--waves 0 / --keys-per-wave 0 seed nothing):
                // fall back to an insert instead of a modulo-by-zero.
                let k = if roll <= 2 || seeded + inserted == 0 {
                    inserted += 1;
                    scatter(seeded + inserted - 1, shape.scatter_mask)
                } else {
                    existing(rng.next(), inserted)
                };
                let key = key_for(segments, shape.key_width, k);
                let len = churn_values.sample(&mut rng);
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
            if let Err(e) = create_checkpoint(
                &db_path,
                store.clone(),
                &name,
                Some(Duration::from_secs(300)),
            )
            .await
            {
                println!("[{db_path}] checkpoint '{name}' failed (continuing): {e}");
            }
        }

        if last_report.elapsed() >= Duration::from_secs(10) {
            let actual = ops_since_report as f64 / last_report.elapsed().as_secs_f64();
            println!(
                "[{db_path}] [t+{:>4}s] {actual:.0} ops/s (target {:.0}) · {puts} puts · {deletes} deletes · {} keys",
                started.elapsed().as_secs(),
                rate * mult,
                seeded + inserted,
            );
            last_report = Instant::now();
            ops_since_report = 0;
        }
    }

    println!("[{db_path}] shutting down (flushing) ...");
    db.close().await?;
    println!("[{db_path}] wrote {puts} puts, {deletes} deletes");
    Ok(())
}

/// Seed missing demo DBs, then churn all of them concurrently at varied
/// rates and phases. Returns once every DB has flushed after Ctrl-C (or,
/// with --seed-only, once seeding finishes).
pub async fn run_traffic(args: TrafficArgs) -> anyhow::Result<()> {
    let dbs = args
        .dbs
        .unwrap_or(if args.target_size.is_some() { 1 } else { 3 });
    anyhow::ensure!(dbs >= 1, "--dbs must be at least 1");
    if args.clean {
        let dir = std::path::Path::new(&args.dir);
        if dir.exists() {
            // Small sanity net for a destructive flag: never wipe / or $HOME.
            let canon = dir.canonicalize()?;
            anyhow::ensure!(
                canon != std::path::Path::new("/"),
                "--clean refuses to delete /"
            );
            if let Ok(home) = std::env::var("HOME") {
                anyhow::ensure!(
                    canon != std::path::Path::new(&home),
                    "--clean refuses to delete your home directory"
                );
            }
            std::fs::remove_dir_all(&canon)?;
            println!("removed {} (--clean)", args.dir);
        }
    }
    std::fs::create_dir_all(&args.dir)?;
    let store: Arc<dyn ObjectStore> = Arc::new(LocalFileSystem::new_with_prefix(&args.dir)?);

    let args = Arc::new(args);
    let token = CancellationToken::new();
    // Ctrl-C cancels the token; tasks also end on their own with
    // --seed-only, so join the tasks rather than blocking on the signal.
    let sigint = tokio::spawn({
        let token = token.clone();
        async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                token.cancel();
            }
        }
    });
    let mut handles = Vec::new();
    for i in 0..dbs {
        let db_path = if dbs == 1 {
            args.path.clone()
        } else {
            format!("{}-{}", args.path, i + 1)
        };
        handles.push(tokio::spawn(run_one(
            args.clone(),
            db_path,
            i,
            store.clone(),
            token.clone(),
        )));
    }

    for handle in handles {
        if let Err(e) = handle.await? {
            println!("traffic task failed: {e}");
        }
    }
    sigint.abort();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_accepts_binary_units_any_case() {
        assert_eq!(parse_size("1024"), Ok(1024));
        assert_eq!(parse_size("7B"), Ok(7));
        assert_eq!(parse_size("128KiB"), Ok(128 * 1024));
        assert_eq!(parse_size("128kb"), Ok(128 * 1024));
        assert_eq!(parse_size("4m"), Ok(4 << 20));
        assert_eq!(parse_size("50GiB"), Ok(50 << 30));
        assert_eq!(parse_size("1TiB"), Ok(1 << 40));
        assert!(parse_size("").is_err());
        assert!(parse_size("GiB").is_err());
        assert!(parse_size("12XB").is_err());
        assert!(parse_size("99999999999TiB").is_err()); // overflow
    }

    #[test]
    fn parse_byte_range_forms() {
        assert_eq!(
            parse_byte_range("512"),
            Ok(ByteRange { min: 512, max: 512 })
        );
        assert_eq!(
            parse_byte_range("4KiB..64KiB"),
            Ok(ByteRange {
                min: 4 << 10,
                max: 64 << 10
            })
        );
        assert!(parse_byte_range("8..4").is_err());
        assert!(parse_byte_range("0").is_err());
    }

    fn args_with(target_size: Option<u64>) -> TrafficArgs {
        TrafficArgs {
            dir: "./demo-data".to_string(),
            path: "demo-db".to_string(),
            dbs: None,
            waves: 12,
            keys_per_wave: 3000,
            compact_secs: 10,
            rate: 150,
            checkpoint_secs: 120,
            clean: false,
            segments: Some(0),
            target_size,
            value_bytes: None,
            sst_bytes: None,
            seed_only: false,
            no_wal: false,
            max_garbage: 32 << 30,
        }
    }

    #[test]
    fn demo_shape_matches_historical_format() {
        let shape = shape_for(&args_with(None), "demo-db");
        assert_eq!(shape.key_space, 36_000);
        assert_eq!(shape.key_width, 8);
        assert_eq!(shape.scatter_mask, (1 << 26) - 1);
        assert_eq!(shape.values, DEMO_VALUES);
        assert_eq!(shape.sst_bytes, DEMO_SST_BYTES);
        assert_eq!(key_for(0, shape.key_width, 42), "user:00000042");
        assert_eq!(key_for(4, shape.key_width, 42), "t02/user:00000042");
    }

    #[test]
    fn bulk_shape_scales_key_space_and_width() {
        // 100GiB of ~34KiB values needs ~3.2M keys: ×2 headroom, width 8.
        let shape = shape_for(&args_with(Some(100 << 30)), "demo-db");
        let avg = (BULK_VALUES.min + BULK_VALUES.max) / 2;
        assert_eq!(shape.key_space, (100u64 << 30) / avg * 2);
        assert_eq!(shape.key_width, 8);
        assert!(shape.scatter_mask + 1 >= shape.key_space * 2);
        assert_eq!(shape.sst_bytes, BULK_SST_BYTES);

        // Small values push past 10^8 keys and the key format widens.
        let mut args = args_with(Some(100 << 30));
        args.value_bytes = Some(ByteRange { min: 512, max: 512 });
        let shape = shape_for(&args, "demo-db");
        assert_eq!(shape.key_space, (100u64 << 30) / 512 * 2);
        assert_eq!(shape.key_width, 9);
        assert_eq!(key_for(0, shape.key_width, 42), "user:000000042");
    }

    #[test]
    fn scatter_is_a_bijection_on_the_mask_domain() {
        let mask = (1u64 << 16) - 1;
        let mut seen = std::collections::HashSet::new();
        for i in 0..=mask {
            assert!(seen.insert(scatter(i, mask)));
        }
    }

    #[test]
    fn byte_range_sampling_stays_inclusive() {
        let mut rng = Lcg(7);
        let r = ByteRange { min: 64, max: 575 };
        for _ in 0..10_000 {
            let len = r.sample(&mut rng);
            assert!((64..=575).contains(&len));
        }
    }
}
