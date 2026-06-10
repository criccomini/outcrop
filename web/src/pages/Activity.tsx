import { Link } from 'react-router-dom'
import { useActivity, useCompactions } from '../api/client'
import type { ActivityDto, CompactionDto } from '../api/types'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { StatusBadge } from '../components/StatusBadge'
import { formatBytes, formatRelative, formatTime } from '../lib/format'
import { narrative } from '../lib/narrative'
import { ulidTimeMs } from '../lib/ulid'

type FeedItem =
  | { kind: 'manifest'; at: number; item: ActivityDto }
  | { kind: 'compaction'; at: number; c: CompactionDto }

/**
 * Compactor jobs interleaved into the transition feed. Jobs are deduped by
 * ULID across `.compactions` versions (newest version wins, so the latest
 * status shows) and timestamped from the ULID. Jobs older than the oldest
 * visible transition are dropped so the merged feed covers one window.
 */
function compactionEvents(
  versions: { compactions: CompactionDto[] }[],
  oldestAt: number,
): FeedItem[] {
  const seen = new Set<string>()
  const events: FeedItem[] = []
  for (const v of versions) {
    for (const c of v.compactions) {
      if (seen.has(c.id)) continue
      seen.add(c.id)
      const at = ulidTimeMs(c.id)
      if (at !== null && at >= oldestAt) events.push({ kind: 'compaction', at, c })
    }
  }
  return events
}

function compactionSummary(c: CompactionDto): string {
  const l0 = c.sources.filter((s) => s.kind === 'l0').length
  const sources = [
    ...(l0 ? [`${l0} L0 SST${l0 > 1 ? 's' : ''}`] : []),
    ...c.sources.filter((s) => s.kind === 'sorted_run').map((s) => `SR ${s.id}`),
  ]
  const dest = c.destination !== undefined ? `SR ${c.destination}` : '—'
  return `${c.is_drain ? 'drain ' : ''}${sources.join(' + ')} → ${dest}`
}

export default function Activity() {
  const query = useActivity()
  const compactions = useCompactions()
  return (
    <div>
      <h1 className="text-3xl">Activity</h1>
      <QueryGate query={query}>
        {(items) => {
          const feed: FeedItem[] = items.map((item) => ({
            kind: 'manifest',
            at: Date.parse(item.at),
            item,
          }))
          if (feed.length > 0) {
            const oldestAt = Math.min(...feed.map((f) => f.at))
            feed.push(...compactionEvents(compactions.data ?? [], oldestAt))
          }
          feed.sort((a, b) => b.at - a.at)
          return (
            <div className="mt-6">
              <Panel>
                {feed.length === 0 ? (
                  <span className="text-sm text-ink-5">
                    Only one manifest version retained — no transitions to show
                    yet.
                  </span>
                ) : (
                  <ol className="divide-y divide-ink-7/50">
                    {feed.map((f) =>
                      f.kind === 'manifest' ? (
                        <li
                          key={`m${f.item.b}`}
                          className="flex items-baseline gap-4 py-2.5"
                        >
                          <span
                            className="w-20 shrink-0 text-right text-xs text-ink-4"
                            title={formatTime(f.item.at)}
                          >
                            {formatRelative(f.item.at)}
                          </span>
                          <Link
                            to={`/manifests/diff?a=${f.item.a}&b=${f.item.b}`}
                            className="shrink-0 font-mono text-xs text-accent hover:text-accent-high"
                          >
                            #{f.item.a} → #{f.item.b}
                          </Link>
                          <span className="min-w-0 text-sm text-ink-2">
                            {narrative(f.item.diff, f.item.at)}
                          </span>
                        </li>
                      ) : (
                        <li
                          key={`c${f.c.id}`}
                          className="flex items-baseline gap-4 py-2.5"
                        >
                          <span
                            className="w-20 shrink-0 text-right text-xs text-ink-4"
                            title={formatTime(new Date(f.at).toISOString())}
                          >
                            {formatRelative(new Date(f.at).toISOString())}
                          </span>
                          <Link
                            to="/compactions"
                            className="shrink-0 font-mono text-xs text-accent hover:text-accent-high"
                          >
                            compaction
                          </Link>
                          <span className="min-w-0 text-sm text-ink-2">
                            {compactionSummary(f.c)}
                            {f.c.bytes_processed > 0 && (
                              <span className="ml-2 text-xs text-ink-4">
                                {formatBytes(f.c.bytes_processed)} processed
                              </span>
                            )}
                            <span className="ml-2">
                              <StatusBadge status={f.c.status} />
                            </span>
                          </span>
                        </li>
                      ),
                    )}
                  </ol>
                )}
                <p className="mt-3 text-xs text-ink-5">
                  Newest first; manifest transitions interleaved with compactor
                  jobs, which are timestamped at their start. Click a
                  transition for the full diff.
                </p>
              </Panel>
            </div>
          )
        }}
      </QueryGate>
    </div>
  )
}
