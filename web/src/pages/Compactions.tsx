import { useCompactions, useCompactorState } from '../api/client'
import type { CompactionDto } from '../api/types'
import { keyText } from '../components/KeyDisplay'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { StatusBadge } from '../components/StatusBadge'
import { formatBytes } from '../lib/format'

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

function CompactionTable({ compactions }: { compactions: CompactionDto[] }) {
  if (compactions.length === 0) {
    return <span className="text-sm text-ink-5">none</span>
  }
  return (
    <table className="w-full text-sm">
      <thead>
        <tr className="text-left text-xs font-semibold uppercase tracking-wider text-ink-5">
          <th className="pb-2 pr-4">Status</th>
          <th className="pb-2 pr-4">Spec</th>
          <th className="pb-2 pr-4">Segment</th>
          <th className="pb-2 pr-4">Processed</th>
          <th className="pb-2 pr-4">Outputs</th>
          <th className="pb-2">ID</th>
        </tr>
      </thead>
      <tbody>
        {compactions.map((c) => (
          <tr key={c.id} className="border-t border-ink-7/50">
            <td className="py-1.5 pr-4">
              <StatusBadge status={c.status} />
            </td>
            <td className="py-1.5 pr-4 font-mono text-xs">{specText(c)}</td>
            <td className="py-1.5 pr-4 font-mono text-xs text-ink-4">
              {c.segment ? keyText(c.segment) : '—'}
            </td>
            <td className="py-1.5 pr-4">{formatBytes(c.bytes_processed)}</td>
            <td className="py-1.5 pr-4">
              {c.output_ssts.length > 0
                ? `${c.output_ssts.length} SSTs · ${formatBytes(
                    c.output_ssts.reduce((acc, o) => acc + o.est_bytes, 0),
                  )}`
                : '—'}
            </td>
            <td className="py-1.5 font-mono text-xs text-ink-4">{c.id}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

export default function Compactions() {
  const stateQuery = useCompactorState()
  const historyQuery = useCompactions()

  return (
    <div>
      <h1 className="text-3xl">Compactions</h1>
      <div className="mt-6 space-y-6">
        <QueryGate query={stateQuery}>
          {(s) => (
            <Panel
              title="Current state"
              action={
                <span className="text-xs text-ink-4">
                  manifest #{s.manifest_id}
                  {s.compactions &&
                    ` · compactions v${s.compactions.id} · epoch ${s.compactions.compactor_epoch}`}
                </span>
              }
            >
              {s.compactions ? (
                <CompactionTable compactions={s.compactions.compactions} />
              ) : (
                <span className="text-sm text-ink-5">
                  No compactions file — the compactor has never run against
                  this DB.
                </span>
              )}
            </Panel>
          )}
        </QueryGate>

        <QueryGate query={historyQuery}>
          {(history) => (
            <Panel title="History">
              {history.length === 0 ? (
                <span className="text-sm text-ink-5">no compactions files</span>
              ) : (
                <div className="space-y-2">
                  {history.map((vc) => (
                    <details
                      key={vc.id}
                      className="rounded-md border border-ink-7/60"
                    >
                      <summary className="cursor-pointer select-none px-3 py-2 text-sm hover:bg-surface-2">
                        <span className="font-medium">v{vc.id}</span>
                        <span className="ml-3 text-ink-4">
                          epoch {vc.compactor_epoch} ·{' '}
                          {vc.compactions.length} recent compaction
                          {vc.compactions.length === 1 ? '' : 's'} ·{' '}
                          {vc.compactions.filter((c) => c.active).length} active
                        </span>
                      </summary>
                      <div className="border-t border-ink-7/60 p-3">
                        <CompactionTable compactions={vc.compactions} />
                      </div>
                    </details>
                  ))}
                </div>
              )}
            </Panel>
          )}
        </QueryGate>
      </div>
    </div>
  )
}
