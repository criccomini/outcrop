import { useEffect, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import { dbUrl, fetchSearch } from '../api/client'
import type { SearchDto } from '../api/types'
import { formatBytes, formatRelative } from '../lib/format'

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="border-t border-ink-7/50 px-3 py-2 first:border-t-0">
      <div className="mb-1 text-xs font-semibold uppercase tracking-wider text-ink-5">
        {title}
      </div>
      {children}
    </div>
  )
}

/**
 * Header ULID/UUID search, scoped to the active DB: finds the SST object
 * itself plus everything referencing it (manifests, compactor jobs,
 * checkpoints).
 */
export function SearchBar({ dbId }: { dbId: string }) {
  const [q, setQ] = useState('')
  const [open, setOpen] = useState(false)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<SearchDto | null>(null)
  const rootRef = useRef<HTMLDivElement>(null)

  // Reset when switching DBs — results belong to one DB.
  useEffect(() => {
    setOpen(false)
    setResult(null)
    setError(null)
  }, [dbId])

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false)
    }
    const onClick = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    window.addEventListener('keydown', onKey)
    window.addEventListener('mousedown', onClick)
    return () => {
      window.removeEventListener('keydown', onKey)
      window.removeEventListener('mousedown', onClick)
    }
  }, [open])

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    const query = q.trim()
    if (query === '') return
    setOpen(true)
    setLoading(true)
    setError(null)
    setResult(null)
    try {
      setResult(await fetchSearch(dbId, query))
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  const close = () => setOpen(false)
  const empty =
    result !== null &&
    !result.sst_object &&
    result.manifests.length === 0 &&
    result.compactions.length === 0 &&
    result.checkpoints.length === 0

  return (
    <div ref={rootRef} className="relative hidden w-full max-w-sm md:block">
      <form onSubmit={submit}>
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Search ULID / checkpoint UUID…"
          spellCheck={false}
          className="w-full rounded-md border border-ink-6 bg-surface-0 px-3 py-1.5 font-mono text-xs text-ink-2 placeholder:font-sans placeholder:text-ink-5 focus:border-ink-4 focus:outline-none"
        />
      </form>
      {open && (
        <div className="absolute left-0 right-0 top-10 z-40 max-h-[70vh] overflow-y-auto rounded-lg border border-ink-7 bg-surface-1 py-1 shadow-lg">
          {loading && <div className="px-3 py-2 text-sm text-ink-4">Searching…</div>}
          {error && <div className="px-3 py-2 text-sm text-red-800">{error}</div>}
          {empty && (
            <div className="px-3 py-2 text-sm text-ink-5">
              No objects or references found for{' '}
              <span className="font-mono text-xs">{result?.query}</span>.
            </div>
          )}
          {result?.sst_object && (
            <Section title="Object">
              <Link
                to={dbUrl(dbId, `/lsm?sst=${result.query}`)}
                onClick={close}
                className="block text-sm text-accent hover:text-accent-high"
              >
                <span className="font-mono text-xs">{result.sst_object.location}</span>
                <span className="ml-2 text-xs text-ink-4">
                  {formatBytes(result.sst_object.size_bytes)} · written{' '}
                  {formatRelative(result.sst_object.last_modified)}
                </span>
              </Link>
            </Section>
          )}
          {result !== null && result.manifests.length > 0 && (
            <Section title={`Manifests referencing it (${result.manifests.length})`}>
              <ul className="space-y-0.5">
                {result.manifests.map((m) => (
                  <li key={m.id} className="flex items-baseline gap-2 text-sm">
                    <Link
                      to={dbUrl(dbId, `/manifests/${m.id}`)}
                      onClick={close}
                      className="shrink-0 font-mono text-xs text-accent hover:text-accent-high"
                    >
                      #{m.id}
                    </Link>
                    <span className="min-w-0 truncate text-xs text-ink-4">
                      {m.places.join(', ')}
                    </span>
                  </li>
                ))}
              </ul>
              {result.manifests_scanned < result.manifests_total && (
                <div className="mt-1 text-xs text-ink-5">
                  Scanned the newest {result.manifests_scanned} of{' '}
                  {result.manifests_total} retained manifests.
                </div>
              )}
            </Section>
          )}
          {result !== null && result.compactions.length > 0 && (
            <Section title={`Compactor jobs (${result.compactions.length})`}>
              <ul className="space-y-0.5">
                {result.compactions.map((c) => (
                  <li key={`${c.job_id}-${c.role}`} className="text-sm">
                    <Link
                      to={dbUrl(dbId, `/compactions/${c.version}`)}
                      onClick={close}
                      className="font-mono text-xs text-accent hover:text-accent-high"
                    >
                      v{c.version}
                    </Link>
                    <span className="ml-2 text-xs text-ink-4">
                      {c.role === 'job' ? 'job id' : 'output SST of job'}{' '}
                      <span className="font-mono">{c.job_id.slice(0, 10)}…</span>
                    </span>
                  </li>
                ))}
              </ul>
            </Section>
          )}
          {result !== null && result.checkpoints.length > 0 && (
            <Section title={`Checkpoints (${result.checkpoints.length})`}>
              <ul className="space-y-0.5">
                {result.checkpoints.map((c) => (
                  <li key={c.id} className="text-sm">
                    <Link
                      to={dbUrl(dbId, '/checkpoints')}
                      onClick={close}
                      className="text-accent hover:text-accent-high"
                    >
                      {c.name ?? c.id.slice(0, 8)}
                    </Link>
                    <span className="ml-2 text-xs text-ink-4">
                      → manifest #{c.manifest_id}
                    </span>
                  </li>
                ))}
              </ul>
            </Section>
          )}
        </div>
      )}
    </div>
  )
}
