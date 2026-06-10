import type { CheckpointDto, CompactionDto, ManifestDiffDto } from '../api/types'
import { formatBytes } from './format'

/** Category of a feed entry, used for the chip rail on the Activity page. */
export type FeedKind = 'flush' | 'compaction' | 'checkpoint' | 'gc' | 'clone' | 'meta'

const plural = (n: number) => (n > 1 ? 's' : '')
const sum = (ns: number[]) => ns.reduce((a, b) => a + b, 0)

function checkpointLabel(c: CheckpointDto): string {
  return c.name ? `'${c.name}'` : c.id.slice(0, 8)
}

function structuralChanges(d: ManifestDiffDto): number {
  return (
    d.l0_removed.length +
    d.runs_added.length +
    d.runs_removed.length +
    d.runs_changed.length +
    d.segments_added.length +
    d.segments_removed.length +
    d.checkpoints_added.length +
    d.checkpoints_removed.length +
    d.checkpoints_changed.length +
    d.external_dbs_added.length +
    d.external_dbs_removed.length
  )
}

/** True when the transition only adds L0 SSTs (plus scalar bookkeeping). */
export function isPureFlush(d: ManifestDiffDto): boolean {
  return d.l0_added.length > 0 && structuralChanges(d) === 0
}

/** True when nothing but scalar fields moved. */
export function isScalarOnly(d: ManifestDiffDto): boolean {
  return d.l0_added.length === 0 && structuralChanges(d) === 0
}

/**
 * Kind + one-line text for a single manifest transition. `at` (the
 * transition time) attributes removals of already-expired checkpoints to
 * the GC.
 */
export function classify(
  d: ManifestDiffDto,
  at?: string,
): { kind: FeedKind; text: string } {
  const kinds = new Set<FeedKind>()
  const parts: string[] = []

  if (d.l0_added.length) {
    kinds.add('flush')
    const bytes = sum(d.l0_added.map((s) => s.est_bytes))
    parts.push(
      `${d.l0_added.length} L0 SST${plural(d.l0_added.length)} flushed · ${formatBytes(bytes)}`,
    )
  }

  // A compaction reads as sources (removed L0s and/or removed runs) flowing
  // into destinations (added runs, or existing runs that changed).
  const sources: string[] = []
  if (d.l0_removed.length)
    sources.push(`${d.l0_removed.length} L0 SST${plural(d.l0_removed.length)}`)
  sources.push(...d.runs_removed.map((r) => `SR ${r.id}`))
  const dests = [
    ...d.runs_added.map((r) => `SR ${r.id}`),
    ...(sources.length ? d.runs_changed.map((r) => `SR ${r.id}`) : []),
  ]
  if (sources.length && dests.length) {
    kinds.add('compaction')
    const bytesIn =
      sum(d.l0_removed.map((s) => s.est_bytes)) +
      sum(d.runs_removed.map((r) => r.est_bytes))
    const bytesOut = sum(d.runs_added.map((r) => r.est_bytes))
    let line = `${sources.join(' + ')} → ${dests.join(', ')} · ${formatBytes(bytesIn)} in`
    if (bytesOut > 0) line += `, ${formatBytes(bytesOut)} out`
    parts.push(line)
  } else {
    if (d.l0_removed.length) {
      kinds.add('compaction')
      parts.push(`${d.l0_removed.length} L0 SSTs removed`)
    }
    if (d.runs_added.length) {
      kinds.add('compaction')
      parts.push(`runs added: ${d.runs_added.map((r) => `SR ${r.id}`).join(', ')}`)
    }
    if (d.runs_removed.length) {
      kinds.add('compaction')
      parts.push(`runs removed: ${d.runs_removed.map((r) => `SR ${r.id}`).join(', ')}`)
    }
    if (d.runs_changed.length) {
      kinds.add('compaction')
      parts.push(`${d.runs_changed.length} run${plural(d.runs_changed.length)} changed`)
    }
  }

  if (d.segments_added.length)
    parts.push(`${d.segments_added.length} segment${plural(d.segments_added.length)} added`)
  if (d.segments_removed.length)
    parts.push(
      `${d.segments_removed.length} segment${plural(d.segments_removed.length)} removed`,
    )

  if (d.checkpoints_added.length) {
    kinds.add('checkpoint')
    parts.push(
      d.checkpoints_added.length === 1
        ? `checkpoint ${checkpointLabel(d.checkpoints_added[0])} added`
        : `${d.checkpoints_added.length} checkpoints added`,
    )
  }
  if (d.checkpoints_removed.length) {
    const t = at ? Date.parse(at) : NaN
    const expired = Number.isNaN(t)
      ? []
      : d.checkpoints_removed.filter(
          (c) => c.expire_time && Date.parse(c.expire_time) <= t,
        )
    const other = d.checkpoints_removed.filter((c) => !expired.includes(c))
    if (expired.length) {
      kinds.add('gc')
      parts.push(
        `${expired.length} expired checkpoint${plural(expired.length)} removed`,
      )
    }
    if (other.length) {
      kinds.add('checkpoint')
      parts.push(
        other.length === 1
          ? `checkpoint ${checkpointLabel(other[0])} removed`
          : `${other.length} checkpoints removed`,
      )
    }
  }
  if (d.checkpoints_changed.length) {
    kinds.add('checkpoint')
    parts.push(
      `${d.checkpoints_changed.length} checkpoint${plural(d.checkpoints_changed.length)} changed`,
    )
  }

  if (d.external_dbs_added.length) {
    kinds.add('clone')
    parts.push(
      `${d.external_dbs_added.length} clone parent${plural(d.external_dbs_added.length)} linked`,
    )
  }
  if (d.external_dbs_removed.length) {
    kinds.add('clone')
    parts.push(
      `${d.external_dbs_removed.length} clone parent${plural(d.external_dbs_removed.length)} unlinked`,
    )
  }

  if (parts.length === 0) {
    const fields = d.scalars.map((s) => s.field)
    parts.push(
      fields.length
        ? `bookkeeping: ${fields.slice(0, 3).join(', ')}${fields.length > 3 ? ', …' : ''}`
        : 'no visible changes',
    )
  }

  const kind =
    (['compaction', 'flush', 'gc', 'checkpoint', 'clone'] as const).find((k) =>
      kinds.has(k),
    ) ?? 'meta'
  return { kind, text: parts.join('; ') }
}

/** "{sources} → SR {dest}" for a compactor job. */
export function compactionJobSummary(c: CompactionDto): string {
  const l0 = c.sources.filter((s) => s.kind === 'l0').length
  const sources = [
    ...(l0 ? [`${l0} L0 SST${plural(l0)}`] : []),
    ...c.sources.filter((s) => s.kind === 'sorted_run').map((s) => `SR ${s.id}`),
  ]
  const dest = c.destination !== undefined ? `SR ${c.destination}` : '—'
  return `${c.is_drain ? 'drain ' : ''}${sources.join(' + ')} → ${dest}`
}
