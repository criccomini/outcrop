use std::collections::{BTreeMap, BTreeSet};

use crate::dto::*;

/// Diff two manifests (as DTOs), keyed by stable identifiers: L0 view ULIDs,
/// sorted-run ids, checkpoint UUIDs, external-db source checkpoint ids.
pub fn diff_manifests(a: &ManifestDto, b: &ManifestDto) -> ManifestDiffDto {
    let (l0_added, l0_removed) = diff_ssts(&a.tree.l0, &b.tree.l0);

    let a_runs: BTreeMap<u32, &SortedRunDto> = a.tree.runs.iter().map(|r| (r.id, r)).collect();
    let b_runs: BTreeMap<u32, &SortedRunDto> = b.tree.runs.iter().map(|r| (r.id, r)).collect();
    let mut runs_added = vec![];
    let mut runs_removed = vec![];
    let mut runs_changed = vec![];
    for (id, run) in &b_runs {
        match a_runs.get(id) {
            None => runs_added.push(run_summary(run)),
            Some(a_run) => {
                let (ssts_added, ssts_removed) = diff_ssts(&a_run.ssts, &run.ssts);
                if !ssts_added.is_empty() || !ssts_removed.is_empty() {
                    runs_changed.push(RunChangeDto {
                        id: *id,
                        ssts_added,
                        ssts_removed,
                    });
                }
            }
        }
    }
    for (id, run) in &a_runs {
        if !b_runs.contains_key(id) {
            runs_removed.push(run_summary(run));
        }
    }

    let a_segs: BTreeSet<&str> = a.segments.iter().map(|s| s.prefix.hex.as_str()).collect();
    let b_segs: BTreeSet<&str> = b.segments.iter().map(|s| s.prefix.hex.as_str()).collect();
    let segments_added = b
        .segments
        .iter()
        .filter(|s| !a_segs.contains(s.prefix.hex.as_str()))
        .map(|s| s.prefix.clone())
        .collect();
    let segments_removed = a
        .segments
        .iter()
        .filter(|s| !b_segs.contains(s.prefix.hex.as_str()))
        .map(|s| s.prefix.clone())
        .collect();

    let a_cps: BTreeMap<&str, &CheckpointDto> =
        a.checkpoints.iter().map(|c| (c.id.as_str(), c)).collect();
    let b_cps: BTreeMap<&str, &CheckpointDto> =
        b.checkpoints.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut checkpoints_added = vec![];
    let mut checkpoints_removed = vec![];
    let mut checkpoints_changed = vec![];
    for (id, cp) in &b_cps {
        match a_cps.get(id) {
            None => checkpoints_added.push((*cp).clone()),
            Some(a_cp) => {
                if a_cp.manifest_id != cp.manifest_id || a_cp.expire_time != cp.expire_time {
                    checkpoints_changed.push(CheckpointChangeDto {
                        id: cp.id.clone(),
                        manifest_id: (a_cp.manifest_id, cp.manifest_id),
                        expire_time: (a_cp.expire_time, cp.expire_time),
                    });
                }
            }
        }
    }
    for (id, cp) in &a_cps {
        if !b_cps.contains_key(id) {
            checkpoints_removed.push((*cp).clone());
        }
    }

    let a_ext: BTreeMap<&str, &ExternalDbDto> = a
        .external_dbs
        .iter()
        .map(|e| (e.source_checkpoint_id.as_str(), e))
        .collect();
    let b_ext: BTreeMap<&str, &ExternalDbDto> = b
        .external_dbs
        .iter()
        .map(|e| (e.source_checkpoint_id.as_str(), e))
        .collect();
    let external_dbs_added = b_ext
        .iter()
        .filter(|(k, _)| !a_ext.contains_key(*k))
        .map(|(_, e)| (*e).clone())
        .collect();
    let external_dbs_removed = a_ext
        .iter()
        .filter(|(k, _)| !b_ext.contains_key(*k))
        .map(|(_, e)| (*e).clone())
        .collect();

    let mut scalars = vec![];
    let mut scalar = |field: &str, va: String, vb: String| {
        if va != vb {
            scalars.push(ScalarChangeDto {
                field: field.to_string(),
                a: va,
                b: vb,
            });
        }
    };
    scalar("initialized", a.initialized.to_string(), b.initialized.to_string());
    scalar("writer_epoch", a.writer_epoch.to_string(), b.writer_epoch.to_string());
    scalar(
        "compactor_epoch",
        a.compactor_epoch.to_string(),
        b.compactor_epoch.to_string(),
    );
    scalar(
        "next_wal_sst_id",
        a.next_wal_sst_id.to_string(),
        b.next_wal_sst_id.to_string(),
    );
    scalar(
        "replay_after_wal_id",
        a.replay_after_wal_id.to_string(),
        b.replay_after_wal_id.to_string(),
    );
    scalar("last_l0_seq", a.last_l0_seq.to_string(), b.last_l0_seq.to_string());
    scalar(
        "last_l0_clock_tick",
        a.last_l0_clock_tick.to_string(),
        b.last_l0_clock_tick.to_string(),
    );
    scalar(
        "recent_snapshot_min_seq",
        a.recent_snapshot_min_seq.to_string(),
        b.recent_snapshot_min_seq.to_string(),
    );
    scalar(
        "last_compacted_l0_sst_view_id",
        format_opt(&a.last_compacted_l0_sst_view_id),
        format_opt(&b.last_compacted_l0_sst_view_id),
    );

    ManifestDiffDto {
        a: a.id,
        b: b.id,
        l0_added,
        l0_removed,
        runs_added,
        runs_removed,
        runs_changed,
        segments_added,
        segments_removed,
        checkpoints_added,
        checkpoints_removed,
        checkpoints_changed,
        external_dbs_added,
        external_dbs_removed,
        scalars,
    }
}

fn format_opt(v: &Option<String>) -> String {
    v.clone().unwrap_or_else(|| "none".to_string())
}

fn run_summary(run: &SortedRunDto) -> SortedRunSummaryDto {
    SortedRunSummaryDto {
        id: run.id,
        est_bytes: run.est_bytes,
        sst_count: run.ssts.len(),
    }
}

/// Added/removed SSTs between two lists, keyed by view id.
fn diff_ssts(a: &[SstViewDto], b: &[SstViewDto]) -> (Vec<SstViewDto>, Vec<SstViewDto>) {
    let a_ids: BTreeSet<&str> = a.iter().map(|s| s.view_id.as_str()).collect();
    let b_ids: BTreeSet<&str> = b.iter().map(|s| s.view_id.as_str()).collect();
    let added = b
        .iter()
        .filter(|s| !a_ids.contains(s.view_id.as_str()))
        .cloned()
        .collect();
    let removed = a
        .iter()
        .filter(|s| !b_ids.contains(s.view_id.as_str()))
        .cloned()
        .collect();
    (added, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sst(view_id: &str, bytes: u64) -> SstViewDto {
        SstViewDto {
            view_id: view_id.to_string(),
            sst_id: SstIdDto::Compacted {
                ulid: view_id.to_string(),
            },
            first_key: None,
            last_key: None,
            est_bytes: bytes,
            compression: None,
            visible_range: None,
        }
    }

    fn run(id: u32, ssts: Vec<SstViewDto>) -> SortedRunDto {
        SortedRunDto {
            id,
            est_bytes: ssts.iter().map(|s| s.est_bytes).sum(),
            ssts,
        }
    }

    fn manifest(id: u64, l0: Vec<SstViewDto>, runs: Vec<SortedRunDto>) -> ManifestDto {
        let l0_bytes = l0.iter().map(|s| s.est_bytes).sum::<u64>();
        let total_bytes = l0_bytes + runs.iter().map(|r| r.est_bytes).sum::<u64>();
        ManifestDto {
            id,
            initialized: true,
            writer_epoch: 1,
            compactor_epoch: 1,
            next_wal_sst_id: 10,
            replay_after_wal_id: 5,
            last_l0_seq: 100,
            last_l0_clock_tick: 0,
            recent_snapshot_min_seq: 0,
            last_compacted_l0_sst_view_id: None,
            wal_object_store_uri: None,
            segment_extractor_name: None,
            tree: TreeDto {
                l0,
                runs,
                l0_bytes,
                total_bytes,
            },
            segments: vec![],
            checkpoints: vec![],
            external_dbs: vec![],
        }
    }

    fn checkpoint(id: &str, manifest_id: u64) -> CheckpointDto {
        CheckpointDto {
            id: id.to_string(),
            name: None,
            manifest_id,
            create_time: chrono::DateTime::from_timestamp(0, 0).unwrap(),
            expire_time: None,
        }
    }

    #[test]
    fn identical_manifests_have_empty_diff() {
        let m = manifest(1, vec![sst("a", 10)], vec![run(1, vec![sst("b", 20)])]);
        let mut m2 = m.clone();
        m2.id = 2;
        let d = diff_manifests(&m, &m2);
        assert!(d.l0_added.is_empty());
        assert!(d.l0_removed.is_empty());
        assert!(d.runs_added.is_empty());
        assert!(d.runs_removed.is_empty());
        assert!(d.runs_changed.is_empty());
        assert!(d.scalars.is_empty());
    }

    #[test]
    fn compaction_shows_l0_removed_and_run_added() {
        let a = manifest(1, vec![sst("a", 10), sst("b", 10)], vec![]);
        let b = manifest(2, vec![], vec![run(1, vec![sst("c", 20)])]);
        let d = diff_manifests(&a, &b);
        assert_eq!(d.l0_removed.len(), 2);
        assert!(d.l0_added.is_empty());
        assert_eq!(d.runs_added.len(), 1);
        assert_eq!(d.runs_added[0].id, 1);
        assert_eq!(d.runs_added[0].sst_count, 1);
    }

    #[test]
    fn run_membership_change_is_reported() {
        let a = manifest(1, vec![], vec![run(1, vec![sst("a", 10), sst("b", 10)])]);
        let b = manifest(2, vec![], vec![run(1, vec![sst("a", 10), sst("c", 30)])]);
        let d = diff_manifests(&a, &b);
        assert_eq!(d.runs_changed.len(), 1);
        assert_eq!(d.runs_changed[0].id, 1);
        assert_eq!(d.runs_changed[0].ssts_added[0].view_id, "c");
        assert_eq!(d.runs_changed[0].ssts_removed[0].view_id, "b");
    }

    #[test]
    fn checkpoint_add_remove_change() {
        let mut a = manifest(1, vec![], vec![]);
        let mut b = manifest(2, vec![], vec![]);
        a.checkpoints = vec![checkpoint("gone", 1), checkpoint("moved", 1)];
        b.checkpoints = vec![checkpoint("moved", 2), checkpoint("new", 2)];
        let d = diff_manifests(&a, &b);
        assert_eq!(d.checkpoints_added.len(), 1);
        assert_eq!(d.checkpoints_added[0].id, "new");
        assert_eq!(d.checkpoints_removed.len(), 1);
        assert_eq!(d.checkpoints_removed[0].id, "gone");
        assert_eq!(d.checkpoints_changed.len(), 1);
        assert_eq!(d.checkpoints_changed[0].manifest_id, (1, 2));
    }

    #[test]
    fn scalar_changes_are_reported() {
        let a = manifest(1, vec![], vec![]);
        let mut b = manifest(2, vec![], vec![]);
        b.next_wal_sst_id = 42;
        b.writer_epoch = 2;
        let d = diff_manifests(&a, &b);
        let fields: Vec<&str> = d.scalars.iter().map(|s| s.field.as_str()).collect();
        assert!(fields.contains(&"next_wal_sst_id"));
        assert!(fields.contains(&"writer_epoch"));
        assert_eq!(fields.len(), 2);
    }
}
