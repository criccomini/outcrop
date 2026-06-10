import { useDbId } from '../api/client'
import type { WalSstDto } from '../api/types'
import { useEscape } from '../lib/escape'
import { formatBytes, formatRelative, formatTime } from '../lib/format'

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

/**
 * Right-hand drawer for a WAL SST, mirroring the compacted-SST drawer on
 * the LSM page. Listing metadata only: slatedb's public SstReader opens
 * compacted SSTs exclusively, so block-level drill-down isn't available
 * for WAL files.
 */
export function WalSstDrawer({
  entry,
  replayAfterWalId,
  onClose,
}: {
  entry: WalSstDto
  replayAfterWalId: number
  onClose: () => void
}) {
  const dbId = useDbId()
  const dbPath = dbId.slice(dbId.indexOf(':') + 1)
  const location = `${dbPath}/wal/${String(entry.id).padStart(20, '0')}.sst`
  const unreplayed = entry.id > replayAfterWalId
  useEscape(true, onClose)
  return (
    <aside className="fixed inset-y-0 right-0 z-30 w-full max-w-[26rem] overflow-y-auto border-l border-ink-7 bg-surface-1 p-5 shadow-lg xl:top-14 xl:z-10">
      <div className="flex items-start justify-between gap-2">
        <div>
          <h3 className="font-serif text-lg text-ink-1">WAL SST</h3>
          <div className="font-mono text-xs break-all text-ink-4">#{entry.id}</div>
        </div>
        <button
          onClick={onClose}
          className="rounded-md px-2 py-1 text-ink-4 hover:bg-surface-2 hover:text-ink-1"
          aria-label="Close"
        >
          ✕
        </button>
      </div>
      <div className="mt-2">
        <SectionTitle>File</SectionTitle>
        <Row label="Size">{formatBytes(entry.size_bytes)}</Row>
        <Row label="Written">
          {formatTime(entry.last_modified)} ({formatRelative(entry.last_modified)})
        </Row>
        <Row label="Location">
          <span className="font-mono text-xs">{location}</span>
        </Row>

        <SectionTitle>Status</SectionTitle>
        <Row label="Replay">
          {unreplayed ? (
            <span className="font-medium text-ink-1">
              un-replayed — re-read into memtables on writer restart
            </span>
          ) : (
            <span>replayed into L0; awaiting garbage collection</span>
          )}
        </Row>
        <Row label="Replay watermark">#{replayAfterWalId}</Row>

        <p className="mt-5 text-xs text-ink-5">
          WAL SSTs share the on-disk format with compacted SSTs, but
          slatedb's public SST reader only opens compacted files, so the
          block index and content stats aren't available here. Once this
          data is flushed to L0, drill into the resulting SST on the LSM
          Tree page.
        </p>
      </div>
    </aside>
  )
}
