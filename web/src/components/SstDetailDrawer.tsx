import { useSst } from '../api/client'
import { formatBytes, formatTime } from '../lib/format'
import { KeyDisplay } from './KeyDisplay'
import { QueryGate } from './QueryGate'

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex justify-between gap-4 py-1 text-sm">
      <span className="shrink-0 text-ink-4">{label}</span>
      <span className="min-w-0 break-all text-right text-ink-2">{children}</span>
    </div>
  )
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h4 className="mt-4 mb-1 text-xs font-semibold uppercase tracking-wider text-ink-5">
      {children}
    </h4>
  )
}

export function SstDetailDrawer({
  ulid,
  onClose,
}: {
  ulid: string
  onClose: () => void
}) {
  const query = useSst(ulid)
  return (
    <aside className="fixed inset-y-0 right-0 z-30 w-full max-w-[26rem] overflow-y-auto border-l border-ink-7 bg-surface-1 p-5 shadow-lg xl:top-14 xl:z-10">
      <div className="flex items-start justify-between gap-2">
        <div>
          <h3 className="font-serif text-lg text-ink-1">SST</h3>
          <div className="font-mono text-xs break-all text-ink-4">{ulid}</div>
        </div>
        <button
          onClick={onClose}
          className="rounded-md px-2 py-1 text-ink-4 hover:bg-surface-2 hover:text-ink-1"
          aria-label="Close"
        >
          ✕
        </button>
      </div>
      <QueryGate query={query}>
        {(sst) => {
          const tombstonePct = sst.stats
            ? (sst.stats.num_deletes / Math.max(sst.stats.num_rows, 1)) * 100
            : null
          return (
            <div className="mt-2">
              <SectionTitle>File</SectionTitle>
              <Row label="Size">{formatBytes(sst.size_bytes)}</Row>
              <Row label="Last modified">{formatTime(sst.last_modified)}</Row>
              <Row label="Location">
                <span className="font-mono text-xs">{sst.location}</span>
              </Row>

              <SectionTitle>Layout</SectionTitle>
              <Row label="First key">
                <KeyDisplay k={sst.info.first_key} />
              </Row>
              <Row label="Last key">
                <KeyDisplay k={sst.info.last_key} />
              </Row>
              <Row label="Type">{sst.info.sst_type}</Row>
              <Row label="Compression">{sst.info.compression ?? 'none'}</Row>
              <Row label="Filter format">{sst.info.filter_format}</Row>
              <Row label="Index">{formatBytes(sst.info.index_len)}</Row>
              <Row label="Filter">{formatBytes(sst.info.filter_len)}</Row>
              <Row label="Stats">{formatBytes(sst.info.stats_len)}</Row>

              {sst.stats && (
                <>
                  <SectionTitle>Contents</SectionTitle>
                  <Row label="Rows">{sst.stats.num_rows.toLocaleString()}</Row>
                  <Row label="Puts">{sst.stats.num_puts.toLocaleString()}</Row>
                  <Row label="Deletes">
                    {sst.stats.num_deletes.toLocaleString()}
                    {tombstonePct !== null && tombstonePct > 0 && (
                      <span className="ml-1 text-ink-4">
                        ({tombstonePct.toFixed(1)}%)
                      </span>
                    )}
                  </Row>
                  <Row label="Merges">{sst.stats.num_merges.toLocaleString()}</Row>
                  <Row label="Raw keys">{formatBytes(sst.stats.raw_key_bytes)}</Row>
                  <Row label="Raw values">
                    {formatBytes(sst.stats.raw_val_bytes)}
                  </Row>
                  <Row label="Blocks">{sst.stats.block_count.toLocaleString()}</Row>
                </>
              )}

              <SectionTitle>
                Block index ({sst.index.total_blocks.toLocaleString()} blocks
                {sst.index.truncated ? ', truncated' : ''})
              </SectionTitle>
              <table className="w-full text-xs">
                <thead>
                  <tr className="text-left text-ink-5">
                    <th className="py-1 pr-2 font-medium">Offset</th>
                    <th className="py-1 font-medium">First key</th>
                  </tr>
                </thead>
                <tbody>
                  {sst.index.blocks.slice(0, 200).map((b, i) => (
                    <tr key={b.offset} className="border-t border-ink-7/50">
                      <td className="py-0.5 pr-2 font-mono text-ink-4">
                        {b.offset.toLocaleString()}
                      </td>
                      <td className="py-0.5">
                        {/* The format stores an empty first key for block 0;
                            the SST's own first key is the real one. */}
                        <KeyDisplay
                          k={
                            i === 0 && b.first_key.hex === ''
                              ? sst.info.first_key
                              : b.first_key
                          }
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {sst.index.blocks.length > 200 && (
                <div className="mt-1 text-xs text-ink-5">
                  Showing first 200 of {sst.index.blocks.length} fetched blocks.
                </div>
              )}
            </div>
          )
        }}
      </QueryGate>
    </aside>
  )
}
