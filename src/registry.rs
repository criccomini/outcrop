//! Multi-DB core: holds the configured stores, runs (and caches) DB
//! discovery across them, and lazily builds one `AppState` + API router per
//! discovered DB. The per-DB router is the unchanged `routes::api_router`,
//! so the registry is purely a layer above the existing single-DB code.

use std::collections::HashMap;
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
    /// store that fails to list is logged and skipped so one bad store
    /// can't blank out the whole fleet.
    pub async fn scan(&self, force: bool) -> Result<ScanResult, ApiError> {
        let mut guard = self.scan.lock().await;
        if !force {
            if let Some((at, result)) = guard.as_ref() {
                if at.elapsed() < self.scan_ttl {
                    return Ok(result.clone());
                }
            }
        }

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
                        tracing::warn!(store = store.name, root, "discovery failed: {e}");
                    }
                }
            }
        }
        found.sort_by(|a, b| a.id.cmp(&b.id));
        found.dedup_by(|a, b| a.id == b.id);

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
    use slatedb::object_store::memory::InMemory;
    use slatedb::object_store::path::Path;

    async fn registry_with_one_db() -> Registry {
        let store = InMemory::new();
        store
            .put(
                &Path::from("demo/manifest/00000000000000000001.manifest"),
                slatedb::bytes::Bytes::from_static(b"x").into(),
            )
            .await
            .unwrap();
        Registry::new(
            vec![Store {
                name: "mem".into(),
                provider: "memory".into(),
                object_store: Arc::new(store),
                roots: vec![String::new()],
            }],
            ScanLimits::default(),
            Duration::from_secs(60),
            Duration::from_secs(5),
        )
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
    async fn resolve_caches_handles_and_rejects_unknown_ids() {
        let registry = registry_with_one_db().await;
        let a = registry.resolve("mem:demo").await.unwrap();
        let b = registry.resolve("mem:demo").await.unwrap();
        assert!(Arc::ptr_eq(&a.state, &b.state));
        assert!(registry.resolve("mem:nope").await.is_err());
        assert!(registry.resolve("other:demo").await.is_err());
    }
}
