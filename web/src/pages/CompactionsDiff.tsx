import { Link, useSearchParams } from 'react-router-dom'
import { useCompactionsVersion, useDbPath } from '../api/client'
import type { CompactionDto } from '../api/types'
import { CompactionJobsTable, outputsText, specText } from '../components/CompactionJobs'
import { Panel } from '../components/Panel'
import { StatusBadge } from '../components/StatusBadge'
import { formatBytes } from '../lib/format'

interface JobChange {
  job: CompactionDto
  changes: string[]
}

/** Job-level diff of two immutable .compactions versions, keyed by ULID. */
function diffVersions(
  a: CompactionDto[],
  b: CompactionDto[],
): { added: CompactionDto[]; removed: CompactionDto[]; changed: JobChange[] } {
  const byId = new Map(a.map((c) => [c.id, c]))
  const added: CompactionDto[] = []
  const changed: JobChange[] = []
  for (const job of b) {
    const old = byId.get(job.id)
    if (old === undefined) {
      added.push(job)
      continue
    }
    const changes: string[] = []
    if (old.status !== job.status) changes.push(`status ${old.status} → ${job.status}`)
    if (old.bytes_processed !== job.bytes_processed)
      changes.push(
        `processed ${formatBytes(old.bytes_processed)} → ${formatBytes(job.bytes_processed)}`,
      )
    if (outputsText(old) !== outputsText(job))
      changes.push(`outputs ${outputsText(old)} → ${outputsText(job)}`)
    if (changes.length > 0) changed.push({ job, changes })
  }
  const inB = new Set(b.map((c) => c.id))
  const removed = a.filter((c) => !inB.has(c.id))
  return { added, removed, changed }
}

export default function CompactionsDiff() {
  const [params] = useSearchParams()
  const a = Number(params.get('a'))
  const b = Number(params.get('b'))
  const queryA = useCompactionsVersion(a)
  const queryB = useCompactionsVersion(b)
  const dbPath = useDbPath()

  const loading = queryA.isPending || queryB.isPending
  const error = queryA.error ?? queryB.error

  return (
    <div>
      <div className="flex items-baseline justify-between gap-4">
        <h1 className="text-3xl">
          Diff{' '}
          <Link
            to={dbPath(`/compactions/${a}`)}
            className="text-accent hover:text-accent-high"
          >
            v{a}
          </Link>{' '}
          →{' '}
          <Link
            to={dbPath(`/compactions/${b}`)}
            className="text-accent hover:text-accent-high"
          >
            v{b}
          </Link>
        </h1>
        <Link
          to={dbPath('/compactions')}
          className="text-sm text-accent hover:text-accent-high"
        >
          ← all compactions
        </Link>
      </div>
      <div className="mt-6 space-y-5">
        {loading ? (
          <div className="py-12 text-center text-ink-4">Loading…</div>
        ) : error ? (
          <div className="rounded-lg border border-accent/40 bg-accent-low px-4 py-3 text-accent-high">
            {error.message}
          </div>
        ) : queryA.data === undefined || queryB.data === undefined ? (
          <Panel>
            <span className="text-sm text-ink-5">
              {queryA.data === undefined ? `v${a}` : `v${b}`} not found
              (possibly GC'd).
            </span>
          </Panel>
        ) : (
          (() => {
            const { added, removed, changed } = diffVersions(
              queryA.data.compactions,
              queryB.data.compactions,
            )
            const epochChanged =
              queryA.data.compactor_epoch !== queryB.data.compactor_epoch
            return (
              <>
                <div className="rounded-lg border border-accent/30 bg-accent-low px-4 py-2 text-sm text-accent-high">
                  {added.length} job{added.length === 1 ? '' : 's'} added ·{' '}
                  {changed.length} changed · {removed.length} dropped
                  {epochChanged &&
                    ` · compactor epoch ${queryA.data.compactor_epoch} → ${queryB.data.compactor_epoch}`}
                </div>
                <Panel title={`Jobs added (${added.length})`}>
                  {added.length === 0 ? (
                    <span className="text-sm text-ink-5">none</span>
                  ) : (
                    <CompactionJobsTable jobs={added} />
                  )}
                </Panel>
                <Panel title={`Jobs changed (${changed.length})`}>
                  {changed.length === 0 ? (
                    <span className="text-sm text-ink-5">none</span>
                  ) : (
                    <ol className="divide-y divide-ink-7/50">
                      {changed.map(({ job, changes }) => (
                        <li
                          key={job.id}
                          className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 py-2"
                        >
                          <StatusBadge status={job.status} />
                          <span className="font-mono text-xs">{specText(job)}</span>
                          <span className="min-w-0 text-sm text-ink-3">
                            {changes.join('; ')}
                          </span>
                          <span className="ml-auto shrink-0 font-mono text-xs text-ink-5">
                            {job.id}
                          </span>
                        </li>
                      ))}
                    </ol>
                  )}
                </Panel>
                <Panel title={`Jobs dropped from the log (${removed.length})`}>
                  {removed.length === 0 ? (
                    <span className="text-sm text-ink-5">none</span>
                  ) : (
                    <CompactionJobsTable jobs={removed} />
                  )}
                </Panel>
              </>
            )
          })()
        )}
      </div>
    </div>
  )
}
