import { useCompactions, useCompactorState } from '../api/client'
import type { CompactionDto } from '../api/types'
import { HelpTip } from '../components/HelpTip'
import { keyText } from '../components/KeyDisplay'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { StatusBadge } from '../components/StatusBadge'
import { formatBytes, formatRelative, formatTime } from '../lib/format'
import { ulidTimeMs } from '../lib/ulid'

function specText(c: CompactionDto): string {
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

export default function Compactions() {
  const stateQuery = useCompactorState()
  const historyQuery = useCompactions()

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-3xl">Compactions</h1>
        {stateQuery.data?.compactions && (
          <span className="font-mono text-sm text-ink-4">
            v{stateQuery.data.compactions.id} · epoch{' '}
            {stateQuery.data.compactions.compactor_epoch}
          </span>
        )}
      </div>
      <div className="mt-6">
        <QueryGate query={historyQuery}>
          {(history) => {
            // One row per job, newest status wins (versions arrive newest
            // first), ordered by start time from the job's ULID.
            const seen = new Set<string>()
            const jobs: CompactionDto[] = []
            for (const vc of history) {
              for (const c of vc.compactions) {
                if (!seen.has(c.id)) {
                  seen.add(c.id)
                  jobs.push(c)
                }
              }
            }
            jobs.sort((a, b) => (ulidTimeMs(b.id) ?? 0) - (ulidTimeMs(a.id) ?? 0))
            const anySegment = jobs.some((c) => c.segment)
            return (
              <Panel
                action={
                  <HelpTip>
                    Newest first; one row per compactor job from the
                    .compactions log, showing its latest status across
                    versions. Sources list sorted runs and L0 SSTs (repeated
                    L0s collapse to ×n); "drain" jobs flush without a
                    destination run.
                  </HelpTip>
                }
              >
                {jobs.length === 0 ? (
                  <span className="text-sm text-ink-5">
                    No compactions file — the compactor has never run against
                    this DB.
                  </span>
                ) : (
                  <div className="overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="text-left text-xs font-semibold uppercase tracking-wider text-ink-5">
                        <th className="pb-2 pr-4">Started</th>
                        <th className="pb-2 pr-4">Status</th>
                        <th className="pb-2 pr-4">Spec</th>
                        {anySegment && <th className="pb-2 pr-4">Segment</th>}
                        <th className="pb-2 pr-4">Processed</th>
                        <th className="pb-2 pr-4">Outputs</th>
                        <th className="pb-2">ID</th>
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
                            <td className="py-1.5 pr-4 font-mono text-xs">
                              {specText(c)}
                            </td>
                            {anySegment && (
                              <td className="py-1.5 pr-4 font-mono text-xs text-ink-4">
                                {c.segment ? keyText(c.segment) : '—'}
                              </td>
                            )}
                            <td className="py-1.5 pr-4">
                              {formatBytes(c.bytes_processed)}
                            </td>
                            <td className="py-1.5 pr-4">
                              {c.output_ssts.length > 0
                                ? `${c.output_ssts.length} SSTs · ${formatBytes(
                                    c.output_ssts.reduce(
                                      (acc, o) => acc + o.est_bytes,
                                      0,
                                    ),
                                  )}`
                                : '—'}
                            </td>
                            <td className="py-1.5 font-mono text-xs text-ink-4">
                              {c.id}
                            </td>
                          </tr>
                        )
                      })}
                    </tbody>
                  </table>
                  </div>
                )}
              </Panel>
            )
          }}
        </QueryGate>
      </div>
    </div>
  )
}
