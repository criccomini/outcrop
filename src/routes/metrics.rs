use std::fmt::Write;
use std::sync::Arc;

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;

use crate::convert;
use crate::state::AppState;

/// Prometheus exposition-format label-value escaping.
fn escape_label(v: &str) -> String {
    v.chars()
        .flat_map(|c| match c {
            '\\' => vec!['\\', '\\'],
            '"' => vec!['\\', '"'],
            '\n' => vec!['\\', 'n'],
            c => vec![c],
        })
        .collect()
}

/// Renders gauges as `# HELP` / `# TYPE` / `name{db="…"} value` triples.
fn render_metrics(db: &str, gauges: &[(&str, &str, f64)]) -> String {
    let db = escape_label(db);
    let mut out = String::new();
    for (name, help, value) in gauges {
        let _ = writeln!(out, "# HELP {name} {help}");
        let _ = writeln!(out, "# TYPE {name} gauge");
        let _ = writeln!(out, "{name}{{db=\"{db}\"}} {value}");
    }
    out
}

/// Prometheus metrics for the inspected DB. Never fails: when the manifest
/// or listing can't be read, reports `slatedb_up 0` (with whatever gauges
/// are still computable) rather than an error status, so scrapes keep
/// working through outages — that is exactly when the signal matters.
pub async fn metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut gauges: Vec<(&str, &str, f64)> = Vec::new();

    let manifest = state.latest_manifest().await.ok().and_then(|m| {
        m.as_ref()
            .as_ref()
            .map(|versioned| convert::manifest_dto(versioned))
    });
    let entries = state.manifest_entries().await.ok();

    let up = manifest.is_some();
    gauges.push((
        "slatedb_up",
        "Whether the latest manifest could be read",
        if up { 1.0 } else { 0.0 },
    ));

    if let Some(dto) = &manifest {
        let (l0_count, run_count, sst_count, l0_bytes, total_bytes) =
            convert::manifest_totals(dto);
        let wal_window = dto
            .next_wal_sst_id
            .saturating_sub(1)
            .saturating_sub(dto.replay_after_wal_id);
        gauges.extend([
            ("slatedb_manifest_id", "Latest manifest id", dto.id as f64),
            (
                "slatedb_total_bytes",
                "Estimated total size of L0 plus all sorted runs",
                total_bytes as f64,
            ),
            ("slatedb_l0_bytes", "Estimated L0 size", l0_bytes as f64),
            ("slatedb_l0_count", "Number of L0 SSTs", l0_count as f64),
            (
                "slatedb_sorted_run_count",
                "Number of sorted runs",
                run_count as f64,
            ),
            ("slatedb_sst_count", "Total SSTs in the tree", sst_count as f64),
            ("slatedb_writer_epoch", "Writer epoch", dto.writer_epoch as f64),
            (
                "slatedb_compactor_epoch",
                "Compactor epoch",
                dto.compactor_epoch as f64,
            ),
            (
                "slatedb_wal_window",
                "WAL SSTs not yet replayed into L0",
                wal_window as f64,
            ),
            (
                "slatedb_checkpoint_count",
                "Checkpoints in the latest manifest",
                dto.checkpoints.len() as f64,
            ),
            (
                "slatedb_clone_count",
                "External DBs (clone parents) referenced",
                dto.external_dbs.len() as f64,
            ),
            (
                "slatedb_last_l0_seq",
                "Last sequence number flushed to L0",
                dto.last_l0_seq as f64,
            ),
        ]);
    }

    if let Some(entries) = &entries {
        gauges.push((
            "slatedb_manifest_count",
            "Manifest versions retained in the object store",
            entries.len() as f64,
        ));
        if let Some(last) = entries.last() {
            gauges.push((
                "slatedb_last_manifest_write_timestamp_seconds",
                "Unix time of the newest manifest write",
                last.last_modified.timestamp() as f64,
            ));
        }
    }

    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        render_metrics(&state.db_path, &gauges),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_label_values() {
        assert_eq!(escape_label(r"a\b"), r"a\\b");
        assert_eq!(escape_label(r#"a"b"#), r#"a\"b"#);
        assert_eq!(escape_label("a\nb"), r"a\nb");
        assert_eq!(escape_label("plain/db-path"), "plain/db-path");
    }

    #[test]
    fn renders_exposition_format() {
        let out = render_metrics("my-db", &[("slatedb_up", "Up", 1.0)]);
        assert_eq!(
            out,
            "# HELP slatedb_up Up\n# TYPE slatedb_up gauge\nslatedb_up{db=\"my-db\"} 1\n"
        );
    }

    #[test]
    fn renders_escaped_db_label() {
        let out = render_metrics("a\"b", &[("slatedb_up", "Up", 0.0)]);
        assert!(out.contains("slatedb_up{db=\"a\\\"b\"} 0"));
    }
}
