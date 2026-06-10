use std::collections::Bound;
use std::collections::VecDeque;
use std::ops::RangeBounds;

use slatedb::bytes::Bytes;
use slatedb::manifest::{
    SortedRun, SsTableHandle, SsTableId, SsTableView, VersionedManifest,
};
use slatedb::Checkpoint;

use crate::dto::*;

pub fn key_dto(b: &[u8]) -> KeyDto {
    let utf8 = std::str::from_utf8(b)
        .ok()
        .filter(|s| !s.is_empty() && s.chars().all(|c| !c.is_control()))
        .map(|s| s.to_string());
    KeyDto {
        hex: hex::encode(b),
        utf8,
    }
}

fn bound_dto(b: Bound<&Bytes>) -> Option<BoundDto> {
    match b {
        Bound::Unbounded => None,
        Bound::Included(k) => Some(BoundDto {
            key: key_dto(k),
            inclusive: true,
        }),
        Bound::Excluded(k) => Some(BoundDto {
            key: key_dto(k),
            inclusive: false,
        }),
    }
}

fn range_dto<R: RangeBounds<Bytes>>(r: &R) -> RangeDto {
    RangeDto {
        start: bound_dto(r.start_bound()),
        end: bound_dto(r.end_bound()),
    }
}

pub fn sst_id_dto(id: &SsTableId) -> SstIdDto {
    match id {
        SsTableId::Wal(id) => SstIdDto::Wal { id: *id },
        SsTableId::Compacted(ulid) => SstIdDto::Compacted {
            ulid: ulid.to_string(),
        },
    }
}

pub fn sst_view_dto(v: &SsTableView) -> SstViewDto {
    let info = &v.sst.info;
    SstViewDto {
        view_id: v.id.to_string(),
        sst_id: sst_id_dto(&v.sst.id),
        first_key: info.first_entry.as_deref().map(key_dto),
        last_key: info.last_entry.as_deref().map(key_dto),
        est_bytes: v.estimate_size(),
        compression: info
            .compression_codec
            .as_ref()
            .map(|c| format!("{c:?}").to_lowercase()),
        visible_range: v.visible_range().map(|r| range_dto(&r)),
    }
}

pub fn sorted_run_dto(run: &SortedRun) -> SortedRunDto {
    SortedRunDto {
        id: run.id,
        est_bytes: run.estimate_size(),
        ssts: run.sst_views.iter().map(sst_view_dto).collect(),
    }
}

pub fn tree_dto(l0: &VecDeque<SsTableView>, compacted: &[SortedRun]) -> TreeDto {
    let l0: Vec<SstViewDto> = l0.iter().map(sst_view_dto).collect();
    let runs: Vec<SortedRunDto> = compacted.iter().map(sorted_run_dto).collect();
    let l0_bytes: u64 = l0.iter().map(|s| s.est_bytes).sum();
    let total_bytes = l0_bytes + runs.iter().map(|r| r.est_bytes).sum::<u64>();
    TreeDto {
        l0,
        runs,
        l0_bytes,
        total_bytes,
    }
}

pub fn checkpoint_dto(c: &Checkpoint) -> CheckpointDto {
    CheckpointDto {
        id: c.id.to_string(),
        name: c.name.clone(),
        manifest_id: c.manifest_id,
        create_time: c.create_time,
        expire_time: c.expire_time,
    }
}

pub fn manifest_dto(m: &VersionedManifest) -> ManifestDto {
    let segments: Vec<SegmentDto> = m
        .segments()
        .iter()
        .map(|seg| SegmentDto {
            prefix: key_dto(seg.prefix()),
            tree: tree_dto(seg.l0(), seg.compacted()),
        })
        .collect();
    let external_dbs: Vec<ExternalDbDto> = m
        .external_dbs()
        .iter()
        .map(|e| ExternalDbDto {
            path: e.path.clone(),
            source_checkpoint_id: e.source_checkpoint_id.to_string(),
            final_checkpoint_id: e.final_checkpoint_id.map(|id| id.to_string()),
            sst_count: e.sst_ids.len(),
            detached: e.final_checkpoint_id.is_none(),
        })
        .collect();
    ManifestDto {
        id: m.id(),
        initialized: m.initialized(),
        writer_epoch: m.writer_epoch(),
        compactor_epoch: m.compactor_epoch(),
        next_wal_sst_id: m.next_wal_sst_id(),
        replay_after_wal_id: m.replay_after_wal_id(),
        last_l0_seq: m.last_l0_seq(),
        last_l0_clock_tick: m.last_l0_clock_tick(),
        recent_snapshot_min_seq: m.recent_snapshot_min_seq(),
        last_compacted_l0_sst_view_id: m
            .last_compacted_l0_sst_view_id()
            .map(|u| u.to_string()),
        wal_object_store_uri: m.wal_object_store_uri().map(|s| s.to_string()),
        segment_extractor_name: m.segment_extractor_name().map(|s| s.to_string()),
        tree: tree_dto(m.l0(), m.compacted()),
        segments,
        checkpoints: m.checkpoints().iter().map(checkpoint_dto).collect(),
        external_dbs,
    }
}

/// (l0_count, run_count, sst_count, l0_bytes, total_bytes) summed over the
/// root tree and all segment trees.
pub fn manifest_totals(dto: &ManifestDto) -> (usize, usize, usize, u64, u64) {
    let trees = std::iter::once(&dto.tree).chain(dto.segments.iter().map(|s| &s.tree));
    let mut l0_count = 0;
    let mut run_count = 0;
    let mut sst_count = 0;
    let mut l0_bytes = 0u64;
    let mut total_bytes = 0u64;
    for t in trees {
        l0_count += t.l0.len();
        run_count += t.runs.len();
        sst_count += t.l0.len() + t.runs.iter().map(|r| r.ssts.len()).sum::<usize>();
        l0_bytes += t.l0_bytes;
        total_bytes += t.total_bytes;
    }
    (l0_count, run_count, sst_count, l0_bytes, total_bytes)
}

pub fn output_sst_dto(h: &SsTableHandle) -> OutputSstDto {
    OutputSstDto {
        sst_id: sst_id_dto(&h.id),
        est_bytes: h.estimate_size(),
    }
}

pub fn compaction_dto(c: &slatedb::compactor::Compaction) -> CompactionDto {
    use slatedb::compactor::SourceId;
    let spec = c.spec();
    let segment = spec.segment();
    let sources = spec
        .sources()
        .iter()
        .map(|s| match s {
            SourceId::SortedRun(id) => SourceDto {
                kind: "sorted_run".to_string(),
                id: id.to_string(),
            },
            SourceId::SstView(ulid) => SourceDto {
                kind: "l0".to_string(),
                id: ulid.to_string(),
            },
        })
        .collect();
    CompactionDto {
        id: c.id().to_string(),
        status: format!("{:?}", c.status()).to_lowercase(),
        is_drain: spec.is_drain(),
        segment: (!segment.is_empty()).then(|| key_dto(segment)),
        sources,
        destination: spec.destination(),
        bytes_processed: c.bytes_processed(),
        output_ssts: c.output_ssts().iter().map(output_sst_dto).collect(),
        active: c.active(),
    }
}

pub fn versioned_compactions_dto(
    vc: &slatedb::VersionedCompactions,
) -> VersionedCompactionsDto {
    VersionedCompactionsDto {
        id: vc.id(),
        compactor_epoch: vc.compactor_epoch(),
        compactions: vc.recent_compactions().map(compaction_dto).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_dto_printable_utf8() {
        let k = key_dto(b"user:0001");
        assert_eq!(k.hex, hex::encode(b"user:0001"));
        assert_eq!(k.utf8.as_deref(), Some("user:0001"));
    }

    #[test]
    fn key_dto_binary_has_no_utf8() {
        let k = key_dto(&[0xff, 0x00, 0x01]);
        assert_eq!(k.hex, "ff0001");
        assert_eq!(k.utf8, None);
    }

    #[test]
    fn key_dto_control_chars_have_no_utf8() {
        let k = key_dto(b"a\nb");
        assert_eq!(k.utf8, None);
    }

    #[test]
    fn key_dto_empty() {
        let k = key_dto(b"");
        assert_eq!(k.hex, "");
        assert_eq!(k.utf8, None);
    }
}
