import { useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { useCompactions, useCompactorState, useDbPath } from '../api/client'
import { CompactionJobsTable } from '../components/CompactionJobs'
import { HelpTip } from '../components/HelpTip'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'

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
          {(history) => (
            <Panel
              action={
                <HelpTip>
                  Newest first; each row is one .compactions file version
                  with its recent compactor jobs nested inside. Pick two
                  versions to diff them, or open one for its full contents.
                </HelpTip>
              }
            >
              {history.length === 0 ? (
                <span className="text-sm text-ink-5">
                  No compactions file — the compactor has never run against
                  this DB.
                </span>
              ) : (
                <ol className="divide-y divide-ink-7/50">
                  {history.map((vc) => (
                    <li key={vc.id} className="flex gap-3 py-3">
                      <input
                        type="checkbox"
                        checked={picked.includes(vc.id)}
                        onChange={() => toggle(vc.id)}
                        className="mt-1 accent-[#b26844]"
                      />
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5">
                          <Link
                            to={dbPath(`/compactions/${vc.id}`)}
                            className="font-mono text-accent hover:text-accent-high"
                          >
                            v{vc.id}
                          </Link>
                          <span className="text-xs text-ink-4">
                            epoch {vc.compactor_epoch} · {vc.compactions.length}{' '}
                            job{vc.compactions.length === 1 ? '' : 's'} ·{' '}
                            {vc.compactions.filter((c) => c.active).length} active
                          </span>
                        </div>
                        <div className="mt-2">
                          <CompactionJobsTable jobs={vc.compactions} />
                        </div>
                      </div>
                    </li>
                  ))}
                </ol>
              )}
            </Panel>
          )}
        </QueryGate>
      </div>
    </div>
  )
}
