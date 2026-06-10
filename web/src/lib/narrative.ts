import type { ManifestDiffDto } from '../api/types'

const plural = (n: number) => (n > 1 ? 's' : '')

/**
 * One-line human summary of a manifest diff. `at` is the transition time;
 * with it, removals of already-expired checkpoints are attributed to the GC.
 */
export function narrative(d: ManifestDiffDto, at?: string): string {
  const parts: string[] = []
  if (d.l0_added.length)
    parts.push(`${d.l0_added.length} L0 SST${plural(d.l0_added.length)} flushed`)

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
    parts.push(`${sources.join(' + ')} compacted into ${dests.join(', ')}`)
  } else {
    if (d.l0_removed.length) parts.push(`${d.l0_removed.length} L0 SSTs removed`)
    if (d.runs_added.length)
      parts.push(`runs added: ${d.runs_added.map((r) => `SR ${r.id}`).join(', ')}`)
    if (d.runs_removed.length)
      parts.push(`runs removed: ${d.runs_removed.map((r) => `SR ${r.id}`).join(', ')}`)
    if (d.runs_changed.length)
      parts.push(`${d.runs_changed.length} run${plural(d.runs_changed.length)} changed`)
  }

  if (d.checkpoints_added.length)
    parts.push(
      `${d.checkpoints_added.length} checkpoint${plural(d.checkpoints_added.length)} added`,
    )
  if (d.checkpoints_removed.length) {
    const t = at ? Date.parse(at) : NaN
    const expired = Number.isNaN(t)
      ? 0
      : d.checkpoints_removed.filter(
          (c) => c.expire_time && Date.parse(c.expire_time) <= t,
        ).length
    const other = d.checkpoints_removed.length - expired
    if (expired) parts.push(`${expired} expired checkpoint${plural(expired)} removed (GC)`)
    if (other) parts.push(`${other} checkpoint${plural(other)} removed`)
  }
  if (d.checkpoints_changed.length)
    parts.push(
      `${d.checkpoints_changed.length} checkpoint${plural(d.checkpoints_changed.length)} changed`,
    )

  if (parts.length === 0) parts.push('only scalar fields changed')
  return parts.join('; ')
}
