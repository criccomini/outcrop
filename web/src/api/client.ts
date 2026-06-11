import { keepPreviousData, useQuery } from '@tanstack/react-query'
import type { QueryClient } from '@tanstack/react-query'
import { useParams } from 'react-router-dom'
import { splitDbSplat } from '../lib/dbroute'
import type {
  ActivityDto,
  CheckpointStatusDto,
  CompactionDto,
  CompactorStateDto,
  DbsDto,
  ExternalDbDto,
  GarbageDto,
  GcEventsDto,
  HealthDto,
  LevelSliceDto,
  LsmSummaryDto,
  ManifestDiffDto,
  ManifestDto,
  ManifestIdDto,
  ManifestSummaryDto,
  OverviewDto,
  SearchDto,
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

/** One-shot ULID/UUID search against a DB (invoked imperatively). */
export function fetchSearch(db: string, q: string): Promise<SearchDto> {
  return fetchJson(`${api(db)}/search?q=${encodeURIComponent(q)}`)
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
 * Summary-first LSM view as of `manifestId` (live latest when undefined),
 * scoped to one segment (undefined = root, or the server's auto-pick for
 * segmented DBs with an empty root). Per-level aggregates plus a coverage
 * histogram; per-SST detail rides along only for small levels, so the
 * payload stays bounded for huge trees. Historical manifests are immutable,
 * so polling is off for them; keepPreviousData stops the scrubber and the
 * segment tabs flashing a loading state on every step.
 */
export function useLsmSummary(manifestId?: number, segment?: number) {
  const db = useDbId()
  const qs = new URLSearchParams()
  if (manifestId !== undefined) qs.set('manifest_id', String(manifestId))
  if (segment !== undefined) qs.set('segment', String(segment))
  const suffix = qs.size > 0 ? `?${qs}` : ''
  return useQuery<LsmSummaryDto, ApiRequestError>({
    queryKey: [db, 'lsm-summary', manifestId ?? 'latest', segment ?? 'auto'],
    queryFn: () => fetchJson(`${api(db)}/lsm/summary${suffix}`),
    meta: manifestId === undefined ? { live: true } : undefined,
    placeholderData: keepPreviousData,
  })
}

/**
 * Per-SST drill-down for one level (L0 when `run` is undefined) within a
 * key range — how histogram-only levels reach individual SSTs. Fetched on
 * demand (bucket click), not polled; manifests are immutable so the
 * result never goes stale.
 */
export function useLevelSlice(
  params: {
    manifestId: number
    segment?: number
    run?: number
    start?: string
    end?: string
  } | null,
) {
  const db = useDbId()
  const qs = new URLSearchParams()
  if (params) {
    qs.set('manifest_id', String(params.manifestId))
    if (params.segment !== undefined) qs.set('segment', String(params.segment))
    if (params.run !== undefined) qs.set('run', String(params.run))
    if (params.start !== undefined) qs.set('start', params.start)
    if (params.end !== undefined) qs.set('end', params.end)
  }
  return useQuery<LevelSliceDto, ApiRequestError>({
    queryKey: [db, 'lsm-level', qs.toString()],
    queryFn: () => fetchJson(`${api(db)}/lsm/level?${qs}`),
    enabled: params !== null,
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

export function useWal(limit = 200) {
  const db = useDbId()
  return useQuery<WalDto, ApiRequestError>({
    queryKey: [db, 'wal', limit],
    queryFn: () => fetchJson(`${api(db)}/wal?limit=${limit}`),
    meta: { live: true },
    placeholderData: keepPreviousData,
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

/** One compactor job by ULID, at its latest recorded state. */
export function useCompactionJob(ulid: string) {
  const db = useDbId()
  return useQuery<CompactionDto, ApiRequestError>({
    queryKey: [db, 'compaction-job', ulid],
    queryFn: () => fetchJson(`${api(db)}/compactions/${ulid}`),
    meta: { live: true }, // active jobs keep updating
    enabled: ulid !== '',
  })
}

/** One immutable `.compactions` version, via the ranged list endpoint. */
export function useCompactionsVersion(id: number) {
  const db = useDbId()
  return useQuery<VersionedCompactionsDto | undefined, ApiRequestError>({
    queryKey: [db, 'compactions-version', id],
    queryFn: async () => {
      const list = await fetchJson<VersionedCompactionsDto[]>(
        `${api(db)}/compactions?start=${id}&limit=1`,
      )
      return list.find((v) => v.id === id)
    },
    enabled: Number.isFinite(id),
    staleTime: Infinity, // immutable once written
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

export function useGcEvents() {
  const db = useDbId()
  return useQuery<GcEventsDto, ApiRequestError>({
    queryKey: [db, 'gc-events'],
    queryFn: () => fetchJson(`${api(db)}/garbage/events`),
    meta: { live: true },
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
