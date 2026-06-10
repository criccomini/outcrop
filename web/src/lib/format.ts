export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`
  const units = ['KiB', 'MiB', 'GiB', 'TiB']
  let value = n
  let unit = ''
  for (const u of units) {
    value /= 1024
    unit = u
    if (value < 1024) break
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${unit}`
}

export function formatTime(iso: string | undefined): string {
  if (!iso) return '—'
  const d = new Date(iso)
  return d.toLocaleString()
}

export function formatRelative(iso: string | undefined): string {
  if (!iso) return '—'
  const ms = Date.now() - new Date(iso).getTime()
  const future = ms < 0
  const abs = Math.abs(ms)
  const s = Math.round(abs / 1000)
  let text: string
  if (s < 60) text = `${s}s`
  else if (s < 3600) text = `${Math.round(s / 60)}m`
  else if (s < 86400) text = `${Math.round(s / 3600)}h`
  else text = `${Math.round(s / 86400)}d`
  return future ? `in ${text}` : `${text} ago`
}

export function formatCount(n: number): string {
  return n.toLocaleString()
}
