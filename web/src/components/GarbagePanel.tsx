import { useGarbage } from '../api/client'
import type { GarbageCategoryDto, GarbageDto } from '../api/types'
import { formatBytes, formatRelative } from '../lib/format'
import { Panel } from './Panel'
import { QueryGate } from './QueryGate'

// Matches the LSM viz palette: slate for healthy data, copper for attention.
const LIVE_COLOR = '#5e6878'
const PINNED_COLOR = '#8b94a3'
const RECLAIMABLE_COLOR = '#b26844'

function StorageBar({ g }: { g: GarbageDto }) {
  const total = Math.max(g.stored_bytes, 1)
  const segments = [
    { label: 'live', bytes: g.live_bytes, color: LIVE_COLOR },
    { label: 'pinned', bytes: g.pinned_bytes, color: PINNED_COLOR },
    { label: 'reclaimable', bytes: g.reclaimable_bytes, color: RECLAIMABLE_COLOR },
  ].filter((s) => s.bytes > 0)
  return (
    <div>
      <div className="flex h-5 gap-px overflow-hidden rounded-sm bg-surface-2">
        {segments.map((s) => (
          <div
            key={s.label}
            title={`${s.label}: ${formatBytes(s.bytes)}`}
            style={{
              width: `${(s.bytes / total) * 100}%`,
              backgroundColor: s.color,
            }}
          />
        ))}
      </div>
      <div className="mt-1.5 flex flex-wrap gap-x-4 gap-y-1 text-xs text-ink-4">
        {segments.map((s) => (
          <span key={s.label} className="inline-flex items-center gap-1.5">
            <span
              className="inline-block h-2.5 w-2.5 rounded-[2px]"
              style={{ backgroundColor: s.color }}
            />
            {s.label} {formatBytes(s.bytes)}
          </span>
        ))}
      </div>
    </div>
  )
}

function CategoryRow({ label, c }: { label: string; c: GarbageCategoryDto }) {
  const cell = (count: number, bytes: number, highlight = false) =>
    count === 0 ? (
      <span className="text-ink-5">—</span>
    ) : (
      <span className={highlight ? 'font-medium text-accent-high' : undefined}>
        {formatBytes(bytes)}
        <span className="ml-1 text-xs text-ink-4">({count.toLocaleString()})</span>
      </span>
    )
  return (
    <tr className="border-t border-ink-7/50">
      <td className="py-1.5 pr-2 text-ink-2">{label}</td>
      <td className="py-1.5 pr-2">{cell(c.stored_count, c.stored_bytes)}</td>
      <td className="py-1.5 pr-2">{cell(c.live_count, c.live_bytes)}</td>
      <td className="py-1.5 pr-2">{cell(c.pinned_count, c.pinned_bytes)}</td>
      <td className="py-1.5">{cell(c.reclaimable_count, c.reclaimable_bytes, true)}</td>
    </tr>
  )
}

export function GarbagePanel() {
  const query = useGarbage()
  return (
    <Panel title="Storage &amp; garbage">
      <QueryGate query={query}>
        {(g) => (
          <div>
            <div className="mb-4 grid grid-cols-2 gap-4 md:grid-cols-3">
              <div>
                <div className="text-xs font-semibold uppercase tracking-wider text-ink-5">
                  Space amplification
                </div>
                <div className="mt-1 text-2xl text-ink-1">
                  {g.space_amp !== undefined ? `${g.space_amp.toFixed(2)}×` : '—'}
                </div>
                <div className="mt-0.5 text-xs text-ink-4">
                  {formatBytes(g.compacted.stored_bytes + g.wal.stored_bytes)} stored
                  / {formatBytes(g.compacted.live_bytes + g.wal.live_bytes)} live
                </div>
              </div>
              <div>
                <div className="text-xs font-semibold uppercase tracking-wider text-ink-5">
                  Reclaimable
                </div>
                <div
                  className={`mt-1 text-2xl ${
                    g.reclaimable_bytes > 0 ? 'text-accent-high' : 'text-ink-1'
                  }`}
                >
                  {formatBytes(g.reclaimable_bytes)}
                </div>
                <div className="mt-0.5 text-xs text-ink-4">
                  {g.oldest_reclaimable_at
                    ? `oldest written ${formatRelative(g.oldest_reclaimable_at)}`
                    : 'nothing for the GC to delete'}
                </div>
              </div>
              <div>
                <div className="text-xs font-semibold uppercase tracking-wider text-ink-5">
                  Pinned by checkpoints
                </div>
                <div className="mt-1 text-2xl text-ink-1">
                  {formatBytes(g.pinned_bytes)}
                </div>
                <div className="mt-0.5 text-xs text-ink-4">
                  {g.live_checkpoint_count} live · {g.expired_checkpoint_count} expired
                  {g.dangling_checkpoint_count > 0
                    ? ` · ${g.dangling_checkpoint_count} dangling`
                    : ''}
                </div>
              </div>
            </div>
            <StorageBar g={g} />
            <table className="mt-4 w-full text-sm">
              <thead>
                <tr className="text-left text-xs font-semibold uppercase tracking-wider text-ink-5">
                  <th className="py-1 pr-2 font-semibold">Objects</th>
                  <th className="py-1 pr-2 font-semibold">Stored</th>
                  <th className="py-1 pr-2 font-semibold">Live</th>
                  <th className="py-1 pr-2 font-semibold">Pinned</th>
                  <th className="py-1 font-semibold">Reclaimable</th>
                </tr>
              </thead>
              <tbody>
                <CategoryRow label="Compacted SSTs" c={g.compacted} />
                <CategoryRow label="WAL SSTs" c={g.wal} />
                <CategoryRow label="Manifests" c={g.manifests} />
              </tbody>
            </table>
            <div className="mt-3 text-xs text-ink-5">
              Live = referenced by the latest manifest; pinned = kept only by an
              unexpired checkpoint; reclaimable = what the garbage collector would
              eventually delete (its min-age grace periods are ignored here).
            </div>
          </div>
        )}
      </QueryGate>
    </Panel>
  )
}
