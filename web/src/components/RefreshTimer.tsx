import { useEffect, useState } from 'react'
import { useIsFetching, useQueryClient } from '@tanstack/react-query'
import {
  getNextLiveTickAt,
  LIVE_REFETCH_MS,
  refreshLiveNow,
} from '../api/client'

const R = 8
const CIRCUMFERENCE = 2 * Math.PI * R

/**
 * Header dial counting down to the next live-data heartbeat (one shared
 * beat for every polling query, so navigation never resets or staggers
 * it): the ring depletes as the beat approaches and spins while a refetch
 * is in flight. Clicking refreshes immediately and restarts the beat.
 */
export function RefreshTimer() {
  const queryClient = useQueryClient()
  const fetching = useIsFetching()
  const [state, setState] = useState<{ secs: number; frac: number } | null>(null)
  useEffect(() => {
    const tick = () => {
      const hasLive = queryClient
        .getQueryCache()
        .getAll()
        .some((q) => q.getObserversCount() > 0 && q.meta?.live === true)
      if (!hasLive) {
        setState(null)
        return
      }
      const remaining = Math.max(0, getNextLiveTickAt() - Date.now())
      setState({
        secs: Math.ceil(remaining / 1000),
        frac: Math.min(1, remaining / LIVE_REFETCH_MS),
      })
    }
    tick()
    const id = setInterval(tick, 100)
    return () => clearInterval(id)
  }, [queryClient])
  if (state === null && fetching === 0) return null
  const frac = fetching > 0 ? 0.25 : (state?.frac ?? 0)
  return (
    <button
      onClick={() => refreshLiveNow(queryClient)}
      className="relative inline-block h-[22px] w-[22px] shrink-0 cursor-pointer"
      title="Auto-refreshes when the ring empties — click to refresh now"
      aria-label="Refresh now"
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
    </button>
  )
}
