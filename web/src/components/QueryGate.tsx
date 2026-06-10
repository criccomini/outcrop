import type { ReactNode } from 'react'
import type { UseQueryResult } from '@tanstack/react-query'
import type { ApiRequestError } from '../api/client'

/** Uniform loading / error handling around a react-query result. */
export function QueryGate<T>({
  query,
  children,
}: {
  query: UseQueryResult<T, ApiRequestError>
  children: (data: T) => ReactNode
}) {
  if (query.isPending) {
    return <div className="py-12 text-center text-ink-4">Loading…</div>
  }
  if (query.isError) {
    return (
      <div className="rounded-lg border border-accent/40 bg-accent-low px-4 py-3 text-accent-high">
        {query.error.message}
      </div>
    )
  }
  return <>{children(query.data)}</>
}
