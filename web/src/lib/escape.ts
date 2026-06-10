import { useEffect, useRef } from 'react'

// Active Escape handlers; each press fires only the winner — highest
// priority, then most recently registered — so layered UI unwinds in
// visual order: full-screen overlays, then drawers, then soft state like
// search focus, regardless of the order they became active.
export const ESCAPE_OVERLAY = 20
export const ESCAPE_DRAWER = 10
export const ESCAPE_SOFT = 0

interface Entry {
  priority: number
  fn: () => void
}

const entries: Entry[] = []
let installed = false

function ensureListener() {
  if (installed) return
  installed = true
  window.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape' || entries.length === 0) return
    let winner = entries[0]
    for (const entry of entries) {
      // >= so later registrations win ties.
      if (entry.priority >= winner.priority) winner = entry
    }
    e.preventDefault()
    winner.fn()
  })
}

/** Registers `handler` on the escape stack while `active`. */
export function useEscape(
  active: boolean,
  handler: () => void,
  priority: number = ESCAPE_DRAWER,
) {
  // Keep the latest handler without re-registering (which would change
  // this entry's stack position).
  const latest = useRef(handler)
  latest.current = handler
  useEffect(() => {
    if (!active) return
    ensureListener()
    const entry: Entry = { priority, fn: () => latest.current() }
    entries.push(entry)
    return () => {
      const i = entries.lastIndexOf(entry)
      if (i >= 0) entries.splice(i, 1)
    }
  }, [active, priority])
}
