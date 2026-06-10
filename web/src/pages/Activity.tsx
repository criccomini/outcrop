import { Fragment } from 'react'
import { Link } from 'react-router-dom'
import { useActivity, useCompactions, useCompactorState } from '../api/client'
import type { ActivityDto, VersionedCompactionsDto } from '../api/types'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { StatusBadge } from '../components/StatusBadge'
import { formatBytes, formatRelative, formatTime } from '../lib/format'
import { classify, compactionJobSummary, isPureFlush, isScalarOnly } from '../lib/feed'
import type { FeedKind } from '../lib/feed'
import { ulidTimeMs } from '../lib/ulid'

const KIND_STYLE: Record<FeedKind, string> = {
  flush: 'border-ink-6 bg-surface-2 text-ink-3',
  compaction: 'border-accent/40 bg-accent-low text-accent-high',
  gc: 'border-accent/40 bg-surface-2 text-accent-high',
  checkpoint: 'border-ink-6 bg-surface-1 text-ink-3',
  clone: 'border-ink-6 bg-surface-1 text-ink-3',
  meta: 'border-transparent bg-transparent text-ink-5',
}

const KIND_LABEL: Record<FeedKind, string> = {
  flush: 'flush',
  compaction: 'compaction',
  gc: 'GC',
  checkpoint: 'checkpoint',
  clone: 'clone',
  meta: 'bookkeeping',
}

interface Row {
  key: string
  at: number
  /** Oldest transition time when this row coalesces several. */
  atOldest?: number
  kind: FeedKind
  text: string
  link: { to: string; label: string }
  /** Compactor-job status badge (failed jobs only). */
  badge?: string
}

const sum = (ns: number[]) => ns.reduce((a, b) => a + b, 0)
const plural = (n: number) => (n > 1 ? 's' : '')

/**
 * Transitions, newest first, with consecutive flush- or bookkeeping-only
 * entries coalesced into one row that links to the combined range diff.
 * Anything structural (compaction, checkpoint, GC, clone) stays its own row.
 */
function transitionRows(items: ActivityDto[]): Row[] {
  const rows: Row[] = []
  let run: ActivityDto[] = []

  const flushRun = () => {
    if (run.length === 0) return
    const newest = run[0]
    const oldest = run[run.length - 1]
    const l0 = run.flatMap((it) => it.diff.l0_added)
    const row: Row = {
      key: `t${newest.b}`,
      at: Date.parse(newest.at),
      kind: l0.length ? 'flush' : 'meta',
      text: l0.length
        ? `${l0.length} L0 SST${plural(l0.length)} flushed · ${formatBytes(sum(l0.map((s) => s.est_bytes)))}`
        : `${run.length} bookkeeping update${plural(run.length)} (scalars only)`,
      link: {
        to: `/manifests/diff?a=${oldest.a}&b=${newest.b}`,
        label: `#${oldest.a} → #${newest.b}`,
      },
    }
    if (run.length > 1) row.atOldest = Date.parse(oldest.at)
    rows.push(row)
    run = []
  }

  for (const it of items) {
    if (isPureFlush(it.diff) || isScalarOnly(it.diff)) {
      run.push(it)
      continue
    }
    flushRun()
    const { kind, text } = classify(it.diff, it.at)
    rows.push({
      key: `t${it.b}`,
      at: Date.parse(it.at),
      kind,
      text,
      link: { to: `/manifests/diff?a=${it.a}&b=${it.b}`, label: `#${it.a} → #${it.b}` },
    })
  }
  flushRun()
  return rows
}

/**
 * Failed compactor jobs in the visible window. Completed jobs are omitted —
 * their result is already in the feed as a compaction transition — and
 * running ones live in the "compacting now" strip instead.
 */
function failedJobRows(
  versions: VersionedCompactionsDto[],
  oldestAt: number,
): Row[] {
  const seen = new Set<string>()
  const rows: Row[] = []
  for (const v of versions) {
    for (const c of v.compactions) {
      if (seen.has(c.id)) continue
      seen.add(c.id)
      if (c.status !== 'failed') continue
      const at = ulidTimeMs(c.id)
      if (at === null || at < oldestAt) continue
      rows.push({
        key: `c${c.id}`,
        at,
        kind: 'compaction',
        text: compactionJobSummary(c),
        link: { to: '/compactions', label: 'details' },
        badge: 'failed',
      })
    }
  }
  return rows
}

/** "3h" style duration for quiet-gap dividers. */
function gapText(ms: number): string {
  const m = Math.round(ms / 60_000)
  if (m < 60) return `${m}m`
  const h = Math.round(m / 60)
  if (h < 24) return `${h}h`
  return `${Math.round(h / 24)}d`
}

const QUIET_GAP_MS = 15 * 60_000

function KindChip({ kind }: { kind: FeedKind }) {
  return (
    <span
      className={`w-24 shrink-0 rounded-full border px-2 py-0.5 text-center text-xs font-medium ${KIND_STYLE[kind]}`}
    >
      {KIND_LABEL[kind]}
    </span>
  )
}

function CompactingNow() {
  const state = useCompactorState()
  const active = state.data?.compactions?.compactions.filter((c) => c.active) ?? []
  if (active.length === 0) return null
  return (
    <div className="mb-4 space-y-1.5">
      {active.map((c) => (
        <div
          key={c.id}
          className="flex items-baseline gap-3 rounded-lg border border-accent/40 bg-accent-low px-4 py-2 text-sm text-accent-high"
        >
          <span className="text-xs font-semibold uppercase tracking-wider opacity-70">
            compacting now
          </span>
          <span className="min-w-0">
            {compactionJobSummary(c)}
            {c.bytes_processed > 0 && (
              <span className="ml-2 text-xs opacity-80">
                {formatBytes(c.bytes_processed)} processed
              </span>
            )}
          </span>
          <Link
            to="/compactions"
            className="ml-auto shrink-0 text-xs underline-offset-2 hover:underline"
          >
            details
          </Link>
        </div>
      ))}
    </div>
  )
}

export default function Activity() {
  const query = useActivity(50)
  const compactions = useCompactions()
  return (
    <div>
      <h1 className="text-3xl">Activity</h1>
      <div className="mt-6">
        <CompactingNow />
        <QueryGate query={query}>
          {(items) => {
            const rows = transitionRows(items)
            if (rows.length > 0) {
              const oldestAt = Math.min(...rows.map((r) => r.atOldest ?? r.at))
              rows.push(...failedJobRows(compactions.data ?? [], oldestAt))
              rows.sort((a, b) => b.at - a.at)
            }
            return (
              <Panel>
                {rows.length === 0 ? (
                  <span className="text-sm text-ink-5">
                    Only one manifest version retained — no transitions to show
                    yet.
                  </span>
                ) : (
                  <ol>
                    {rows.map((row, i) => {
                      const prev = rows[i - 1]
                      const gap =
                        prev !== undefined ? (prev.atOldest ?? prev.at) - row.at : 0
                      return (
                        <Fragment key={row.key}>
                          {gap > QUIET_GAP_MS && (
                            <li className="border-t border-ink-7/50 py-1.5 text-center text-xs text-ink-5">
                              quiet for {gapText(gap)}
                            </li>
                          )}
                          <li
                            className={`flex items-baseline gap-3 py-2.5 ${i > 0 ? 'border-t border-ink-7/50' : ''}`}
                          >
                            <span
                              className="w-20 shrink-0 text-right text-xs text-ink-4"
                              title={
                                row.atOldest !== undefined
                                  ? `${formatTime(new Date(row.atOldest).toISOString())} – ${formatTime(new Date(row.at).toISOString())}`
                                  : formatTime(new Date(row.at).toISOString())
                              }
                            >
                              {formatRelative(new Date(row.at).toISOString())}
                            </span>
                            <KindChip kind={row.kind} />
                            <span className="min-w-0 text-sm text-ink-2">
                              {row.text}
                              {row.badge && (
                                <span className="ml-2">
                                  <StatusBadge status={row.badge} />
                                </span>
                              )}
                            </span>
                            <Link
                              to={row.link.to}
                              className="ml-auto shrink-0 font-mono text-xs text-accent hover:text-accent-high"
                            >
                              {row.link.label}
                            </Link>
                          </li>
                        </Fragment>
                      )
                    })}
                  </ol>
                )}
                <p className="mt-3 text-xs text-ink-5">
                  Newest first. Runs of consecutive flushes are grouped into one
                  entry; every entry links to its manifest diff. In-flight
                  compactions appear in the strip above, failed ones in the
                  feed; completed compactions show as transitions.
                </p>
              </Panel>
            )
          }}
        </QueryGate>
      </div>
    </div>
  )
}
