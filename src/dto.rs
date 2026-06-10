use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct KeyDto {
    pub hex: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utf8: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct BoundDto {
    pub key: KeyDto,
    pub inclusive: bool,
}

/// A key range; a missing bound means unbounded on that side.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct RangeDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<BoundDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<BoundDto>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SstIdDto {
    Wal { id: u64 },
    Compacted { ulid: String },
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct SstViewDto {
    pub view_id: String,
    pub sst_id: SstIdDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_key: Option<KeyDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_key: Option<KeyDto>,
    pub est_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible_range: Option<RangeDto>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct SortedRunDto {
    pub id: u32,
    pub est_bytes: u64,
    pub ssts: Vec<SstViewDto>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct TreeDto {
    pub l0: Vec<SstViewDto>,
    pub runs: Vec<SortedRunDto>,
    pub l0_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct SegmentDto {
    pub prefix: KeyDto,
    pub tree: TreeDto,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct CheckpointDto {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub manifest_id: u64,
    pub create_time: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_time: Option<DateTime<Utc>>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct CheckpointStatusDto {
    #[serde(flatten)]
    pub checkpoint: CheckpointDto,
    /// Whether the referenced manifest still exists in the object store.
    pub manifest_available: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct ExternalDbDto {
    pub path: String,
    pub source_checkpoint_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_checkpoint_id: Option<String>,
    pub sst_count: usize,
    /// A clone with no final checkpoint on its parent no longer depends on it.
    pub detached: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct ManifestDto {
    pub id: u64,
    pub initialized: bool,
    pub writer_epoch: u64,
    pub compactor_epoch: u64,
    pub next_wal_sst_id: u64,
    pub replay_after_wal_id: u64,
    pub last_l0_seq: u64,
    pub last_l0_clock_tick: i64,
    pub recent_snapshot_min_seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_compacted_l0_sst_view_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wal_object_store_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_extractor_name: Option<String>,
    pub tree: TreeDto,
    pub segments: Vec<SegmentDto>,
    pub checkpoints: Vec<CheckpointDto>,
    pub external_dbs: Vec<ExternalDbDto>,
}

/// Lightweight manifest listing entry: built from the object-store LIST
/// alone, without fetching any manifest contents.
#[derive(Serialize, Clone, Debug)]
pub struct ManifestIdDto {
    pub id: u64,
    pub last_modified: DateTime<Utc>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ManifestSummaryDto {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<DateTime<Utc>>,
    pub writer_epoch: u64,
    pub compactor_epoch: u64,
    pub l0_count: usize,
    pub sorted_run_count: usize,
    pub sst_count: usize,
    pub est_total_bytes: u64,
    pub checkpoint_count: usize,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct WarningDto {
    pub code: &'static str,
    /// "info" | "warn" | "error"
    pub severity: &'static str,
    pub message: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct OverviewDto {
    pub db_path: String,
    pub provider: String,
    pub manifest_id: u64,
    pub initialized: bool,
    pub writer_epoch: u64,
    pub compactor_epoch: u64,
    pub l0_count: usize,
    pub sorted_run_count: usize,
    pub sst_count: usize,
    pub l0_bytes: u64,
    pub est_total_bytes: u64,
    pub segment_count: usize,
    pub next_wal_sst_id: u64,
    pub replay_after_wal_id: u64,
    pub last_l0_seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_l0_approx_time: Option<DateTime<Utc>>,
    pub recent_snapshot_min_seq: u64,
    pub checkpoint_count: usize,
    pub clone_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wal_object_store_uri: Option<String>,
    pub manifest_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest_manifest_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_manifest_written_at: Option<DateTime<Utc>>,
    pub warnings: Vec<WarningDto>,
}

#[derive(Serialize, Clone, Debug)]
pub struct LsmDto {
    pub manifest_id: u64,
    pub tree: TreeDto,
    pub segments: Vec<SegmentDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_extractor_name: Option<String>,
}

/// Per-level aggregate for the summary LSM view. `ssts` is present only
/// when the level is small enough to render SSTs individually.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct LevelSummaryDto {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<u32>,
    pub is_l0: bool,
    pub sst_count: usize,
    pub est_bytes: u64,
    /// SSTs-per-bucket depth across the key space (read amplification).
    pub coverage: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssts: Option<Vec<SstViewDto>>,
}

/// Segment descriptor without tree contents: enough for tabs and totals.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct SegmentMetaDto {
    pub prefix: KeyDto,
    pub sst_count: usize,
    pub est_bytes: u64,
}

/// Summary-first LSM payload: O(levels × buckets) regardless of how many
/// SSTs the tree holds, so huge DBs render without shipping every SST.
#[derive(Serialize, Clone, Debug)]
pub struct LsmSummaryDto {
    pub manifest_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_extractor_name: Option<String>,
    pub segments: Vec<SegmentMetaDto>,
    /// Segment whose levels are below; absent = the root tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment: Option<usize>,
    pub root_sst_count: usize,
    /// Bucket count of every `coverage` array in `levels`.
    pub buckets: usize,
    /// Bucket edge keys (bucket i spans bucket_keys[i]..bucket_keys[i+1]);
    /// empty when the tree has no keyed SSTs.
    pub bucket_keys: Vec<KeyDto>,
    pub levels: Vec<LevelSummaryDto>,
    pub total_bytes: u64,
    pub l0_bytes: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct SstInfoDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_key: Option<KeyDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_key: Option<KeyDto>,
    pub index_offset: u64,
    pub index_len: u64,
    pub filter_offset: u64,
    pub filter_len: u64,
    pub stats_offset: u64,
    pub stats_len: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<String>,
    pub sst_type: String,
    pub filter_format: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct SstStatsDto {
    pub num_puts: u64,
    pub num_deletes: u64,
    pub num_merges: u64,
    pub num_rows: u64,
    pub raw_key_bytes: u64,
    pub raw_val_bytes: u64,
    pub block_count: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct BlockMetaDto {
    pub offset: u64,
    pub first_key: KeyDto,
}

#[derive(Serialize, Clone, Debug)]
pub struct BlockIndexDto {
    pub blocks: Vec<BlockMetaDto>,
    pub total_blocks: usize,
    pub truncated: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct SstDetailDto {
    pub ulid: String,
    pub location: String,
    pub size_bytes: u64,
    pub last_modified: DateTime<Utc>,
    pub info: SstInfoDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<SstStatsDto>,
    pub index: BlockIndexDto,
}

#[derive(Serialize, Clone, Debug)]
pub struct SourceDto {
    pub kind: String,
    pub id: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct OutputSstDto {
    pub sst_id: SstIdDto,
    pub est_bytes: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct CompactionDto {
    pub id: String,
    pub status: String,
    pub is_drain: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment: Option<KeyDto>,
    pub sources: Vec<SourceDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<u32>,
    pub bytes_processed: u64,
    pub output_ssts: Vec<OutputSstDto>,
    pub active: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct VersionedCompactionsDto {
    pub id: u64,
    pub compactor_epoch: u64,
    pub compactions: Vec<CompactionDto>,
}

#[derive(Serialize, Clone, Debug)]
pub struct CompactorStateDto {
    pub manifest_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compactions: Option<VersionedCompactionsDto>,
}

#[derive(Serialize, Clone, Debug)]
pub struct SortedRunSummaryDto {
    pub id: u32,
    pub est_bytes: u64,
    pub sst_count: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct RunChangeDto {
    pub id: u32,
    pub ssts_added: Vec<SstViewDto>,
    pub ssts_removed: Vec<SstViewDto>,
}

#[derive(Serialize, Clone, Debug)]
pub struct CheckpointChangeDto {
    pub id: String,
    /// (value in a, value in b)
    pub manifest_id: (u64, u64),
    pub expire_time: (Option<DateTime<Utc>>, Option<DateTime<Utc>>),
}

#[derive(Serialize, Clone, Debug)]
pub struct ScalarChangeDto {
    pub field: String,
    pub a: String,
    pub b: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct ManifestDiffDto {
    pub a: u64,
    pub b: u64,
    pub l0_added: Vec<SstViewDto>,
    pub l0_removed: Vec<SstViewDto>,
    pub runs_added: Vec<SortedRunSummaryDto>,
    pub runs_removed: Vec<SortedRunSummaryDto>,
    pub runs_changed: Vec<RunChangeDto>,
    pub segments_added: Vec<KeyDto>,
    pub segments_removed: Vec<KeyDto>,
    pub checkpoints_added: Vec<CheckpointDto>,
    pub checkpoints_removed: Vec<CheckpointDto>,
    pub checkpoints_changed: Vec<CheckpointChangeDto>,
    pub external_dbs_added: Vec<ExternalDbDto>,
    pub external_dbs_removed: Vec<ExternalDbDto>,
    pub scalars: Vec<ScalarChangeDto>,
}

#[derive(Serialize, Clone, Debug)]
pub struct WalSstDto {
    pub id: u64,
    pub size_bytes: u64,
    pub last_modified: DateTime<Utc>,
}

#[derive(Serialize, Clone, Debug)]
pub struct WalDto {
    pub next_wal_sst_id: u64,
    pub replay_after_wal_id: u64,
    /// Across all WAL SSTs, not just the returned page.
    pub total_bytes: u64,
    pub total_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wal_object_store_uri: Option<String>,
    /// Newest first, truncated to the requested limit.
    pub entries: Vec<WalSstDto>,
}

/// Aggregate view of one SST-list change: enough for a feed line.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct SstDeltaDto {
    pub count: usize,
    pub bytes: u64,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct RunChangeSummaryDto {
    pub id: u32,
    pub added: SstDeltaDto,
    pub removed: SstDeltaDto,
}

/// Aggregate manifest diff for the activity feed: the same classification
/// signal as [`ManifestDiffDto`] without the per-SST lists, so payloads
/// stay bounded however churny the transition. The full diff is one click
/// away on the manifest-diff page.
#[derive(Serialize, Clone, Debug)]
pub struct DiffSummaryDto {
    pub l0_added: SstDeltaDto,
    pub l0_removed: SstDeltaDto,
    pub runs_added: Vec<SortedRunSummaryDto>,
    pub runs_removed: Vec<SortedRunSummaryDto>,
    pub runs_changed: Vec<RunChangeSummaryDto>,
    pub segments_added: usize,
    pub segments_removed: usize,
    pub checkpoints_added: Vec<CheckpointDto>,
    pub checkpoints_removed: Vec<CheckpointDto>,
    pub checkpoints_changed: usize,
    pub external_dbs_added: usize,
    pub external_dbs_removed: usize,
    pub scalars: Vec<ScalarChangeDto>,
}

/// One manifest transition (a → b) in the activity feed.
#[derive(Serialize, Clone, Debug)]
pub struct ActivityDto {
    pub a: u64,
    pub b: u64,
    /// When manifest `b` was written.
    pub at: DateTime<Utc>,
    pub diff: DiffSummaryDto,
}

#[derive(Serialize, Clone, Debug)]
pub struct HealthDto {
    pub status: &'static str,
    pub store_count: usize,
    /// Discovered DBs as of the last scan; absent before the first scan.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_count: Option<usize>,
}

#[derive(Serialize, Clone, Debug)]
pub struct DbInfoDto {
    /// "{store}:{path}" — the URL-safe identity used by /api/dbs/{id}.
    pub id: String,
    pub store: String,
    pub path: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct DbsDto {
    pub scanned_at: DateTime<Utc>,
    pub dbs: Vec<DbInfoDto>,
}

/// One object observed to have disappeared from a listing between two
/// refreshes — almost always a GC deletion.
#[derive(Serialize, Clone, Debug)]
pub struct GcEventDto {
    /// "compacted" | "wal" | "manifest"
    pub kind: &'static str,
    /// ULID for compacted SSTs, numeric id for WAL SSTs and manifests.
    pub id: String,
    pub size_bytes: u64,
    pub written_at: DateTime<Utc>,
    /// Last listing refresh that still saw the object.
    pub last_seen_at: DateTime<Utc>,
    /// First listing refresh that no longer saw it.
    pub missing_at: DateTime<Utc>,
    /// Whether the latest cached manifest still referenced it when it
    /// vanished (true = anomaly); absent when no manifest was available
    /// to judge against.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referenced: Option<bool>,
}

#[derive(Serialize, Clone, Debug)]
pub struct GcEventsDto {
    /// Observation starts when this server first listed the DB; sweeps
    /// before that (or while no dashboard is running) are not recorded.
    pub observing_since: DateTime<Utc>,
    /// Newest first, capped.
    pub events: Vec<GcEventDto>,
}

#[derive(Serialize, Clone, Debug)]
pub struct SearchSstObjectDto {
    pub location: String,
    pub size_bytes: u64,
    pub last_modified: DateTime<Utc>,
}

#[derive(Serialize, Clone, Debug)]
pub struct SearchManifestHitDto {
    pub id: u64,
    /// Where in the tree the ULID matched, e.g. "SST in SR 3" or
    /// "L0 view id".
    pub places: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct SearchCompactionHitDto {
    /// Newest .compactions version containing this hit.
    pub version: u64,
    pub job_id: String,
    /// "job" (the ULID is the job id) or "output" (an output SST).
    pub role: &'static str,
}

#[derive(Serialize, Clone, Debug)]
pub struct SearchCheckpointHitDto {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub manifest_id: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct SearchDto {
    pub query: String,
    /// The compacted SST object itself, when it exists in the store.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sst_object: Option<SearchSstObjectDto>,
    /// Manifests referencing the ULID, newest first (capped).
    pub manifests: Vec<SearchManifestHitDto>,
    pub manifests_scanned: usize,
    pub manifests_total: usize,
    pub compactions: Vec<SearchCompactionHitDto>,
    pub checkpoints: Vec<SearchCheckpointHitDto>,
}

/// Per-directory breakdown of stored objects: live (referenced by the latest
/// manifest), pinned (kept alive only by an unexpired checkpoint), and
/// reclaimable (what the GC would eventually delete).
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct GarbageCategoryDto {
    pub stored_count: usize,
    pub stored_bytes: u64,
    pub live_count: usize,
    pub live_bytes: u64,
    pub pinned_count: usize,
    pub pinned_bytes: u64,
    pub reclaimable_count: usize,
    pub reclaimable_bytes: u64,
}

/// One unexpired checkpoint and how much storage it keeps alive beyond
/// what the latest manifest already references.
#[derive(Serialize, Clone, Debug)]
pub struct GarbagePinnerDto {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub manifest_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_time: Option<DateTime<Utc>>,
    pub manifest_available: bool,
    /// Data bytes (compacted + WAL) referenced only via this checkpoint.
    pub extra_bytes: u64,
    pub extra_count: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct GarbageDto {
    pub manifest_id: u64,
    pub live_checkpoint_count: usize,
    pub expired_checkpoint_count: usize,
    /// Unexpired checkpoints whose manifest no longer exists.
    pub dangling_checkpoint_count: usize,
    /// Unexpired checkpoints, heaviest pinner first.
    pub pinners: Vec<GarbagePinnerDto>,
    pub compacted: GarbageCategoryDto,
    pub wal: GarbageCategoryDto,
    pub manifests: GarbageCategoryDto,
    pub stored_bytes: u64,
    pub live_bytes: u64,
    pub pinned_bytes: u64,
    pub reclaimable_bytes: u64,
    /// stored / live over data objects (compacted + WAL); absent when
    /// nothing is live.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_amp: Option<f64>,
    /// When the oldest reclaimable object was written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest_reclaimable_at: Option<DateTime<Utc>>,
}
