use crate::bsc::BscClient;
use crate::config::LedgerConfig;
use crate::db::{self, Chain};
use crate::error::{LedgerError, LedgerResult};
use crate::lcd::LcdClient;
use crate::parser::{parse_bep20_transfer_log, parse_cw20_wasm_events};
use sqlx::PgPool;

pub async fn ingest_terra_height(
    pool: &PgPool,
    lcd: &LcdClient,
    cfg: &LedgerConfig,
    height: i64,
) -> LedgerResult<usize> {
    let mut tx = pool.begin().await?;
    db::lock_chain(&mut tx, Chain::Terra).await?;
    admit_contiguous(&mut tx, Chain::Terra, height).await?;
    let block = lcd.canonical_block(height).await?;
    verify_registration_baselines(
        pool,
        &mut tx,
        Chain::Terra,
        height,
        Some(&block.source),
        None,
    )
    .await?;
    let registered: Vec<String> = sqlx::query_scalar(
        "SELECT wallet_address FROM voting_registrations WHERE chain='terra' AND status='active'",
    )
    .fetch_all(&mut *tx)
    .await?;
    let refs: Vec<&str> = registered.iter().map(String::as_str).collect();
    let txs = block.executions;
    let mut count = 0;
    for execution in txs {
        // Use original execution-event position, independent of the registered set.
        for (index, event) in execution.wasm_events()?.into_iter().enumerate() {
            for transfer in parse_cw20_wasm_events(&[event], &cfg.cl8y_token_address, &refs)? {
                db::persist_terra_leg(
                    &mut tx,
                    &transfer,
                    height,
                    &execution.tx_hash(),
                    index as i32,
                )
                .await?;
                count += 1;
            }
        }
    }
    if block.source.block_hash(height).await? != block.hash {
        return Err(LedgerError::Provider("terra_changed_during_ingest"));
    }
    db::publish_checkpoint(&mut tx, Chain::Terra, height, &block.hash, block.tip).await?;
    tx.commit().await?;
    Ok(count)
}
pub async fn ingest_bsc_range(
    pool: &PgPool,
    bsc: &BscClient,
    cfg: &LedgerConfig,
    from: i64,
    to: i64,
) -> LedgerResult<usize> {
    if from > to {
        return Ok(0);
    }
    let mut tx = pool.begin().await?;
    db::lock_chain(&mut tx, Chain::Bsc).await?;
    admit_contiguous(&mut tx, Chain::Bsc, from).await?;
    let baselines:Vec<(String,i64,Option<String>)>=sqlx::query_as("SELECT wallet_address,COALESCE(baseline_height,registered_at_height),baseline_block_hash FROM voting_registrations WHERE chain='bsc' AND status='active' AND NOT baseline_verified AND COALESCE(baseline_height,registered_at_height)<=$1").bind(to).fetch_all(&mut *tx).await?;
    let expected = baselines
        .iter()
        .map(|(_, h, hash)| (*h, hash.clone()))
        .collect::<Vec<_>>();
    let range = match bsc
        .canonical_range(&cfg.bsc_cl8y_token_address, from, to, &expected)
        .await
    {
        Ok(range) => range,
        Err(e) => {
            if matches!(e, LedgerError::Provider("registration_baseline_orphaned")) {
                db::degrade(pool, Chain::Bsc, "registration_baseline_orphaned").await?;
            }
            return Err(e);
        }
    };
    for (wallet, _, _) in baselines {
        sqlx::query("UPDATE voting_registrations SET baseline_verified=true WHERE chain='bsc' AND wallet_address=$1").bind(wallet).execute(&mut *tx).await?;
    }
    let registered: Vec<String> = sqlx::query_scalar(
        "SELECT wallet_address FROM voting_registrations WHERE chain='bsc' AND status='active'",
    )
    .fetch_all(&mut *tx)
    .await?;
    let refs = registered.iter().map(String::as_str).collect::<Vec<_>>();
    let mut count = 0;
    for log in &range.logs {
        if let Some(t) = parse_bep20_transfer_log(log, &cfg.bsc_cl8y_token_address, &refs)? {
            db::persist_bsc_leg(&mut tx, &t).await?;
            count += 1;
        }
    }
    for (height, hash) in &range.hashes {
        sqlx::query("INSERT INTO ledger_canonical_blocks(chain,height,block_hash) VALUES('bsc',$1,$2) ON CONFLICT(chain,height) DO UPDATE SET block_hash=$2").bind(height).bind(hash).execute(&mut *tx).await?;
    }
    if range.source.block_hash(to).await? != range.end_hash {
        return Err(LedgerError::Provider("bsc_changed_during_ingest"));
    }
    let tip = range.source.block_number().await?;
    if to > tip.saturating_sub(crate::bsc::BSC_FINALITY_DEPTH) {
        return Err(LedgerError::Provider("bsc_head_regressed"));
    }
    db::publish_checkpoint(&mut tx, Chain::Bsc, to, &range.end_hash, tip).await?;
    tx.commit().await?;
    Ok(count)
}
pub async fn maybe_rewind_terra(pool: &PgPool, lcd: &LcdClient, last: i64) -> LedgerResult<()> {
    canonicality(pool, Chain::Terra, last, Some(lcd), None).await
}
pub async fn maybe_rewind_bsc(pool: &PgPool, bsc: &BscClient, last: i64) -> LedgerResult<()> {
    canonicality(pool, Chain::Bsc, last, None, Some(bsc)).await
}
async fn canonicality(
    pool: &PgPool,
    chain: Chain,
    last: i64,
    lcd: Option<&LcdClient>,
    bsc: Option<&BscClient>,
) -> LedgerResult<()> {
    if last <= 0 {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    db::lock_chain(&mut tx, chain).await?;
    let last: Option<i64> =
        sqlx::query_scalar("SELECT height FROM ledger_checkpoints WHERE chain=$1")
            .bind(chain.as_str())
            .fetch_optional(&mut *tx)
            .await?;
    let last = last.unwrap_or(0);
    if last <= 0 {
        return Err(LedgerError::Provider("missing_canonical_checkpoint"));
    }
    let hash = |height| async move {
        match chain {
            Chain::Terra => lcd.unwrap().block_hash(height).await,
            Chain::Bsc => bsc.unwrap().block_hash(height).await,
        }
    };
    let recent:Vec<(i64,String)>=sqlx::query_as("SELECT height,block_hash FROM ledger_canonical_blocks WHERE chain=$1 ORDER BY height DESC LIMIT 129").bind(chain.as_str()).fetch_all(&mut *tx).await?;
    if recent.first().is_none_or(|(height, _)| *height != last) {
        db::degrade(pool, chain, "missing_canonical_history").await?;
        return Err(LedgerError::Provider("missing_canonical_history"));
    }
    if hash(last).await? == recent[0].1 {
        return Ok(());
    }
    let mut ancestor = None;
    for (height, stored) in recent.iter().skip(1) {
        if hash(*height).await? == *stored {
            ancestor = Some(*height);
            break;
        }
    }
    let Some(ancestor) = ancestor else {
        let epoch_column = match chain {
            Chain::Terra => "terra_epoch",
            Chain::Bsc => "bsc_epoch",
        };
        sqlx::query(&format!("UPDATE voting.proposals SET snapshot_validity='invalid',snapshot_incident='fork_beyond_horizon: common ancestor unknown; review required' WHERE status='open' AND ({epoch_column}=(SELECT epoch FROM ledger_checkpoints WHERE chain=$1) OR {epoch_column} IS NULL)"))
            .bind(chain.as_str()).execute(&mut *tx).await?;
        sqlx::query("UPDATE ledger_checkpoints SET complete=false,canonical=false,degraded_reason='fork_beyond_horizon' WHERE chain=$1").bind(chain.as_str()).execute(&mut *tx).await?;
        tx.commit().await?;
        return Err(LedgerError::Provider("fork_beyond_horizon"));
    };
    let (transfers, balances, column) = match chain {
        Chain::Terra => ("cl8y_cw20_transfers", "cl8y_balances", "height"),
        Chain::Bsc => ("cl8y_bep20_transfers", "cl8y_bsc_balances", "bsc_block"),
    };
    sqlx::query(&format!("DELETE FROM {transfers} WHERE {column}>$1"))
        .bind(ancestor)
        .execute(&mut *tx)
        .await?;
    sqlx::query(&format!("DELETE FROM {balances} WHERE {column}>$1"))
        .bind(ancestor)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM ledger_canonical_blocks WHERE chain=$1 AND height>$2")
        .bind(chain.as_str())
        .bind(ancestor)
        .execute(&mut *tx)
        .await?;
    // Frozen values are never rewritten. Flag their source fork for explicit review.
    let snap_column = match chain {
        Chain::Terra => "terra_height",
        Chain::Bsc => "bsc_block",
    };
    // Lifecycle migration supplies these metadata columns; absent columns must fail closed.
    sqlx::query(&format!("UPDATE voting.proposals SET snapshot_validity='invalid',snapshot_incident='canonical source fork' WHERE {snap_column}>$1"))
        .bind(ancestor).execute(&mut *tx).await?;
    let affected:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM voting_registrations WHERE chain=$1 AND COALESCE(baseline_height,registered_at_height)>$2 AND status='active')").bind(chain.as_str()).bind(ancestor).fetch_one(&mut *tx).await?;
    sqlx::query("UPDATE ledger_checkpoints SET height=$2 WHERE chain=$1")
        .bind(chain.as_str())
        .bind(ancestor)
        .execute(&mut *tx)
        .await?;
    db::publish_checkpoint(&mut tx, chain, ancestor, &hash(ancestor).await?, ancestor).await?;
    if affected {
        sqlx::query("UPDATE ledger_checkpoints SET complete=false,canonical=false,degraded_reason='registration_baseline_orphaned' WHERE chain=$1").bind(chain.as_str()).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    if affected {
        return Err(LedgerError::Provider("registration_baseline_orphaned"));
    }
    Ok(())
}
pub const INGEST_CHUNK: i64 = 32;
pub const INGEST_STALE_GAP: i64 = 10_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestWindow {
    pub start: i64,
    pub end: i64,
    pub jumped_to_tip: bool,
}
/// Only an empty electorate may bootstrap at tip (caller admission). Existing
/// nonzero cursors always catch up; unavailable history degrades instead of skips.
pub fn ingest_window(last: i64, tip: i64, chunk: i64, _stale_gap: i64) -> Option<IngestWindow> {
    if tip <= 0 || last >= tip {
        return None;
    }
    let start = if last <= 0 { tip } else { last + 1 };
    Some(IngestWindow {
        start,
        end: start.saturating_add(chunk.max(1) - 1).min(tip),
        jumped_to_tip: last <= 0,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn never_skip_existing_history() {
        let w = ingest_window(1, 30_162_104, 32, 10_000).unwrap();
        assert_eq!(w.start, 2);
        assert_eq!(w.end, 33);
        assert!(!w.jumped_to_tip);
    }
    #[test]
    fn empty_boot_and_caught_up() {
        assert_eq!(ingest_window(100, 100, 32, 10_000), None);
        assert_eq!(ingest_window(0, 500, 32, 10_000).unwrap().start, 500);
    }
}

async fn admit_contiguous(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    chain: Chain,
    from: i64,
) -> LedgerResult<()> {
    let current: Option<i64> =
        sqlx::query_scalar("SELECT height FROM ledger_checkpoints WHERE chain=$1")
            .bind(chain.as_str())
            .fetch_optional(&mut **tx)
            .await?;
    if current.is_some_and(|h| h > 0 && from > h + 1) {
        return Err(LedgerError::Provider("noncontiguous_ingest"));
    }
    Ok(())
}

async fn verify_registration_baselines(
    pool: &PgPool,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    chain: Chain,
    to: i64,
    lcd: Option<&LcdClient>,
    bsc: Option<&BscClient>,
) -> LedgerResult<()> {
    let baselines:Vec<(String,i64,Option<String>)>=sqlx::query_as("SELECT wallet_address,COALESCE(baseline_height,registered_at_height),baseline_block_hash FROM voting_registrations WHERE chain=$1 AND status='active' AND NOT baseline_verified AND COALESCE(baseline_height,registered_at_height)<=$2").bind(chain.as_str()).bind(to).fetch_all(&mut **tx).await?;
    let mut hashes = std::collections::HashMap::new();
    for (wallet, height, expected) in baselines {
        let actual = if let Some(hash) = hashes.get(&height) {
            hash
        } else {
            let hash = match chain {
                Chain::Terra => lcd.unwrap().block_hash(height).await?,
                Chain::Bsc => bsc.unwrap().block_hash(height).await?,
            };
            hashes.insert(height, hash);
            hashes.get(&height).unwrap()
        };
        if expected.as_ref() != Some(actual) {
            db::degrade(pool, chain, "registration_baseline_orphaned").await?;
            return Err(LedgerError::Provider("registration_baseline_orphaned"));
        }
        sqlx::query("UPDATE voting_registrations SET baseline_verified=true WHERE chain=$1 AND wallet_address=$2").bind(chain.as_str()).bind(wallet).execute(&mut **tx).await?;
    }
    Ok(())
}
