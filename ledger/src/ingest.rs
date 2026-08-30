use sqlx::PgPool;

use crate::bsc::BscClient;
use crate::config::LedgerConfig;
use crate::db::{self, Chain};
use crate::error::LedgerResult;
use crate::lcd::LcdClient;
use crate::parser::{parse_bep20_transfer_log, parse_cw20_wasm_events};

pub async fn ingest_terra_height(
    pool: &PgPool,
    lcd: &LcdClient,
    cfg: &LedgerConfig,
    height: i64,
) -> LedgerResult<usize> {
    let registered = db::registered_set(pool, Chain::Terra).await?;
    let registered_refs: Vec<&str> = registered.iter().map(|s| s.as_str()).collect();
    let txs = lcd.block_txs(height).await?;
    let mut count = 0;
    for tx in txs {
        let events = tx.wasm_events();
        let transfers = parse_cw20_wasm_events(&events, &cfg.cl8y_token_address, &registered_refs)?;
        if transfers.is_empty() {
            continue;
        }
        db::apply_cw20_transfers(pool, &transfers, height, &tx.tx_hash()).await?;
        count += transfers.len();
    }
    db::set_state(pool, "last_indexed_height", &height.to_string()).await?;
    Ok(count)
}

pub async fn ingest_bsc_range(
    pool: &PgPool,
    bsc: &BscClient,
    cfg: &LedgerConfig,
    from_block: i64,
    to_block: i64,
) -> LedgerResult<usize> {
    if from_block > to_block {
        return Ok(0);
    }
    let registered = db::registered_set(pool, Chain::Bsc).await?;
    let registered_refs: Vec<&str> = registered.iter().map(|s| s.as_str()).collect();
    let logs = bsc
        .transfer_logs(&cfg.bsc_cl8y_token_address, from_block, to_block)
        .await?;
    let mut parsed = Vec::new();
    for log in &logs {
        if let Some(t) =
            parse_bep20_transfer_log(log, &cfg.bsc_cl8y_token_address, &registered_refs)?
        {
            parsed.push(t);
        }
    }
    db::apply_bep20_transfers(pool, &parsed).await?;
    db::set_state(pool, "last_indexed_bsc_block", &to_block.to_string()).await?;
    Ok(parsed.len())
}

pub async fn maybe_rewind_terra(
    pool: &PgPool,
    lcd: &LcdClient,
    last_height: i64,
) -> LedgerResult<()> {
    if last_height <= 0 {
        return Ok(());
    }
    let stored = db::get_state(pool, "last_indexed_block_hash").await?;
    let Some(stored) = stored else {
        return Ok(());
    };
    match lcd.block_hash(last_height).await {
        Ok(actual) if actual != stored => {
            tracing::warn!(
                last_height,
                "terra reorg detected; unwinding transfers after fork"
            );
            db::rewind_terra(pool, last_height.saturating_sub(1)).await?;
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "could not verify terra block hash"),
    }
    Ok(())
}

/// Heights/blocks ingested per poll (Terra and BSC). Caps `poll_once` so a
/// catch-up cannot starve the next registration-intent pass (#14).
pub const INGEST_CHUNK: i64 = 2_000;

/// Remaining gap above this is treated as a stale cursor: jump to tip (L1).
/// A low non-zero `last_indexed_*` must not walk millions of historical heights.
pub const INGEST_STALE_GAP: i64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestWindow {
    pub start: i64,
    pub end: i64,
    pub jumped_to_tip: bool,
}

/// Inclusive `[start, end]` to ingest this poll.
///
/// - `last <= 0`: jump to tip (boot / empty cursor; no archive backfill).
/// - `last > 0` and remaining gap `> INGEST_STALE_GAP`: jump to tip (stale cursor).
/// - otherwise: at most `chunk` heights/blocks, same cap Terra and BSC.
pub fn ingest_window(last: i64, tip: i64, chunk: i64, stale_gap: i64) -> Option<IngestWindow> {
    if tip <= 0 {
        return None;
    }
    if last <= 0 {
        return Some(IngestWindow {
            start: tip,
            end: tip,
            jumped_to_tip: true,
        });
    }
    let start = last + 1;
    if start > tip {
        return None;
    }
    let remaining = tip - start + 1;
    if remaining > stale_gap {
        return Some(IngestWindow {
            start: tip,
            end: tip,
            jumped_to_tip: true,
        });
    }
    let end = start
        .saturating_add(chunk.saturating_sub(1).max(0))
        .min(tip);
    Some(IngestWindow {
        start,
        end,
        jumped_to_tip: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tip_is_none() {
        assert_eq!(ingest_window(0, 0, INGEST_CHUNK, INGEST_STALE_GAP), None);
        assert_eq!(ingest_window(10, -1, INGEST_CHUNK, INGEST_STALE_GAP), None);
    }

    #[test]
    fn boot_cursor_jumps_to_tip() {
        assert_eq!(
            ingest_window(0, 30_162_104, INGEST_CHUNK, INGEST_STALE_GAP),
            Some(IngestWindow {
                start: 30_162_104,
                end: 30_162_104,
                jumped_to_tip: true,
            })
        );
    }

    #[test]
    fn caught_up_is_none() {
        assert_eq!(
            ingest_window(100, 100, INGEST_CHUNK, INGEST_STALE_GAP),
            None
        );
        assert_eq!(
            ingest_window(120, 100, INGEST_CHUNK, INGEST_STALE_GAP),
            None
        );
    }

    #[test]
    fn live_gap_is_chunked() {
        assert_eq!(
            ingest_window(100, 3_100, INGEST_CHUNK, INGEST_STALE_GAP),
            Some(IngestWindow {
                start: 101,
                end: 2_100,
                jumped_to_tip: false,
            })
        );
        assert_eq!(
            ingest_window(3_000, 3_100, INGEST_CHUNK, INGEST_STALE_GAP),
            Some(IngestWindow {
                start: 3_001,
                end: 3_100,
                jumped_to_tip: false,
            })
        );
    }

    #[test]
    fn stale_nonzero_cursor_jumps_to_tip() {
        assert_eq!(
            ingest_window(1, 30_162_104, INGEST_CHUNK, INGEST_STALE_GAP),
            Some(IngestWindow {
                start: 30_162_104,
                end: 30_162_104,
                jumped_to_tip: true,
            })
        );
    }

    #[test]
    fn gap_at_stale_threshold_is_chunked_not_jumped() {
        let last = 100;
        let tip = last + INGEST_STALE_GAP;
        assert_eq!(
            ingest_window(last, tip, INGEST_CHUNK, INGEST_STALE_GAP),
            Some(IngestWindow {
                start: 101,
                end: 101 + INGEST_CHUNK - 1,
                jumped_to_tip: false,
            })
        );
    }
}
