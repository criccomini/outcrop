//! Garbage / space-amplification report: classifies every stored object as
//! live (referenced by the latest manifest), pinned (referenced only via an
//! unexpired checkpoint), or reclaimable (what the GC would eventually
//! delete). Mirrors slatedb's GC rules: expired checkpoints are treated as
//! already removed, manifests survive while the latest or a live checkpoint
//! references them, WAL SSTs survive while any live manifest still needs
//! them for replay, and compacted SSTs survive while any live manifest's
//! tree references them. The GC's min-age grace periods are ignored, so
//! this is the steady-state estimate.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use ulid::Ulid;

use crate::dto::{GarbageCategoryDto, GarbageDto};
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

pub struct GarbageInputs<'a> {
    pub latest: ManifestRefs,
    /// Manifests referenced by unexpired checkpoints (deduped, latest's own
    /// id excluded).
    pub pinned: Vec<ManifestRefs>,
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
    fn space_amp_absent_when_nothing_live() {
        let inputs = GarbageInputs {
            latest: refs(1, &[], 0, 1),
            pinned: vec![],
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
