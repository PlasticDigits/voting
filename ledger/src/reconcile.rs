//! Explicit live baseline recovery; old checkpoints and frozen weights remain intact.
use crate::{
    bsc::{BscClient, BSC_FINALITY_DEPTH},
    config::LedgerConfig,
    db::{self, Chain},
    lcd::LcdClient,
    register::LiveBalanceSource,
    LedgerError,
};
use sqlx::PgPool;

async fn reconcile_baseline(
    pool: &PgPool,
    source: &dyn LiveBalanceSource,
    lcd: &LcdClient,
    bsc: &BscClient,
    cfg: &LedgerConfig,
    chain: Chain,
    only_if_required: bool,
) -> Result<bool, LedgerError> {
    let mut tx = pool.begin().await?;
    db::lock_chain(&mut tx, chain).await?;
    if only_if_required {
        let healthy:Option<bool>=sqlx::query_scalar("SELECT complete AND canonical AND degraded_reason IS NULL FROM ledger_checkpoints WHERE chain=$1").bind(chain.as_str()).fetch_optional(&mut *tx).await?;
        if healthy == Some(true) {
            return Ok(false);
        }
    }
    let wallets:Vec<String>=sqlx::query_scalar("SELECT wallet_address FROM voting_registrations WHERE chain=$1 AND status='active' ORDER BY wallet_address").bind(chain.as_str()).fetch_all(&mut *tx).await?;
    let snapshot = source.bulk_live_balances(chain, &wallets).await?;
    let height = snapshot.height;
    let hash = snapshot.block_hash;
    if height <= 0
        || snapshot.balances.len() != wallets.len()
        || snapshot
            .balances
            .iter()
            .any(|b| b < &num_bigint::BigInt::from(0))
    {
        return Err(LedgerError::Provider("invalid_bulk_baseline"));
    }
    let actual = match chain {
        Chain::Terra => lcd.block_hash(height).await?,
        Chain::Bsc => bsc.block_hash(height).await?,
    };
    if actual != hash {
        return Err(LedgerError::Provider("divergent_recovery_provider"));
    }
    if chain == Chain::Bsc {
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            loop {
                if bsc.block_number().await? >= height + BSC_FINALITY_DEPTH {
                    return Ok::<_, LedgerError>(());
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        })
        .await
        .map_err(|_| LedgerError::Provider("reconciliation_finality_deadline"))??;
        if bsc.block_hash(height).await? != hash {
            return Err(LedgerError::Provider("reconciliation_source_fork"));
        }
    }
    for (wallet, balance) in wallets.iter().zip(&snapshot.balances) {
        let (table, column) = match chain {
            Chain::Terra => ("cl8y_balances", "height"),
            Chain::Bsc => ("cl8y_bsc_balances", "bsc_block"),
        };
        // Reject overwriting an already-recorded checkpoint at the baseline height.
        sqlx::query(&format!(
            "INSERT INTO {table}(wallet_address,{column},balance) VALUES($1,$2,$3::numeric)"
        ))
        .bind(wallet)
        .bind(height)
        .bind(balance.to_string())
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE voting_registrations SET baseline_height=$3,baseline_block_hash=$4,baseline_verified=true,baseline_epoch=COALESCE((SELECT epoch+1 FROM ledger_checkpoints WHERE chain=$1),1) WHERE chain=$1 AND wallet_address=$2").bind(chain.as_str()).bind(wallet).bind(height).bind(&hash).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE ledger_checkpoints SET epoch=epoch+1 WHERE chain=$1")
        .bind(chain.as_str())
        .execute(&mut *tx)
        .await?;
    db::publish_checkpoint(
        &mut tx,
        chain,
        height,
        &hash,
        match chain {
            Chain::Terra => lcd.latest_height().await?,
            Chain::Bsc => bsc.block_number().await?,
        },
    )
    .await?;
    tx.commit().await?;
    tracing::info!(
        chain = chain.as_str(),
        height,
        holders = wallets.len(),
        token = if chain == Chain::Terra {
            &cfg.cl8y_token_address
        } else {
            &cfg.bsc_cl8y_token_address
        },
        "explicit live baseline epoch committed"
    );
    Ok(true)
}

pub async fn reconcile_live_baseline(
    pool: &PgPool,
    source: &dyn LiveBalanceSource,
    lcd: &LcdClient,
    bsc: &BscClient,
    cfg: &LedgerConfig,
    chain: Chain,
) -> Result<(), LedgerError> {
    reconcile_baseline(pool, source, lcd, bsc, cfg, chain, false)
        .await
        .map(|_| ())
}

/// Explicit deploy-time recovery request; committed healthy epochs are skipped on restart.
pub async fn reconcile_if_required(
    pool: &PgPool,
    source: &dyn LiveBalanceSource,
    lcd: &LcdClient,
    bsc: &BscClient,
    cfg: &LedgerConfig,
) -> Result<usize, LedgerError> {
    let mut repaired = 0;
    for chain in [Chain::Terra, Chain::Bsc] {
        let holders: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM voting_registrations WHERE chain=$1 AND status='active'",
        )
        .bind(chain.as_str())
        .fetch_one(pool)
        .await?;
        let needs:Option<bool>=sqlx::query_scalar("SELECT NOT complete OR NOT canonical OR degraded_reason IS NOT NULL FROM ledger_checkpoints WHERE chain=$1").bind(chain.as_str()).fetch_optional(pool).await?;
        if needs == Some(false) {
            continue;
        }
        let cursor = match chain {
            Chain::Terra => db::indexed_terra_height(pool).await?,
            Chain::Bsc => db::indexed_bsc_block(pool).await?,
        };
        if holders == 0 && cursor == 0 && needs.is_none() {
            continue;
        }

        sqlx::query("INSERT INTO ledger_checkpoints(chain,degraded_reason) VALUES($1,'baseline_recovery_requested') ON CONFLICT(chain) DO NOTHING").bind(chain.as_str()).execute(pool).await?;
        if reconcile_baseline(pool, source, lcd, bsc, cfg, chain, true).await? {
            repaired += 1;
        }
    }
    Ok(repaired)
}
