import type { KeyDto } from '../api/types'

export function keyText(k: KeyDto | undefined): string {
  if (!k) return '—'
  if (k.utf8 !== undefined) return k.utf8
  return k.hex.length > 0 ? `0x${k.hex}` : '(empty)'
}

export function KeyDisplay({
  k,
  className = '',
}: {
  k: KeyDto | undefined
  className?: string
}) {
  if (!k) return <span className={`text-ink-5 ${className}`}>—</span>
  return (
    <span
      className={`font-mono text-xs ${className}`}
      title={`hex: ${k.hex}${k.utf8 !== undefined ? `\nutf8: ${k.utf8}` : ''}`}
    >
      {keyText(k)}
    </span>
  )
}
