import { Link } from 'react-router-dom'
import { useCompactionJob, useDbPath } from '../api/client'
import { specText } from '../components/CompactionJobs'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { StatusBadge } from '../components/StatusBadge'
import { keyText } from '../components/KeyDisplay'
import { formatBytes, formatRelative, formatTime } from '../lib/format'
import { ulidTimeMs } from '../lib/ulid'

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex justify-between gap-4 border-t border-ink-7/50 py-1.5 text-sm first:border-t-0">
      <span className="shrink-0 text-ink-4">{label}</span>
      <span className="min-w-0 break-all text-right text-ink-2">{children}</span>
    </div>
  )
}

export default function CompactionJob({ id }: { id: string }) {
  const query = useCompactionJob(id)
  const dbPath = useDbPath()
  const startedMs = ulidTimeMs(id)
  const startedIso = startedMs !== null ? new Date(startedMs).toISOString() : undefined
  return (
    <div>
      <div className="flex items-baseline justify-between gap-4">
        <h1 className="text-3xl">Compaction job</h1>
        <Link
          to={dbPath('/compactions')}
          className="text-sm text-accent hover:text-accent-high"
        >
          ← all compactions
        </Link>
      </div>
      <div className="mt-1 font-mono text-xs text-ink-4">{id}</div>
      <div className="mt-6 space-y-5">
        <QueryGate query={query}>
          {(c) => (
            <>
              <Panel title="Job">
                <Row label="Status">
                  <StatusBadge status={c.status} />
                </Row>
                {startedIso && (
                  <Row label="Started">
                    {formatTime(startedIso)} ({formatRelative(startedIso)})
                  </Row>
                )}
                <Row label="Spec">
                  <span className="font-mono text-xs">{specText(c)}</span>
                </Row>
                {c.segment && (
                  <Row label="Segment">
                    <span className="font-mono text-xs">{keyText(c.segment)}</span>
                  </Row>
                )}
                <Row label="Drain">{c.is_drain ? 'yes' : 'no'}</Row>
                <Row label="Active">{c.active ? 'yes' : 'no'}</Row>
                <Row label="Bytes processed">{formatBytes(c.bytes_processed)}</Row>
              </Panel>
              <Panel title={`Output SSTs (${c.output_ssts.length})`}>
                {c.output_ssts.length === 0 ? (
                  <span className="text-sm text-ink-5">
                    none recorded{c.active ? ' yet' : ''}
                  </span>
                ) : (
                  <ol className="divide-y divide-ink-7/50">
                    {c.output_ssts.map((o) => (
                      <li
                        key={o.sst_id.kind === 'compacted' ? o.sst_id.ulid : String(o.sst_id.id)}
                        className="flex items-baseline gap-3 py-1.5 text-sm"
                      >
                        {o.sst_id.kind === 'compacted' ? (
                          <Link
                            to={dbPath(`/lsm?sst=${o.sst_id.ulid}`)}
                            className="font-mono text-xs text-accent hover:text-accent-high"
                          >
                            {o.sst_id.ulid}
                          </Link>
                        ) : (
                          <span className="font-mono text-xs">wal #{o.sst_id.id}</span>
                        )}
                        <span className="ml-auto shrink-0 text-xs text-ink-4">
                          {formatBytes(o.est_bytes)}
                        </span>
                      </li>
                    ))}
                  </ol>
                )}
              </Panel>
            </>
          )}
        </QueryGate>
      </div>
    </div>
  )
}
