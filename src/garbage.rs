//! Garbage / space-amplification report: classifies every stored object as
//! live (referenced by the latest manifest), pinned (referenced only via an
//! unexpired checkpoint), or reclaimable (what the GC would eventually
//! delete). Mirrors slatedb's GC rules: expired checkpoints are treated as
//! already removed, manifests survive while the latest or a live checkpoint
//! references them, WAL SSTs survive while any live manifest still needs
//! them for replay, and compacted SSTs survive while any live manifest's
//! tree references them. The GC's min-age grace periods are ignored, so
//! this is the steady-state estimate.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use ulid::Ulid;

use crate::dto::{GarbageCategoryDto, GarbageDto, GarbagePinnerDto};
use crate::state::{CompactedEntry, ManifestEntry, WalEntry};

/// The references one manifest version holds: compacted-SST ULIDs across the
/// root tree and all segments, plus the WAL window it needs for replay.
pub struct ManifestRefs {
    pub id: u64,
    pub compacted: HashSet<Ulid>,
    pub replay_after_wal_id: u64,
    pub next_wal_sst_id: u64,
}

impl ManifestRefs {
    fn needs_wal(&self, wal_id: u64) -> bool {
        wal_id > self.replay_after_wal_id && wal_id < self.next_wal_sst_id
    }
}

/// An unexpired checkpoint, as input to per-checkpoint pinning attribution.
pub struct CheckpointPin {
    pub id: String,
    pub name: Option<String>,
    pub manifest_id: u64,
    pub expire_time: Option<DateTime<Utc>>,
}

pub struct GarbageInputs<'a> {
    pub latest: ManifestRefs,
    /// Manifests referenced by unexpired checkpoints (deduped, latest's own
    /// id excluded).
    pub pinned: Vec<ManifestRefs>,
    /// All unexpired checkpoints.
    pub pinned_checkpoints: Vec<CheckpointPin>,
    pub live_checkpoint_count: usize,
    pub expired_checkpoint_count: usize,
    /// Unexpired checkpoints whose manifest no longer exists.
    pub dangling_checkpoint_count: usize,
    pub compacted_listing: &'a [CompactedEntry],
    pub wal_listing: &'a [WalEntry],
    pub manifest_listing: &'a [ManifestEntry],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Live,
    Pinned,
    Reclaimable,
}

#[derive(Default)]
struct CategoryAcc {
    dto: GarbageCategoryDto,
}

impl CategoryAcc {
    fn add(&mut self, class: Class, bytes: u64) {
        self.dto.stored_count += 1;
        self.dto.stored_bytes += bytes;
        match class {
            Class::Live => {
                self.dto.live_count += 1;
                self.dto.live_bytes += bytes;
            }
            Class::Pinned => {
                self.dto.pinned_count += 1;
                self.dto.pinned_bytes += bytes;
            }
            Class::Reclaimable => {
                self.dto.reclaimable_count += 1;
                self.dto.reclaimable_bytes += bytes;
            }
        }
    }
}

pub fn compute_garbage(inputs: &GarbageInputs) -> GarbageDto {
    let mut compacted = CategoryAcc::default();
    let mut wal = CategoryAcc::default();
    let mut manifests = CategoryAcc::default();
    let mut oldest_reclaimable_at: Option<DateTime<Utc>> = None;

    let note_reclaimable = |oldest: &mut Option<DateTime<Utc>>, at: DateTime<Utc>| {
        if oldest.is_none_or(|o| at < o) {
            *oldest = Some(at);
        }
    };

    for entry in inputs.compacted_listing {
        let class = if inputs.latest.compacted.contains(&entry.ulid) {
            Class::Live
        } else if inputs.pinned.iter().any(|m| m.compacted.contains(&entry.ulid)) {
            Class::Pinned
        } else {
            note_reclaimable(&mut oldest_reclaimable_at, entry.last_modified);
            Class::Reclaimable
        };
        compacted.add(class, entry.size_bytes);
    }

    for entry in inputs.wal_listing {
        // Anything at or past the latest manifest's window start is live:
        // the replay window itself, plus SSTs the writer has appended since
        // the manifest was last written.
        let class = if entry.id > inputs.latest.replay_after_wal_id {
            Class::Live
        } else if inputs.pinned.iter().any(|m| m.needs_wal(entry.id)) {
            Class::Pinned
        } else {
            note_reclaimable(&mut oldest_reclaimable_at, entry.last_modified);
            Class::Reclaimable
        };
        wal.add(class, entry.size_bytes);
    }

    let pinned_manifest_ids: HashSet<u64> = inputs.pinned.iter().map(|m| m.id).collect();
    for entry in inputs.manifest_listing {
        // Ids newer than the cached latest can appear when the writer is
        // racing the listing; never call them garbage.
        let class = if entry.id >= inputs.latest.id {
            Class::Live
        } else if pinned_manifest_ids.contains(&entry.id) {
            Class::Pinned
        } else {
            note_reclaimable(&mut oldest_reclaimable_at, entry.last_modified);
            Class::Reclaimable
        };
        manifests.add(class, entry.size_bytes);
    }

    // Per-checkpoint attribution: data bytes a checkpoint's manifest
    // references beyond what the latest manifest already keeps alive.
    // Checkpoints sharing a manifest report the same number — this is a
    // per-checkpoint view, not a disjoint partition of pinned bytes.
    let compacted_sizes: HashMap<Ulid, u64> = inputs
        .compacted_listing
        .iter()
        .map(|e| (e.ulid, e.size_bytes))
        .collect();
    let refs_by_id: HashMap<u64, &ManifestRefs> =
        inputs.pinned.iter().map(|m| (m.id, m)).collect();
    let mut pinners: Vec<GarbagePinnerDto> = inputs
        .pinned_checkpoints
        .iter()
        .map(|cp| {
            let refs = refs_by_id.get(&cp.manifest_id);
            let (mut extra_bytes, mut extra_count) = (0u64, 0usize);
            if let Some(refs) = refs {
                for u in refs.compacted.difference(&inputs.latest.compacted) {
                    if let Some(sz) = compacted_sizes.get(u) {
                        extra_bytes += sz;
                        extra_count += 1;
                    }
                }
                for w in inputs.wal_listing {
                    if refs.needs_wal(w.id) && w.id <= inputs.latest.replay_after_wal_id {
                        extra_bytes += w.size_bytes;
                        extra_count += 1;
                    }
                }
            }
            GarbagePinnerDto {
                id: cp.id.clone(),
                name: cp.name.clone(),
                manifest_id: cp.manifest_id,
                expire_time: cp.expire_time,
                manifest_available: refs.is_some() || cp.manifest_id >= inputs.latest.id,
                extra_bytes,
                extra_count,
            }
        })
        .collect();
    pinners.sort_by(|a, b| b.extra_bytes.cmp(&a.extra_bytes));

    let (compacted, wal, manifests) = (compacted.dto, wal.dto, manifests.dto);
    let stored_bytes = compacted.stored_bytes + wal.stored_bytes + manifests.stored_bytes;
    let live_bytes = compacted.live_bytes + wal.live_bytes + manifests.live_bytes;
    let pinned_bytes = compacted.pinned_bytes + wal.pinned_bytes + manifests.pinned_bytes;
    let reclaimable_bytes =
        compacted.reclaimable_bytes + wal.reclaimable_bytes + manifests.reclaimable_bytes;

    // Space amp over data objects only (manifests are metadata noise).
    let data_stored = compacted.stored_bytes + wal.stored_bytes;
    let data_live = compacted.live_bytes + wal.live_bytes;
    let space_amp = (data_live > 0).then(|| data_stored as f64 / data_live as f64);

    GarbageDto {
        manifest_id: inputs.latest.id,
        live_checkpoint_count: inputs.live_checkpoint_count,
        expired_checkpoint_count: inputs.expired_checkpoint_count,
        dangling_checkpoint_count: inputs.dangling_checkpoint_count,
        pinners,
        compacted,
        wal,
        manifests,
        stored_bytes,
        live_bytes,
        pinned_bytes,
        reclaimable_bytes,
        space_amp,
        oldest_reclaimable_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn ulid(n: u128) -> Ulid {
        Ulid::from(n)
    }

    fn refs(id: u64, compacted: &[Ulid], replay_after: u64, next: u64) -> ManifestRefs {
        ManifestRefs {
            id,
            compacted: compacted.iter().copied().collect(),
            replay_after_wal_id: replay_after,
            next_wal_sst_id: next,
        }
    }

    fn compacted_entry(u: Ulid, size: u64, at: i64) -> CompactedEntry {
        CompactedEntry {
            ulid: u,
            size_bytes: size,
            last_modified: ts(at),
        }
    }

    fn wal_entry(id: u64, size: u64, at: i64) -> WalEntry {
        WalEntry {
            id,
            size_bytes: size,
            last_modified: ts(at),
        }
    }

    fn manifest_entry(id: u64, size: u64, at: i64) -> ManifestEntry {
        ManifestEntry {
            id,
            size_bytes: size,
            last_modified: ts(at),
        }
    }

    #[test]
    fn classifies_compacted_ssts() {
        let live = ulid(1);
        let pinned = ulid(2);
        let orphan = ulid(3);
        let inputs = GarbageInputs {
            latest: refs(10, &[live], 5, 8),
            pinned: vec![refs(7, &[live, pinned], 3, 6)],
            pinned_checkpoints: vec![],
            live_checkpoint_count: 1,
            expired_checkpoint_count: 0,
            dangling_checkpoint_count: 0,
            compacted_listing: &[
                compacted_entry(live, 100, 1000),
                compacted_entry(pinned, 40, 900),
                compacted_entry(orphan, 7, 800),
            ],
            wal_listing: &[],
            manifest_listing: &[],
        };
        let out = compute_garbage(&inputs);
        assert_eq!(out.compacted.stored_count, 3);
        assert_eq!(out.compacted.stored_bytes, 147);
        assert_eq!(out.compacted.live_bytes, 100);
        assert_eq!(out.compacted.pinned_bytes, 40);
        assert_eq!(out.compacted.reclaimable_bytes, 7);
        assert_eq!(out.oldest_reclaimable_at, Some(ts(800)));
    }

    #[test]
    fn wal_window_is_live_and_checkpointed_window_is_pinned() {
        let inputs = GarbageInputs {
            // Latest replays after #6: WAL 7+ live.
            latest: refs(10, &[], 6, 9),
            // Checkpointed manifest still needs WAL (4, 6).
            pinned: vec![refs(8, &[], 4, 7)],
            pinned_checkpoints: vec![],
            live_checkpoint_count: 1,
            expired_checkpoint_count: 0,
            dangling_checkpoint_count: 0,
            compacted_listing: &[],
            wal_listing: &[
                wal_entry(3, 10, 100), // reclaimable
                wal_entry(5, 10, 200), // pinned by checkpoint
                wal_entry(7, 10, 300), // live window
                wal_entry(9, 10, 400), // newer than manifest: live
            ],
            manifest_listing: &[],
        };
        let out = compute_garbage(&inputs);
        assert_eq!(out.wal.live_bytes, 20);
        assert_eq!(out.wal.pinned_bytes, 10);
        assert_eq!(out.wal.reclaimable_bytes, 10);
        assert_eq!(out.oldest_reclaimable_at, Some(ts(100)));
    }

    #[test]
    fn manifests_keep_latest_and_checkpoint_targets() {
        let inputs = GarbageInputs {
            latest: refs(5, &[], 0, 1),
            pinned: vec![refs(3, &[], 0, 1)],
            pinned_checkpoints: vec![],
            live_checkpoint_count: 1,
            expired_checkpoint_count: 2,
            dangling_checkpoint_count: 0,
            compacted_listing: &[],
            wal_listing: &[],
            manifest_listing: &[
                manifest_entry(1, 5, 100),
                manifest_entry(3, 5, 200),
                manifest_entry(5, 5, 300),
                manifest_entry(6, 5, 400), // listing raced a newer write
            ],
        };
        let out = compute_garbage(&inputs);
        assert_eq!(out.manifests.live_count, 2); // #5 and the newer #6
        assert_eq!(out.manifests.pinned_count, 1);
        assert_eq!(out.manifests.reclaimable_count, 1);
        assert_eq!(out.expired_checkpoint_count, 2);
    }

    #[test]
    fn space_amp_ignores_manifest_bytes() {
        let live = ulid(1);
        let orphan = ulid(2);
        let inputs = GarbageInputs {
            latest: refs(2, &[live], 0, 1),
            pinned: vec![],
            pinned_checkpoints: vec![],
            live_checkpoint_count: 0,
            expired_checkpoint_count: 0,
            dangling_checkpoint_count: 0,
            compacted_listing: &[
                compacted_entry(live, 100, 100),
                compacted_entry(orphan, 100, 100),
            ],
            wal_listing: &[],
            manifest_listing: &[manifest_entry(2, 1_000_000, 100)],
        };
        let out = compute_garbage(&inputs);
        assert_eq!(out.space_amp, Some(2.0));
    }

    #[test]
    fn pinner_attribution_counts_extra_data_only() {
        let shared = ulid(1);
        let pinned_only = ulid(2);
        let cp = |id: &str, manifest_id: u64| CheckpointPin {
            id: id.to_string(),
            name: None,
            manifest_id,
            expire_time: None,
        };
        let inputs = GarbageInputs {
            // Latest references `shared` and replays after WAL #6.
            latest: refs(10, &[shared], 6, 9),
            // Checkpointed manifest also holds `pinned_only` and WAL (4, 7).
            pinned: vec![refs(7, &[shared, pinned_only], 4, 7)],
            pinned_checkpoints: vec![cp("old", 7), cp("at-latest", 10), cp("gone", 3)],
            live_checkpoint_count: 3,
            expired_checkpoint_count: 0,
            dangling_checkpoint_count: 1,
            compacted_listing: &[
                compacted_entry(shared, 100, 100),
                compacted_entry(pinned_only, 40, 100),
            ],
            wal_listing: &[
                wal_entry(5, 10, 100), // in checkpoint window, replayed in latest
                wal_entry(7, 10, 100), // live in latest — not extra
            ],
            manifest_listing: &[],
        };
        let out = compute_garbage(&inputs);
        assert_eq!(out.pinners.len(), 3);
        // Sorted heaviest first: the old checkpoint pins SST #2 + WAL #5.
        assert_eq!(out.pinners[0].id, "old");
        assert_eq!(out.pinners[0].extra_bytes, 50);
        assert_eq!(out.pinners[0].extra_count, 2);
        assert!(out.pinners[0].manifest_available);
        let at_latest = out.pinners.iter().find(|p| p.id == "at-latest").unwrap();
        assert_eq!(at_latest.extra_bytes, 0);
        assert!(at_latest.manifest_available);
        let gone = out.pinners.iter().find(|p| p.id == "gone").unwrap();
        assert_eq!(gone.extra_bytes, 0);
        assert!(!gone.manifest_available);
    }

    #[test]
    fn space_amp_absent_when_nothing_live() {
        let inputs = GarbageInputs {
            latest: refs(1, &[], 0, 1),
            pinned: vec![],
            pinned_checkpoints: vec![],
            live_checkpoint_count: 0,
            expired_checkpoint_count: 0,
            dangling_checkpoint_count: 0,
            compacted_listing: &[],
            wal_listing: &[],
            manifest_listing: &[],
        };
        let out = compute_garbage(&inputs);
        assert_eq!(out.space_amp, None);
        assert_eq!(out.oldest_reclaimable_at, None);
    }
}
