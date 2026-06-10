import { useOverview } from '../api/client'
import type { WarningDto } from '../api/types'
import { HelpTip } from '../components/HelpTip'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'

const SEVERITY_STYLES: Record<WarningDto['severity'], string> = {
  error: 'border-red-300 bg-red-50 text-red-800',
  warn: 'border-accent/40 bg-accent-low text-accent-high',
  info: 'border-ink-6 bg-surface-2 text-ink-3',
}

export default function Alerts() {
  const query = useOverview()
  return (
    <div>
      <h1 className="text-3xl">Alerts</h1>
      <div className="mt-6">
        <Panel
          action={
            <HelpTip>
              Computed from the latest manifest and object listings on every
              refresh: L0 backlog, WAL window growth, stale manifests,
              expired or dangling checkpoints.
            </HelpTip>
          }
        >
          <QueryGate query={query}>
            {(o) =>
              o.warnings.length === 0 ? (
                <span className="text-sm text-ink-5">
                  No alerts — the latest manifest looks healthy.
                </span>
              ) : (
                <ol className="divide-y divide-ink-7/50">
                  {o.warnings.map((w) => (
                    <li
                      key={w.code + w.message}
                      className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 py-2.5"
                    >
                      <span
                        className={`w-16 shrink-0 rounded-full border px-2 py-0.5 text-center text-xs font-medium ${SEVERITY_STYLES[w.severity]}`}
                      >
                        {w.severity}
                      </span>
                      <span className="min-w-0 flex-1 basis-52 text-sm text-ink-2">
                        {w.message}
                      </span>
                    </li>
                  ))}
                </ol>
              )
            }
          </QueryGate>
        </Panel>
      </div>
    </div>
  )
}
