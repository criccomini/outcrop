import { Fragment, useMemo, useState } from 'react'
import { useLevelSlice } from '../api/client'
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
                    title={`${level.sst_count.toLocaleString()} SSTs · ${formatBytes(level.est_bytes)}\nToo many to draw — click a bucket in the key-range view to list SSTs`}
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
  manifestId,
  segment,
  selected,
  onSelect,
}: {
  levels: LevelSummaryDto[]
  bucketKeys: KeyDto[]
  manifestId: number
  segment?: number
  selected: string | null
  onSelect: (ulid: string) => void
}) {
  // Histogram-only levels drill down by bucket: clicking one fetches and
  // lists the SSTs overlapping that bucket's key range.
  const [probe, setProbe] = useState<{ label: string; bucket: number } | null>(
    null,
  )
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
        const probed = probe !== null && probe.label === level.label
        return (
          <Fragment key={level.label}>
            <div className="flex items-center gap-3">
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
                    activeBucket={probed ? probe.bucket : null}
                    onBucket={(bucket) =>
                      setProbe(
                        probed && probe.bucket === bucket
                          ? null
                          : { label: level.label, bucket },
                      )
                    }
                  />
                )}
              </div>
            </div>
            {probed && (
              <BucketSlice
                level={level}
                manifestId={manifestId}
                segment={segment}
                start={bucketKeys[probe.bucket]}
                end={bucketKeys[probe.bucket + 1]}
                selected={selected}
                onSelect={onSelect}
                onClose={() => setProbe(null)}
              />
            )}
          </Fragment>
        )
      })}
    </div>
  )
}

/**
 * Drill-down panel for one histogram bucket of a level too large for
 * inline per-SST detail: fetches the SSTs overlapping the bucket's key
 * range on demand and lists them as clickable chips.
 */
function BucketSlice({
  level,
  manifestId,
  segment,
  start,
  end,
  selected,
  onSelect,
  onClose,
}: {
  level: LevelSummaryDto
  manifestId: number
  segment?: number
  start?: KeyDto
  end?: KeyDto
  selected: string | null
  onSelect: (ulid: string) => void
  onClose: () => void
}) {
  const slice = useLevelSlice({
    manifestId,
    segment,
    run: level.run_id,
    start: start?.hex,
    end: end?.hex,
  })
  return (
    <div className="ml-[7.75rem] rounded-md border border-ink-7 bg-surface-2 px-3 py-2 text-xs">
      <div className="mb-1.5 flex items-baseline gap-2">
        <span className="font-semibold text-ink-2">{level.label}</span>
        <span className="text-ink-4">
          SSTs covering ≈ {start ? keyText(start) : '…'} …{' '}
          {end ? keyText(end) : '…'}
          {slice.data && ` (${slice.data.total.toLocaleString()})`}
        </span>
        <button
          onClick={onClose}
          className="ml-auto rounded px-1.5 text-ink-4 hover:bg-surface-3 hover:text-ink-1"
          aria-label="Close SST list"
        >
          ✕
        </button>
      </div>
      {slice.isPending && <div className="text-ink-4">Loading…</div>}
      {slice.error && (
        <div className="text-ink-4">{slice.error.message}</div>
      )}
      {slice.data && (
        <div className="flex flex-wrap gap-1">
          {slice.data.ssts.map((sst) => {
            const ulid = sstUlid(sst)
            return (
              <button
                key={sst.view_id}
                disabled={!ulid}
                title={sstTitle(sst)}
                onClick={() => {
                  if (ulid) onSelect(ulid)
                }}
                className={`rounded border bg-surface-1 px-2 py-0.5 font-mono transition-colors ${
                  selected !== null && selected === ulid
                    ? 'border-accent-high text-accent-high'
                    : 'border-ink-6 text-ink-3 hover:border-accent hover:text-ink-1'
                }`}
              >
                …{ulid ? ulid.slice(-6) : '?'} · {formatBytes(sst.est_bytes)}
              </button>
            )
          })}
          {slice.data.truncated && (
            <span className="self-center text-ink-5">
              +{(slice.data.total - slice.data.ssts.length).toLocaleString()}{' '}
              more not shown
            </span>
          )}
          {slice.data.total === 0 && (
            <span className="text-ink-5">no SSTs overlap this bucket</span>
          )}
        </div>
      )}
    </div>
  )
}

/**
 * Histogram strip for a level too large to draw per-SST. Buckets with any
 * coverage are clickable and open the per-bucket SST drill-down.
 */
function CoverageStrip({
  level,
  color,
  bucketKeys,
  activeBucket,
  onBucket,
}: {
  level: LevelSummaryDto
  color: string
  bucketKeys: KeyDto[]
  activeBucket: number | null
  onBucket: (bucket: number) => void
}) {
  const maxDepth = Math.max(...level.coverage, 1)
  return (
    <div className="flex h-full w-full">
      {level.coverage.map((depth, i) => {
        const a = bucketKeys[i]
        const b = bucketKeys[i + 1]
        const range = a && b ? `≈ ${keyText(a)} … ${keyText(b)}` : ''
        return (
          <button
            key={i}
            disabled={depth === 0}
            onClick={() => onBucket(i)}
            title={
              depth > 0
                ? `${range}\n${depth} SST${depth === 1 ? '' : 's'} deep\nClick to list SSTs`
                : undefined
            }
            className={`h-full flex-1 transition-opacity enabled:hover:opacity-60 ${
              activeBucket === i ? 'ring-2 ring-inset ring-accent-high' : ''
            }`}
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
