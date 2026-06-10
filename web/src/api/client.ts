import { keepPreviousData, useQuery } from '@tanstack/react-query'
import type {
  ActivityDto,
  CheckpointStatusDto,
  CompactorStateDto,
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

export function useHealth() {
  return useQuery<HealthDto, ApiRequestError>({
    queryKey: ['health'],
    queryFn: () => fetchJson('/api/health'),
  })
}

export function useOverview() {
  return useQuery<OverviewDto, ApiRequestError>({
    queryKey: ['overview'],
    queryFn: () => fetchJson('/api/overview'),
    refetchInterval: LIVE_REFETCH_MS,
  })
}

/**
 * LSM tree as of `manifestId`, or the live latest when undefined. Historical
 * manifests are immutable, so polling is off for them; keepPreviousData
 * stops the scrubber flashing a loading state on every step.
 */
export function useLsm(manifestId?: number) {
  return useQuery<LsmDto, ApiRequestError>({
    queryKey: ['lsm', manifestId ?? 'latest'],
    queryFn: () =>
      fetchJson(
        manifestId === undefined ? '/api/lsm' : `/api/lsm?manifest_id=${manifestId}`,
      ),
    refetchInterval: manifestId === undefined ? LIVE_REFETCH_MS : false,
    placeholderData: keepPreviousData,
  })
}

export function useManifestIds() {
  return useQuery<ManifestIdDto[], ApiRequestError>({
    queryKey: ['manifest-ids'],
    queryFn: () => fetchJson('/api/manifests/ids'),
    refetchInterval: LIVE_REFETCH_MS,
  })
}

export function useActivity(limit = 20) {
  return useQuery<ActivityDto[], ApiRequestError>({
    queryKey: ['activity', limit],
    queryFn: () => fetchJson(`/api/activity?limit=${limit}`),
    refetchInterval: LIVE_REFETCH_MS,
  })
}

export function useManifests(limit = 50) {
  return useQuery<ManifestSummaryDto[], ApiRequestError>({
    queryKey: ['manifests', limit],
    queryFn: () => fetchJson(`/api/manifests?limit=${limit}`),
    refetchInterval: LIVE_REFETCH_MS,
  })
}

export function useManifest(id: string) {
  return useQuery<ManifestDto, ApiRequestError>({
    queryKey: ['manifest', id],
    queryFn: () => fetchJson(`/api/manifests/${id}`),
  })
}

export function useManifestDiff(a: number, b: number) {
  return useQuery<ManifestDiffDto, ApiRequestError>({
    queryKey: ['manifest-diff', a, b],
    queryFn: () => fetchJson(`/api/manifests/diff?a=${a}&b=${b}`),
  })
}

export function useSst(ulid: string | null) {
  return useQuery<SstDetailDto, ApiRequestError>({
    queryKey: ['sst', ulid],
    queryFn: () => fetchJson(`/api/ssts/${ulid}`),
    enabled: ulid !== null,
  })
}

export function useWal() {
  return useQuery<WalDto, ApiRequestError>({
    queryKey: ['wal'],
    queryFn: () => fetchJson('/api/wal'),
    refetchInterval: LIVE_REFETCH_MS,
  })
}

export function useCompactorState() {
  return useQuery<CompactorStateDto, ApiRequestError>({
    queryKey: ['compactor-state'],
    queryFn: () => fetchJson('/api/compactor/state'),
    refetchInterval: LIVE_REFETCH_MS,
  })
}

export function useCompactions(limit = 20) {
  return useQuery<VersionedCompactionsDto[], ApiRequestError>({
    queryKey: ['compactions', limit],
    queryFn: () => fetchJson(`/api/compactions?limit=${limit}`),
    refetchInterval: LIVE_REFETCH_MS,
  })
}

export function useCheckpoints() {
  return useQuery<CheckpointStatusDto[], ApiRequestError>({
    queryKey: ['checkpoints'],
    queryFn: () => fetchJson('/api/checkpoints'),
    refetchInterval: LIVE_REFETCH_MS,
  })
}

export function useGarbage() {
  return useQuery<GarbageDto, ApiRequestError>({
    queryKey: ['garbage'],
    queryFn: () => fetchJson('/api/garbage'),
    refetchInterval: LIVE_REFETCH_MS,
  })
}

export function useClones() {
  return useQuery<ExternalDbDto[], ApiRequestError>({
    queryKey: ['clones'],
    queryFn: () => fetchJson('/api/clones'),
  })
}
