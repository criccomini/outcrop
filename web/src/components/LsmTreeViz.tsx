import { useMemo } from 'react'
import type { SstViewDto, TreeDto } from '../api/types'
import { formatBytes } from '../lib/format'
import { keyText } from './KeyDisplay'

const RUN_COLORS = ['#3d4856', '#5e6878', '#8b94a3', '#586e84', '#70869c']
const L0_COLOR = '#b26844'

interface Level {
  label: string
  ssts: SstViewDto[]
  bytes: number
  isL0: boolean
  color: string
}

function buildLevels(tree: TreeDto): Level[] {
  return [
    { label: 'L0', ssts: tree.l0, bytes: tree.l0_bytes, isL0: true, color: L0_COLOR },
    ...tree.runs.map((r, i) => ({
      label: `SR ${r.id}`,
      ssts: r.ssts,
      bytes: r.est_bytes,
      isL0: false,
      color: RUN_COLORS[i % RUN_COLORS.length],
    })),
  ]
}

function sstUlid(sst: SstViewDto): string | null {
  return sst.sst_id.kind === 'compacted' ? sst.sst_id.ulid : null
}

function sstTitle(sst: SstViewDto): string {
  return [
    `${keyText(sst.first_key)} … ${keyText(sst.last_key)}`,
    formatBytes(sst.est_bytes),
    sst.view_id,
  ].join('\n')
}

/** Bars scaled by size: how much data lives in each level. */
export function SizeView({
  tree,
  selected,
  onSelect,
}: {
  tree: TreeDto
  selected: string | null
  onSelect: (ulid: string) => void
}) {
  const levels = buildLevels(tree)
  const maxBytes = Math.max(...levels.map((l) => l.bytes), 1)
  return (
    <div className="space-y-1.5">
      {levels.map((level) => (
        <div key={level.label} className="flex items-center gap-3">
          <LevelLabel level={level} />
          {/* The percentage must resolve against this track, not the row:
              sized against the row, the 100% bar gets flex-shrunk while
              smaller bars don't, distorting the proportions. */}
          <div className="min-w-0 flex-1">
            <div
              className="flex h-7 gap-px"
              style={{ width: `${Math.max((level.bytes / maxBytes) * 100, 1)}%` }}
            >
              {level.ssts.map((sst) => (
                <button
                  key={sst.view_id}
                  title={sstTitle(sst)}
                  onClick={() => {
                    const ulid = sstUlid(sst)
                    if (ulid) onSelect(ulid)
                  }}
                  className={`min-w-0.5 rounded-[2px] transition-opacity hover:opacity-75 ${
                    selected === sstUlid(sst) ? 'ring-2 ring-accent-high' : ''
                  }`}
                  style={{
                    backgroundColor: level.color,
                    flexGrow: Math.max(sst.est_bytes, 1),
                  }}
                />
              ))}
              {level.ssts.length === 0 && (
                <div className="self-center text-xs text-ink-5">empty</div>
              )}
            </div>
          </div>
        </div>
      ))}
    </div>
  )
}

/**
 * Spans positioned in the global key space, rank-scaled so skewed keyspaces
 * stay readable. Overlap between L0 SSTs (and across levels) reads directly
 * as read amplification.
 */
export function KeyRangeView({
  tree,
  selected,
  onSelect,
}: {
  tree: TreeDto
  selected: string | null
  onSelect: (ulid: string) => void
}) {
  // Memoized so the rank map below is only rebuilt when the tree changes,
  // not on every selection-induced re-render.
  const levels = useMemo(() => buildLevels(tree), [tree])

  const pos = useMemo(() => {
    // Hex strings compare in byte order, so ranking hex keys ranks keys.
    const keys = new Set<string>()
    for (const level of levels) {
      for (const sst of level.ssts) {
        if (sst.first_key) keys.add(sst.first_key.hex)
        if (sst.last_key) keys.add(sst.last_key.hex)
      }
    }
    const sorted = [...keys].sort()
    const rank = new Map(sorted.map((k, i) => [k, i]))
    const n = Math.max(sorted.length - 1, 1)
    return (hex: string) => (rank.get(hex) ?? 0) / n
  }, [levels])

  return (
    <div className="space-y-1.5">
      {levels.map((level) => (
        <div key={level.label} className="flex items-center gap-3">
          <LevelLabel level={level} />
          <div className="relative h-5 flex-1 rounded-sm bg-surface-2">
            {level.ssts.map((sst) => {
              if (!sst.first_key || !sst.last_key) return null
              const left = pos(sst.first_key.hex) * 100
              const width = Math.max(
                pos(sst.last_key.hex) * 100 - left,
                0.4,
              )
              return (
                <button
                  key={sst.view_id}
                  title={sstTitle(sst)}
                  onClick={() => {
                    const ulid = sstUlid(sst)
                    if (ulid) onSelect(ulid)
                  }}
                  className={`absolute top-0.5 bottom-0.5 rounded-[2px] transition-opacity hover:opacity-100 ${
                    selected === sstUlid(sst) ? 'ring-2 ring-accent-high' : ''
                  }`}
                  style={{
                    left: `${left}%`,
                    width: `${width}%`,
                    backgroundColor: level.color,
                    opacity: level.isL0 ? 0.55 : 0.9,
                  }}
                />
              )
            })}
          </div>
        </div>
      ))}
    </div>
  )
}

function LevelLabel({ level }: { level: Level }) {
  return (
    <div className="w-28 shrink-0 text-right">
      <span className="text-sm font-semibold text-ink-2">{level.label}</span>
      <span className="ml-2 text-xs text-ink-4">
        {level.ssts.length} · {formatBytes(level.bytes)}
      </span>
    </div>
  )
}
