const CROCKFORD = '0123456789ABCDEFGHJKMNPQRSTVWXYZ'

/** Milliseconds since epoch encoded in a ULID's first 10 chars, or null. */
export function ulidTimeMs(id: string): number | null {
  if (id.length < 10) return null
  let ms = 0
  for (const ch of id.slice(0, 10).toUpperCase()) {
    const v = CROCKFORD.indexOf(ch)
    if (v < 0) return null
    ms = ms * 32 + v
  }
  return ms
}
