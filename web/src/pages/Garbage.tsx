import { Link } from 'react-router-dom'
import { useActivity, useGarbage } from '../api/client'
import type { GarbageDto } from '../api/types'
import { GarbagePanel } from '../components/GarbagePanel'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { classify } from '../lib/feed'
import { formatBytes, formatRelative, formatTime } from '../lib/format'

function Pinners({ g }: { g: GarbageDto }) {
  if (g.pinners.length === 0) {
    return (
      <span className="text-sm text-ink-5">
        No unexpired checkpoints — nothing is pinned beyond the latest
        manifest.
      </span>
    )
  }
  return (
    <>
      <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="text-left text-xs font-semibold uppercase tracking-wider text-ink-5">
            <th className="py-1 pr-2 font-semibold">Checkpoint</th>
            <th className="py-1 pr-2 font-semibold">Manifest</th>
            <th className="py-1 pr-2 font-semibold">Expires</th>
            <th className="py-1 font-semibold">Keeps alive beyond latest</th>
          </tr>
        </thead>
        <tbody>
          {g.pinners.map((p) => (
            <tr key={p.id} className="border-t border-ink-7/50">
              <td className="max-w-0 truncate py-1.5 pr-2" title={p.id}>
                {p.name ? (
                  <span className="text-ink-2">{p.name}</span>
                ) : (
                  <span className="font-mono text-xs text-ink-4">
                    {p.id.slice(0, 8)}
                  </span>
                )}
              </td>
              <td className="py-1.5 pr-2">
                {p.manifest_available ? (
                  <Link
                    to={`/manifests/${p.manifest_id}`}
                    className="font-mono text-xs text-accent hover:text-accent-high"
                  >
                    #{p.manifest_id}
                  </Link>
                ) : (
                  <span
                    className="font-mono text-xs text-red-800"
                    title="This manifest no longer exists"
                  >
                    #{p.manifest_id} missing
                  </span>
                )}
              </td>
              <td
                className="py-1.5 pr-2 text-ink-3"
                title={p.expire_time ? formatTime(p.expire_time) : undefined}
              >
                {p.expire_time ? formatRelative(p.expire_time) : 'never'}
              </td>
              <td className="py-1.5">
                {p.extra_count === 0 ? (
                  <span className="text-ink-5">—</span>
                ) : (
                  <span className="font-semibold text-ink-1">
                    {formatBytes(p.extra_bytes)}
                    <span className="ml-1 text-xs font-normal text-ink-4">
                      ({p.extra_count.toLocaleString()} object
                      {p.extra_count > 1 ? 's' : ''})
                    </span>
                  </span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>
      <div className="mt-3 text-xs text-ink-5">
        "Keeps alive" counts data objects referenced only via the checkpoint's
        manifest; checkpoints sharing a manifest report the same objects, so
        the column is not a disjoint sum. Releasing a checkpoint makes its
        objects reclaimable on the next GC sweep.
      </div>
    </>
  )
}

function RecentSweeps() {
  const query = useActivity(50)
  return (
    <QueryGate query={query}>
      {(items) => {
        const sweeps = items
          .map((it) => ({ it, c: classify(it.diff, it.at) }))
          .filter(({ c }) => c.kind === 'gc')
        if (sweeps.length === 0) {
          return (
            <span className="text-sm text-ink-5">
              No GC sweeps among the last {items.length} manifest transitions.
              Sweeps appear here when the GC strips expired checkpoints from
              the manifest; object deletions show up as the reclaimable
              numbers above going down.
            </span>
          )
        }
        return (
          <ol className="divide-y divide-ink-7/50">
            {sweeps.map(({ it, c }) => (
              <li
                key={it.b}
                className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 py-2"
              >
                <span
                  className="w-20 shrink-0 text-right text-xs text-ink-4"
                  title={formatTime(it.at)}
                >
                  {formatRelative(it.at)}
                </span>
                <span className="min-w-0 text-sm text-ink-2">{c.text}</span>
                <Link
                  to={`/manifests/diff?a=${it.a}&b=${it.b}`}
                  className="ml-auto shrink-0 font-mono text-xs text-accent hover:text-accent-high"
                >
                  #{it.a} → #{it.b}
                </Link>
              </li>
            ))}
          </ol>
        )
      }}
    </QueryGate>
  )
}

export default function Garbage() {
  const query = useGarbage()
  return (
    <div>
      <h1 className="text-3xl">Garbage Collection</h1>
      <div className="mt-6 space-y-5">
        <GarbagePanel />
        <Panel
          title="Recent GC sweeps"
          action={
            <Link
              to="/activity?kinds=gc"
              className="text-xs text-accent hover:text-accent-high"
            >
              view in activity →
            </Link>
          }
        >
          <RecentSweeps />
        </Panel>
        <Panel title="Checkpoints pinning storage">
          <QueryGate query={query}>{(g) => <Pinners g={g} />}</QueryGate>
        </Panel>
      </div>
    </div>
  )
}
