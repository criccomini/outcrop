import type { CompactionDto } from '../api/types'
import { keyText } from './KeyDisplay'
import { StatusBadge } from './StatusBadge'
import { formatBytes, formatRelative, formatTime } from '../lib/format'
import { ulidTimeMs } from '../lib/ulid'

export function specText(c: CompactionDto): string {
  const sources = c.sources
    .map((s) => (s.kind === 'sorted_run' ? `SR ${s.id}` : 'L0'))
    .reduce<string[]>((acc, label) => {
      // Collapse repeated L0 sources into "L0 ×n".
      const last = acc[acc.length - 1]
      if (label === 'L0' && last?.startsWith('L0')) {
        const n = last === 'L0' ? 2 : Number(last.slice(4)) + 1
        acc[acc.length - 1] = `L0 ×${n}`
      } else {
        acc.push(label)
      }
      return acc
    }, [])
  const dst = c.is_drain
    ? 'drain'
    : c.destination !== undefined
      ? `SR ${c.destination}`
      : '?'
  return `[${sources.join(', ')}] → ${dst}`
}

export function outputsText(c: CompactionDto): string {
  return c.output_ssts.length > 0
    ? `${c.output_ssts.length} SSTs · ${formatBytes(
        c.output_ssts.reduce((acc, o) => acc + o.est_bytes, 0),
      )}`
    : '—'
}

/** The flat job table shared by the compactions list, detail, and diff. */
export function CompactionJobsTable({ jobs }: { jobs: CompactionDto[] }) {
  if (jobs.length === 0) {
    return <span className="text-sm text-ink-5">no jobs</span>
  }
  const anySegment = jobs.some((c) => c.segment)
  return (
    <div className="overflow-x-auto">
    <table className="w-full text-sm">
      <thead>
        <tr className="text-left text-xs font-semibold uppercase tracking-wider text-ink-5">
          <th className="pb-1.5 pr-4">Started</th>
          <th className="pb-1.5 pr-4">Status</th>
          <th className="pb-1.5 pr-4">Spec</th>
          {anySegment && <th className="pb-1.5 pr-4">Segment</th>}
          <th className="pb-1.5 pr-4">Processed</th>
          <th className="pb-1.5 pr-4">Outputs</th>
          <th className="pb-1.5">ID</th>
        </tr>
      </thead>
      <tbody>
        {jobs.map((c) => {
          const at = ulidTimeMs(c.id)
          const iso = at !== null ? new Date(at).toISOString() : undefined
          return (
            <tr key={c.id} className="border-t border-ink-7/50">
              <td
                className="py-1.5 pr-4 text-ink-3"
                title={iso ? formatTime(iso) : undefined}
              >
                {iso ? formatRelative(iso) : '—'}
              </td>
              <td className="py-1.5 pr-4">
                <StatusBadge status={c.status} />
              </td>
              <td className="py-1.5 pr-4 font-mono text-xs">{specText(c)}</td>
              {anySegment && (
                <td className="py-1.5 pr-4 font-mono text-xs text-ink-4">
                  {c.segment ? keyText(c.segment) : '—'}
                </td>
              )}
              <td className="py-1.5 pr-4">{formatBytes(c.bytes_processed)}</td>
              <td className="py-1.5 pr-4">{outputsText(c)}</td>
              <td className="py-1.5 font-mono text-xs text-ink-4">{c.id}</td>
            </tr>
          )
        })}
      </tbody>
    </table>
    </div>
  )
}
