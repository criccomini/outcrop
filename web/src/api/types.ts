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
}

export interface LsmDto {
  manifest_id: number
  tree: TreeDto
  segments: SegmentDto[]
  segment_extractor_name?: string
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
  total_bytes: number
  wal_object_store_uri?: string
  entries: WalSstDto[]
}

export interface ActivityDto {
  a: number
  b: number
  at: string
  diff: ManifestDiffDto
}

export interface HealthDto {
  status: string
  db_path: string
  provider: string
}
