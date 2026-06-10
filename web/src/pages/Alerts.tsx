import { useOverview } from '../api/client'
import type { WarningDto } from '../api/types'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'

const SEVERITY_STYLES: Record<WarningDto['severity'], string> = {
  error: 'border-red-300 bg-red-50 text-red-800',
  warn: 'border-accent/40 bg-accent-low text-accent-high',
  info: 'border-ink-7 bg-surface-2 text-ink-3',
}

export default function Alerts() {
  const query = useOverview()
  return (
    <div>
      <h1 className="text-3xl">Alerts</h1>
      <div className="mt-6">
        <QueryGate query={query}>
          {(o) =>
            o.warnings.length === 0 ? (
              <Panel>
                <span className="text-sm text-ink-5">
                  No alerts — the latest manifest looks healthy.
                </span>
              </Panel>
            ) : (
              <div className="space-y-2">
                {o.warnings.map((w) => (
                  <div
                    key={w.code + w.message}
                    className={`rounded-lg border px-4 py-3 text-sm ${SEVERITY_STYLES[w.severity]}`}
                  >
                    <span className="mr-2 text-xs font-semibold uppercase tracking-wider opacity-70">
                      {w.severity}
                    </span>
                    {w.message}
                  </div>
                ))}
              </div>
            )
          }
        </QueryGate>
        <p className="mt-3 text-xs text-ink-5">
          Computed from the latest manifest and object listings on every
          refresh: L0 backlog, WAL window growth, stale manifests, expired or
          dangling checkpoints.
        </p>
      </div>
    </div>
  )
}
