import { Link } from 'react-router-dom'
import { useDbPath, useOverview } from '../api/client'
import { GarbagePanel } from '../components/GarbagePanel'
import { Stat } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { formatBytes, formatRelative, formatTime } from '../lib/format'

export default function Overview() {
  const query = useOverview()
  const dbPath = useDbPath()
  return (
    <div>
      <h1 className="text-3xl">Overview</h1>
      <QueryGate query={query}>
        {(o) => (
          <div className="mt-6">
            <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
              <Stat
                label="Total size"
                value={formatBytes(o.est_total_bytes)}
                sub={`L0: ${formatBytes(o.l0_bytes)}`}
              />
              <Stat
                label="SSTs"
                value={o.sst_count.toLocaleString()}
                sub={`${o.l0_count} in L0 · ${o.sorted_run_count} sorted runs`}
              />
              <Stat
                label="Manifest"
                value={
                  <Link
                    to={dbPath(`/manifests/${o.manifest_id}`)}
                    className="text-accent hover:text-accent-high"
                  >
                    #{o.manifest_id}
                  </Link>
                }
                sub={`${o.manifest_count} versions retained${
                  o.oldest_manifest_id !== undefined
                    ? ` (oldest #${o.oldest_manifest_id})`
                    : ''
                }`}
              />
              <Stat
                label="Last L0 write"
                value={
                  o.last_l0_approx_time
                    ? formatRelative(o.last_l0_approx_time)
                    : '—'
                }
                sub={
                  o.last_l0_approx_time
                    ? `${formatTime(o.last_l0_approx_time)} · seq ${o.last_l0_seq.toLocaleString()}`
                    : `seq ${o.last_l0_seq.toLocaleString()}`
                }
              />
              <Stat
                label="WAL window"
                value={(
                  o.next_wal_sst_id - 1 - o.replay_after_wal_id
                ).toLocaleString()}
                sub={`replay after #${o.replay_after_wal_id} · next #${o.next_wal_sst_id}`}
              />
              <Stat
                label="Epochs"
                value={`w${o.writer_epoch} / c${o.compactor_epoch}`}
                sub="writer / compactor"
              />
              <Stat
                label="Checkpoints"
                value={
                  <Link
                    to={dbPath('/checkpoints')}
                    className="text-accent hover:text-accent-high"
                  >
                    {o.checkpoint_count}
                  </Link>
                }
                sub={
                  o.clone_count > 0
                    ? `${o.clone_count} clone parent${o.clone_count > 1 ? 's' : ''}`
                    : 'no clone parents'
                }
              />
              <Stat
                label="Segments"
                value={o.segment_count || '—'}
                sub={
                  o.wal_object_store_uri
                    ? `WAL store: ${o.wal_object_store_uri}`
                    : 'single object store'
                }
              />
            </div>
          </div>
        )}
      </QueryGate>
      <div className="mt-6">
        <GarbagePanel
          action={
            <Link
              to={dbPath('/garbage')}
              className="text-xs text-accent hover:text-accent-high"
            >
              details →
            </Link>
          }
        />
      </div>
    </div>
  )
}
