import { Link } from 'react-router-dom'
import { useDbPath, useManifest } from '../api/client'
import { JsonTree } from '../components/JsonTree'
import { Panel, Stat } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { formatBytes } from '../lib/format'

export default function ManifestDetail({ id }: { id: string }) {
  const query = useManifest(id)
  const dbPath = useDbPath()

  return (
    <div>
      <div className="flex items-baseline gap-4">
        <h1 className="text-3xl">Manifest #{id}</h1>
        <Link to={dbPath('/manifests')} className="text-sm text-accent hover:text-accent-high">
          ← all manifests
        </Link>
      </div>
      <QueryGate query={query}>
        {(m) => {
          const sstCount =
            m.tree.l0.length +
            m.tree.runs.reduce((acc, r) => acc + r.ssts.length, 0)
          return (
            <div className="mt-6 space-y-6">
              <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
                <Stat label="Size" value={formatBytes(m.tree.total_bytes)} />
                <Stat
                  label="SSTs"
                  value={sstCount}
                  sub={`${m.tree.l0.length} in L0 · ${m.tree.runs.length} runs`}
                />
                <Stat
                  label="Checkpoints"
                  value={m.checkpoints.length}
                  sub={`${m.external_dbs.length} clone refs`}
                />
                <Stat
                  label="Epochs"
                  value={`w${m.writer_epoch} / c${m.compactor_epoch}`}
                  sub={m.initialized ? 'initialized' : 'NOT initialized'}
                />
              </div>
              <Panel title="Contents">
                <div className="overflow-x-auto">
                  <JsonTree value={m} />
                </div>
              </Panel>
            </div>
          )
        }}
      </QueryGate>
    </div>
  )
}
