import { useState } from 'react'
import { useWal } from '../api/client'
import { HelpTip } from '../components/HelpTip'
import { Panel, Stat } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { WalSstDrawer } from '../components/WalSstDrawer'
import { formatBytes, formatRelative, formatTime } from '../lib/format'

export default function Wal() {
  const query = useWal()
  const [selected, setSelected] = useState<number | null>(null)
  return (
    <div>
      <h1 className="text-3xl">WAL</h1>
      <QueryGate query={query}>
        {(wal) => {
          const window = Math.max(
            0,
            wal.next_wal_sst_id - 1 - wal.replay_after_wal_id,
          )
          return (
            <div className="mt-6 space-y-6">
              <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
                <Stat
                  label="WAL SSTs"
                  value={wal.entries.length.toLocaleString()}
                  sub={`${formatBytes(wal.total_bytes)} total`}
                />
                <Stat
                  label="Un-replayed window"
                  value={window.toLocaleString()}
                  sub={`replay after #${wal.replay_after_wal_id} · next #${wal.next_wal_sst_id}`}
                />
                <Stat
                  label="Newest write"
                  value={
                    wal.entries.length > 0
                      ? formatRelative(wal.entries[0].last_modified)
                      : '—'
                  }
                  sub={
                    wal.entries.length > 0
                      ? formatTime(wal.entries[0].last_modified)
                      : 'no WAL SSTs'
                  }
                />
                <Stat
                  label="WAL store"
                  value={wal.wal_object_store_uri ? 'separate' : 'main'}
                  sub={wal.wal_object_store_uri ?? 'same object store as the DB'}
                />
              </div>

              <Panel
                title={`WAL SSTs (${wal.entries.length})`}
                action={
                  <HelpTip>
                    Newest first. Un-replayed SSTs (id above the replay
                    watermark) would be re-read into memtables on writer
                    restart; replayed ones are awaiting garbage collection.
                  </HelpTip>
                }
              >
                {wal.entries.length === 0 ? (
                  <span className="text-sm text-ink-5">
                    {wal.wal_object_store_uri
                      ? 'No WAL SSTs in the main store — the WAL lives in a separate object store, which this dashboard does not list.'
                      : 'No WAL SSTs (all garbage-collected, or nothing written yet).'}
                  </span>
                ) : (
                  <div className="overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="text-left text-xs font-semibold uppercase tracking-wider text-ink-5">
                        <th className="pb-2 pr-4">ID</th>
                        <th className="pb-2 pr-4">Size</th>
                        <th className="pb-2 pr-4">Written</th>
                        <th className="pb-2">Status</th>
                      </tr>
                    </thead>
                    <tbody>
                      {wal.entries.map((e) => {
                        const unreplayed = e.id > wal.replay_after_wal_id
                        return (
                          <tr
                            key={e.id}
                            onClick={() =>
                              setSelected(selected === e.id ? null : e.id)
                            }
                            className={`cursor-pointer border-t border-ink-7/50 hover:bg-surface-2 ${
                              unreplayed ? 'bg-accent-low/40' : ''
                            } ${selected === e.id ? 'bg-surface-2' : ''}`}
                          >
                            <td className="py-1.5 pr-4 font-mono text-xs">
                              #{e.id}
                            </td>
                            <td className="py-1.5 pr-4">
                              {formatBytes(e.size_bytes)}
                            </td>
                            <td
                              className="py-1.5 pr-4 text-ink-3"
                              title={formatTime(e.last_modified)}
                            >
                              {formatRelative(e.last_modified)}
                            </td>
                            <td className="py-1.5">
                              {unreplayed ? (
                                <span className="font-medium text-ink-1">
                                  un-replayed
                                </span>
                              ) : (
                                <span className="text-ink-5">replayed</span>
                              )}
                            </td>
                          </tr>
                        )
                      })}
                    </tbody>
                  </table>
                  </div>
                )}
              </Panel>
              {selected !== null &&
                (() => {
                  const entry = wal.entries.find((e) => e.id === selected)
                  return entry ? (
                    <WalSstDrawer
                      entry={entry}
                      replayAfterWalId={wal.replay_after_wal_id}
                      onClose={() => setSelected(null)}
                    />
                  ) : null
                })()}
            </div>
          )
        }}
      </QueryGate>
    </div>
  )
}
