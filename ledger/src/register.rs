use async_trait::async_trait;
use num_bigint::BigInt;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::{self, Chain};
use crate::error::{LedgerError, LedgerResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationOutcome {
    pub chain: &'static str,
    pub wallet: String,
    pub height: i64,
    pub initial_balance: String,
    pub inserted: bool,
}

#[derive(Debug, Clone)]
pub struct LiveBalanceSnapshot {
    pub height: i64,
    pub amount: BigInt,
    pub block_hash: String,
}

#[derive(Debug, Clone)]
pub struct LiveBalancesSnapshot {
    pub height: i64,
    pub block_hash: String,
    pub balances: Vec<BigInt>,
}

/// Live tip balance. Implementations must not invent amounts (fail closed).
#[async_trait]
pub trait LiveBalanceSource: Send + Sync {
    async fn terra_live_balance(&self, address: &str) -> LedgerResult<LiveBalanceSnapshot>;
    async fn bsc_live_balance(&self, address: &str) -> LedgerResult<LiveBalanceSnapshot>;
    async fn bulk_live_balances(
        &self,
        _chain: Chain,
        _wallets: &[String],
    ) -> LedgerResult<LiveBalancesSnapshot> {
        Err(LedgerError::Provider("bulk_snapshot_unsupported"))
    }
}

pub async fn register_wallet(
    pool: &PgPool,
    source: &dyn LiveBalanceSource,
    chain: Chain,
    wallet: &str,
    signature_id: Option<Uuid>,
) -> LedgerResult<RegistrationOutcome> {
    let wallet_normalized = db::normalize_wallet(chain, wallet)?;
    let wallet = wallet_normalized.as_str();
    if let Some(existing) = db::get_registration(pool, chain, wallet).await? {
        return Ok(RegistrationOutcome {
            chain: chain.as_str(),
            wallet: existing.wallet_address,
            height: existing.registered_at_height,
            initial_balance: existing.initial_balance,
            inserted: false,
        });
    }

    let observed = match chain {
        Chain::Terra => source.terra_live_balance(wallet).await?,
        Chain::Bsc => source.bsc_live_balance(wallet).await?,
    };
    let height = observed.height;
    let amount = observed.amount;
    let block_hash = observed.block_hash;
    let valid_hash = match chain {
        Chain::Terra => block_hash.len() == 64 && block_hash.bytes().all(|b| b.is_ascii_hexdigit()),
        Chain::Bsc => {
            block_hash.len() == 66
                && block_hash.starts_with("0x")
                && block_hash[2..].bytes().all(|b| b.is_ascii_hexdigit())
        }
    };
    if !valid_hash {
        return Err(LedgerError::Provider("unbound_registration_hash"));
    }
    if height <= 0 {
        return Err(LedgerError::Lcd(
            "live height/block must be positive; refusing to invent a snapshot".into(),
        ));
    }

    let mut tx = pool.begin().await?;
    db::lock_chain(&mut tx, chain).await?;
    let cursor_key = match chain {
        Chain::Terra => "last_indexed_height",
        Chain::Bsc => "last_indexed_bsc_block",
    };
    let cursor: Option<String> = sqlx::query_scalar("SELECT value FROM indexer_state WHERE key=$1")
        .bind(cursor_key)
        .fetch_optional(&mut *tx)
        .await?;
    if cursor
        .as_deref()
        .is_some_and(|s| s.parse::<i64>().ok().is_none_or(|h| h < 0))
    {
        return Err(LedgerError::Provider("malformed_registration_cursor"));
    }
    if cursor
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .is_some_and(|h| h > height)
    {
        return Err(LedgerError::Provider("registration_snapshot_overtaken"));
    }
    if amount < BigInt::from(0) {
        return Err(LedgerError::Amount("negative live balance".into()));
    }
    let known: Option<String> = sqlx::query_scalar(
        "SELECT block_hash FROM ledger_canonical_blocks WHERE chain=$1 AND height=$2",
    )
    .bind(chain.as_str())
    .bind(height)
    .fetch_optional(&mut *tx)
    .await?;
    if known.as_deref().is_some_and(|h| h != block_hash) {
        return Err(LedgerError::Provider("registration_source_fork"));
    }
    let verified = known.is_some();
    let inserted=sqlx::query("INSERT INTO voting_registrations(chain,wallet_address,registered_at_height,initial_balance,signature_id,baseline_height,baseline_block_hash,baseline_verified,baseline_epoch) VALUES($1,$2,$3,$4::numeric,$5,$3,$6,$7,COALESCE((SELECT epoch FROM ledger_checkpoints WHERE chain=$1),1)) ON CONFLICT(chain,wallet_address) DO NOTHING")
        .bind(chain.as_str()).bind(wallet).bind(height).bind(amount.to_string()).bind(signature_id).bind(block_hash).bind(verified).execute(&mut *tx).await?.rows_affected()>0;
    tx.commit().await?;
    let stored = db::get_registration(pool, chain, wallet)
        .await?
        .ok_or_else(|| LedgerError::NotFound("registration vanished after insert".into()))?;
    Ok(RegistrationOutcome {
        chain: chain.as_str(),
        wallet: stored.wallet_address,
        height: stored.registered_at_height,
        initial_balance: stored.initial_balance,
        inserted,
    })
}

pub async fn process_pending_intents(
    pool: &PgPool,
    source: &dyn LiveBalanceSource,
) -> LedgerResult<Vec<RegistrationOutcome>> {
    let pending = db::pending_intents(pool).await?;
    let mut out = Vec::new();
    // Bounded batches with four in-flight items, each having a deadline.
    for batch in pending.chunks(4) {
        let one = |i: usize| async move {
            let Some((id, chain, wallet, sig)) = batch.get(i) else {
                return Ok(None);
            };
            let claimed=sqlx::query("UPDATE voting.registration_intents SET last_attempt_at=NOW(),retry_after=NOW()+INTERVAL '30 seconds',attempts=attempts+1 WHERE id=$1 AND processed_at IS NULL AND (retry_after IS NULL OR retry_after<=NOW())")
                .bind(id).execute(pool).await?.rows_affected()>0;
            if !claimed {
                return Ok(None);
            }
            let chain = match chain.as_str() {
                "terra" => Chain::Terra,
                "bsc" => Chain::Bsc,
                _ => return Err(LedgerError::Conflict("invalid intent chain".into())),
            };
            match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                register_wallet(pool, source, chain, wallet, *sig),
            )
            .await
            {
                Ok(Ok(outcome)) => {
                    db::mark_intent_processed(pool, *id).await?;
                    Ok(Some(outcome))
                }
                result => {
                    let class = match result {
                        Err(_) => "deadline",
                        Ok(Err(_)) => "live_balance_or_database",
                        _ => unreachable!(),
                    };
                    sqlx::query("UPDATE voting.registration_intents SET last_error_class=$2,retry_after=NOW()+make_interval(secs=>LEAST(300,5*attempts)) WHERE id=$1").bind(id).bind(class).execute(pool).await?;
                    Ok(None)
                }
            }
        };
        let (a, b, c, d) = tokio::join!(one(0), one(1), one(2), one(3));
        for result in [a, b, c, d] {
            if let Some(o) = result? {
                out.push(o);
            }
        }
    }
    Ok(out)
}
/// Bound the entire pass (SQL claims included), then publish fresh progress state.
/// A failed state write is fatal so supervision cannot retain a false healthy worker.
pub async fn supervised_intent_pass(
    pool: &PgPool,
    source: &dyn LiveBalanceSource,
    deadline: std::time::Duration,
) -> LedgerResult<bool> {
    let successful = matches!(
        tokio::time::timeout(deadline, process_pending_intents(pool, source)).await,
        Ok(Ok(_))
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        db::set_state(
            pool,
            "intents_ok",
            if successful { "true" } else { "false" },
        ),
    )
    .await
    .map_err(|_| LedgerError::Provider("intent_progress_publish_deadline"))??;
    Ok(successful)
}
