//! Bounded per-chain polling. Independent callers cannot short-circuit a sibling.
use crate::{
    bsc::{BscClient, BSC_FINALITY_DEPTH},
    config::LedgerConfig,
    db::{self, Chain},
    ingest::{self, INGEST_CHUNK, INGEST_STALE_GAP},
    lcd::LcdClient,
    LedgerError,
};
pub async fn poll_chain(
    pool: &sqlx::PgPool,
    lcd: &Option<LcdClient>,
    bsc: &BscClient,
    cfg: &LedgerConfig,
    chain: Chain,
) -> Result<(), LedgerError> {
    poll_chain_progress(pool, lcd, bsc, cfg, chain)
        .await
        .map(|_| ())
}
/// True only when successful forward progress still exceeds the readiness lag budget.
/// Supervisors yield and immediately continue bounded catch-up instead of sleeping.
pub async fn poll_chain_progress(
    pool: &sqlx::PgPool,
    lcd: &Option<LcdClient>,
    bsc: &BscClient,
    cfg: &LedgerConfig,
    chain: Chain,
) -> Result<bool, LedgerError> {
    let cursor = match chain {
        Chain::Terra => db::indexed_terra_height(pool).await?,
        Chain::Bsc => db::indexed_bsc_block(pool).await?,
    };
    let permanent:Option<String>=sqlx::query_scalar("SELECT degraded_reason FROM ledger_checkpoints WHERE chain=$1 AND degraded_reason IN ('legacy_electorate_requires_reconciliation','missing_canonical_history','fork_beyond_horizon','registration_baseline_orphaned','lost_registered_history','baseline_recovery_requested','accounting_corruption','accounting_invalid_amount')").bind(chain.as_str()).fetch_optional(pool).await?.flatten();
    if permanent.is_some() {
        return Err(LedgerError::Provider("continuity_requires_reconciliation"));
    }
    if cursor == 0 && db::max_registered_height(pool, chain).await? > 0 {
        db::degrade(pool, chain, "lost_registered_history").await?;
        return Err(LedgerError::Provider("lost_registered_history"));
    }
    let tip = match chain {
        Chain::Terra => {
            let lcd = lcd
                .as_ref()
                .ok_or(LedgerError::Provider("no_terra_provider"))?;
            ingest::maybe_rewind_terra(pool, lcd, cursor).await?;
            let canonical_cursor = db::indexed_terra_height(pool).await?;
            lcd.latest_height_at_least(canonical_cursor).await?
        }
        Chain::Bsc => {
            if !bsc.enabled() {
                return Err(LedgerError::Provider("no_bsc_provider"));
            }
            ingest::maybe_rewind_bsc(pool, bsc, cursor).await?;
            let canonical_cursor = db::indexed_bsc_block(pool).await?;
            let minimum = if canonical_cursor > 0 {
                canonical_cursor.saturating_add(BSC_FINALITY_DEPTH)
            } else {
                0
            };
            bsc.block_number_at_least(minimum)
                .await?
                .saturating_sub(BSC_FINALITY_DEPTH)
        }
    };
    let after_rewind = match chain {
        Chain::Terra => db::indexed_terra_height(pool).await?,
        Chain::Bsc => db::indexed_bsc_block(pool).await?,
    };
    if let Some(w) = ingest::ingest_window(after_rewind, tip, INGEST_CHUNK, INGEST_STALE_GAP) {
        match chain {
            Chain::Terra => {
                for h in w.start..=w.end {
                    ingest::ingest_terra_height(pool, lcd.as_ref().unwrap(), cfg, h).await?;
                }
            }
            Chain::Bsc => {
                ingest::ingest_bsc_next_range(pool, bsc, cfg, w.start).await?;
            }
        }
    } else if after_rewind == tip && tip > 0 {
        // Revalidate a stationary tip before renewing freshness.
        let hash = match chain {
            Chain::Terra => lcd.as_ref().unwrap().block_hash(tip).await?,
            Chain::Bsc => bsc.block_hash(tip).await?,
        };
        let mut tx = pool.begin().await?;
        db::lock_chain(&mut tx, chain).await?;
        db::publish_checkpoint(
            &mut tx,
            chain,
            tip,
            &hash,
            if chain == Chain::Bsc {
                tip + BSC_FINALITY_DEPTH
            } else {
                tip
            },
        )
        .await?;
        tx.commit().await?;
    } else {
        return Err(LedgerError::Provider("provider_head_regressed"));
    }
    let behind: bool = sqlx::query_scalar(
        "SELECT height>$2 AND network_tip-height>$3 FROM ledger_checkpoints WHERE chain=$1",
    )
    .bind(chain.as_str())
    .bind(after_rewind)
    .bind(if chain == Chain::Bsc { 17_i64 } else { 2_i64 })
    .fetch_one(pool)
    .await?;
    Ok(behind)
}
