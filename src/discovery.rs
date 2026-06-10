//! Auto-discovery of SlateDB roots: walks an object store's "directory"
//! tree via delimiter listings, detecting a DB wherever a prefix has a
//! `manifest/` child containing `<u64>.manifest` objects. Detected DBs are
//! not descended into, and the walk is bounded by depth and a total prefix
//! budget so a huge bucket can't turn discovery into a LIST storm.

use std::collections::VecDeque;

use futures::{StreamExt, TryStreamExt};
use slatedb::object_store::path::Path;
use slatedb::object_store::ObjectStore;

pub struct ScanLimits {
    /// How many "directory" levels below each root to descend.
    pub max_depth: usize,
    /// Total prefixes visited per root before the walk stops with a warn.
    pub max_prefixes: usize,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_prefixes: 2000,
        }
    }
}

/// Objects probed per `manifest/` dir before giving up. Strays that hide
/// the numbered manifests must sort before the digits (e.g. `.DS_Store`),
/// and there are never many of those.
const DB_ROOT_PROBE_LIMIT: usize = 50;

/// True when `prefix/manifest/` contains at least one `<u64>.manifest`.
/// Listings are lexicographic and strays like `.DS_Store` sort before the
/// numbered manifests, so a bounded prefix of the listing is scanned
/// rather than only the first object.
async fn is_db_root(store: &dyn ObjectStore, manifest_prefix: &Path) -> bool {
    let mut stream = store.list(Some(manifest_prefix)).take(DB_ROOT_PROBE_LIMIT);
    while let Ok(Some(meta)) = stream.try_next().await {
        let is_manifest = meta
            .location
            .filename()
            .and_then(|name| name.strip_suffix(".manifest"))
            .is_some_and(|stem| stem.parse::<u64>().is_ok());
        if is_manifest {
            return true;
        }
    }
    false
}

/// DB root paths under `root`, breadth-first, sorted.
pub async fn discover(
    store: &dyn ObjectStore,
    root: &str,
    limits: &ScanLimits,
) -> anyhow::Result<Vec<String>> {
    let mut found = Vec::new();
    let mut queue: VecDeque<(Path, usize)> = VecDeque::new();
    queue.push_back((Path::from(root), 0));
    let mut visited = 0usize;

    while let Some((prefix, depth)) = queue.pop_front() {
        visited += 1;
        if visited > limits.max_prefixes {
            tracing::warn!(
                root,
                budget = limits.max_prefixes,
                "discovery stopped early: prefix budget exhausted"
            );
            break;
        }

        // An empty path means "the store root"; object_store wants None.
        let listing = if prefix.as_ref().is_empty() {
            store.list_with_delimiter(None).await?
        } else {
            store.list_with_delimiter(Some(&prefix)).await?
        };

        let manifest_child = listing
            .common_prefixes
            .iter()
            .find(|p| p.filename() == Some("manifest"));
        if let Some(manifest_prefix) = manifest_child {
            if is_db_root(store, manifest_prefix).await {
                found.push(prefix.as_ref().to_string());
                continue; // never descend into a DB
            }
        }

        if depth < limits.max_depth {
            for child in listing.common_prefixes {
                queue.push_back((child, depth + 1));
            }
        }
    }

    found.sort();
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slatedb::object_store::memory::InMemory;

    async fn store_with(paths: &[&str]) -> InMemory {
        let store = InMemory::new();
        for p in paths {
            store
                .put(&Path::from(*p), slatedb::bytes::Bytes::from_static(b"x").into())
                .await
                .unwrap();
        }
        store
    }

    fn limits(depth: usize, prefixes: usize) -> ScanLimits {
        ScanLimits {
            max_depth: depth,
            max_prefixes: prefixes,
        }
    }

    #[tokio::test]
    async fn finds_dbs_at_mixed_depths() {
        let store = store_with(&[
            "demo-db/manifest/00000000000000000001.manifest",
            "demo-db/compacted/01HX.sst",
            "teams/a/db1/manifest/00000000000000000007.manifest",
            "teams/a/notes/readme.txt",
            "logs/2026/06/10/app.log",
        ])
        .await;
        let dbs = discover(&store, "", &limits(4, 2000)).await.unwrap();
        assert_eq!(dbs, vec!["demo-db", "teams/a/db1"]);
    }

    #[tokio::test]
    async fn scoped_root_only_sees_its_subtree() {
        let store = store_with(&[
            "demo-db/manifest/00000000000000000001.manifest",
            "teams/a/db1/manifest/00000000000000000007.manifest",
        ])
        .await;
        let dbs = discover(&store, "teams", &limits(4, 2000)).await.unwrap();
        assert_eq!(dbs, vec!["teams/a/db1"]);
    }

    #[tokio::test]
    async fn manifest_dir_without_numbered_manifests_is_not_a_db() {
        let store = store_with(&["app/manifest/readme.txt", "app/data/file.bin"]).await;
        let dbs = discover(&store, "", &limits(4, 2000)).await.unwrap();
        assert!(dbs.is_empty());
    }

    #[tokio::test]
    async fn stray_files_in_manifest_dir_do_not_hide_a_db() {
        // `.DS_Store` sorts before the digits, so a first-object-only probe
        // would misread this as "not a DB".
        let store = store_with(&[
            "db/manifest/.DS_Store",
            "db/manifest/00000000000000000001.manifest",
        ])
        .await;
        let dbs = discover(&store, "", &limits(4, 2000)).await.unwrap();
        assert_eq!(dbs, vec!["db"]);
    }

    #[tokio::test]
    async fn respects_depth_limit() {
        let store = store_with(&[
            "a/b/c/d/e/db/manifest/00000000000000000001.manifest",
            "shallow/manifest/00000000000000000001.manifest",
        ])
        .await;
        // Depth 4 can't reach a/b/c/d/e/db (depth 6).
        let dbs = discover(&store, "", &limits(4, 2000)).await.unwrap();
        assert_eq!(dbs, vec!["shallow"]);
        let dbs = discover(&store, "", &limits(8, 2000)).await.unwrap();
        assert_eq!(dbs, vec!["a/b/c/d/e/db", "shallow"]);
    }

    #[tokio::test]
    async fn respects_prefix_budget() {
        let mut paths = Vec::new();
        for i in 0..50 {
            paths.push(format!("dir{i:02}/sub/file.txt"));
        }
        paths.push("zz-db/manifest/00000000000000000001.manifest".to_string());
        let store = InMemory::new();
        for p in &paths {
            store
                .put(
                    &Path::from(p.as_str()),
                    slatedb::bytes::Bytes::from_static(b"x").into(),
                )
                .await
                .unwrap();
        }
        // Budget of 5 stops the walk long before zz-db is reached.
        let dbs = discover(&store, "", &limits(4, 5)).await.unwrap();
        assert!(dbs.is_empty());
    }

    #[tokio::test]
    async fn does_not_descend_into_detected_dbs() {
        // A DB containing an object that *looks* like a nested DB path must
        // not be reported twice — detection stops descent.
        let store = store_with(&[
            "db/manifest/00000000000000000001.manifest",
            "db/compacted/manifest/00000000000000000001.manifest",
        ])
        .await;
        let dbs = discover(&store, "", &limits(6, 2000)).await.unwrap();
        assert_eq!(dbs, vec!["db"]);
    }
}
