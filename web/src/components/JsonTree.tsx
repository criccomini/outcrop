/** Collapsible tree view for an arbitrary JSON value. */
export function JsonTree({ value, label }: { value: unknown; label?: string }) {
  return (
    <div className="font-mono text-xs leading-5">
      <Node value={value} label={label} depth={0} />
    </div>
  )
}

function Node({
  value,
  label,
  depth,
}: {
  value: unknown
  label?: string
  depth: number
}) {
  const prefix = label !== undefined && (
    <span className="text-ink-3">{label}: </span>
  )

  if (value === null || value === undefined) {
    return (
      <div>
        {prefix}
        <span className="text-ink-5">null</span>
      </div>
    )
  }
  if (typeof value === 'string') {
    return (
      <div>
        {prefix}
        <span className="break-all text-accent-high">"{value}"</span>
      </div>
    )
  }
  if (typeof value === 'number' || typeof value === 'boolean') {
    return (
      <div>
        {prefix}
        <span className="text-ink-1">{String(value)}</span>
      </div>
    )
  }

  const entries = Array.isArray(value)
    ? value.map((v, i) => [String(i), v] as const)
    : Object.entries(value as Record<string, unknown>)
  const kind = Array.isArray(value) ? `[${entries.length}]` : `{${entries.length}}`

  if (entries.length === 0) {
    return (
      <div>
        {prefix}
        <span className="text-ink-5">{Array.isArray(value) ? '[]' : '{}'}</span>
      </div>
    )
  }

  return (
    <details open={depth < 2}>
      <summary className="cursor-pointer select-none">
        {prefix}
        <span className="text-ink-5">{kind}</span>
      </summary>
      <div className="ml-4 border-l border-ink-7/60 pl-3">
        {entries.map(([k, v]) => (
          <Node key={k} label={k} value={v} depth={depth + 1} />
        ))}
      </div>
    </details>
  )
}
