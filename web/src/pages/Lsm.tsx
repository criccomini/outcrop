import { useState } from 'react'
import { useLsm } from '../api/client'
import { KeyRangeView, SizeView } from '../components/LsmTreeViz'
import { Panel } from '../components/Panel'
import { QueryGate } from '../components/QueryGate'
import { SstDetailDrawer } from '../components/SstDetailDrawer'
import { keyText } from '../components/KeyDisplay'
import type { TreeDto } from '../api/types'

export default function Lsm() {
  const query = useLsm()
  const [selected, setSelected] = useState<string | null>(null)
  const [segmentIdx, setSegmentIdx] = useState<number>(-1)

  return (
    <div>
      <h1 className="text-3xl">LSM Tree</h1>
      <QueryGate query={query}>
        {(lsm) => {
          const hasSegments = lsm.segments.length > 0
          const tree: TreeDto =
            hasSegments && segmentIdx >= 0
              ? lsm.segments[segmentIdx].tree
              : lsm.tree
          return (
            <div className="mt-6 space-y-6">
              <div className="text-sm text-ink-4">
                As of manifest{' '}
                <span className="font-mono">#{lsm.manifest_id}</span>
                {lsm.segment_extractor_name && (
                  <span className="ml-3">
                    segment extractor:{' '}
                    <span className="font-mono">
                      {lsm.segment_extractor_name}
                    </span>
                  </span>
                )}
              </div>

              {hasSegments && (
                <div className="flex flex-wrap gap-1">
                  <SegmentTab
                    label="root"
                    active={segmentIdx === -1}
                    onClick={() => setSegmentIdx(-1)}
                  />
                  {lsm.segments.map((seg, i) => (
                    <SegmentTab
                      key={seg.prefix.hex}
                      label={keyText(seg.prefix)}
                      active={segmentIdx === i}
                      onClick={() => setSegmentIdx(i)}
                    />
                  ))}
                </div>
              )}

              <Panel title="Levels by size">
                <SizeView tree={tree} selected={selected} onSelect={setSelected} />
                <p className="mt-3 text-xs text-ink-5">
                  Bar length is proportional to level size; segments within a
                  bar are individual SSTs. Click an SST for details.
                </p>
              </Panel>

              <Panel title="Key-range coverage">
                <KeyRangeView
                  tree={tree}
                  selected={selected}
                  onSelect={setSelected}
                />
                <p className="mt-3 text-xs text-ink-5">
                  Horizontal position is the key space (rank-scaled).
                  Translucent overlapping spans in L0 are SSTs a point read may
                  have to consult — vertical overlap reads as read
                  amplification.
                </p>
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
