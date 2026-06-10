use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

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
    pub compacted_listing: TtlCache<Vec<CompactedEntry>>,
    pub compactor_state: TtlCache<CompactorStateDto>,
    pub manifest_by_id: LruMap<u64, VersionedManifest>,
    pub sst_details: LruMap<Ulid, SstDetailDto>,
    /// Summaries are pure functions of an immutable manifest, keyed by
    /// (manifest id, requested segment index; -1 = root/auto).
    pub lsm_summaries: LruMap<(u64, i64), LsmSummaryDto>,
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
            compacted_listing: TtlCache::new(ttl),
            compactor_state: TtlCache::new(ttl),
            manifest_by_id: LruMap::new(256),
            sst_details: LruMap::new(64),
            lsm_summaries: LruMap::new(128),
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
                        last_seen_at: prev_at,
                        missing_at: now,
                        referenced: refs.as_ref().map(|r| r.compacted.contains(ulid)),
                    });
                }
            }
        }
        obs.prev_compacted = Some((now, current));
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
                let prefix = self.root_path.child("manifest");
                let mut stream = self.object_store.list(Some(&prefix));
                let mut entries = Vec::new();
                while let Some(meta) = stream
                    .try_next()
                    .await
                    .map_err(|e| ApiError::Internal(format!("listing manifests: {e}")))?
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
                let prefix = self.root_path.child("wal");
                let mut stream = self.object_store.list(Some(&prefix));
                let mut entries = Vec::new();
                while let Some(meta) = stream
                    .try_next()
                    .await
                    .map_err(|e| ApiError::Internal(format!("listing wal: {e}")))?
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

    /// Listing of compacted SSTs, ascending by ULID. One object-store LIST.
    pub async fn compacted_entries(&self) -> Result<Arc<Vec<CompactedEntry>>, ApiError> {
        self.compacted_listing
            .get_with(|| async {
                let prefix = self.root_path.child("compacted");
                let mut stream = self.object_store.list(Some(&prefix));
                let mut entries = Vec::new();
                while let Some(meta) = stream
                    .try_next()
                    .await
                    .map_err(|e| ApiError::Internal(format!("listing compacted: {e}")))?
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
                self.observe_compacted(&entries);
                Ok(entries)
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
