import { useState } from 'react'
import { Link, Navigate } from 'react-router-dom'
import { useQueryClient } from '@tanstack/react-query'
import { dbUrl, rescanDbs, useDbs, useOverview } from '../api/client'
import type { DbInfoDto } from '../api/types'
import { HelpTip } from '../components/HelpTip'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { formatBytes, formatRelative } from '../lib/format'

function DbRow({ db }: { db: DbInfoDto }) {
  const overview = useOverview(db.id)
  const o = overview.data
  const warnCount =
    o?.warnings.filter((w) => w.severity !== 'info').length ?? 0
  return (
    <tr className="border-t border-ink-7/50 hover:bg-surface-2">
      <td className="py-2 pr-4">
        <Link
          to={dbUrl(db.id)}
          className="font-medium text-accent hover:text-accent-high"
        >
          {db.path}
        </Link>
        {warnCount > 0 && (
          <Link
            to={dbUrl(db.id, '/alerts')}
            className="ml-2 inline-block rounded-full bg-accent px-1.5 py-0.5 text-xs font-semibold leading-none text-white"
            title={`${warnCount} alert${warnCount > 1 ? 's' : ''}`}
          >
            {warnCount}
          </Link>
        )}
      </td>
      <td className="py-2 pr-4">
        <Link
          to={`/db/${db.store}`}
          className="rounded-full border border-ink-6 bg-surface-2 px-2 py-0.5 text-xs text-ink-3 hover:bg-surface-0"
        >
          {db.store}
        </Link>
      </td>
      <td className="py-2 pr-4">{o ? formatBytes(o.est_total_bytes) : '—'}</td>
      <td className="py-2 pr-4">
        {o
          ? `${o.sst_count.toLocaleString()} (${o.l0_count} L0 · ${o.sorted_run_count} runs)`
          : '—'}
      </td>
      <td className="py-2 pr-4">{o ? o.checkpoint_count : '—'}</td>
      <td
        className="py-2 text-ink-3"
        title={o?.latest_manifest_written_at}
      >
        {o?.latest_manifest_written_at
          ? formatRelative(o.latest_manifest_written_at)
          : '—'}
      </td>
    </tr>
  )
}

export default function Fleet({ store }: { store?: string }) {
  const query = useDbs()
  const queryClient = useQueryClient()
  const [rescanning, setRescanning] = useState(false)

  // With a single DB overall there's nothing to choose — jump straight in.
  // (Store-scoped listings always render the list.)
  if (store === undefined && query.data?.dbs.length === 1) {
    return <Navigate to={dbUrl(query.data.dbs[0].id)} replace />
  }

  const rescan = async () => {
    setRescanning(true)
    try {
      await rescanDbs(queryClient)
    } finally {
      setRescanning(false)
    }
  }

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-3xl">
          {store !== undefined ? (
            <>
              Databases in <span className="font-mono">{store}</span>
            </>
          ) : (
            'Databases'
          )}
        </h1>
        <button
          onClick={rescan}
          disabled={rescanning}
          className="rounded-md bg-accent px-4 py-1.5 text-sm font-medium text-white transition-colors hover:bg-accent-high disabled:cursor-not-allowed disabled:bg-ink-6"
        >
          {rescanning ? 'Scanning…' : 'Rescan'}
        </button>
      </div>
      <div className="mt-6">
        <QueryGate query={query}>
          {(all) => {
            const d =
              store === undefined
                ? all
                : { ...all, dbs: all.dbs.filter((db) => db.store === store) }
            return (
            <Panel
              action={
                <HelpTip>
                  Auto-discovered by walking the configured stores for
                  prefixes with a manifest/ directory (last scan{' '}
                  {formatRelative(d.scanned_at)}); rescans happen
                  automatically in the background, or on demand with the
                  Rescan button.
                </HelpTip>
              }
            >
              {d.dbs.length === 0 ? (
                <span className="text-sm text-ink-5">
                  {store !== undefined
                    ? `No SlateDBs discovered in store '${store}'. `
                    : 'No SlateDBs discovered under the configured roots. '}
                  DBs are detected by their{' '}
                  <span className="font-mono">manifest/</span> directory; use
                  Rescan after creating one.
                </span>
              ) : (
                <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="text-left text-xs font-semibold uppercase tracking-wider text-ink-5">
                      <th className="pb-2 pr-4">Database</th>
                      <th className="pb-2 pr-4">Store</th>
                      <th className="pb-2 pr-4">Size</th>
                      <th className="pb-2 pr-4">SSTs</th>
                      <th className="pb-2 pr-4">Checkpoints</th>
                      <th className="pb-2">Last write</th>
                    </tr>
                  </thead>
                  <tbody>
                    {d.dbs.map((db) => (
                      <DbRow key={db.id} db={db} />
                    ))}
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
