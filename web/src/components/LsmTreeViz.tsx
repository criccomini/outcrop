import { useMemo } from 'react'
import type { KeyDto, LevelSummaryDto, SstViewDto } from '../api/types'
import { formatBytes } from '../lib/format'
import { keyText } from './KeyDisplay'

const RUN_COLORS = ['#3d4856', '#5e6878', '#8b94a3', '#586e84', '#70869c']
const L0_COLOR = '#b26844'

function levelColor(level: LevelSummaryDto, index: number): string {
  // L0 is always the first level, so runs start at index 1.
  return level.is_l0 ? L0_COLOR : RUN_COLORS[(index - 1) % RUN_COLORS.length]
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
  levels,
  selected,
  onSelect,
}: {
  levels: LevelSummaryDto[]
  selected: string | null
  onSelect: (ulid: string) => void
}) {
  const maxBytes = Math.max(...levels.map((l) => l.est_bytes), 1)
  return (
    <div className="space-y-1.5">
      {levels.map((level, i) => {
        const color = levelColor(level, i)
        return (
          <div key={level.label} className="flex items-center gap-3">
            <LevelLabel level={level} />
            {/* The percentage must resolve against this track, not the row:
                sized against the row, the 100% bar gets flex-shrunk while
                smaller bars don't, distorting the proportions. */}
            <div className="min-w-0 flex-1">
              <div
                className="flex h-7 gap-px"
                style={{
                  width: `${Math.max((level.est_bytes / maxBytes) * 100, 1)}%`,
                }}
              >
                {level.ssts !== undefined ? (
                  level.ssts.map((sst) => (
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
                        backgroundColor: color,
                        flexGrow: Math.max(sst.est_bytes, 1),
                      }}
                    />
                  ))
                ) : (
                  <div
                    title={`${level.sst_count.toLocaleString()} SSTs · ${formatBytes(level.est_bytes)}`}
                    className="flex-1 rounded-[2px]"
                    style={{ backgroundColor: color }}
                  />
                )}
                {level.sst_count === 0 && (
                  <div className="self-center text-xs text-ink-5">empty</div>
                )}
              </div>
            </div>
          </div>
        )
      })}
    </div>
  )
}

/**
 * Levels positioned in the global key space, rank-scaled via the summary's
 * bucket edges so skewed keyspaces stay readable. Small levels render each
 * SST as a clickable span; large ones render their coverage histogram, with
 * darkness = how many SSTs deep a read at that key goes (read amplification).
 */
export function KeyRangeView({
  levels,
  bucketKeys,
  selected,
  onSelect,
}: {
  levels: LevelSummaryDto[]
  bucketKeys: KeyDto[]
  selected: string | null
  onSelect: (ulid: string) => void
}) {
  // Position a key at bucket resolution by binary-searching the bucket
  // edges — the same x-axis the histograms use, so detailed and
  // aggregated levels line up.
  const posOf = useMemo(() => {
    const edges = bucketKeys.map((k) => k.hex)
    return (hex: string) => {
      if (edges.length < 2) return 0
      let lo = 0
      let hi = edges.length
      while (lo < hi) {
        const mid = (lo + hi) >> 1
        if (edges[mid] <= hex) lo = mid + 1
        else hi = mid
      }
      return Math.max(lo - 1, 0) / (edges.length - 1)
    }
  }, [bucketKeys])

  return (
    <div className="space-y-1.5">
      {levels.map((level, i) => {
        const color = levelColor(level, i)
        return (
          <div key={level.label} className="flex items-center gap-3">
            <LevelLabel level={level} />
            <div className="relative h-5 flex-1 overflow-hidden rounded-sm bg-surface-2">
              {level.ssts !== undefined ? (
                level.ssts.map((sst) => {
                  if (!sst.first_key || !sst.last_key) return null
                  const left = posOf(sst.first_key.hex) * 100
                  const width = Math.max(
                    posOf(sst.last_key.hex) * 100 - left,
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
                        backgroundColor: color,
                        opacity: level.is_l0 ? 0.55 : 0.9,
                      }}
                    />
                  )
                })
              ) : (
                <CoverageStrip
                  level={level}
                  color={color}
                  bucketKeys={bucketKeys}
                />
              )}
            </div>
          </div>
        )
      })}
    </div>
  )
}

/** Histogram strip for a level too large to draw per-SST. */
function CoverageStrip({
  level,
  color,
  bucketKeys,
}: {
  level: LevelSummaryDto
  color: string
  bucketKeys: KeyDto[]
}) {
  const maxDepth = Math.max(...level.coverage, 1)
  return (
    <div className="flex h-full w-full">
      {level.coverage.map((depth, i) => {
        const a = bucketKeys[i]
        const b = bucketKeys[i + 1]
        const range = a && b ? `≈ ${keyText(a)} … ${keyText(b)}` : ''
        return (
          <div
            key={i}
            title={
              depth > 0
                ? `${range}\n${depth} SST${depth === 1 ? '' : 's'} deep`
                : undefined
            }
            className="h-full flex-1"
            style={{
              backgroundColor: color,
              opacity: depth === 0 ? 0 : 0.3 + 0.6 * (depth / maxDepth),
            }}
          />
        )
      })}
    </div>
  )
}

function LevelLabel({ level }: { level: LevelSummaryDto }) {
  return (
    <div className="w-28 shrink-0 text-right">
      <span className="text-sm font-semibold text-ink-2">{level.label}</span>
      <span className="ml-2 text-xs text-ink-4">
        {level.sst_count.toLocaleString()} · {formatBytes(level.est_bytes)}
      </span>
    </div>
  )
}
