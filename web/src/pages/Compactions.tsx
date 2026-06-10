import { useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { useCompactions, useCompactorState, useDbPath } from '../api/client'
import { outputsText, specText } from '../components/CompactionJobs'
import { HelpTip } from '../components/HelpTip'
import { keyText } from '../components/KeyDisplay'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { StatusBadge } from '../components/StatusBadge'
import { formatBytes, formatRelative, formatTime } from '../lib/format'
import { ulidTimeMs } from '../lib/ulid'

export default function Compactions() {
  const stateQuery = useCompactorState()
  const historyQuery = useCompactions()
  const navigate = useNavigate()
  const dbPath = useDbPath()
  // In pick order, capped at two: a third pick replaces the oldest one.
  const [picked, setPicked] = useState<number[]>([])
  // The diff itself is always oldest → newest.
  const [a, b] = [...picked].sort((x, y) => x - y)

  function toggle(id: number) {
    setPicked((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev.slice(-1), id],
    )
  }

  return (
    <div>
      <div className="flex items-center justify-between gap-3">
        <h1 className="text-3xl">Compactions</h1>
        <div className="flex items-center gap-3">
          {stateQuery.data?.compactions && (
            <span className="font-mono text-sm text-ink-4">
              epoch {stateQuery.data.compactions.compactor_epoch}
            </span>
          )}
          <button
            disabled={picked.length !== 2}
            onClick={() => navigate(dbPath(`/compactions/diff?a=${a}&b=${b}`))}
            className="rounded-md bg-accent px-4 py-1.5 text-sm font-medium text-white transition-colors hover:bg-accent-high disabled:cursor-not-allowed disabled:bg-ink-6"
          >
            Diff {picked.length === 2 ? `v${a} → v${b}` : 'two versions'}
          </button>
        </div>
      </div>
      <div className="mt-6">
        <QueryGate query={historyQuery}>
          {(history) => {
            const anySegment = history.some((vc) =>
              vc.compactions.some((c) => c.segment),
            )
            // checkbox + Started/Status/Spec/(Segment)/Processed/Outputs/ID
            const jobCols = 6 + (anySegment ? 1 : 0)
            return (
              <Panel
                action={
                  <HelpTip>
                    Newest first; each group is one .compactions file version
                    with its recent compactor jobs beneath. Pick two versions
                    to diff them, or open one for its full contents.
                  </HelpTip>
                }
              >
                {history.length === 0 ? (
                  <span className="text-sm text-ink-5">
                    No compactions file — the compactor has never run against
                    this DB.
                  </span>
                ) : (
                  <div className="overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="text-left text-xs font-semibold uppercase tracking-wider text-ink-5">
                        <th className="pb-2 pr-2"></th>
                        <th className="pb-2 pr-4">Started</th>
                        <th className="pb-2 pr-4">Status</th>
                        <th className="pb-2 pr-4">Spec</th>
                        {anySegment && <th className="pb-2 pr-4">Segment</th>}
                        <th className="pb-2 pr-4">Processed</th>
                        <th className="pb-2 pr-4">Outputs</th>
                        <th className="pb-2">ID</th>
                      </tr>
                    </thead>
                    {history.map((vc) => (
                      <tbody key={vc.id} className="border-t border-ink-7/50">
                        <tr>
                          <td className="py-2 pr-2">
                            <input
                              type="checkbox"
                              checked={picked.includes(vc.id)}
                              onChange={() => toggle(vc.id)}
                              className="accent-[#b26844]"
                            />
                          </td>
                          <td colSpan={jobCols} className="py-2">
                            <Link
                              to={dbPath(`/compactions/${vc.id}`)}
                              className="font-mono text-accent hover:text-accent-high"
                            >
                              v{vc.id}
                            </Link>
                            <span className="ml-3 text-xs text-ink-4">
                              epoch {vc.compactor_epoch} ·{' '}
                              {vc.compactions.length} job
                              {vc.compactions.length === 1 ? '' : 's'} ·{' '}
                              {vc.compactions.filter((c) => c.active).length}{' '}
                              active
                            </span>
                          </td>
                        </tr>
                        {vc.compactions.map((c) => {
                          const at = ulidTimeMs(c.id)
                          const iso =
                            at !== null ? new Date(at).toISOString() : undefined
                          return (
                            <tr key={c.id} className="border-t border-ink-7/30">
                              <td className="pr-2"></td>
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
                              <td className="py-1.5 pr-4">{outputsText(c)}</td>
                              <td className="py-1.5 font-mono text-xs">
                                <Link
                                  to={dbPath(`/compactions/job/${c.id}`)}
                                  className="text-accent hover:text-accent-high"
                                >
                                  {c.id}
                                </Link>
                              </td>
                            </tr>
                          )
                        })}
                      </tbody>
                    ))}
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
