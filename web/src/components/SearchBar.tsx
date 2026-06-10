import { useEffect, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import { dbUrl, fetchSearch } from '../api/client'
import type { SearchDto } from '../api/types'
import { formatBytes, formatRelative } from '../lib/format'

const ULID_RE = /^[0-7][0-9A-HJKMNP-TV-Z]{25}$/i
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

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
  const inputRef = useRef<HTMLInputElement>(null)

  // "/" (when not typing elsewhere) and Cmd/Ctrl-K focus the search box.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null
      const typing =
        target !== null &&
        (target.tagName === 'INPUT' ||
          target.tagName === 'TEXTAREA' ||
          target.tagName === 'SELECT' ||
          target.isContentEditable)
      const cmdK = (e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k'
      const slash =
        e.key === '/' && !typing && !e.metaKey && !e.ctrlKey && !e.altKey
      if (cmdK || slash) {
        e.preventDefault()
        inputRef.current?.focus()
        inputRef.current?.select()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

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

  // Stale-response guard: only the latest in-flight search may apply.
  const seq = useRef(0)
  const runSearch = async (query: string) => {
    const id = ++seq.current
    setOpen(true)
    setLoading(true)
    setError(null)
    try {
      const r = await fetchSearch(dbId, query)
      if (seq.current === id) setResult(r)
    } catch (err) {
      if (seq.current === id) {
        setResult(null)
        setError(err instanceof Error ? err.message : String(err))
      }
    } finally {
      if (seq.current === id) setLoading(false)
    }
  }

  const query = q.trim()
  const validQuery = ULID_RE.test(query) || UUID_RE.test(query)

  // Search as you type: debounced, and only once the text is structurally
  // a ULID/UUID — partial ids would just 400 against the exact-match API.
  useEffect(() => {
    if (query === '') {
      setOpen(false)
      setResult(null)
      setError(null)
      setLoading(false)
      return
    }
    if (!validQuery) {
      setResult(null)
      setError(null)
      setLoading(false)
      setOpen(true)
      return
    }
    const t = setTimeout(() => void runSearch(query), 300)
    return () => clearTimeout(t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, validQuery, dbId])

  const submit = (e: React.FormEvent) => {
    e.preventDefault()
    if (validQuery) void runSearch(query)
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
          ref={inputRef}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Search ULID / checkpoint UUID…"
          spellCheck={false}
          className="w-full rounded-md border border-ink-6 bg-surface-0 py-1.5 pl-3 pr-8 font-mono text-xs text-ink-2 placeholder:font-sans placeholder:text-ink-5 focus:border-ink-4 focus:outline-none"
        />
        <kbd className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 rounded border border-ink-6 bg-surface-1 px-1.5 font-sans text-[10px] text-ink-5">
          /
        </kbd>
      </form>
      {open && (
        <div className="absolute left-0 right-0 top-10 z-40 max-h-[70vh] overflow-y-auto rounded-lg border border-ink-7 bg-surface-1 py-1 shadow-lg">
          {!validQuery && query !== '' && (
            <div className="px-3 py-2 text-sm text-ink-5">
              Keep typing — ULIDs are 26 characters, checkpoint UUIDs 36
              <span className="ml-1 font-mono text-xs">({query.length})</span>
            </div>
          )}
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
