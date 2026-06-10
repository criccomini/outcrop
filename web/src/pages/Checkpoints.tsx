import { Link } from 'react-router-dom'
import { useCheckpoints, useClones, useDbPath } from '../api/client'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { formatRelative, formatTime } from '../lib/format'

function ExpiryCell({ expireTime }: { expireTime?: string }) {
  if (!expireTime) return <span className="text-ink-4">never</span>
  const remaining = new Date(expireTime).getTime() - Date.now()
  const cls =
    remaining < 0
      ? 'text-red-700 font-medium'
      : remaining < 3600_000
        ? 'text-accent-high font-medium'
        : 'text-ink-2'
  return (
    <span className={cls} title={formatTime(expireTime)}>
      {remaining < 0 ? 'expired' : formatRelative(expireTime)}
    </span>
  )
}

export default function Checkpoints() {
  const checkpoints = useCheckpoints()
  const clones = useClones()
  const dbPath = useDbPath()

  return (
    <div>
      <h1 className="text-3xl">Checkpoints</h1>
      <div className="mt-6 space-y-6">
        <QueryGate query={checkpoints}>
          {(cps) => (
            <Panel title={`Checkpoints (${cps.length})`}>
              {cps.length === 0 ? (
                <span className="text-sm text-ink-5">no checkpoints</span>
              ) : (
                <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="text-left text-xs font-semibold uppercase tracking-wider text-ink-5">
                      <th className="pb-2 pr-4">Name</th>
                      <th className="pb-2 pr-4">Manifest</th>
                      <th className="pb-2 pr-4">Created</th>
                      <th className="pb-2 pr-4">Expires</th>
                      <th className="pb-2">ID</th>
                    </tr>
                  </thead>
                  <tbody>
                    {cps.map((c) => (
                      <tr key={c.id} className="border-t border-ink-7/50">
                        <td className="py-1.5 pr-4">
                          {c.name ?? <span className="text-ink-5">—</span>}
                        </td>
                        <td className="py-1.5 pr-4">
                          {c.manifest_available ? (
                            <Link
                              to={dbPath(`/manifests/${c.manifest_id}`)}
                              className="font-mono text-accent hover:text-accent-high"
                            >
                              #{c.manifest_id}
                            </Link>
                          ) : (
                            <span
                              className="font-mono text-ink-5"
                              title="manifest no longer in object store"
                            >
                              #{c.manifest_id} (gone)
                            </span>
                          )}
                        </td>
                        <td className="py-1.5 pr-4" title={formatTime(c.create_time)}>
                          {formatRelative(c.create_time)}
                        </td>
                        <td className="py-1.5 pr-4">
                          <ExpiryCell expireTime={c.expire_time} />
                        </td>
                        <td className="py-1.5 font-mono text-xs text-ink-4">{c.id}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                </div>
              )}
            </Panel>
          )}
        </QueryGate>

        <QueryGate query={clones}>
          {(ext) => (
            <Panel title={`Clone parents (${ext.length})`}>
              {ext.length === 0 ? (
                <span className="text-sm text-ink-5">
                  This DB was not cloned from another DB.
                </span>
              ) : (
                <div className="space-y-3">
                  {ext.map((e) => (
                    <div
                      key={e.source_checkpoint_id}
                      className="rounded-md border border-ink-7/60 p-3 text-sm"
                    >
                      <div className="font-mono text-ink-1">{e.path}</div>
                      <div className="mt-1 text-xs text-ink-4">
                        source checkpoint{' '}
                        <span className="font-mono">{e.source_checkpoint_id}</span>
                        {e.final_checkpoint_id && (
                          <>
                            {' '}
                            · final checkpoint{' '}
                            <span className="font-mono">{e.final_checkpoint_id}</span>
                          </>
                        )}
                        {' '}· {e.sst_count} shared SSTs ·{' '}
                        {e.detached ? (
                          <span className="font-medium text-ink-2">detached</span>
                        ) : (
                          <span className="font-medium text-accent-high">
                            still references parent
                          </span>
                        )}
                      </div>
                    </div>
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
