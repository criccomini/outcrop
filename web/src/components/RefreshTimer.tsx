import { useEffect, useState } from 'react'
import { useIsFetching, useQueryClient } from '@tanstack/react-query'
import type { QueryClient } from '@tanstack/react-query'

/**
 * When the soonest auto-refetch among actively observed queries will fire,
 * or null when nothing on the current page polls. `refetchInterval` lives on
 * each query's observers, which react-query's types keep private — hence the
 * structural cast.
 */
function nextRefreshAt(queryClient: QueryClient): number | null {
  let next: number | null = null
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
    next = next === null ? at : Math.min(next, at)
  }
  return next
}

/** Header countdown to the next auto-refresh of the page's live data. */
export function RefreshTimer() {
  const queryClient = useQueryClient()
  const fetching = useIsFetching()
  const [secs, setSecs] = useState<number | null>(null)
  useEffect(() => {
    const tick = () => {
      const at = nextRefreshAt(queryClient)
      setSecs(at === null ? null : Math.max(0, Math.ceil((at - Date.now()) / 1000)))
    }
    tick()
    const id = setInterval(tick, 250)
    return () => clearInterval(id)
  }, [queryClient])
  if (secs === null && fetching === 0) return null
  return (
    <span
      className="hidden w-14 font-mono text-xs text-ink-5 sm:inline-block"
      title="Live data refreshes automatically"
    >
      <span className={fetching > 0 ? 'inline-block animate-spin' : 'inline-block'}>
        ↻
      </span>
      <span className="ml-1">{fetching > 0 ? '…' : `${secs}s`}</span>
    </span>
  )
}
