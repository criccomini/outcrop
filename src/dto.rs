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
}

#[derive(Serialize, Clone, Debug)]
pub struct LsmDto {
    pub manifest_id: u64,
    pub tree: TreeDto,
    pub segments: Vec<SegmentDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_extractor_name: Option<String>,
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
pub struct HealthDto {
    pub status: &'static str,
    pub db_path: String,
    pub provider: String,
}
