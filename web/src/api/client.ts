import { keepPreviousData, useQuery } from '@tanstack/react-query'
import type { QueryClient } from '@tanstack/react-query'
import { useParams } from 'react-router-dom'
import { splitDbSplat } from '../lib/dbroute'
import type {
  ActivityDto,
  CheckpointStatusDto,
  CompactorStateDto,
  DbsDto,
  ExternalDbDto,
  GarbageDto,
  HealthDto,
  LsmDto,
  ManifestDiffDto,
  ManifestDto,
  ManifestIdDto,
  ManifestSummaryDto,
  OverviewDto,
  SstDetailDto,
  VersionedCompactionsDto,
  WalDto,
} from './types'

declare global {
  interface Window {
    /** Injected into index.html by `serve --ui-only --api-url …`. */
    SLATEDB_API_BASE?: string
  }
}

/** Remote API origin in ui-only deployments; same-origin ('') otherwise. */
const API_BASE = window.SLATEDB_API_BASE ?? ''

export class ApiRequestError extends Error {
  status: number

  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function fetchJson<T>(url: string): Promise<T> {
  const res = await fetch(API_BASE + url)
  if (!res.ok) {
    let message = `${res.status} ${res.statusText}`
    try {
      const body = await res.json()
      if (body.error) message = body.error
    } catch {
      // not JSON; keep the status line
    }
    throw new ApiRequestError(res.status, message)
  }
  return res.json()
}

/** Poll interval for pages that track live DB state. */
export const LIVE_REFETCH_MS = 10_000

// All live queries (meta.live) refresh together on one heartbeat instead of
// each query keeping its own staggered refetchInterval. That keeps the
// header's countdown ring meaningful: one beat, one spin, one reset —
// regardless of when individual pages mounted their queries.
let liveTimer: ReturnType<typeof setTimeout> | undefined
let nextLiveTickAt = 0
let pollingClient: QueryClient | null = null

/** When the next live-data heartbeat fires (ms since epoch). */
export function getNextLiveTickAt(): number {
  return nextLiveTickAt
}

function fireLiveTick(queryClient: QueryClient) {
  void queryClient.invalidateQueries({
    refetchType: 'active',
    predicate: (q) => q.meta?.live === true,
  })
  scheduleLiveTick(queryClient)
}

function scheduleLiveTick(queryClient: QueryClient) {
  clearTimeout(liveTimer)
  nextLiveTickAt = Date.now() + LIVE_REFETCH_MS
  liveTimer = setTimeout(() => fireLiveTick(queryClient), LIVE_REFETCH_MS)
}

/** Starts the heartbeat; idempotent (StrictMode double-invokes effects). */
export function startLivePolling(queryClient: QueryClient): void {
  if (pollingClient === queryClient) return
  pollingClient = queryClient
  scheduleLiveTick(queryClient)
}

/** Refreshes all live queries immediately and restarts the beat. */
export function refreshLiveNow(queryClient: QueryClient): void {
  fireLiveTick(queryClient)
}

/**
 * The active DB id ("store:path") from the /db/{store}/{path…} route.
 * Hooks accept an explicit id so pages outside that route (the fleet
 * view) can query any DB.
 */
export function useDbId(explicit?: string): string {
  const params = useParams()
  if (explicit !== undefined) return explicit
  const store = params.store
  if (!store) return ''
  const { path } = splitDbSplat(params['*'] ?? '')
  return path === '' ? '' : `${store}:${path}`
}

/** App route for a DB id: ('s:p', '/lsm') → '/db/s/p/lsm'. */
export function dbUrl(id: string, path = ''): string {
  const i = id.indexOf(':')
  const [store, dbPath] = i === -1 ? [id, ''] : [id.slice(0, i), id.slice(i + 1)]
  return `/db/${store}/${dbPath}${path}`
}

/** Prefixes an app route with the active DB: '/lsm' → '/db/{store}/{path}/lsm'. */
export function useDbPath(): (path: string) => string {
  const db = useDbId()
  return (path: string) => dbUrl(db, path)
}

function api(db: string): string {
  return `/api/dbs/${encodeURIComponent(db)}`
}

export function useHealth() {
  return useQuery<HealthDto, ApiRequestError>({
    queryKey: ['health'],
    queryFn: () => fetchJson('/api/health'),
  })
}

export function useDbs() {
  return useQuery<DbsDto, ApiRequestError>({
    queryKey: ['dbs'],
    queryFn: () => fetchJson('/api/dbs'),
    meta: { live: true },
  })
}

/** Forces a discovery rescan, then refreshes the cached list. */
export async function rescanDbs(queryClient: QueryClient): Promise<void> {
  const fresh = await fetchJson<DbsDto>('/api/dbs?rescan=1')
  queryClient.setQueryData(['dbs'], fresh)
}

export function useOverview(dbId?: string) {
  const db = useDbId(dbId)
  return useQuery<OverviewDto, ApiRequestError>({
    queryKey: [db, 'overview'],
    queryFn: () => fetchJson(`${api(db)}/overview`),
    meta: { live: true },
    enabled: db !== '',
  })
}

/**
 * LSM tree as of `manifestId`, or the live latest when undefined. Historical
 * manifests are immutable, so polling is off for them; keepPreviousData
 * stops the scrubber flashing a loading state on every step.
 */
export function useLsm(manifestId?: number) {
  const db = useDbId()
  return useQuery<LsmDto, ApiRequestError>({
    queryKey: [db, 'lsm', manifestId ?? 'latest'],
    queryFn: () =>
      fetchJson(
        manifestId === undefined
          ? `${api(db)}/lsm`
          : `${api(db)}/lsm?manifest_id=${manifestId}`,
      ),
    meta: manifestId === undefined ? { live: true } : undefined,
    placeholderData: keepPreviousData,
  })
}

export function useManifestIds() {
  const db = useDbId()
  return useQuery<ManifestIdDto[], ApiRequestError>({
    queryKey: [db, 'manifest-ids'],
    queryFn: () => fetchJson(`${api(db)}/manifests/ids`),
    meta: { live: true },
  })
}

export function useActivity(limit = 20) {
  const db = useDbId()
  return useQuery<ActivityDto[], ApiRequestError>({
    queryKey: [db, 'activity', limit],
    queryFn: () => fetchJson(`${api(db)}/activity?limit=${limit}`),
    meta: { live: true },
  })
}

export function useManifests(limit = 50) {
  const db = useDbId()
  return useQuery<ManifestSummaryDto[], ApiRequestError>({
    queryKey: [db, 'manifests', limit],
    queryFn: () => fetchJson(`${api(db)}/manifests?limit=${limit}`),
    meta: { live: true },
  })
}

export function useManifest(id: string) {
  const db = useDbId()
  return useQuery<ManifestDto, ApiRequestError>({
    queryKey: [db, 'manifest', id],
    queryFn: () => fetchJson(`${api(db)}/manifests/${id}`),
    staleTime: Infinity, // immutable once written
  })
}

export function useManifestDiff(a: number, b: number) {
  const db = useDbId()
  return useQuery<ManifestDiffDto, ApiRequestError>({
    queryKey: [db, 'manifest-diff', a, b],
    queryFn: () => fetchJson(`${api(db)}/manifests/diff?a=${a}&b=${b}`),
    staleTime: Infinity, // immutable once written
  })
}

export function useSst(ulid: string | null) {
  const db = useDbId()
  return useQuery<SstDetailDto, ApiRequestError>({
    queryKey: [db, 'sst', ulid],
    queryFn: () => fetchJson(`${api(db)}/ssts/${ulid}`),
    enabled: ulid !== null,
    staleTime: Infinity, // immutable once written
  })
}

export function useWal() {
  const db = useDbId()
  return useQuery<WalDto, ApiRequestError>({
    queryKey: [db, 'wal'],
    queryFn: () => fetchJson(`${api(db)}/wal`),
    meta: { live: true },
  })
}

export function useCompactorState() {
  const db = useDbId()
  return useQuery<CompactorStateDto, ApiRequestError>({
    queryKey: [db, 'compactor-state'],
    queryFn: () => fetchJson(`${api(db)}/compactor/state`),
    meta: { live: true },
  })
}

export function useCompactions(limit = 20) {
  const db = useDbId()
  return useQuery<VersionedCompactionsDto[], ApiRequestError>({
    queryKey: [db, 'compactions', limit],
    queryFn: () => fetchJson(`${api(db)}/compactions?limit=${limit}`),
    meta: { live: true },
  })
}

export function useCheckpoints() {
  const db = useDbId()
  return useQuery<CheckpointStatusDto[], ApiRequestError>({
    queryKey: [db, 'checkpoints'],
    queryFn: () => fetchJson(`${api(db)}/checkpoints`),
    meta: { live: true },
  })
}

export function useClones() {
  const db = useDbId()
  return useQuery<ExternalDbDto[], ApiRequestError>({
    queryKey: [db, 'clones'],
    queryFn: () => fetchJson(`${api(db)}/clones`),
  })
}

export function useGarbage(dbId?: string) {
  const db = useDbId(dbId)
  return useQuery<GarbageDto, ApiRequestError>({
    queryKey: [db, 'garbage'],
    queryFn: () => fetchJson(`${api(db)}/garbage`),
    meta: { live: true },
    enabled: db !== '',
  })
}
