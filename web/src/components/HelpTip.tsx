import type { ReactNode } from 'react'

/**
 * A "?" badge for a widget's upper-right corner; the explanation appears
 * in a popover on hover (or keyboard focus).
 */
export function HelpTip({ children }: { children: ReactNode }) {
  return (
    <span className="group relative inline-flex" tabIndex={0}>
      <span
        className="flex h-5 w-5 cursor-help items-center justify-center rounded-full border border-ink-6 bg-surface-2 text-xs font-semibold text-ink-4 transition-colors group-hover:border-ink-4 group-hover:text-ink-1"
        aria-label="What is this?"
      >
        ?
      </span>
      <span className="pointer-events-none invisible absolute right-0 top-7 z-20 w-72 rounded-lg border border-ink-7 bg-surface-1 p-3 text-left text-xs font-normal normal-case tracking-normal text-ink-3 shadow-lg group-focus-within:visible group-hover:visible">
        {children}
      </span>
    </span>
  )
}
