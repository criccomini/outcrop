import { useEffect, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import { useLsm, useManifestIds } from '../api/client'
import { HelpTip } from '../components/HelpTip'
import { KeyRangeView, SizeView } from '../components/LsmTreeViz'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { SstDetailDrawer } from '../components/SstDetailDrawer'
import { keyText } from '../components/KeyDisplay'
import { formatTime } from '../lib/format'
import type { TreeDto } from '../api/types'

export default function Lsm() {
  const [params, setParams] = useSearchParams()
  const rawId = params.get('manifest_id')
  const manifestId = rawId !== null ? Number(rawId) : undefined
  const query = useLsm(manifestId)
  const ids = useManifestIds()
  const [selected, setSelected] = useState<string | null>(null)
  const [segmentIdx, setSegmentIdx] = useState<number>(-1)

  // A different manifest may have different segments, and its SSTs may since
  // have been GC'd — reset the drill-down state when the view target moves.
  useEffect(() => {
    setSelected(null)
    setSegmentIdx(-1)
  }, [manifestId])

  function viewManifest(id: number | undefined) {
    setParams(id === undefined ? {} : { manifest_id: String(id) }, {
      replace: true,
    })
  }

  const idList = ids.data ?? []
  const scrubIdx =
    manifestId === undefined
      ? idList.length - 1
      : idList.findIndex((e) => e.id === manifestId)

  return (
    <div>
      <h1 className="text-3xl">LSM Tree</h1>
      <QueryGate query={query}>
        {(lsm) => {
          const hasSegments = lsm.segments.length > 0
          // In a segmented DB the root tree only matters if something
          // actually landed there (keys without an extractable prefix);
          // otherwise hide its tab and default to the first segment.
          const rootEmpty = lsm.tree.l0.length === 0 && lsm.tree.runs.length === 0
          const showRoot = !hasSegments || !rootEmpty
          const effIdx = segmentIdx >= 0 ? segmentIdx : showRoot ? -1 : 0
          const tree: TreeDto =
            hasSegments && effIdx >= 0 && effIdx < lsm.segments.length
              ? lsm.segments[effIdx].tree
              : lsm.tree
          const historical = manifestId !== undefined
          return (
            <div className="mt-6 space-y-6">
              <div className="flex flex-wrap items-center gap-x-6 gap-y-2 text-sm text-ink-4">
                <span>
                  As of manifest{' '}
                  <span className="font-mono">#{lsm.manifest_id}</span>
                </span>
                {idList.length > 1 && (
                  <span className="flex items-center gap-3">
                    <input
                      type="range"
                      min={0}
                      max={idList.length - 1}
                      value={scrubIdx >= 0 ? scrubIdx : idList.length - 1}
                      onChange={(e) => {
                        const idx = Number(e.target.value)
                        const entry = idList[idx]
                        if (!entry) return
                        viewManifest(
                          idx === idList.length - 1 ? undefined : entry.id,
                        )
                      }}
                      className="w-56 accent-[#b26844]"
                    />
                    <span className="text-xs text-ink-5">
                      #{idList[0]?.id} … #{idList[idList.length - 1]?.id}
                    </span>
                  </span>
                )}
                {lsm.segment_extractor_name && (
                  <span>
                    segment extractor:{' '}
                    <span className="font-mono">
                      {lsm.segment_extractor_name}
                    </span>
                  </span>
                )}
              </div>

              {historical && (
                <div className="flex items-center gap-3 rounded-lg border border-accent/30 bg-accent-low px-4 py-2 text-sm text-accent-high">
                  <span>
                    Viewing historical manifest{' '}
                    <span className="font-mono">#{lsm.manifest_id}</span>
                    {scrubIdx >= 0 && idList[scrubIdx] && (
                      <> from {formatTime(idList[scrubIdx].last_modified)}</>
                    )}{' '}
                    — polling paused.
                  </span>
                  <button
                    onClick={() => viewManifest(undefined)}
                    className="rounded-md bg-accent px-2.5 py-0.5 text-xs font-medium text-white transition-colors hover:bg-accent-high"
                  >
                    Back to latest
                  </button>
                </div>
              )}

              {hasSegments && (
                <div className="flex flex-wrap gap-1">
                  {showRoot && (
                    <SegmentTab
                      label="root"
                      active={effIdx === -1}
                      onClick={() => setSegmentIdx(-1)}
                    />
                  )}
                  {lsm.segments.map((seg, i) => (
                    <SegmentTab
                      key={seg.prefix.hex}
                      label={keyText(seg.prefix)}
                      active={effIdx === i}
                      onClick={() => setSegmentIdx(i)}
                    />
                  ))}
                </div>
              )}

              <Panel
                title="Levels by size"
                action={
                  <HelpTip>
                    Bar length is proportional to level size; segments within
                    a bar are individual SSTs. Click an SST for details.
                  </HelpTip>
                }
              >
                <SizeView tree={tree} selected={selected} onSelect={setSelected} />
              </Panel>

              <Panel
                title="Key-range coverage"
                action={
                  <HelpTip>
                    Horizontal position is the key space (rank-scaled).
                    Translucent overlapping spans in L0 are SSTs a point read
                    may have to consult — vertical overlap reads as read
                    amplification.
                  </HelpTip>
                }
              >
                <KeyRangeView
                  tree={tree}
                  selected={selected}
                  onSelect={setSelected}
                />
              </Panel>
            </div>
          )
        }}
      </QueryGate>
      {selected && (
        <SstDetailDrawer ulid={selected} onClose={() => setSelected(null)} />
      )}
    </div>
  )
}

function SegmentTab({
  label,
  active,
  onClick,
}: {
  label: string
  active: boolean
  onClick: () => void
}) {
  return (
    <button
      onClick={onClick}
      className={`rounded-md px-3 py-1 font-mono text-xs transition-colors ${
        active
          ? 'bg-accent-low text-accent-high'
          : 'bg-surface-2 text-ink-3 hover:bg-surface-3'
      }`}
    >
      {label}
    </button>
  )
}
