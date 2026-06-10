import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'

export function Panel({
  title,
  action,
  children,
}: {
  title?: ReactNode
  action?: ReactNode
  children: ReactNode
}) {
  return (
    <section className="rounded-lg border border-ink-7/60 bg-surface-1 shadow-sm">
      {(title || action) && (
        <div className="flex items-center justify-between border-b border-ink-7/60 px-4 py-2.5">
          <h3 className="font-serif text-base text-ink-1">{title}</h3>
          {action}
        </div>
      )}
      <div className="p-4">{children}</div>
    </section>
  )
}

export function Stat({
  label,
  value,
  sub,
  to,
}: {
  label: string
  value: ReactNode
  sub?: ReactNode
  /** Makes the whole tile a link. */
  to?: string
}) {
  const body = (
    <>
      <div className="text-xs font-semibold uppercase tracking-wider text-ink-5">
        {label}
      </div>
      <div className="mt-1 text-2xl text-ink-1">{value}</div>
      {sub && <div className="mt-0.5 text-xs text-ink-4">{sub}</div>}
    </>
  )
  const style = 'rounded-lg border border-ink-7/60 bg-surface-1 px-4 py-3 shadow-sm'
  if (to) {
    return (
      <Link to={to} className={`${style} block transition-colors hover:border-ink-5`}>
        {body}
      </Link>
    )
  }
  return <div className={style}>{body}</div>
}
