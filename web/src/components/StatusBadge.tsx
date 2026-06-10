const STYLES: Record<string, string> = {
  submitted: 'bg-surface-2 text-ink-3 border-ink-6',
  running: 'bg-accent-low text-accent-high border-accent/40',
  compacted: 'bg-accent-low text-accent-high border-accent/40',
  completed: 'bg-surface-2 text-ink-4 border-ink-7',
  failed: 'bg-red-50 text-red-800 border-red-300',
}

export function StatusBadge({ status }: { status: string }) {
  const style = STYLES[status] ?? 'bg-surface-2 text-ink-3 border-ink-6'
  return (
    <span
      className={`inline-block rounded-full border px-2 py-0.5 text-xs font-medium ${style}`}
    >
      {status}
    </span>
  )
}
