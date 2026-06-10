import { useEffect, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import { dbUrl, fetchSearch } from '../api/client'
import type { SearchDto } from '../api/types'
import { ESCAPE_OVERLAY, ESCAPE_SOFT, useEscape } from '../lib/escape'
import { formatBytes, formatRelative } from '../lib/format'

const ULID_RE = /^[0-7][0-9A-HJKMNP-TV-Z]{25}$/i
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/**
 * Search-as-you-type state shared by the desktop bar and the mobile
 * overlay: debounced, fired only once the text is structurally a
 * ULID/UUID, with a stale-response guard.
 */
function useUlidSearch(dbId: string) {
  const [q, setQ] = useState('')
  const [open, setOpen] = useState(false)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<SearchDto | null>(null)

  // Reset when switching DBs — results belong to one DB.
  useEffect(() => {
    setQ('')
    setOpen(false)
    setResult(null)
    setError(null)
  }, [dbId])

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

  return {
    q,
    setQ,
    open,
    setOpen,
    loading,
    error,
    result,
    query,
    validQuery,
    submit,
  }
}

type Search = ReturnType<typeof useUlidSearch>

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

/** Popover/panel body: hint, status, and result sections with links. */
function SearchResults({
  dbId,
  search,
  onNavigate,
}: {
  dbId: string
  search: Search
  onNavigate: () => void
}) {
  const { query, validQuery, loading, error, result } = search
  const empty =
    result !== null &&
    !result.sst_object &&
    result.manifests.length === 0 &&
    result.compactions.length === 0 &&
    result.checkpoints.length === 0
  return (
    <>
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
            onClick={onNavigate}
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
                  onClick={onNavigate}
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
                  to={dbUrl(dbId, `/compactions/job/${c.job_id}`)}
                  onClick={onNavigate}
                  className="font-mono text-xs text-accent hover:text-accent-high"
                >
                  {c.job_id.slice(0, 10)}…
                </Link>
                <span className="ml-2 text-xs text-ink-4">
                  {c.role === 'job' ? 'matched job id' : 'output SST of this job'}
                  {' · in '}
                </span>
                <Link
                  to={dbUrl(dbId, `/compactions/${c.version}`)}
                  onClick={onNavigate}
                  className="font-mono text-xs text-accent hover:text-accent-high"
                >
                  v{c.version}
                </Link>
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
                  onClick={onNavigate}
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
    </>
  )
}

/**
 * Desktop header search: inline input with a results popover; '/' and
 * Cmd/Ctrl-K focus it.
 */
export function SearchBar({ dbId }: { dbId: string }) {
  const search = useUlidSearch(dbId)
  const { q, setQ, open, setOpen, submit } = search
  const rootRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const [focused, setFocused] = useState(false)

  // Esc closes the popover and unfocuses the box — but as low-priority
  // soft state, so an open drawer consumes the first press and a second
  // press lands here.
  useEscape(
    focused || open,
    () => {
      setOpen(false)
      inputRef.current?.blur()
    },
    ESCAPE_SOFT,
  )

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

  useEffect(() => {
    if (!open) return
    const onClick = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    window.addEventListener('mousedown', onClick)
    return () => window.removeEventListener('mousedown', onClick)
  }, [open, setOpen])

  return (
    <div ref={rootRef} className="relative hidden w-full max-w-sm md:block">
      <form onSubmit={submit}>
        <input
          ref={inputRef}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
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
          <SearchResults dbId={dbId} search={search} onNavigate={() => setOpen(false)} />
        </div>
      )}
    </div>
  )
}

/**
 * Mobile search: a magnifier button in the header expanding into a
 * full-width overlay with an autofocused input — an inline bar can't fit
 * a 26-char ULID next to the hamburger, logo, and refresh dial.
 */
export function MobileSearch({ dbId }: { dbId: string }) {
  const [expanded, setExpanded] = useState(false)
  const search = useUlidSearch(dbId)
  const { q, setQ, open, submit } = search

  const close = () => {
    setExpanded(false)
    setQ('')
  }

  // The overlay covers everything, so it outranks drawers underneath.
  useEscape(expanded, close, ESCAPE_OVERLAY)

  return (
    <>
      <button
        onClick={() => setExpanded(true)}
        className="rounded-md p-1.5 text-ink-3 hover:bg-surface-2 hover:text-ink-1 md:hidden"
        aria-label="Search"
      >
        <svg
          width="18"
          height="18"
          viewBox="0 0 20 20"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.8"
          strokeLinecap="round"
        >
          <circle cx="9" cy="9" r="5.5" />
          <path d="M13.2 13.2 17.5 17.5" />
        </svg>
      </button>
      {expanded && (
        <div className="fixed inset-0 z-50 md:hidden">
          <div className="absolute inset-0 bg-ink-1/30" onClick={close} aria-hidden />
          <div className="absolute inset-x-0 top-0 border-b border-ink-7 bg-surface-1 p-3">
            <form onSubmit={submit} className="flex items-center gap-2">
              <input
                autoFocus
                value={q}
                onChange={(e) => setQ(e.target.value)}
                placeholder="Search ULID / checkpoint UUID…"
                spellCheck={false}
                className="min-w-0 flex-1 rounded-md border border-ink-6 bg-surface-0 px-3 py-2 font-mono text-xs text-ink-2 placeholder:font-sans placeholder:text-ink-5 focus:border-ink-4 focus:outline-none"
              />
              <button
                type="button"
                onClick={close}
                className="shrink-0 text-sm text-ink-3 hover:text-ink-1"
              >
                Cancel
              </button>
            </form>
            {open && (
              <div className="mt-2 max-h-[60vh] overflow-y-auto rounded-lg border border-ink-7 bg-surface-1 py-1 shadow-lg">
                <SearchResults dbId={dbId} search={search} onNavigate={close} />
              </div>
            )}
          </div>
        </div>
      )}
    </>
  )
}
