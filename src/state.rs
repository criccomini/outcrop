use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use slatedb::admin::{Admin, AdminBuilder};
use slatedb::manifest::{SsTableId, VersionedManifest};
use slatedb::object_store::path::Path;
use slatedb::object_store::ObjectStore;
use slatedb::SstReader;
use ulid::Ulid;

use crate::cache::{LruMap, TtlCache};
use crate::dto::{CompactorStateDto, GcEventDto, LsmSummaryDto, SstDetailDto};
use crate::error::ApiError;

/// A manifest file as seen by a raw object-store listing. `list_manifests`
/// returns no timestamps, so this is where last_modified comes from.
#[derive(Clone, Debug)]
pub struct ManifestEntry {
    pub id: u64,
    pub size_bytes: u64,
    pub last_modified: DateTime<Utc>,
}

/// A WAL SST as seen by a raw object-store listing.
#[derive(Clone, Debug)]
pub struct WalEntry {
    pub id: u64,
    pub size_bytes: u64,
    pub last_modified: DateTime<Utc>,
}

/// A compacted SST as seen by a raw object-store listing.
#[derive(Clone, Debug)]
pub struct CompactedEntry {
    pub ulid: Ulid,
    pub size_bytes: u64,
    pub last_modified: DateTime<Utc>,
}

const GC_EVENT_CAP: usize = 500;

/// Most often the full reconciling sweep of `compacted/` may run.
const FULL_SWEEP_FLOOR: Duration = Duration::from_secs(60);
/// A sweep that took T schedules the next no sooner than FACTOR × T, so
/// sweep overhead stays a small fraction of wall time on huge DBs.
const FULL_SWEEP_FACTOR: u32 = 20;

/// Incrementally maintained listing of `compacted/` — the one directory
/// that grows with DB size, so full LISTs of it must not happen per poll.
/// Refreshes after the first are a single offset LIST from the last-seen
/// ULID (new SSTs sort after existing ones); deletions are reconciled by
/// background full sweeps on the adaptive schedule above, during which
/// requests keep serving the current snapshot.
pub struct CompactedCache {
    ttl: Duration,
    full_floor: Duration,
    full_factor: u32,
    inner: tokio::sync::Mutex<CompactedInner>,
}

#[derive(Default)]
struct CompactedInner {
    entries: Arc<Vec<CompactedEntry>>,
    fetched_at: Option<Instant>,
    full_at: Option<Instant>,
    full_dur: Duration,
    sweeping: bool,
}

impl CompactedCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            full_floor: FULL_SWEEP_FLOOR,
            full_factor: FULL_SWEEP_FACTOR,
            inner: tokio::sync::Mutex::new(CompactedInner::default()),
        }
    }

    #[cfg(test)]
    fn with_schedule(ttl: Duration, full_floor: Duration, full_factor: u32) -> Self {
        Self {
            ttl,
            full_floor,
            full_factor,
            inner: tokio::sync::Mutex::new(CompactedInner::default()),
        }
    }
}

/// Union of a sweep's results with snapshot entries written at/after `t0`:
/// a sweep that raced new writes must not make them vanish from the
/// snapshot (and then read as deletions) just because its LIST pages
/// predate them.
fn graft_recent(
    swept: Vec<CompactedEntry>,
    snapshot: &[CompactedEntry],
    t0: DateTime<Utc>,
) -> Vec<CompactedEntry> {
    let mut merged: BTreeMap<Ulid, CompactedEntry> =
        swept.into_iter().map(|e| (e.ulid, e)).collect();
    for e in snapshot {
        if e.last_modified >= t0 {
            merged.entry(e.ulid).or_insert_with(|| e.clone());
        }
    }
    merged.into_values().collect()
}

/// Observes object deletions by diffing consecutive listing refreshes.
/// SlateDB's GC leaves no record of what it deletes — objects simply
/// vanish from the store — so disappearance between two listings is the
/// only evidence there is. In-memory and per-process: observation starts
/// at the first listing and sweeps that happen while no dashboard is
/// running are never seen.
pub struct GcObserver {
    started_at: DateTime<Utc>,
    prev_compacted: Option<(DateTime<Utc>, HashMap<Ulid, (u64, DateTime<Utc>)>)>,
    prev_wal: Option<(DateTime<Utc>, HashMap<u64, (u64, DateTime<Utc>)>)>,
    prev_manifests: Option<(DateTime<Utc>, HashMap<u64, (u64, DateTime<Utc>)>)>,
    events: VecDeque<GcEventDto>,
}

impl GcObserver {
    fn new() -> Self {
        Self {
            started_at: Utc::now(),
            prev_compacted: None,
            prev_wal: None,
            prev_manifests: None,
            events: VecDeque::new(),
        }
    }

    fn push(&mut self, event: GcEventDto) {
        self.events.push_front(event);
        self.events.truncate(GC_EVENT_CAP);
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }

    pub fn events(&self) -> impl Iterator<Item = &GcEventDto> {
        self.events.iter()
    }
}

/// What the latest cached manifest keeps alive, for classifying whether a
/// vanished object was still referenced (which would be an anomaly).
struct LatestRefs {
    manifest_id: u64,
    compacted: std::collections::HashSet<Ulid>,
    replay_after_wal_id: u64,
    checkpoint_manifests: std::collections::HashSet<u64>,
}

pub struct AppState {
    pub admin: Admin,
    pub sst_reader: SstReader,
    pub object_store: Arc<dyn ObjectStore>,
    pub root_path: Path,
    pub db_path: String,
    pub provider: String,
    pub latest_manifest: TtlCache<Option<VersionedManifest>>,
    pub manifest_listing: TtlCache<Vec<ManifestEntry>>,
    pub wal_listing: TtlCache<Vec<WalEntry>>,
    pub compacted: CompactedCache,
    pub compactor_state: TtlCache<CompactorStateDto>,
    pub manifest_by_id: LruMap<u64, VersionedManifest>,
    pub sst_details: LruMap<Ulid, SstDetailDto>,
    /// Summaries are pure functions of an immutable manifest, keyed by
    /// (manifest id, requested segment index; -1 = root/auto).
    pub lsm_summaries: LruMap<(u64, i64), LsmSummaryDto>,
    /// Activity transitions, keyed (a, b): a diff of two immutable
    /// manifests never changes, so steady-state polls only compute pairs
    /// they have not seen before.
    pub activity_cache: LruMap<(u64, u64), crate::dto::ActivityDto>,
    pub gc_observer: std::sync::Mutex<GcObserver>,
}

impl AppState {
    pub fn new(
        db_path: String,
        provider: String,
        object_store: Arc<dyn ObjectStore>,
        ttl: Duration,
    ) -> Self {
        let admin = AdminBuilder::new(db_path.clone(), object_store.clone()).build();
        let sst_reader = SstReader::new(db_path.clone(), object_store.clone(), None, None);
        Self {
            admin,
            sst_reader,
            object_store,
            root_path: Path::from(db_path.clone()),
            db_path,
            provider,
            latest_manifest: TtlCache::new(ttl),
            manifest_listing: TtlCache::new(ttl),
            wal_listing: TtlCache::new(ttl),
            compacted: CompactedCache::new(ttl),
            compactor_state: TtlCache::new(ttl),
            manifest_by_id: LruMap::new(256),
            sst_details: LruMap::new(64),
            lsm_summaries: LruMap::new(128),
            activity_cache: LruMap::new(512),
            gc_observer: std::sync::Mutex::new(GcObserver::new()),
        }
    }

    /// Reference set from the latest *cached* manifest (no fetch); None
    /// when nothing is cached or a refresh holds the lock.
    fn peek_latest_refs(&self) -> Option<LatestRefs> {
        let cached = self.latest_manifest.peek()?;
        let m = cached.as_ref().as_ref()?;
        let mut compacted = std::collections::HashSet::new();
        let mut collect = |l0: &std::collections::VecDeque<slatedb::manifest::SsTableView>,
                           runs: &[slatedb::manifest::SortedRun]| {
            for view in l0.iter().chain(runs.iter().flat_map(|r| r.sst_views.iter())) {
                if let SsTableId::Compacted(ulid) = view.sst.id {
                    compacted.insert(ulid);
                }
            }
        };
        collect(m.l0(), m.compacted());
        for seg in m.segments() {
            collect(seg.l0(), seg.compacted());
        }
        // Only unexpired checkpoints pin manifests: the GC removes expired
        // ones and then deletes their targets in the same sweep, so counting
        // them here would flag every such deletion as an anomaly.
        let now = Utc::now();
        Some(LatestRefs {
            manifest_id: m.id(),
            compacted,
            replay_after_wal_id: m.replay_after_wal_id(),
            checkpoint_manifests: m
                .checkpoints()
                .iter()
                .filter(|c| !c.expire_time.is_some_and(|t| t <= now))
                .map(|c| c.manifest_id)
                .collect(),
        })
    }

    fn observe_compacted(&self, entries: &[CompactedEntry]) {
        let now = Utc::now();
        let refs = self.peek_latest_refs();
        let current: HashMap<Ulid, (u64, DateTime<Utc>)> = entries
            .iter()
            .map(|e| (e.ulid, (e.size_bytes, e.last_modified)))
            .collect();
        let mut obs = self.gc_observer.lock().unwrap();
        if let Some((prev_at, prev)) = obs.prev_compacted.take() {
            for (ulid, (size, written)) in &prev {
                if !current.contains_key(ulid) {
                    obs.push(GcEventDto {
                        kind: "compacted",
                        id: ulid.to_string(),
                        size_bytes: *size,
                        written_at: *written,
                        // Entries noted between observations were written
                        // after prev_at; never report a last-seen older
                        // than the write itself.
                        last_seen_at: prev_at.max(*written),
                        missing_at: now,
                        referenced: refs.as_ref().map(|r| r.compacted.contains(ulid)),
                    });
                }
            }
        }
        obs.prev_compacted = Some((now, current));
    }

    /// Fold incrementally discovered SSTs into the observer's snapshot
    /// without diffing, so ones deleted again before the next reconciling
    /// sweep still surface as deletion events there.
    fn note_compacted_additions(&self, new: &[CompactedEntry]) {
        let mut obs = self.gc_observer.lock().unwrap();
        if let Some((_, prev)) = obs.prev_compacted.as_mut() {
            for e in new {
                prev.insert(e.ulid, (e.size_bytes, e.last_modified));
            }
        }
    }

    fn observe_wal(&self, entries: &[WalEntry]) {
        let now = Utc::now();
        let refs = self.peek_latest_refs();
        let current: HashMap<u64, (u64, DateTime<Utc>)> = entries
            .iter()
            .map(|e| (e.id, (e.size_bytes, e.last_modified)))
            .collect();
        let mut obs = self.gc_observer.lock().unwrap();
        if let Some((prev_at, prev)) = obs.prev_wal.take() {
            for (id, (size, written)) in &prev {
                if !current.contains_key(id) {
                    obs.push(GcEventDto {
                        kind: "wal",
                        id: format!("#{id}"),
                        size_bytes: *size,
                        written_at: *written,
                        last_seen_at: prev_at,
                        missing_at: now,
                        referenced: refs.as_ref().map(|r| *id > r.replay_after_wal_id),
                    });
                }
            }
        }
        obs.prev_wal = Some((now, current));
    }

    fn observe_manifests(&self, entries: &[ManifestEntry]) {
        let now = Utc::now();
        let refs = self.peek_latest_refs();
        let current: HashMap<u64, (u64, DateTime<Utc>)> = entries
            .iter()
            .map(|e| (e.id, (e.size_bytes, e.last_modified)))
            .collect();
        let mut obs = self.gc_observer.lock().unwrap();
        if let Some((prev_at, prev)) = obs.prev_manifests.take() {
            for (id, (size, written)) in &prev {
                if !current.contains_key(id) {
                    obs.push(GcEventDto {
                        kind: "manifest",
                        id: format!("#{id}"),
                        size_bytes: *size,
                        written_at: *written,
                        last_seen_at: prev_at,
                        missing_at: now,
                        referenced: refs.as_ref().map(|r| {
                            *id == r.manifest_id || r.checkpoint_manifests.contains(id)
                        }),
                    });
                }
            }
        }
        obs.prev_manifests = Some((now, current));
    }

    pub async fn latest_manifest(&self) -> Result<Arc<Option<VersionedManifest>>, ApiError> {
        self.latest_manifest
            .get_with(|| async {
                self.admin
                    .read_manifest(None)
                    .await
                    .map_err(ApiError::from)
            })
            .await
    }

    /// Listing of manifest files, ascending by id. One object-store LIST.
    pub async fn manifest_entries(&self) -> Result<Arc<Vec<ManifestEntry>>, ApiError> {
        self.manifest_listing
            .get_with(|| async {
                let prefix = self.root_path.clone().join("manifest");
                let mut stream = self.object_store.list(Some(&prefix));
                let mut entries = Vec::new();
                while let Some(meta) = stream
                    .try_next()
                    .await
                    .map_err(|e| {
                        tracing::debug!("listing manifests at '{}': {e}", self.db_path);
                        ApiError::Internal("error listing manifests".to_string())
                    })?
                {
                    let Some(name) = meta.location.filename() else {
                        continue;
                    };
                    let Some(stem) = name.strip_suffix(".manifest") else {
                        continue;
                    };
                    let Ok(id) = stem.parse::<u64>() else {
                        continue;
                    };
                    entries.push(ManifestEntry {
                        id,
                        size_bytes: meta.size,
                        last_modified: meta.last_modified,
                    });
                }
                entries.sort_by_key(|e| e.id);
                self.observe_manifests(&entries);
                Ok(entries)
            })
            .await
    }

    /// Listing of WAL SSTs, ascending by id. One object-store LIST.
    pub async fn wal_entries(&self) -> Result<Arc<Vec<WalEntry>>, ApiError> {
        self.wal_listing
            .get_with(|| async {
                let prefix = self.root_path.clone().join("wal");
                let mut stream = self.object_store.list(Some(&prefix));
                let mut entries = Vec::new();
                while let Some(meta) = stream
                    .try_next()
                    .await
                    .map_err(|e| {
                        tracing::debug!("listing wal at '{}': {e}", self.db_path);
                        ApiError::Internal("error listing wal".to_string())
                    })?
                {
                    let Some(name) = meta.location.filename() else {
                        continue;
                    };
                    let Some(stem) = name.strip_suffix(".sst") else {
                        continue;
                    };
                    let Ok(id) = stem.parse::<u64>() else {
                        continue;
                    };
                    entries.push(WalEntry {
                        id,
                        size_bytes: meta.size,
                        last_modified: meta.last_modified,
                    });
                }
                entries.sort_by_key(|e| e.id);
                self.observe_wal(&entries);
                Ok(entries)
            })
            .await
    }

    /// One LIST over `compacted/`, full or from an offset, without caching.
    async fn list_compacted(&self, offset: Option<&Path>) -> Result<Vec<CompactedEntry>, ApiError> {
        let prefix = self.root_path.clone().join("compacted");
        let mut stream = match offset {
            Some(offset) => self.object_store.list_with_offset(Some(&prefix), offset),
            None => self.object_store.list(Some(&prefix)),
        };
        let mut entries = Vec::new();
        while let Some(meta) = stream
            .try_next()
            .await
            .map_err(|e| {
                tracing::debug!("listing compacted at '{}': {e}", self.db_path);
                ApiError::Internal("error listing compacted SSTs".to_string())
            })?
        {
            let Some(name) = meta.location.filename() else {
                continue;
            };
            let Some(stem) = name.strip_suffix(".sst") else {
                continue;
            };
            let Ok(ulid) = Ulid::from_string(stem) else {
                continue;
            };
            entries.push(CompactedEntry {
                ulid,
                size_bytes: meta.size,
                last_modified: meta.last_modified,
            });
        }
        entries.sort_by_key(|e| e.ulid);
        Ok(entries)
    }

    /// Listing of compacted SSTs, ascending by ULID, served from the
    /// incrementally maintained snapshot (see [`CompactedCache`]). Note the
    /// staleness contract: additions appear within one TTL, deletions only
    /// at the next reconciling sweep.
    pub async fn compacted_entries(self: &Arc<Self>) -> Result<Arc<Vec<CompactedEntry>>, ApiError> {
        let mut inner = self.compacted.inner.lock().await;
        match inner.fetched_at {
            // First call: nothing to serve yet, so the full list runs inline.
            None => {
                let started = Instant::now();
                let entries = self.list_compacted(None).await?;
                self.observe_compacted(&entries);
                inner.full_dur = started.elapsed();
                inner.entries = Arc::new(entries);
                let done = Instant::now();
                inner.fetched_at = Some(done);
                inner.full_at = Some(done);
            }
            Some(at) if at.elapsed() >= self.compacted.ttl => match inner.entries.last().map(|e| e.ulid) {
                Some(last_ulid) => {
                    let offset = self
                        .root_path
                        .clone()
                        .join("compacted")
                        .join(format!("{last_ulid}.sst"));
                    let new = self.list_compacted(Some(&offset)).await?;
                    if !new.is_empty() {
                        self.note_compacted_additions(&new);
                        let mut merged = (*inner.entries).clone();
                        merged.extend(new);
                        merged.sort_by_key(|e| e.ulid);
                        merged.dedup_by_key(|e| e.ulid);
                        inner.entries = Arc::new(merged);
                    }
                    inner.fetched_at = Some(Instant::now());
                }
                None => {
                    // Empty snapshot: a full list is as cheap as an
                    // incremental one and doubles as a sweep.
                    let started = Instant::now();
                    let entries = self.list_compacted(None).await?;
                    self.observe_compacted(&entries);
                    inner.full_dur = started.elapsed();
                    inner.entries = Arc::new(entries);
                    let done = Instant::now();
                    inner.fetched_at = Some(done);
                    inner.full_at = Some(done);
                }
            },
            Some(_) => {}
        }
        // Kick a background reconciling sweep when one is due; requests
        // keep serving the snapshot while it pages through the store.
        let interval = self
            .compacted
            .full_floor
            .max(inner.full_dur * self.compacted.full_factor);
        if !inner.sweeping && inner.full_at.is_some_and(|at| at.elapsed() >= interval) {
            inner.sweeping = true;
            let state = self.clone();
            tokio::spawn(async move { state.full_sweep().await });
        }
        Ok(inner.entries.clone())
    }

    async fn full_sweep(self: Arc<Self>) {
        // Entries created while the sweep's LIST pages stream in may be
        // missing from its result; the margin below is grafted back from
        // the snapshot so they cannot read as deletions.
        let t0 = Utc::now() - chrono::Duration::seconds(1);
        let started = Instant::now();
        let result = self.list_compacted(None).await;
        let dur = started.elapsed();
        let mut inner = self.compacted.inner.lock().await;
        inner.sweeping = false;
        match result {
            Ok(swept) => {
                let merged = graft_recent(swept, &inner.entries, t0);
                self.observe_compacted(&merged);
                inner.entries = Arc::new(merged);
                let done = Instant::now();
                inner.full_at = Some(done);
                inner.fetched_at = Some(done);
                inner.full_dur = dur;
            }
            Err(e) => {
                tracing::warn!("compacted sweep at '{}' failed: {e}", self.db_path);
                // Try again after a full interval, not immediately.
                inner.full_at = Some(Instant::now());
            }
        }
    }

    /// Compactor state (latest manifest id + compactions file), TTL-cached
    /// and shared by the compactions and garbage endpoints.
    pub async fn compactor_state_dto(&self) -> Result<Arc<CompactorStateDto>, ApiError> {
        self.compactor_state
            .get_with(|| async {
                let view = self
                    .admin
                    .read_compactor_state_view()
                    .await
                    .map_err(ApiError::from)?;
                Ok::<_, ApiError>(CompactorStateDto {
                    manifest_id: view.manifest().id(),
                    compactions: view
                        .compactions()
                        .map(crate::convert::versioned_compactions_dto),
                })
            })
            .await
    }

    /// Manifest by id, served from the immutable LRU when possible.
    /// Returns None if the manifest does not exist (e.g. GC'd).
    pub async fn manifest_by_id(
        &self,
        id: u64,
    ) -> Result<Option<Arc<VersionedManifest>>, ApiError> {
        if let Some(m) = self.manifest_by_id.get(&id) {
            return Ok(Some(m));
        }
        match self.admin.read_manifest(Some(id)).await? {
            Some(m) => {
                let m = Arc::new(m);
                self.manifest_by_id.put(id, m.clone());
                Ok(Some(m))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slatedb::object_store::memory::InMemory;
    use slatedb::object_store::ObjectStoreExt;

    fn ulid_at(ms: u64, i: u128) -> Ulid {
        Ulid::from_parts(ms, i)
    }

    async fn put_sst(store: &dyn ObjectStore, ulid: Ulid) {
        store
            .put(
                &Path::from(format!("db/compacted/{ulid}.sst")),
                slatedb::bytes::Bytes::from_static(b"x").into(),
            )
            .await
            .unwrap();
    }

    async fn delete_sst(store: &dyn ObjectStore, ulid: Ulid) {
        store
            .delete(&Path::from(format!("db/compacted/{ulid}.sst")))
            .await
            .unwrap();
    }

    fn state_with(store: Arc<dyn ObjectStore>, cache: CompactedCache) -> Arc<AppState> {
        let mut s = AppState::new(
            "db".to_string(),
            "memory".to_string(),
            store,
            Duration::from_secs(5),
        );
        s.compacted = cache;
        Arc::new(s)
    }

    #[tokio::test]
    async fn compacted_refresh_is_incremental_between_sweeps() {
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let (u1, u2, u3) = (ulid_at(1000, 1), ulid_at(2000, 2), ulid_at(3000, 3));
        put_sst(store.as_ref(), u1).await;
        put_sst(store.as_ref(), u2).await;
        // TTL zero so every call refreshes; sweep schedule far away.
        let state = state_with(
            store.clone(),
            CompactedCache::with_schedule(Duration::ZERO, Duration::from_secs(3600), 1000),
        );
        let first = state.compacted_entries().await.unwrap();
        assert_eq!(
            first.iter().map(|e| e.ulid).collect::<Vec<_>>(),
            vec![u1, u2]
        );

        put_sst(store.as_ref(), u3).await;
        delete_sst(store.as_ref(), u1).await;
        let second = state.compacted_entries().await.unwrap();
        // u3 arrived via the offset list; u1's deletion stays invisible
        // until a sweep — proof the refresh did not re-list the directory.
        assert_eq!(
            second.iter().map(|e| e.ulid).collect::<Vec<_>>(),
            vec![u1, u2, u3]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compacted_sweep_reconciles_deletions_and_records_event() {
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let (u1, u2) = (ulid_at(1000, 1), ulid_at(2000, 2));
        put_sst(store.as_ref(), u1).await;
        put_sst(store.as_ref(), u2).await;
        // Sweeps due immediately after the seeding call. The deletion still
        // takes ~1s to surface: the sweep's recent-write graft keeps the
        // just-written u1 until it ages past the race margin.
        let state = state_with(
            store.clone(),
            CompactedCache::with_schedule(Duration::ZERO, Duration::ZERO, 0),
        );
        state.compacted_entries().await.unwrap();
        delete_sst(store.as_ref(), u1).await;

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let entries = state.compacted_entries().await.unwrap();
            if entries.len() == 1 {
                assert_eq!(entries[0].ulid, u2);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "sweep never reconciled the deletion"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let obs = state.gc_observer.lock().unwrap();
        assert!(obs
            .events()
            .any(|e| e.kind == "compacted" && e.id == u1.to_string()));
    }

    #[test]
    fn graft_recent_keeps_only_young_missing_entries() {
        let t0 = Utc::now();
        let entry = |ulid: Ulid, at: DateTime<Utc>| CompactedEntry {
            ulid,
            size_bytes: 1,
            last_modified: at,
        };
        let (u1, u2, u3) = (ulid_at(1, 1), ulid_at(2, 2), ulid_at(3, 3));
        let snapshot = vec![
            entry(u1, t0 - chrono::Duration::seconds(30)), // old, deleted
            entry(u2, t0 - chrono::Duration::seconds(20)), // still listed
            entry(u3, t0 + chrono::Duration::seconds(1)),  // young, raced
        ];
        let swept = vec![entry(u2, t0 - chrono::Duration::seconds(20))];
        let merged = graft_recent(swept, &snapshot, t0);
        assert_eq!(
            merged.iter().map(|e| e.ulid).collect::<Vec<_>>(),
            vec![u2, u3]
        );
    }
}
