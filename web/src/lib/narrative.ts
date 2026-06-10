import type { ManifestDiffDto } from '../api/types'

/** One-line human summary of a manifest diff. */
export function narrative(d: ManifestDiffDto): string {
  const parts: string[] = []
  if (d.l0_added.length) parts.push(`${d.l0_added.length} L0 SST${d.l0_added.length > 1 ? 's' : ''} flushed`)
  if (d.l0_removed.length && d.runs_added.length)
    parts.push(
      `${d.l0_removed.length} L0 SST${d.l0_removed.length > 1 ? 's' : ''} compacted into ${d.runs_added.map((r) => `SR ${r.id}`).join(', ')}`,
    )
  else {
    if (d.l0_removed.length) parts.push(`${d.l0_removed.length} L0 SSTs removed`)
    if (d.runs_added.length) parts.push(`runs added: ${d.runs_added.map((r) => `SR ${r.id}`).join(', ')}`)
  }
  if (d.runs_removed.length) parts.push(`runs removed: ${d.runs_removed.map((r) => `SR ${r.id}`).join(', ')}`)
  if (d.runs_changed.length) parts.push(`${d.runs_changed.length} run${d.runs_changed.length > 1 ? 's' : ''} changed`)
  if (d.checkpoints_added.length) parts.push(`${d.checkpoints_added.length} checkpoint${d.checkpoints_added.length > 1 ? 's' : ''} added`)
  if (d.checkpoints_removed.length) parts.push(`${d.checkpoints_removed.length} checkpoint${d.checkpoints_removed.length > 1 ? 's' : ''} removed`)
  if (parts.length === 0) parts.push('only scalar fields changed')
  return parts.join('; ')
}
