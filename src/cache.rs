use std::future::Future;
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Single-value TTL cache. The mutex is held across the refresh, so
/// concurrent callers during a refresh share one object-store fetch.
pub struct TtlCache<T> {
    ttl: Duration,
    slot: tokio::sync::Mutex<Option<(Instant, Arc<T>)>>,
}

impl<T> TtlCache<T> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            slot: tokio::sync::Mutex::new(None),
        }
    }

    pub async fn get_with<F, Fut, E>(&self, fetch: F) -> Result<Arc<T>, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let mut slot = self.slot.lock().await;
        if let Some((at, value)) = slot.as_ref() {
            if at.elapsed() < self.ttl {
                return Ok(value.clone());
            }
        }
        let value = Arc::new(fetch().await?);
        *slot = Some((Instant::now(), value.clone()));
        Ok(value)
    }
}

/// LRU cache for objects that are immutable once written (manifests by id,
/// SST details by ULID).
pub struct LruMap<K: Hash + Eq, V> {
    inner: std::sync::Mutex<lru::LruCache<K, Arc<V>>>,
}

impl<K: Hash + Eq, V> LruMap<K, V> {
    pub fn new(cap: usize) -> Self {
        Self {
            inner: std::sync::Mutex::new(lru::LruCache::new(
                NonZeroUsize::new(cap).expect("cache capacity must be non-zero"),
            )),
        }
    }

    pub fn get(&self, k: &K) -> Option<Arc<V>> {
        self.inner.lock().unwrap().get(k).cloned()
    }

    pub fn put(&self, k: K, v: Arc<V>) {
        self.inner.lock().unwrap().put(k, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn ttl_cache_serves_cached_value_within_ttl() {
        let cache: TtlCache<u64> = TtlCache::new(Duration::from_secs(60));
        let calls = AtomicUsize::new(0);
        for _ in 0..3 {
            let v = cache
                .get_with(|| async {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, ()>(42)
                })
                .await
                .unwrap();
            assert_eq!(*v, 42);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn ttl_cache_refreshes_after_expiry() {
        let cache: TtlCache<u64> = TtlCache::new(Duration::ZERO);
        let calls = AtomicUsize::new(0);
        for _ in 0..3 {
            cache
                .get_with(|| async {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, ()>(1)
                })
                .await
                .unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
}
