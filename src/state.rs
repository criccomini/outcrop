use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use slatedb::admin::{Admin, AdminBuilder};
use slatedb::manifest::VersionedManifest;
use slatedb::object_store::path::Path;
use slatedb::object_store::ObjectStore;
use slatedb::SstReader;
use ulid::Ulid;

use crate::cache::{LruMap, TtlCache};
use crate::dto::{CompactorStateDto, SstDetailDto};
use crate::error::ApiError;

/// A manifest file as seen by a raw object-store listing. `list_manifests`
/// returns no timestamps, so this is where last_modified comes from.
#[derive(Clone, Debug)]
pub struct ManifestEntry {
    pub id: u64,
    pub last_modified: DateTime<Utc>,
}

/// A WAL SST as seen by a raw object-store listing.
#[derive(Clone, Debug)]
pub struct WalEntry {
    pub id: u64,
    pub size_bytes: u64,
    pub last_modified: DateTime<Utc>,
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
    pub compactor_state: TtlCache<CompactorStateDto>,
    pub manifest_by_id: LruMap<u64, VersionedManifest>,
    pub sst_details: LruMap<Ulid, SstDetailDto>,
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
            compactor_state: TtlCache::new(ttl),
            manifest_by_id: LruMap::new(256),
            sst_details: LruMap::new(64),
        }
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
                        last_modified: meta.last_modified,
                    });
                }
                entries.sort_by_key(|e| e.id);
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
