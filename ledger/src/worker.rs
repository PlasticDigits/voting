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
            lcd.latest_height().await?
        }
        Chain::Bsc => {
            if !bsc.enabled() {
                return Err(LedgerError::Provider("no_bsc_provider"));
            }
            ingest::maybe_rewind_bsc(pool, bsc, cursor).await?;
            bsc.block_number().await?.saturating_sub(BSC_FINALITY_DEPTH)
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
                ingest::ingest_bsc_range(pool, bsc, cfg, w.start, w.end).await?;
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
    Ok(())
}
