//! Pure aggregation for the summary-first LSM endpoint: per-level rollups
//! plus a bucketed key-coverage histogram, so the payload stays
//! O(levels × buckets) for arbitrarily large trees instead of O(SSTs).

use crate::dto::{
    KeyDto, LevelSummaryDto, LsmSummaryDto, ManifestDto, SegmentMetaDto, SstViewDto, TreeDto,
};

/// Levels at or below this SST count ship per-SST detail.
pub const DETAIL_CAP: usize = 64;
/// Key-space buckets in each coverage histogram.
pub const BUCKETS: usize = 96;
/// Boundary keys are sampled down to this many before ranking; bucket
/// resolution only needs ~BUCKETS distinct edges, so this is generous.
const MAX_BOUNDARY_SAMPLE: usize = 8192;

/// Summarize `m` viewed at `segment` (None = root tree). Returns None when
/// the segment index is out of range.
pub fn summarize(m: &ManifestDto, segment: Option<usize>) -> Option<LsmSummaryDto> {
    let root_sst_count = tree_sst_count(&m.tree);
    // Auto-pick: a segmented DB with an empty root tree opens on its first
    // segment, since the root view would always be blank.
    let segment = match segment {
        Some(i) if i >= m.segments.len() => return None,
        Some(i) => Some(i),
        None if root_sst_count == 0 && !m.segments.is_empty() => Some(0),
        None => None,
    };
    let tree = match segment {
        Some(i) => &m.segments[i].tree,
        None => &m.tree,
    };
    let (levels, bucket_keys) = summarize_tree(tree, BUCKETS, DETAIL_CAP);
    Some(LsmSummaryDto {
        manifest_id: m.id,
        segment_extractor_name: m.segment_extractor_name.clone(),
        segments: m
            .segments
            .iter()
            .map(|s| SegmentMetaDto {
                prefix: s.prefix.clone(),
                sst_count: tree_sst_count(&s.tree),
                est_bytes: s.tree.total_bytes,
            })
            .collect(),
        segment,
        root_sst_count,
        buckets: BUCKETS,
        bucket_keys,
        levels,
        total_bytes: tree.total_bytes,
        l0_bytes: tree.l0_bytes,
    })
}

fn tree_sst_count(t: &TreeDto) -> usize {
    t.l0.len() + t.runs.iter().map(|r| r.ssts.len()).sum::<usize>()
}

/// One level per L0 + sorted run, each with a coverage histogram over the
/// tree's key space, rank-scaled by distinct SST boundary keys (the same
/// scaling the per-SST view uses) so skewed keyspaces stay readable.
fn summarize_tree(
    tree: &TreeDto,
    buckets: usize,
    detail_cap: usize,
) -> (Vec<LevelSummaryDto>, Vec<KeyDto>) {
    struct Src<'a> {
        label: String,
        run_id: Option<u32>,
        is_l0: bool,
        ssts: &'a [SstViewDto],
        bytes: u64,
    }
    let mut srcs = vec![Src {
        label: "L0".to_string(),
        run_id: None,
        is_l0: true,
        ssts: &tree.l0,
        bytes: tree.l0_bytes,
    }];
    for r in &tree.runs {
        srcs.push(Src {
            label: format!("SR {}", r.id),
            run_id: Some(r.id),
            is_l0: false,
            ssts: &r.ssts,
            bytes: r.est_bytes,
        });
    }

    // Distinct boundary keys, sorted. Hex strings compare in byte order.
    let mut boundaries: Vec<&KeyDto> = srcs
        .iter()
        .flat_map(|s| s.ssts.iter())
        .flat_map(|s| s.first_key.iter().chain(s.last_key.iter()))
        .collect();
    boundaries.sort_by(|a, b| a.hex.cmp(&b.hex));
    boundaries.dedup_by(|a, b| a.hex == b.hex);
    if boundaries.len() > MAX_BOUNDARY_SAMPLE {
        let n = boundaries.len();
        boundaries = (0..MAX_BOUNDARY_SAMPLE)
            .map(|i| boundaries[i * (n - 1) / (MAX_BOUNDARY_SAMPLE - 1)])
            .collect();
    }
    let n = boundaries.len();

    let rank_of = |hex: &str| -> usize {
        boundaries
            .partition_point(|k| k.hex.as_str() < hex)
            .min(n.saturating_sub(1))
    };
    let bucket_of_rank = |i: usize| -> usize {
        if n <= 1 {
            return 0;
        }
        let pos = i as f64 / (n - 1) as f64;
        ((pos * buckets as f64) as usize).min(buckets - 1)
    };

    let levels = srcs
        .iter()
        .map(|src| {
            // Coverage is the max point-depth inside each bucket: how many
            // of the level's SSTs a read at that key must consult. Counting
            // SSTs *touching* a bucket would inflate dense disjoint runs
            // (many tiny SSTs per bucket) far past their true read amp of 1.
            let mut starts = vec![0u32; n];
            let mut ends = vec![0u32; n];
            let mut any = false;
            for sst in src.ssts {
                let (Some(f), Some(l)) = (&sst.first_key, &sst.last_key) else {
                    continue;
                };
                let r0 = rank_of(&f.hex);
                let r1 = rank_of(&l.hex).max(r0);
                starts[r0] += 1;
                ends[r1] += 1;
                any = true;
            }
            let mut coverage = vec![0u32; buckets];
            if any {
                // Depth at boundary i is starts≤i minus ends<i; depth in the
                // open gap after i is starts≤i minus ends≤i, and it applies
                // to every bucket between boundary i and boundary i+1.
                let mut starts_le = 0u32;
                let mut ends_le = 0u32;
                for i in 0..n {
                    let prev_ends_le = ends_le;
                    starts_le += starts[i];
                    ends_le += ends[i];
                    let b = bucket_of_rank(i);
                    coverage[b] = coverage[b].max(starts_le - prev_ends_le);
                    let gap = starts_le - ends_le;
                    if gap > 0 && i + 1 < n {
                        for c in &mut coverage[b..=bucket_of_rank(i + 1)] {
                            *c = (*c).max(gap);
                        }
                    }
                }
            }
            LevelSummaryDto {
                label: src.label.clone(),
                run_id: src.run_id,
                is_l0: src.is_l0,
                sst_count: src.ssts.len(),
                est_bytes: src.bytes,
                coverage,
                ssts: (src.ssts.len() <= detail_cap).then(|| src.ssts.to_vec()),
            }
        })
        .collect();

    // Bucket edges for tooltips, at evenly spaced ranks.
    let bucket_keys = if n == 0 {
        Vec::new()
    } else {
        (0..=buckets)
            .map(|i| boundaries[i * (n - 1) / buckets].clone())
            .collect()
    };
    (levels, bucket_keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{SegmentDto, SortedRunDto, SstIdDto};

    fn key(s: &str) -> KeyDto {
        KeyDto {
            hex: hex::encode(s),
            utf8: Some(s.to_string()),
        }
    }

    fn sst(first: &str, last: &str, bytes: u64) -> SstViewDto {
        SstViewDto {
            view_id: format!("{first}-{last}"),
            sst_id: SstIdDto::Compacted {
                ulid: "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
            },
            first_key: Some(key(first)),
            last_key: Some(key(last)),
            est_bytes: bytes,
            compression: None,
            visible_range: None,
        }
    }

    fn tree(l0: Vec<SstViewDto>, runs: Vec<SortedRunDto>) -> TreeDto {
        let l0_bytes = l0.iter().map(|s| s.est_bytes).sum::<u64>();
        let total_bytes = l0_bytes + runs.iter().map(|r| r.est_bytes).sum::<u64>();
        TreeDto {
            l0,
            runs,
            l0_bytes,
            total_bytes,
        }
    }

    fn run(id: u32, ssts: Vec<SstViewDto>) -> SortedRunDto {
        let est_bytes = ssts.iter().map(|s| s.est_bytes).sum();
        SortedRunDto { id, est_bytes, ssts }
    }

    fn manifest(tree: TreeDto, segments: Vec<SegmentDto>) -> ManifestDto {
        ManifestDto {
            id: 7,
            initialized: true,
            writer_epoch: 1,
            compactor_epoch: 1,
            next_wal_sst_id: 1,
            replay_after_wal_id: 0,
            last_l0_seq: 0,
            last_l0_clock_tick: 0,
            recent_snapshot_min_seq: 0,
            last_compacted_l0_sst_view_id: None,
            wal_object_store_uri: None,
            segment_extractor_name: None,
            tree,
            segments,
            checkpoints: vec![],
            external_dbs: vec![],
        }
    }

    #[test]
    fn overlapping_l0_depth_adds_up() {
        let t = tree(
            vec![sst("a", "z", 10), sst("a", "z", 10), sst("a", "m", 10)],
            vec![],
        );
        let (levels, edges) = summarize_tree(&t, 10, 64);
        assert_eq!(levels.len(), 1);
        let cov = &levels[0].coverage;
        // Both full-span SSTs cover every bucket; "a".."m" adds depth at
        // the low end only.
        assert_eq!(cov[0], 3);
        assert_eq!(cov[9], 2);
        assert!(cov.iter().all(|&d| d >= 2));
        assert_eq!(edges.len(), 11);
        assert_eq!(edges[0].utf8.as_deref(), Some("a"));
        assert_eq!(edges[10].utf8.as_deref(), Some("z"));
    }

    #[test]
    fn disjoint_sorted_run_depth_is_at_most_one() {
        let t = tree(
            vec![],
            vec![run(0, vec![sst("a", "f", 5), sst("g", "m", 5), sst("n", "z", 5)])],
        );
        let (levels, _) = summarize_tree(&t, 12, 64);
        let sr = &levels[1];
        assert_eq!(sr.label, "SR 0");
        assert_eq!(sr.run_id, Some(0));
        assert!(sr.coverage.iter().all(|&d| d <= 1));
        assert!(sr.coverage.iter().any(|&d| d == 1));
    }

    #[test]
    fn detail_cap_strips_per_sst_lists() {
        let big: Vec<SstViewDto> = (0..5)
            .map(|i| sst(&format!("k{i:02}a"), &format!("k{i:02}z"), 1))
            .collect();
        let t = tree(vec![sst("a", "b", 1)], vec![run(0, big)]);
        let (levels, _) = summarize_tree(&t, 8, 2);
        assert!(levels[0].ssts.is_some(), "small L0 keeps detail");
        assert!(levels[1].ssts.is_none(), "run over cap drops detail");
        assert_eq!(levels[1].sst_count, 5);
    }

    #[test]
    fn empty_tree_summarizes_to_zeroes() {
        let t = tree(vec![], vec![]);
        let (levels, edges) = summarize_tree(&t, 8, 64);
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].sst_count, 0);
        assert!(levels[0].coverage.iter().all(|&d| d == 0));
        assert_eq!(levels[0].ssts.as_deref(), Some(&[][..]));
        assert!(edges.is_empty());
    }

    #[test]
    fn boundary_sampling_keeps_extremes() {
        // > MAX_BOUNDARY_SAMPLE distinct boundaries forces the sample path.
        let ssts: Vec<SstViewDto> = (0..5000)
            .map(|i| sst(&format!("k{:06}", i * 2), &format!("k{:06}", i * 2 + 1), 1))
            .collect();
        let t = tree(vec![], vec![run(0, ssts)]);
        let (levels, edges) = summarize_tree(&t, BUCKETS, 64);
        assert_eq!(edges.len(), BUCKETS + 1);
        assert_eq!(edges[0].utf8.as_deref(), Some("k000000"));
        assert_eq!(edges[BUCKETS].utf8.as_deref(), Some("k009999"));
        // A dense disjoint run covers the whole space one deep (sampling
        // can merge an end and the next start into one rank, reading as 2).
        assert!(levels[1].coverage.iter().all(|&d| (1..=2).contains(&d)));
    }

    #[test]
    fn summarize_picks_root_by_default() {
        let m = manifest(tree(vec![sst("a", "z", 5)], vec![]), vec![]);
        let s = summarize(&m, None).unwrap();
        assert_eq!(s.segment, None);
        assert_eq!(s.root_sst_count, 1);
        assert_eq!(s.manifest_id, 7);
        assert_eq!(s.buckets, BUCKETS);
    }

    #[test]
    fn summarize_auto_falls_to_first_segment_when_root_empty() {
        let seg = SegmentDto {
            prefix: key("t00/"),
            tree: tree(vec![sst("t00/a", "t00/z", 9)], vec![]),
        };
        let m = manifest(tree(vec![], vec![]), vec![seg]);
        let s = summarize(&m, None).unwrap();
        assert_eq!(s.segment, Some(0));
        assert_eq!(s.total_bytes, 9);
        assert_eq!(s.segments.len(), 1);
        assert_eq!(s.segments[0].sst_count, 1);
    }

    #[test]
    fn summarize_rejects_out_of_range_segment() {
        let m = manifest(tree(vec![], vec![]), vec![]);
        assert!(summarize(&m, Some(0)).is_none());
    }
}
