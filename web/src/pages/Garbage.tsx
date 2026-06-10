import { Link } from 'react-router-dom'
import { useActivity, useGarbage } from '../api/client'
import type { GarbageDto } from '../api/types'
import { GarbagePanel } from '../components/GarbagePanel'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { classify } from '../lib/feed'
import { formatBytes, formatRelative, formatTime } from '../lib/format'

/**
 * Banners derived from the garbage report. GC runs embedded in a writer or
 * standalone process the dashboard can't see, so "is it running?" is
 * inferred from the evidence it leaves behind.
 */
function HealthHints({ g }: { g?: GarbageDto }) {
  if (!g) return null
  const hints: { key: string; style: string; text: string }[] = []
  const warn = 'border-accent/40 bg-accent-low text-accent-high'
  const info = 'border-ink-7 bg-surface-2 text-ink-3'
  if (g.expired_checkpoint_count > 0) {
    hints.push({
      key: 'expired',
      style: warn,
      text: `${g.expired_checkpoint_count} expired checkpoint${g.expired_checkpoint_count > 1 ? 's' : ''} still recorded in the manifest — the GC removes these on its next sweep.`,
    })
  }
  if (g.dangling_checkpoint_count > 0) {
    hints.push({
      key: 'dangling',
      style: warn,
      text: `${g.dangling_checkpoint_count} unexpired checkpoint${g.dangling_checkpoint_count > 1 ? 's' : ''} reference a manifest that no longer exists — readers cannot open ${g.dangling_checkpoint_count > 1 ? 'them' : 'it'}.`,
    })
  }
  const oldest = g.oldest_reclaimable_at ? Date.parse(g.oldest_reclaimable_at) : NaN
  if (
    g.reclaimable_bytes > 0 &&
    !Number.isNaN(oldest) &&
    Date.now() - oldest > 30 * 60_000
  ) {
    hints.push({
      key: 'stale-garbage',
      style: info,
      text: `The oldest reclaimable object was written ${formatRelative(g.oldest_reclaimable_at)}. With slatedb's default GC cadence (sweep every minute, 5-minute min age) this backlog suggests no GC is currently running against this DB.`,
    })
  }
  if (hints.length === 0) return null
  return (
    <div className="space-y-1.5">
      {hints.map((h) => (
        <div key={h.key} className={`rounded-lg border px-4 py-2 text-sm ${h.style}`}>
          {h.text}
        </div>
      ))}
    </div>
  )
}

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
                  <span className="font-medium text-accent-high">
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
              <li key={it.b} className="flex items-baseline gap-3 py-2">
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
        <HealthHints g={query.data} />
        <GarbagePanel />
        <Panel title="Checkpoints pinning storage">
          <QueryGate query={query}>{(g) => <Pinners g={g} />}</QueryGate>
        </Panel>
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
      </div>
    </div>
  )
}
