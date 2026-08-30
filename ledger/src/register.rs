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

/// Live tip balance. Implementations must not invent amounts (fail closed).
#[async_trait]
pub trait LiveBalanceSource: Send + Sync {
    async fn terra_live_balance(&self, address: &str) -> LedgerResult<(i64, BigInt)>;
    async fn bsc_live_balance(&self, address: &str) -> LedgerResult<(i64, BigInt)>;
}

pub async fn register_wallet(
    pool: &PgPool,
    source: &dyn LiveBalanceSource,
    chain: Chain,
    wallet: &str,
    signature_id: Option<Uuid>,
) -> LedgerResult<RegistrationOutcome> {
    if let Some(existing) = db::get_registration(pool, chain, wallet).await? {
        return Ok(RegistrationOutcome {
            chain: chain.as_str(),
            wallet: existing.wallet_address,
            height: existing.registered_at_height,
            initial_balance: existing.initial_balance,
            inserted: false,
        });
    }

    let (height, amount) = match chain {
        Chain::Terra => source.terra_live_balance(wallet).await?,
        Chain::Bsc => source.bsc_live_balance(wallet).await?,
    };
    if height <= 0 {
        return Err(LedgerError::Lcd(
            "live height/block must be positive; refusing to invent a snapshot".into(),
        ));
    }

    let inserted =
        db::insert_registration(pool, chain, wallet, height, &amount, signature_id).await?;
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
    for (id, chain, wallet, sig) in pending {
        let chain = match chain.as_str() {
            "terra" => Chain::Terra,
            "bsc" => Chain::Bsc,
            other => {
                tracing::warn!(chain = other, "skipping unknown registration chain");
                continue;
            }
        };
        match register_wallet(pool, source, chain, &wallet, sig).await {
            Ok(outcome) => {
                db::mark_intent_processed(pool, id).await?;
                out.push(outcome);
            }
            Err(e) => {
                // Leave processed_at NULL so the next intent pass retries (L11).
                tracing::error!(error = %e, wallet, "registration live-balance failed; will retry");
            }
        }
    }
    Ok(out)
}
