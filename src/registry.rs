//! Multi-DB core: holds the configured stores, runs (and caches) DB
//! discovery across them, and lazily builds one `AppState` + API router per
//! discovered DB. The per-DB router is the unchanged `routes::api_router`,
//! so the registry is purely a layer above the existing single-DB code.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use chrono::{DateTime, Utc};
use slatedb::object_store::ObjectStore;

use crate::discovery::{discover, ScanLimits};
use crate::error::ApiError;
use crate::state::AppState;

pub struct Store {
    pub name: String,
    pub provider: String,
    pub object_store: Arc<dyn ObjectStore>,
    pub roots: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct DbInfo {
    pub id: String,
    pub store: String,
    pub path: String,
}

#[derive(Clone)]
pub struct DbHandle {
    pub state: Arc<AppState>,
    pub router: Router,
}

type ScanResult = (DateTime<Utc>, Vec<DbInfo>);

/// Whether DB `path` lies under discovery root `root` (both
/// store-relative, '/'-separated, no trailing slashes).
fn under_root(path: &str, root: &str) -> bool {
    root.is_empty()
        || path == root
        || path.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}

pub struct Registry {
    stores: Vec<Store>,
    limits: ScanLimits,
    scan_ttl: Duration,
    /// TTL for each DB's mutable-state caches (AppState).
    cache_ttl: Duration,
    /// Mutex held across the scan so concurrent callers share one walk,
    /// mirroring `TtlCache`.
    scan: tokio::sync::Mutex<Option<(Instant, ScanResult)>>,
    dbs: std::sync::Mutex<HashMap<String, DbHandle>>,
}

impl Registry {
    pub fn new(
        stores: Vec<Store>,
        limits: ScanLimits,
        scan_ttl: Duration,
        cache_ttl: Duration,
    ) -> Self {
        Self {
            stores,
            limits,
            scan_ttl,
            cache_ttl,
            scan: tokio::sync::Mutex::new(None),
            dbs: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn store_count(&self) -> usize {
        self.stores.len()
    }

    /// Latest scan without triggering one (for /api/health).
    pub fn peek_db_count(&self) -> Option<usize> {
        self.scan
            .try_lock()
            .ok()
            .and_then(|g| g.as_ref().map(|(_, (_, dbs))| dbs.len()))
    }

    /// Discovers DBs across every store root, cached for `scan_ttl`. A
    /// (store, root) whose walk fails is logged and keeps the previous
    /// scan's DBs, so one bad store can't blank out the whole fleet and a
    /// transient LIST error can't 404 a store's live DBs for a scan_ttl.
    pub async fn scan(&self, force: bool) -> Result<ScanResult, ApiError> {
        let mut guard = self.scan.lock().await;
        if !force {
            if let Some((at, result)) = guard.as_ref() {
                if at.elapsed() < self.scan_ttl {
                    return Ok(result.clone());
                }
            }
        }
        let prev: Vec<DbInfo> = guard
            .as_ref()
            .map(|(_, (_, dbs))| dbs.clone())
            .unwrap_or_default();

        let mut found: Vec<DbInfo> = Vec::new();
        for store in &self.stores {
            for root in &store.roots {
                match discover(store.object_store.as_ref(), root, &self.limits).await {
                    Ok(paths) => found.extend(paths.into_iter().map(|path| DbInfo {
                        id: format!("{}:{}", store.name, path),
                        store: store.name.clone(),
                        path,
                    })),
                    Err(e) => {
                        tracing::warn!(
                            store = store.name,
                            root,
                            "discovery failed, keeping previously found DBs: {e}"
                        );
                        let root = root.trim_matches('/');
                        found.extend(
                            prev.iter()
                                .filter(|d| d.store == store.name && under_root(&d.path, root))
                                .cloned(),
                        );
                    }
                }
            }
        }
        found.sort_by(|a, b| a.id.cmp(&b.id));
        found.dedup_by(|a, b| a.id == b.id);

        // Drop handles for DBs that vanished: a DB recreated at the same
        // path must get a fresh AppState, or its manifest-id-keyed caches
        // (ids restart at 1) would serve the old DB's manifests forever.
        {
            let ids: HashSet<&str> = found.iter().map(|d| d.id.as_str()).collect();
            self.dbs.lock().unwrap().retain(|id, _| ids.contains(id.as_str()));
        }

        let result = (Utc::now(), found);
        *guard = Some((Instant::now(), result.clone()));
        Ok(result)
    }

    /// Resolves a DB id to its lazily-created handle. Only ids present in
    /// the current scan resolve, so removed DBs 404 after the next scan.
    pub async fn resolve(&self, id: &str) -> Result<DbHandle, ApiError> {
        let (_, dbs) = self.scan(false).await?;
        let Some(info) = dbs.iter().find(|d| d.id == id) else {
            return Err(ApiError::NotFound(format!("no such database: {id}")));
        };

        let mut map = self.dbs.lock().unwrap();
        if let Some(handle) = map.get(id) {
            return Ok(handle.clone());
        }
        let store = self
            .stores
            .iter()
            .find(|s| s.name == info.store)
            .expect("scan only yields configured stores");
        let state = Arc::new(AppState::new(
            info.path.clone(),
            store.provider.clone(),
            store.object_store.clone(),
            self.cache_ttl,
        ));
        let handle = DbHandle {
            state: state.clone(),
            router: crate::routes::api_router(state),
        };
        map.insert(id.to_string(), handle.clone());
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    use futures::stream::BoxStream;
    use slatedb::object_store::memory::InMemory;
    use slatedb::object_store::path::Path;
    use slatedb::object_store::{
        self, CopyOptions, GetOptions, GetResult, ListResult, MultipartUpload, ObjectMeta,
        ObjectStoreExt, PutMultipartOptions, PutOptions, PutPayload, PutResult,
    };

    /// InMemory wrapper whose delimiter LISTs (what discovery walks) can be
    /// made to fail, simulating a store outage.
    #[derive(Debug, Default)]
    struct FlakyStore {
        inner: InMemory,
        fail_lists: AtomicBool,
    }

    impl std::fmt::Display for FlakyStore {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "FlakyStore")
        }
    }

    #[async_trait::async_trait]
    impl ObjectStore for FlakyStore {
        async fn put_opts(
            &self,
            location: &Path,
            payload: PutPayload,
            opts: PutOptions,
        ) -> object_store::Result<PutResult> {
            self.inner.put_opts(location, payload, opts).await
        }
        async fn put_multipart_opts(
            &self,
            location: &Path,
            opts: PutMultipartOptions,
        ) -> object_store::Result<Box<dyn MultipartUpload>> {
            self.inner.put_multipart_opts(location, opts).await
        }
        async fn get_opts(
            &self,
            location: &Path,
            options: GetOptions,
        ) -> object_store::Result<GetResult> {
            self.inner.get_opts(location, options).await
        }
        fn delete_stream(
            &self,
            locations: BoxStream<'static, object_store::Result<Path>>,
        ) -> BoxStream<'static, object_store::Result<Path>> {
            self.inner.delete_stream(locations)
        }
        fn list(&self, prefix: Option<&Path>) -> BoxStream<'static, object_store::Result<ObjectMeta>> {
            self.inner.list(prefix)
        }
        async fn list_with_delimiter(
            &self,
            prefix: Option<&Path>,
        ) -> object_store::Result<ListResult> {
            if self.fail_lists.load(Ordering::Relaxed) {
                return Err(object_store::Error::Generic {
                    store: "flaky",
                    source: "simulated outage".into(),
                });
            }
            self.inner.list_with_delimiter(prefix).await
        }
        async fn copy_opts(
            &self,
            from: &Path,
            to: &Path,
            options: CopyOptions,
        ) -> object_store::Result<()> {
            self.inner.copy_opts(from, to, options).await
        }
    }

    const DEMO_MANIFEST: &str = "demo/manifest/00000000000000000001.manifest";

    fn registry_over(store: Arc<dyn ObjectStore>) -> Registry {
        Registry::new(
            vec![Store {
                name: "mem".into(),
                provider: "memory".into(),
                object_store: store,
                roots: vec![String::new()],
            }],
            ScanLimits::default(),
            Duration::from_secs(60),
            Duration::from_secs(5),
        )
    }

    async fn registry_with_one_db() -> Registry {
        let store = InMemory::new();
        store
            .put(
                &Path::from(DEMO_MANIFEST),
                slatedb::bytes::Bytes::from_static(b"x").into(),
            )
            .await
            .unwrap();
        registry_over(Arc::new(store))
    }

    #[tokio::test]
    async fn scan_finds_and_ids_dbs() {
        let registry = registry_with_one_db().await;
        let (_, dbs) = registry.scan(false).await.unwrap();
        assert_eq!(dbs.len(), 1);
        assert_eq!(dbs[0].id, "mem:demo");
        assert_eq!(dbs[0].store, "mem");
        assert_eq!(dbs[0].path, "demo");
        assert_eq!(registry.peek_db_count(), Some(1));
    }

    #[tokio::test]
    async fn failed_discovery_keeps_previous_dbs() {
        let store = Arc::new(FlakyStore::default());
        store
            .put(
                &Path::from(DEMO_MANIFEST),
                slatedb::bytes::Bytes::from_static(b"x").into(),
            )
            .await
            .unwrap();
        let registry = registry_over(store.clone());
        let (_, dbs) = registry.scan(false).await.unwrap();
        assert_eq!(dbs.len(), 1);

        // Outage: a forced scan must not blank out the known DBs, and they
        // must keep resolving.
        store.fail_lists.store(true, Ordering::Relaxed);
        let (_, dbs) = registry.scan(true).await.unwrap();
        assert_eq!(dbs.len(), 1);
        assert_eq!(dbs[0].id, "mem:demo");
        assert!(registry.resolve("mem:demo").await.is_ok());

        // Recovery: a real deletion is still observed by the next walk.
        store.fail_lists.store(false, Ordering::Relaxed);
        store.delete(&Path::from(DEMO_MANIFEST)).await.unwrap();
        let (_, dbs) = registry.scan(true).await.unwrap();
        assert!(dbs.is_empty());
    }

    #[tokio::test]
    async fn recreated_db_gets_a_fresh_state() {
        let store = Arc::new(InMemory::new());
        store
            .put(
                &Path::from(DEMO_MANIFEST),
                slatedb::bytes::Bytes::from_static(b"x").into(),
            )
            .await
            .unwrap();
        let registry = registry_over(store.clone());
        let old = registry.resolve("mem:demo").await.unwrap();

        // DB deleted: the next scan must evict its handle.
        store.delete(&Path::from(DEMO_MANIFEST)).await.unwrap();
        registry.scan(true).await.unwrap();
        assert!(registry.resolve("mem:demo").await.is_err());

        // Recreated at the same path: must not see the old AppState (its
        // manifest-id-keyed caches belong to the deleted DB).
        store
            .put(
                &Path::from(DEMO_MANIFEST),
                slatedb::bytes::Bytes::from_static(b"y").into(),
            )
            .await
            .unwrap();
        registry.scan(true).await.unwrap();
        let new = registry.resolve("mem:demo").await.unwrap();
        assert!(!Arc::ptr_eq(&old.state, &new.state));
    }

    #[test]
    fn under_root_matches_whole_segments_only() {
        assert!(under_root("teams/a/db1", ""));
        assert!(under_root("teams/a/db1", "teams"));
        assert!(under_root("teams", "teams"));
        assert!(!under_root("teams2/db", "teams"));
        assert!(!under_root("team", "teams"));
    }

    #[tokio::test]
    async fn resolve_caches_handles_and_rejects_unknown_ids() {
        let registry = registry_with_one_db().await;
        let a = registry.resolve("mem:demo").await.unwrap();
        let b = registry.resolve("mem:demo").await.unwrap();
        assert!(Arc::ptr_eq(&a.state, &b.state));
        assert!(registry.resolve("mem:nope").await.is_err());
        assert!(registry.resolve("other:demo").await.is_err());
    }
}
