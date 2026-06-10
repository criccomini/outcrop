import { useEffect, useState } from 'react'
import { useIsFetching, useQueryClient } from '@tanstack/react-query'
import type { QueryClient } from '@tanstack/react-query'

/**
 * The soonest auto-refetch among actively observed queries (and the poll
 * interval it runs on), or null when nothing on the current page polls.
 * `refetchInterval` lives on each query's observers, which react-query's
 * types keep private — hence the structural cast.
 */
function nextRefresh(
  queryClient: QueryClient,
): { at: number; interval: number } | null {
  let next: { at: number; interval: number } | null = null
  for (const q of queryClient.getQueryCache().getAll()) {
    if (q.getObserversCount() === 0 || q.state.dataUpdatedAt === 0) continue
    const { observers } = q as unknown as {
      observers: { options: { refetchInterval?: number | false } }[]
    }
    let interval: number | null = null
    for (const o of observers ?? []) {
      if (typeof o.options.refetchInterval === 'number') {
        interval = Math.min(interval ?? o.options.refetchInterval, o.options.refetchInterval)
      }
    }
    if (interval === null) continue
    const at = q.state.dataUpdatedAt + interval
    if (next === null || at < next.at) next = { at, interval }
  }
  return next
}

const R = 8
const CIRCUMFERENCE = 2 * Math.PI * R

/**
 * Header dial counting down to the next auto-refresh: the ring depletes as
 * the refetch approaches and spins while one is in flight.
 */
export function RefreshTimer() {
  const queryClient = useQueryClient()
  const fetching = useIsFetching()
  const [state, setState] = useState<{ secs: number; frac: number } | null>(null)
  useEffect(() => {
    const tick = () => {
      const next = nextRefresh(queryClient)
      if (next === null) {
        setState(null)
        return
      }
      const remaining = Math.max(0, next.at - Date.now())
      setState({
        secs: Math.ceil(remaining / 1000),
        frac: Math.min(1, remaining / next.interval),
      })
    }
    tick()
    const id = setInterval(tick, 100)
    return () => clearInterval(id)
  }, [queryClient])
  if (state === null && fetching === 0) return null
  const frac = fetching > 0 ? 0.25 : (state?.frac ?? 0)
  return (
    <span
      className="relative hidden h-[22px] w-[22px] shrink-0 sm:inline-block"
      title="Live data refreshes automatically"
    >
      <svg
        width="22"
        height="22"
        viewBox="0 0 22 22"
        className={fetching > 0 ? 'animate-spin' : '-rotate-90'}
      >
        <circle
          cx="11"
          cy="11"
          r={R}
          fill="none"
          strokeWidth="2"
          stroke="currentColor"
          className="text-ink-7"
        />
        <circle
          cx="11"
          cy="11"
          r={R}
          fill="none"
          strokeWidth="2"
          stroke="currentColor"
          strokeLinecap="round"
          className="text-accent"
          strokeDasharray={`${CIRCUMFERENCE * frac} ${CIRCUMFERENCE}`}
        />
      </svg>
      {fetching === 0 && state !== null && (
        <span className="absolute inset-0 flex items-center justify-center text-[9px] font-medium tabular-nums text-ink-3">
          {state.secs}
        </span>
      )}
    </span>
  )
}
