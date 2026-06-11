// Mirrors src/dto.rs on the Rust side.

export interface KeyDto {
  hex: string
  utf8?: string
}

export interface BoundDto {
  key: KeyDto
  inclusive: boolean
}

export interface RangeDto {
  start?: BoundDto
  end?: BoundDto
}

export type SstIdDto =
  | { kind: 'wal'; id: number }
  | { kind: 'compacted'; ulid: string }

export interface SstViewDto {
  view_id: string
  sst_id: SstIdDto
  first_key?: KeyDto
  last_key?: KeyDto
  est_bytes: number
  compression?: string
  visible_range?: RangeDto
}

export interface SortedRunDto {
  id: number
  est_bytes: number
  ssts: SstViewDto[]
}

export interface TreeDto {
  l0: SstViewDto[]
  runs: SortedRunDto[]
  l0_bytes: number
  total_bytes: number
}

export interface SegmentDto {
  prefix: KeyDto
  tree: TreeDto
}

export interface CheckpointDto {
  id: string
  name?: string
  manifest_id: number
  create_time: string
  expire_time?: string
}

export interface CheckpointStatusDto extends CheckpointDto {
  manifest_available: boolean
}

export interface ExternalDbDto {
  path: string
  source_checkpoint_id: string
  final_checkpoint_id?: string
  sst_count: number
  detached: boolean
}

export interface ManifestDto {
  id: number
  initialized: boolean
  writer_epoch: number
  compactor_epoch: number
  next_wal_sst_id: number
  replay_after_wal_id: number
  last_l0_seq: number
  last_l0_clock_tick: number
  recent_snapshot_min_seq: number
  last_compacted_l0_sst_view_id?: string
  wal_object_store_uri?: string
  segment_extractor_name?: string
  tree: TreeDto
  segments: SegmentDto[]
  checkpoints: CheckpointDto[]
  external_dbs: ExternalDbDto[]
}

export interface ManifestIdDto {
  id: number
  last_modified: string
}

export interface ManifestSummaryDto {
  id: number
  last_modified?: string
  writer_epoch: number
  compactor_epoch: number
  l0_count: number
  sorted_run_count: number
  sst_count: number
  est_total_bytes: number
  checkpoint_count: number
}

export interface WarningDto {
  code: string
  severity: 'info' | 'warn' | 'error'
  message: string
}

export interface OverviewDto {
  db_path: string
  provider: string
  manifest_id: number
  initialized: boolean
  writer_epoch: number
  compactor_epoch: number
  l0_count: number
  sorted_run_count: number
  sst_count: number
  l0_bytes: number
  est_total_bytes: number
  segment_count: number
  next_wal_sst_id: number
  replay_after_wal_id: number
  last_l0_seq: number
  last_l0_approx_time?: string
  recent_snapshot_min_seq: number
  checkpoint_count: number
  clone_count: number
  wal_object_store_uri?: string
  manifest_count: number
  oldest_manifest_id?: number
  latest_manifest_written_at?: string
  warnings: WarningDto[]
}

export interface LsmDto {
  manifest_id: number
  tree: TreeDto
  segments: SegmentDto[]
  segment_extractor_name?: string
}

export interface LevelSummaryDto {
  label: string
  run_id?: number
  is_l0: boolean
  sst_count: number
  est_bytes: number
  /** Max SSTs-deep per key-space bucket (read amplification). */
  coverage: number[]
  /** Present only when the level is small enough to render per-SST. */
  ssts?: SstViewDto[]
}

/** On-demand SST listing for one level restricted to a key range. */
export interface LevelSliceDto {
  total: number
  truncated: boolean
  ssts: SstViewDto[]
}

export interface SegmentMetaDto {
  prefix: KeyDto
  sst_count: number
  est_bytes: number
}

export interface LsmSummaryDto {
  manifest_id: number
  segment_extractor_name?: string
  segments: SegmentMetaDto[]
  /** Segment whose levels are below; absent = the root tree. */
  segment?: number
  root_sst_count: number
  buckets: number
  /** Bucket edge keys; empty when the tree has no keyed SSTs. */
  bucket_keys: KeyDto[]
  levels: LevelSummaryDto[]
  total_bytes: number
  l0_bytes: number
}

export interface SstInfoDto {
  first_key?: KeyDto
  last_key?: KeyDto
  index_offset: number
  index_len: number
  filter_offset: number
  filter_len: number
  stats_offset: number
  stats_len: number
  compression?: string
  sst_type: string
  filter_format: string
}

export interface SstStatsDto {
  num_puts: number
  num_deletes: number
  num_merges: number
  num_rows: number
  raw_key_bytes: number
  raw_val_bytes: number
  block_count: number
}

export interface BlockMetaDto {
  offset: number
  first_key: KeyDto
}

export interface BlockIndexDto {
  blocks: BlockMetaDto[]
  total_blocks: number
  truncated: boolean
}

export interface SstDetailDto {
  ulid: string
  location: string
  size_bytes: number
  last_modified: string
  info: SstInfoDto
  stats?: SstStatsDto
  index: BlockIndexDto
}

export interface SourceDto {
  kind: string
  id: string
}

export interface OutputSstDto {
  sst_id: SstIdDto
  est_bytes: number
}

export interface CompactionDto {
  id: string
  status: string
  is_drain: boolean
  segment?: KeyDto
  sources: SourceDto[]
  destination?: number
  bytes_processed: number
  output_ssts: OutputSstDto[]
  active: boolean
}

export interface VersionedCompactionsDto {
  id: number
  compactor_epoch: number
  compactions: CompactionDto[]
}

export interface CompactorStateDto {
  manifest_id: number
  compactions?: VersionedCompactionsDto
}

export interface SortedRunSummaryDto {
  id: number
  est_bytes: number
  sst_count: number
}

export interface RunChangeDto {
  id: number
  ssts_added: SstViewDto[]
  ssts_removed: SstViewDto[]
}

export interface CheckpointChangeDto {
  id: string
  manifest_id: [number, number]
  expire_time: [string | null, string | null]
}

export interface ScalarChangeDto {
  field: string
  a: string
  b: string
}

export interface ManifestDiffDto {
  a: number
  b: number
  l0_added: SstViewDto[]
  l0_removed: SstViewDto[]
  runs_added: SortedRunSummaryDto[]
  runs_removed: SortedRunSummaryDto[]
  runs_changed: RunChangeDto[]
  segments_added: KeyDto[]
  segments_removed: KeyDto[]
  checkpoints_added: CheckpointDto[]
  checkpoints_removed: CheckpointDto[]
  checkpoints_changed: CheckpointChangeDto[]
  external_dbs_added: ExternalDbDto[]
  external_dbs_removed: ExternalDbDto[]
  scalars: ScalarChangeDto[]
}

export interface WalSstDto {
  id: number
  size_bytes: number
  last_modified: string
}

export interface WalDto {
  next_wal_sst_id: number
  replay_after_wal_id: number
  /** Across all WAL SSTs, not just the returned page. */
  total_bytes: number
  total_count: number
  wal_object_store_uri?: string
  /** Newest first, truncated to the requested limit. */
  entries: WalSstDto[]
}

/** Aggregate view of one SST-list change: enough for a feed line. */
export interface SstDeltaDto {
  count: number
  bytes: number
}

export interface RunChangeSummaryDto {
  id: number
  added: SstDeltaDto
  removed: SstDeltaDto
}

/** ManifestDiffDto collapsed to counts and byte sums for the feed. */
export interface DiffSummaryDto {
  l0_added: SstDeltaDto
  l0_removed: SstDeltaDto
  runs_added: SortedRunSummaryDto[]
  runs_removed: SortedRunSummaryDto[]
  runs_changed: RunChangeSummaryDto[]
  segments_added: number
  segments_removed: number
  checkpoints_added: CheckpointDto[]
  checkpoints_removed: CheckpointDto[]
  checkpoints_changed: number
  external_dbs_added: number
  external_dbs_removed: number
  scalars: ScalarChangeDto[]
}

export interface ActivityDto {
  a: number
  b: number
  at: string
  diff: DiffSummaryDto
}

export interface HealthDto {
  status: string
  store_count: number
  db_count?: number
}

export interface DbInfoDto {
  /** "{store}:{path}" — the identity used by /api/dbs/{id} and /db/{id}. */
  id: string
  store: string
  path: string
}

export interface DbsDto {
  scanned_at: string
  dbs: DbInfoDto[]
}

export interface GarbageCategoryDto {
  stored_count: number
  stored_bytes: number
  live_count: number
  live_bytes: number
  pinned_count: number
  pinned_bytes: number
  reclaimable_count: number
  reclaimable_bytes: number
}

export interface GarbagePinnerDto {
  id: string
  name?: string
  manifest_id: number
  expire_time?: string
  manifest_available: boolean
  extra_bytes: number
  extra_count: number
}

export interface GarbageDto {
  manifest_id: number
  live_checkpoint_count: number
  expired_checkpoint_count: number
  dangling_checkpoint_count: number
  pinners: GarbagePinnerDto[]
  compacted: GarbageCategoryDto
  wal: GarbageCategoryDto
  manifests: GarbageCategoryDto
  stored_bytes: number
  live_bytes: number
  pinned_bytes: number
  reclaimable_bytes: number
  space_amp?: number
  oldest_reclaimable_at?: string
}

export interface SearchSstObjectDto {
  location: string
  size_bytes: number
  last_modified: string
}

export interface SearchManifestHitDto {
  id: number
  places: string[]
}

export interface SearchCompactionHitDto {
  version: number
  job_id: string
  role: 'job' | 'output'
}

export interface SearchCheckpointHitDto {
  id: string
  name?: string
  manifest_id: number
}

export interface SearchDto {
  query: string
  sst_object?: SearchSstObjectDto
  manifests: SearchManifestHitDto[]
  manifests_scanned: number
  manifests_total: number
  compactions: SearchCompactionHitDto[]
  checkpoints: SearchCheckpointHitDto[]
}

export interface GcEventDto {
  kind: 'compacted' | 'wal' | 'manifest'
  id: string
  size_bytes: number
  written_at: string
  last_seen_at: string
  missing_at: string
  referenced?: boolean
}

export interface GcEventsDto {
  observing_since: string
  events: GcEventDto[]
}
