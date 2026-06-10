use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};

use crate::dto::{ManifestDto, TreeDto, WarningDto};

/// slatedb's default `l0_max_ssts`; the writer's actual setting isn't
/// recorded in the manifest, so warn at the default.
const L0_WARN_COUNT: usize = 8;
/// How stale the latest manifest may be before an informational notice.
const STALE_MANIFEST_AFTER_MINS: i64 = 10;
/// Un-replayed WAL SSTs before warning about replay backlog.
const WAL_WINDOW_WARN: u64 = 100;

pub struct WarningInputs<'a> {
    pub manifest: &'a ManifestDto,
    /// Manifest ids that still exist in the object store.
    pub live_manifest_ids: &'a HashSet<u64>,
    pub latest_manifest_written_at: Option<DateTime<Utc>>,
    /// Injected so rules are testable with a fixed clock.
    pub now: DateTime<Utc>,
}

pub fn compute_warnings(i: &WarningInputs) -> Vec<WarningDto> {
    let mut out = Vec::new();
    let m = i.manifest;

    let expired = m
        .checkpoints
        .iter()
        .filter(|c| c.expire_time.is_some_and(|t| t < i.now))
        .count();
    if expired > 0 {
        out.push(WarningDto {
            code: "expired_checkpoint",
            severity: "warn",
            message: format!(
                "{expired} expired checkpoint{} still in the manifest — is the garbage collector running?",
                if expired > 1 { "s" } else { "" }
            ),
        });
    }

    let gced: Vec<u64> = m
        .checkpoints
        .iter()
        .filter(|c| !i.live_manifest_ids.contains(&c.manifest_id))
        .map(|c| c.manifest_id)
        .collect();
    if !gced.is_empty() {
        out.push(WarningDto {
            code: "checkpoint_manifest_gced",
            severity: "error",
            message: format!(
                "{} checkpoint{} reference manifest versions that no longer exist (e.g. #{})",
                gced.len(),
                if gced.len() > 1 { "s" } else { "" },
                gced[0]
            ),
        });
    }

    // l0_max_ssts applies per tree: check the root tree and each segment
    // tree individually rather than summed totals.
    let trees: Vec<(Option<&str>, &TreeDto)> = std::iter::once((None, &m.tree))
        .chain(
            m.segments
                .iter()
                .map(|s| (Some(s.prefix.hex.as_str()), &s.tree)),
        )
        .collect();
    for (segment, tree) in trees {
        if tree.l0.len() >= L0_WARN_COUNT {
            let location = match segment {
                Some(prefix) => format!("segment 0x{prefix}"),
                None => "L0".to_string(),
            };
            out.push(WarningDto {
                code: "l0_backlog",
                severity: "warn",
                message: format!(
                    "{} L0 SSTs in {location} (slatedb's default flush cap is {L0_WARN_COUNT}) — is the compactor keeping up?",
                    tree.l0.len()
                ),
            });
        }
    }

    if let Some(at) = i.latest_manifest_written_at {
        if i.now - at > Duration::minutes(STALE_MANIFEST_AFTER_MINS) {
            out.push(WarningDto {
                code: "stale_manifest",
                severity: "info",
                message: format!(
                    "no manifest update in over {STALE_MANIFEST_AFTER_MINS} minutes — the DB may simply be idle"
                ),
            });
        }
    }

    let wal_window = m
        .next_wal_sst_id
        .saturating_sub(1)
        .saturating_sub(m.replay_after_wal_id);
    if wal_window > WAL_WINDOW_WARN {
        out.push(WarningDto {
            code: "wal_window_large",
            severity: "warn",
            message: format!(
                "{wal_window} un-replayed WAL SSTs — restart replay will be slow and L0 flushes may be falling behind"
            ),
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{CheckpointDto, KeyDto, SegmentDto, SstIdDto, SstViewDto};

    fn manifest() -> ManifestDto {
        ManifestDto {
            id: 10,
            initialized: true,
            writer_epoch: 1,
            compactor_epoch: 1,
            next_wal_sst_id: 6,
            replay_after_wal_id: 5,
            last_l0_seq: 100,
            last_l0_clock_tick: 0,
            recent_snapshot_min_seq: 0,
            last_compacted_l0_sst_view_id: None,
            wal_object_store_uri: None,
            segment_extractor_name: None,
            tree: TreeDto {
                l0: vec![],
                runs: vec![],
                l0_bytes: 0,
                total_bytes: 0,
            },
            segments: vec![],
            checkpoints: vec![],
            external_dbs: vec![],
        }
    }

    fn sst(view_id: &str) -> SstViewDto {
        SstViewDto {
            view_id: view_id.to_string(),
            sst_id: SstIdDto::Compacted {
                ulid: view_id.to_string(),
            },
            first_key: None,
            last_key: None,
            est_bytes: 1,
            compression: None,
            visible_range: None,
        }
    }

    fn checkpoint(manifest_id: u64, expire_time: Option<DateTime<Utc>>) -> CheckpointDto {
        CheckpointDto {
            id: format!("cp-{manifest_id}"),
            name: None,
            manifest_id,
            create_time: DateTime::from_timestamp(0, 0).unwrap(),
            expire_time,
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_000_000, 0).unwrap()
    }

    fn codes(m: &ManifestDto, live: &[u64], written_at: Option<DateTime<Utc>>) -> Vec<&'static str> {
        let live: HashSet<u64> = live.iter().copied().collect();
        compute_warnings(&WarningInputs {
            manifest: m,
            live_manifest_ids: &live,
            latest_manifest_written_at: written_at,
            now: now(),
        })
        .iter()
        .map(|w| w.code)
        .collect()
    }

    #[test]
    fn healthy_manifest_has_no_warnings() {
        let m = manifest();
        assert!(codes(&m, &[10], Some(now())).is_empty());
    }

    #[test]
    fn expired_checkpoint_warns() {
        let mut m = manifest();
        m.checkpoints = vec![
            checkpoint(10, Some(now() - Duration::minutes(1))),
            checkpoint(10, Some(now() + Duration::minutes(1))),
            checkpoint(10, None),
        ];
        assert_eq!(codes(&m, &[10], Some(now())), vec!["expired_checkpoint"]);
    }

    #[test]
    fn checkpoint_referencing_gced_manifest_errors() {
        let mut m = manifest();
        m.checkpoints = vec![checkpoint(3, None)];
        assert_eq!(
            codes(&m, &[10], Some(now())),
            vec!["checkpoint_manifest_gced"]
        );
    }

    #[test]
    fn l0_backlog_warns_per_tree() {
        let mut m = manifest();
        m.tree.l0 = (0..8).map(|n| sst(&format!("r{n}"))).collect();
        m.segments = vec![
            SegmentDto {
                prefix: KeyDto {
                    hex: "aa".to_string(),
                    utf8: None,
                },
                tree: TreeDto {
                    l0: (0..9).map(|n| sst(&format!("s{n}"))).collect(),
                    runs: vec![],
                    l0_bytes: 0,
                    total_bytes: 0,
                },
            },
            SegmentDto {
                prefix: KeyDto {
                    hex: "bb".to_string(),
                    utf8: None,
                },
                tree: TreeDto {
                    l0: vec![sst("t0")],
                    runs: vec![],
                    l0_bytes: 0,
                    total_bytes: 0,
                },
            },
        ];
        assert_eq!(
            codes(&m, &[10], Some(now())),
            vec!["l0_backlog", "l0_backlog"]
        );
    }

    #[test]
    fn stale_manifest_is_informational() {
        let m = manifest();
        let written = now() - Duration::minutes(11);
        let live: HashSet<u64> = [10].into_iter().collect();
        let warnings = compute_warnings(&WarningInputs {
            manifest: &m,
            live_manifest_ids: &live,
            latest_manifest_written_at: Some(written),
            now: now(),
        });
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, "stale_manifest");
        assert_eq!(warnings[0].severity, "info");
    }

    #[test]
    fn large_wal_window_warns() {
        let mut m = manifest();
        m.next_wal_sst_id = 202;
        m.replay_after_wal_id = 100;
        assert_eq!(codes(&m, &[10], Some(now())), vec!["wal_window_large"]);
    }

    #[test]
    fn wal_window_never_underflows() {
        let mut m = manifest();
        m.next_wal_sst_id = 1;
        m.replay_after_wal_id = 0;
        assert!(codes(&m, &[10], Some(now())).is_empty());
    }
}
