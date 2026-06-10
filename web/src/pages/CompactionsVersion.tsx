import { Link } from 'react-router-dom'
import { useCompactionsVersion, useDbPath } from '../api/client'
import { CompactionJobsTable } from '../components/CompactionJobs'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'

export default function CompactionsVersion({ id }: { id: string }) {
  const query = useCompactionsVersion(Number(id))
  const dbPath = useDbPath()
  return (
    <div>
      <div className="flex items-baseline gap-4">
        <h1 className="text-3xl">Compactions v{id}</h1>
        <Link
          to={dbPath('/compactions')}
          className="text-sm text-accent hover:text-accent-high"
        >
          ← all compactions
        </Link>
      </div>
      <div className="mt-6">
        <QueryGate query={query}>
          {(vc) =>
            vc === undefined ? (
              <Panel>
                <span className="text-sm text-ink-5">
                  Version v{id} not found (possibly GC'd).
                </span>
              </Panel>
            ) : (
              <Panel
                title={`Jobs (${vc.compactions.length})`}
                action={
                  <span className="text-xs text-ink-4">
                    epoch {vc.compactor_epoch} ·{' '}
                    {vc.compactions.filter((c) => c.active).length} active
                  </span>
                }
              >
                <CompactionJobsTable jobs={vc.compactions} />
              </Panel>
            )
          }
        </QueryGate>
      </div>
    </div>
  )
}
