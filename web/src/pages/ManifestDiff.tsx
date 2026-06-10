import { Link, useSearchParams } from 'react-router-dom'
import { useManifestDiff } from '../api/client'
import type { ManifestDiffDto, SstViewDto } from '../api/types'
import { KeyDisplay } from '../components/KeyDisplay'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { formatBytes } from '../lib/format'

function narrative(d: ManifestDiffDto): string {
  const parts: string[] = []
  if (d.l0_added.length) parts.push(`${d.l0_added.length} L0 SST${d.l0_added.length > 1 ? 's' : ''} flushed`)
  if (d.l0_removed.length && d.runs_added.length)
    parts.push(
      `${d.l0_removed.length} L0 SST${d.l0_removed.length > 1 ? 's' : ''} compacted into ${d.runs_added.map((r) => `SR ${r.id}`).join(', ')}`,
    )
  else {
    if (d.l0_removed.length) parts.push(`${d.l0_removed.length} L0 SSTs removed`)
    if (d.runs_added.length) parts.push(`runs added: ${d.runs_added.map((r) => `SR ${r.id}`).join(', ')}`)
  }
  if (d.runs_removed.length) parts.push(`runs removed: ${d.runs_removed.map((r) => `SR ${r.id}`).join(', ')}`)
  if (d.runs_changed.length) parts.push(`${d.runs_changed.length} run${d.runs_changed.length > 1 ? 's' : ''} changed`)
  if (d.checkpoints_added.length) parts.push(`${d.checkpoints_added.length} checkpoint${d.checkpoints_added.length > 1 ? 's' : ''} added`)
  if (d.checkpoints_removed.length) parts.push(`${d.checkpoints_removed.length} checkpoint${d.checkpoints_removed.length > 1 ? 's' : ''} removed`)
  if (parts.length === 0) parts.push('only scalar fields changed')
  return parts.join('; ')
}

function SstChips({ ssts, tone }: { ssts: SstViewDto[]; tone: 'add' | 'del' }) {
  if (ssts.length === 0) return <span className="text-sm text-ink-5">none</span>
  const style =
    tone === 'add'
      ? 'bg-accent-low text-accent-high'
      : 'bg-surface-2 text-ink-4 line-through'
  return (
    <div className="flex flex-wrap gap-1.5">
      {ssts.map((s) => (
        <span
          key={s.view_id}
          className={`rounded-md px-2 py-0.5 font-mono text-xs ${style}`}
          title={s.view_id}
        >
          <KeyDisplay k={s.first_key} />
          <span className="mx-1">…</span>
          <KeyDisplay k={s.last_key} />
          <span className="ml-1.5 opacity-70">{formatBytes(s.est_bytes)}</span>
        </span>
      ))}
    </div>
  )
}

export default function ManifestDiff() {
  const [params] = useSearchParams()
  const a = Number(params.get('a'))
  const b = Number(params.get('b'))
  const query = useManifestDiff(a, b)

  return (
    <div>
      <div className="flex items-baseline gap-4">
        <h1 className="text-3xl">
          Diff{' '}
          <Link to={`/manifests/${a}`} className="text-accent hover:text-accent-high">
            #{a}
          </Link>{' '}
          →{' '}
          <Link to={`/manifests/${b}`} className="text-accent hover:text-accent-high">
            #{b}
          </Link>
        </h1>
        <Link to="/manifests" className="text-sm text-accent hover:text-accent-high">
          ← all manifests
        </Link>
      </div>
      <QueryGate query={query}>
        {(d) => (
          <div className="mt-6 space-y-6">
            <div className="rounded-lg border border-accent/30 bg-accent-low px-4 py-2.5 text-sm text-accent-high">
              {narrative(d)}
            </div>

            <Panel title="L0">
              <div className="space-y-3">
                <div>
                  <div className="mb-1 text-xs font-semibold uppercase tracking-wider text-ink-5">
                    Added
                  </div>
                  <SstChips ssts={d.l0_added} tone="add" />
                </div>
                <div>
                  <div className="mb-1 text-xs font-semibold uppercase tracking-wider text-ink-5">
                    Removed
                  </div>
                  <SstChips ssts={d.l0_removed} tone="del" />
                </div>
              </div>
            </Panel>

            <Panel title="Sorted runs">
              <div className="space-y-3 text-sm">
                {d.runs_added.map((r) => (
                  <div key={`a${r.id}`}>
                    <span className="rounded-md bg-accent-low px-2 py-0.5 font-medium text-accent-high">
                      + SR {r.id}
                    </span>
                    <span className="ml-2 text-ink-4">
                      {r.sst_count} SSTs · {formatBytes(r.est_bytes)}
                    </span>
                  </div>
                ))}
                {d.runs_removed.map((r) => (
                  <div key={`r${r.id}`}>
                    <span className="rounded-md bg-surface-2 px-2 py-0.5 font-medium text-ink-4 line-through">
                      − SR {r.id}
                    </span>
                    <span className="ml-2 text-ink-4">
                      {r.sst_count} SSTs · {formatBytes(r.est_bytes)}
                    </span>
                  </div>
                ))}
                {d.runs_changed.map((r) => (
                  <div key={`c${r.id}`} className="space-y-1.5">
                    <div className="font-medium text-ink-2">SR {r.id} changed</div>
                    <SstChips ssts={r.ssts_added} tone="add" />
                    <SstChips ssts={r.ssts_removed} tone="del" />
                  </div>
                ))}
                {d.runs_added.length + d.runs_removed.length + d.runs_changed.length ===
                  0 && <span className="text-ink-5">no changes</span>}
              </div>
            </Panel>

            {(d.checkpoints_added.length > 0 ||
              d.checkpoints_removed.length > 0 ||
              d.checkpoints_changed.length > 0) && (
              <Panel title="Checkpoints">
                <div className="space-y-1.5 text-sm">
                  {d.checkpoints_added.map((c) => (
                    <div key={c.id}>
                      <span className="text-accent-high">+ {c.name ?? c.id}</span>
                      <span className="ml-2 text-ink-4">→ manifest #{c.manifest_id}</span>
                    </div>
                  ))}
                  {d.checkpoints_removed.map((c) => (
                    <div key={c.id}>
                      <span className="text-ink-4 line-through">− {c.name ?? c.id}</span>
                    </div>
                  ))}
                  {d.checkpoints_changed.map((c) => (
                    <div key={c.id}>
                      <span className="font-mono text-xs">{c.id}</span>
                      <span className="ml-2 text-ink-4">
                        manifest #{c.manifest_id[0]} → #{c.manifest_id[1]}
                      </span>
                    </div>
                  ))}
                </div>
              </Panel>
            )}

            {(d.external_dbs_added.length > 0 || d.external_dbs_removed.length > 0) && (
              <Panel title="Clone references">
                <div className="space-y-1.5 text-sm">
                  {d.external_dbs_added.map((e) => (
                    <div key={e.source_checkpoint_id} className="text-accent-high">
                      + {e.path} ({e.sst_count} shared SSTs)
                    </div>
                  ))}
                  {d.external_dbs_removed.map((e) => (
                    <div key={e.source_checkpoint_id} className="text-ink-4 line-through">
                      − {e.path}
                    </div>
                  ))}
                </div>
              </Panel>
            )}

            <Panel title="Scalar fields">
              {d.scalars.length === 0 ? (
                <span className="text-sm text-ink-5">no changes</span>
              ) : (
                <table className="w-full text-sm">
                  <thead>
                    <tr className="text-left text-xs font-semibold uppercase tracking-wider text-ink-5">
                      <th className="pb-2 pr-4">Field</th>
                      <th className="pb-2 pr-4">#{d.a}</th>
                      <th className="pb-2">#{d.b}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {d.scalars.map((s) => (
                      <tr key={s.field} className="border-t border-ink-7/50">
                        <td className="py-1 pr-4 font-mono text-xs">{s.field}</td>
                        <td className="py-1 pr-4 font-mono text-xs text-ink-4">{s.a}</td>
                        <td className="py-1 font-mono text-xs text-ink-1">{s.b}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </Panel>
          </div>
        )}
      </QueryGate>
    </div>
  )
}
