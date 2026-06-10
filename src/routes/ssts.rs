use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use ulid::Ulid;

use crate::convert::key_dto;
use crate::dto::{BlockIndexDto, BlockMetaDto, SstDetailDto, SstInfoDto, SstStatsDto};
use crate::error::ApiError;
use crate::state::AppState;

/// Cap on block-index entries returned; large SSTs can have tens of
/// thousands of blocks.
const MAX_INDEX_BLOCKS: usize = 2000;

pub async fn get_one(
    State(state): State<Arc<AppState>>,
    Path(ulid_str): Path<String>,
) -> Result<Json<SstDetailDto>, ApiError> {
    let ulid = Ulid::from_string(&ulid_str)
        .map_err(|_| ApiError::BadRequest(format!("invalid SST ULID '{ulid_str}'")))?;

    if let Some(detail) = state.sst_details.get(&ulid) {
        return Ok(Json((*detail).clone()));
    }

    let file = state.sst_reader.open(ulid).await.map_err(|e| {
        ApiError::NotFound(format!(
            "SST {ulid} could not be opened (possibly GC'd): {e}"
        ))
    })?;

    let meta = file.metadata().await.map_err(ApiError::from)?;
    // Stats blocks may be absent in SSTs written by older format versions;
    // tolerate read failures rather than failing the whole drill-down.
    let stats = file.stats().await.ok().flatten();
    let index = file.index().await.map_err(ApiError::from)?;

    let info = file.info();
    let detail = SstDetailDto {
        ulid: ulid.to_string(),
        location: meta.location.to_string(),
        size_bytes: meta.size,
        last_modified: meta.last_modified,
        info: SstInfoDto {
            first_key: info.first_entry.as_deref().map(key_dto),
            last_key: info.last_entry.as_deref().map(key_dto),
            index_offset: info.index_offset,
            index_len: info.index_len,
            filter_offset: info.filter_offset,
            filter_len: info.filter_len,
            stats_offset: info.stats_offset,
            stats_len: info.stats_len,
            compression: info
                .compression_codec
                .as_ref()
                .map(|c| format!("{c:?}").to_lowercase()),
            sst_type: format!("{:?}", info.sst_type).to_lowercase(),
            filter_format: format!("{:?}", info.filter_format).to_lowercase(),
        },
        stats: stats.map(|s| SstStatsDto {
            num_puts: s.num_puts,
            num_deletes: s.num_deletes,
            num_merges: s.num_merges,
            num_rows: s.num_rows(),
            raw_key_bytes: s.raw_key_size,
            raw_val_bytes: s.raw_val_size,
            block_count: s.block_stats.len(),
        }),
        index: BlockIndexDto {
            total_blocks: index.len(),
            truncated: index.len() > MAX_INDEX_BLOCKS,
            blocks: index
                .iter()
                .take(MAX_INDEX_BLOCKS)
                .map(|(offset, first_key)| BlockMetaDto {
                    offset: *offset,
                    first_key: key_dto(first_key),
                })
                .collect(),
        },
    };

    let detail = Arc::new(detail);
    state.sst_details.put(ulid, detail.clone());
    Ok(Json((*detail).clone()))
}
