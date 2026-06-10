/**
 * URLs are /db/{store}/{db-path…}/{page} where the DB path may itself
 * contain slashes, so the page is recognized from the tail: the last
 * segment when it's a known page name, `manifests/<id>` or
 * `manifests/diff` for those two, otherwise the whole splat is the DB
 * path (overview). A DB whose final path segment collides with a page
 * name is ambiguous and loses — acceptable for object-store layouts.
 */
export const DB_PAGES = [
  'alerts',
  'activity',
  'lsm',
  'wal',
  'manifests',
  'compactions',
  'checkpoints',
  'garbage',
] as const

export interface DbRoute {
  /** DB path within the store ('' on the store-listing page). */
  path: string
  /**
   * '' = overview; otherwise a DB_PAGES entry, or '{manifests|compactions}/diff'
   * or '{manifests|compactions}/id' for the two-level pages.
   */
  page: string
  /** The id when page ends in '/id'. */
  arg?: string
}

export function splitDbSplat(splat: string): DbRoute {
  const segs = splat.split('/').filter(Boolean)
  const parent = segs[segs.length - 2]
  if (segs.length >= 2 && (parent === 'manifests' || parent === 'compactions')) {
    const last = segs[segs.length - 1]
    if (last === 'diff') {
      return { path: segs.slice(0, -2).join('/'), page: `${parent}/diff` }
    }
    if (/^\d+$/.test(last)) {
      return { path: segs.slice(0, -2).join('/'), page: `${parent}/id`, arg: last }
    }
  }
  const last = segs[segs.length - 1]
  if (last !== undefined && (DB_PAGES as readonly string[]).includes(last)) {
    return { path: segs.slice(0, -1).join('/'), page: last }
  }
  return { path: segs.join('/'), page: '' }
}
