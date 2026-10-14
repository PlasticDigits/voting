use num_bigint::BigInt;
use sqlx::{PgPool, Postgres, Transaction};

use crate::amount::parse_raw_amount;
use crate::config::{normalize_evm_addr, normalize_terra_addr};
use crate::error::{LedgerError, LedgerResult};
use crate::parser::{apply_transfer, Bep20Transfer, Cw20Transfer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chain {
    Terra,
    Bsc,
}

impl Chain {
    pub fn as_str(self) -> &'static str {
        match self {
            Chain::Terra => "terra",
            Chain::Bsc => "bsc",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Registration {
    pub chain: String,
    pub wallet_address: String,
    pub registered_at_height: i64,
    pub initial_balance: String,
    pub status: String,
}

pub async fn migrate(pool: &PgPool) -> LedgerResult<()> {
    let mut connection = pool.acquire().await?;
    sqlx::query("SET search_path TO public, pg_temp")
        .execute(&mut *connection)
        .await?;
    sqlx::migrate!("./migrations").run(&mut *connection).await?;
    Ok(())
}

pub async fn connect(database_url: &str) -> LedgerResult<PgPool> {
    Ok(PgPool::connect(database_url).await?)
}

pub fn normalize_wallet(chain: Chain, raw: &str) -> LedgerResult<String> {
    match chain {
        Chain::Terra => Ok(normalize_terra_addr(raw.to_string())),
        Chain::Bsc => normalize_evm_addr(raw),
    }
}

pub async fn insert_registration(
    pool: &PgPool,
    chain: Chain,
    wallet: &str,
    height: i64,
    initial_balance: &BigInt,
    signature_id: Option<uuid::Uuid>,
) -> LedgerResult<bool> {
    let wallet = normalize_wallet(chain, wallet)?;
    validate_amount(initial_balance)?;
    if height <= 0 {
        return Err(LedgerError::Conflict("invalid registration height".into()));
    }
    let mut tx = pool.begin().await?;
    lock_chain(&mut tx, chain).await?;
    let result = sqlx::query(
        r#"
        INSERT INTO voting_registrations
            (chain, wallet_address, registered_at_height, initial_balance, signature_id)
        VALUES ($1, $2, $3, $4::numeric, $5)
        ON CONFLICT (chain, wallet_address) DO NOTHING
        "#,
    )
    .bind(chain.as_str())
    .bind(&wallet)
    .bind(height)
    .bind(initial_balance.to_string())
    .bind(signature_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(result.rows_affected() > 0)
}

pub async fn get_registration(
    pool: &PgPool,
    chain: Chain,
    wallet: &str,
) -> LedgerResult<Option<Registration>> {
    let wallet = normalize_wallet(chain, wallet)?;
    let row = sqlx::query_as::<_, (String, String, i64, String, String)>(
        r#"
        SELECT chain, wallet_address, registered_at_height, initial_balance::text, status
        FROM voting_registrations
        WHERE chain = $1 AND wallet_address = $2
        "#,
    )
    .bind(chain.as_str())
    .bind(&wallet)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(chain, wallet_address, registered_at_height, initial_balance, status)| Registration {
            chain,
            wallet_address,
            registered_at_height,
            initial_balance,
            status,
        },
    ))
}

pub async fn max_registered_height(pool: &PgPool, chain: Chain) -> LedgerResult<i64> {
    let v: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT MAX(COALESCE(baseline_height,registered_at_height))
        FROM voting_registrations
        WHERE chain = $1 AND status = 'active'
        "#,
    )
    .bind(chain.as_str())
    .fetch_one(pool)
    .await?;
    Ok(v.unwrap_or(0))
}

pub async fn registered_set(pool: &PgPool, chain: Chain) -> LedgerResult<Vec<String>> {
    let rows = sqlx::query_as::<_, (String,)>(
        "SELECT wallet_address FROM voting_registrations WHERE chain = $1 AND status = 'active'",
    )
    .bind(chain.as_str())
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(a,)| a).collect())
}

pub async fn balance_at(
    pool: &PgPool,
    chain: Chain,
    wallet: &str,
    height: i64,
) -> LedgerResult<BigInt> {
    let wallet = normalize_wallet(chain, wallet)?;
    let fn_name = match chain {
        Chain::Terra => "public.voting_cl8y_balance_at",
        Chain::Bsc => "public.voting_bsc_cl8y_balance_at",
    };
    let sql = format!("SELECT {fn_name}($1, $2)::text");
    let raw: String = sqlx::query_scalar(&sql)
        .bind(&wallet)
        .bind(height)
        .fetch_one(pool)
        .await?;
    parse_raw_amount(&raw)
}

pub async fn apply_cw20_transfers(
    pool: &PgPool,
    transfers: &[Cw20Transfer],
    height: i64,
    tx_hash: &str,
) -> LedgerResult<()> {
    if transfers.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    lock_chain(&mut tx, Chain::Terra).await?;
    for (i, t) in transfers.iter().enumerate() {
        persist_terra_leg(&mut tx, t, height, tx_hash, i as i32).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn persist_terra_leg(
    tx: &mut Transaction<'_, Postgres>,
    t: &Cw20Transfer,
    height: i64,
    tx_hash: &str,
    msg_index: i32,
) -> LedgerResult<()> {
    validate_amount(&t.amount)?;
    if height <= 0 || tx_hash.is_empty() {
        return Err(LedgerError::Conflict("invalid Terra event identity".into()));
    }
    let inserted = sqlx::query(
        r#"
        INSERT INTO cl8y_cw20_transfers
            (height, tx_hash, msg_index, from_address, to_address, amount, action)
        VALUES ($1, $2, $3, $4, $5, $6::numeric, $7)
        ON CONFLICT (tx_hash, msg_index) DO NOTHING
        "#,
    )
    .bind(height)
    .bind(tx_hash)
    .bind(msg_index)
    .bind(&t.from)
    .bind(&t.to)
    .bind(t.amount.to_string())
    .bind(&t.action)
    .execute(&mut **tx)
    .await?;

    if inserted.rows_affected() == 0 {
        let same: bool=sqlx::query_scalar("SELECT height=$3 AND from_address=$4 AND to_address=$5 AND amount=$6::numeric AND action=$7 FROM cl8y_cw20_transfers WHERE tx_hash=$1 AND msg_index=$2")
            .bind(tx_hash).bind(msg_index).bind(height).bind(&t.from).bind(&t.to).bind(t.amount.to_string()).bind(&t.action).fetch_one(&mut **tx).await?;
        if !same {
            return Err(LedgerError::Conflict(
                "conflicting Terra event identity".into(),
            ));
        }
        return Ok(());
    }
    for wallet in unique_nonempty(&[&t.from, &t.to]) {
        if !is_registered_in_tx(tx, Chain::Terra, &wallet, height).await? {
            continue;
        }
        let prev = tip_balance_in_tx(tx, Chain::Terra, &wallet).await?;
        let next = apply_transfer(&prev, &wallet, &t.from, &t.to, &t.amount);
        upsert_checkpoint(tx, Chain::Terra, &wallet, height, &next).await?;
    }
    Ok(())
}

pub async fn apply_bep20_transfers(pool: &PgPool, transfers: &[Bep20Transfer]) -> LedgerResult<()> {
    if transfers.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    lock_chain(&mut tx, Chain::Bsc).await?;
    for t in transfers {
        persist_bsc_leg(&mut tx, t).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn persist_bsc_leg(
    tx: &mut Transaction<'_, Postgres>,
    t: &Bep20Transfer,
) -> LedgerResult<()> {
    validate_amount(&t.amount)?;
    if t.bsc_block <= 0 || t.tx_hash.is_empty() || t.log_index < 0 {
        return Err(LedgerError::Conflict("invalid BSC event identity".into()));
    }
    let inserted = sqlx::query(
        r#"
        INSERT INTO cl8y_bep20_transfers
            (bsc_block, tx_hash, log_index, from_address, to_address, amount)
        VALUES ($1, $2, $3, $4, $5, $6::numeric)
        ON CONFLICT (tx_hash, log_index) DO NOTHING
        "#,
    )
    .bind(t.bsc_block)
    .bind(&t.tx_hash)
    .bind(t.log_index)
    .bind(&t.from)
    .bind(&t.to)
    .bind(t.amount.to_string())
    .execute(&mut **tx)
    .await?;

    if inserted.rows_affected() == 0 {
        let same: bool=sqlx::query_scalar("SELECT bsc_block=$3 AND from_address=$4 AND to_address=$5 AND amount=$6::numeric FROM cl8y_bep20_transfers WHERE tx_hash=$1 AND log_index=$2")
            .bind(&t.tx_hash).bind(t.log_index).bind(t.bsc_block).bind(&t.from).bind(&t.to).bind(t.amount.to_string()).fetch_one(&mut **tx).await?;
        if !same {
            return Err(LedgerError::Conflict(
                "conflicting BSC event identity".into(),
            ));
        }
        return Ok(());
    }
    for wallet in unique_nonempty(&[&t.from, &t.to]) {
        if !is_registered_in_tx(tx, Chain::Bsc, &wallet, t.bsc_block).await? {
            continue;
        }
        let prev = tip_balance_in_tx(tx, Chain::Bsc, &wallet).await?;
        let next = apply_transfer(&prev, &wallet, &t.from, &t.to, &t.amount);
        upsert_checkpoint(tx, Chain::Bsc, &wallet, t.bsc_block, &next).await?;
    }
    Ok(())
}

async fn is_registered_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    chain: Chain,
    wallet: &str,
    height: i64,
) -> LedgerResult<bool> {
    let found: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM voting_registrations WHERE chain = $1 AND wallet_address = $2 AND status = 'active' AND COALESCE(baseline_height,registered_at_height) < $3",
    )
    .bind(chain.as_str())
    .bind(wallet)
    .bind(height)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(found.is_some())
}

async fn tip_balance_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    chain: Chain,
    wallet: &str,
) -> LedgerResult<BigInt> {
    let fn_name = match chain {
        Chain::Terra => "public.voting_cl8y_balance_at",
        Chain::Bsc => "public.voting_bsc_cl8y_balance_at",
    };
    // i64::MAX as "tip" — function returns latest checkpoint or initial.
    let sql = format!("SELECT {fn_name}($1, $2)::text");
    let raw: String = sqlx::query_scalar(&sql)
        .bind(wallet)
        .bind(i64::MAX)
        .fetch_one(&mut **tx)
        .await?;
    parse_raw_amount(&raw)
}

async fn upsert_checkpoint(
    tx: &mut Transaction<'_, Postgres>,
    chain: Chain,
    wallet: &str,
    height: i64,
    balance: &BigInt,
) -> LedgerResult<()> {
    let (table, column) = match chain {
        Chain::Terra => ("cl8y_balances", "height"),
        Chain::Bsc => ("cl8y_bsc_balances", "bsc_block"),
    };
    let latest: Option<i64> = sqlx::query_scalar(&format!(
        "SELECT MAX({column}) FROM {table} WHERE wallet_address=$1"
    ))
    .bind(wallet)
    .fetch_one(&mut **tx)
    .await?;
    if latest.is_some_and(|h| h > height) {
        return Err(LedgerError::Conflict(
            "out-of-order accounting event".into(),
        ));
    }
    if balance < &BigInt::from(0) {
        return Err(LedgerError::Conflict(
            "balance underflow; accounting degraded".into(),
        ));
    }
    validate_amount(balance)?;
    match chain {
        Chain::Terra => {
            sqlx::query(
                r#"
                INSERT INTO cl8y_balances (wallet_address, height, balance)
                VALUES ($1, $2, $3::numeric)
                ON CONFLICT (wallet_address, height) DO UPDATE SET balance = EXCLUDED.balance
                "#,
            )
            .bind(wallet)
            .bind(height)
            .bind(balance.to_string())
            .execute(&mut **tx)
            .await?;
        }
        Chain::Bsc => {
            sqlx::query(
                r#"
                INSERT INTO cl8y_bsc_balances (wallet_address, bsc_block, balance)
                VALUES ($1, $2, $3::numeric)
                ON CONFLICT (wallet_address, bsc_block) DO UPDATE SET balance = EXCLUDED.balance
                "#,
            )
            .bind(wallet)
            .bind(height)
            .bind(balance.to_string())
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

pub async fn rewind_terra(pool: &PgPool, after_height: i64) -> LedgerResult<()> {
    let mut tx = pool.begin().await?;
    lock_chain(&mut tx, Chain::Terra).await?;
    sqlx::query("DELETE FROM cl8y_cw20_transfers WHERE height > $1")
        .bind(after_height)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM cl8y_balances WHERE height > $1")
        .bind(after_height)
        .execute(&mut *tx)
        .await?;
    set_state_in_tx(&mut tx, "last_indexed_height", &after_height.to_string()).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn rewind_bsc(pool: &PgPool, after_block: i64) -> LedgerResult<()> {
    let mut tx = pool.begin().await?;
    lock_chain(&mut tx, Chain::Bsc).await?;
    sqlx::query("DELETE FROM cl8y_bep20_transfers WHERE bsc_block > $1")
        .bind(after_block)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM cl8y_bsc_balances WHERE bsc_block > $1")
        .bind(after_block)
        .execute(&mut *tx)
        .await?;
    set_state_in_tx(&mut tx, "last_indexed_bsc_block", &after_block.to_string()).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn get_state(pool: &PgPool, key: &str) -> LedgerResult<Option<String>> {
    let v: Option<String> = sqlx::query_scalar("SELECT value FROM indexer_state WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(v)
}

pub async fn set_state(pool: &PgPool, key: &str, value: &str) -> LedgerResult<()> {
    sqlx::query(
        r#"
        INSERT INTO indexer_state (key, value, updated_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_state_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    key: &str,
    value: &str,
) -> LedgerResult<()> {
    sqlx::query(
        r#"
        INSERT INTO indexer_state (key, value, updated_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn indexed_terra_height(pool: &PgPool) -> LedgerResult<i64> {
    parse_cursor(get_state(pool, "last_indexed_height").await?)
}

pub async fn indexed_bsc_block(pool: &PgPool) -> LedgerResult<i64> {
    parse_cursor(get_state(pool, "last_indexed_bsc_block").await?)
}

pub async fn mark_intent_processed(pool: &PgPool, id: uuid::Uuid) -> LedgerResult<()> {
    sqlx::query("UPDATE voting.registration_intents SET processed_at = NOW(),retry_after=NULL,last_error_class=NULL WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| {
            tracing::error!(error_class="database", "mark_intent_processed failed (L12)");
            LedgerError::Database(e)
        })
}

pub async fn pending_intents(
    pool: &PgPool,
) -> LedgerResult<Vec<(uuid::Uuid, String, String, Option<uuid::Uuid>)>> {
    sqlx::query_as::<_, (uuid::Uuid, String, String, Option<uuid::Uuid>)>(
        r#"
        SELECT id, chain, wallet_address, signature_id
        FROM voting.registration_intents
        WHERE processed_at IS NULL AND (retry_after IS NULL OR retry_after<=NOW())
        ORDER BY COALESCE(last_attempt_at, created_at) ASC
        LIMIT 16
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!(
            error_class = "database",
            "pending_intents query failed; not treating as an empty queue (L12)"
        );
        LedgerError::Database(e)
    })
}

fn unique_nonempty(addrs: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for a in addrs {
        let n = a.trim().to_ascii_lowercase();
        if n.is_empty() || n == crate::parser::ZERO_EVM_ADDRESS {
            continue;
        }
        if !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// Production discipline shared by registration, ingest, rewind and freeze.
pub async fn lock_chain(tx: &mut Transaction<'_, Postgres>, chain: Chain) -> LedgerResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(8136, $1)")
        .bind(match chain {
            Chain::Terra => 1_i32,
            Chain::Bsc => 2_i32,
        })
        .execute(&mut **tx)
        .await?;
    Ok(())
}
pub async fn publish_checkpoint(
    tx: &mut Transaction<'_, Postgres>,
    chain: Chain,
    height: i64,
    hash: &str,
    tip: i64,
) -> LedgerResult<()> {
    let current: Option<i64> =
        sqlx::query_scalar("SELECT height FROM ledger_checkpoints WHERE chain=$1")
            .bind(chain.as_str())
            .fetch_optional(&mut **tx)
            .await?;
    if current.is_some_and(|h| h > height) {
        return Err(LedgerError::Provider("checkpoint_regression"));
    }
    sqlx::query("INSERT INTO ledger_canonical_blocks(chain,height,block_hash) VALUES($1,$2,$3) ON CONFLICT(chain,height) DO UPDATE SET block_hash=EXCLUDED.block_hash")
        .bind(chain.as_str()).bind(height).bind(hash).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO ledger_checkpoints(chain,height,block_hash,observed_at,complete,canonical,network_tip) VALUES($1,$2,$3,NOW(),true,true,$4) ON CONFLICT(chain) DO UPDATE SET height=$2,block_hash=$3,observed_at=NOW(),complete=true,canonical=true,degraded_reason=NULL,network_tip=$4")
        .bind(chain.as_str()).bind(height).bind(hash).bind(tip).execute(&mut **tx).await?;
    set_state_in_tx(
        tx,
        match chain {
            Chain::Terra => "last_indexed_height",
            Chain::Bsc => "last_indexed_bsc_block",
        },
        &height.to_string(),
    )
    .await?;
    set_state_in_tx(
        tx,
        match chain {
            Chain::Terra => "last_indexed_block_hash",
            Chain::Bsc => "last_indexed_bsc_block_hash",
        },
        hash,
    )
    .await?;
    sqlx::query("DELETE FROM ledger_canonical_blocks WHERE chain=$1 AND height < $2")
        .bind(chain.as_str())
        .bind(height.saturating_sub(128))
        .execute(&mut **tx)
        .await?;
    Ok(())
}
pub async fn degrade(pool: &PgPool, chain: Chain, reason: &str) -> LedgerResult<()> {
    sqlx::query("INSERT INTO ledger_checkpoints(chain,degraded_reason) VALUES($1,$2) ON CONFLICT(chain) DO UPDATE SET complete=false,canonical=false,degraded_reason=CASE WHEN ledger_checkpoints.degraded_reason IN ('legacy_electorate_requires_reconciliation','missing_canonical_history','fork_beyond_horizon','registration_baseline_orphaned','lost_registered_history','baseline_recovery_requested','accounting_corruption','accounting_invalid_amount') THEN ledger_checkpoints.degraded_reason ELSE $2 END")
        .bind(chain.as_str()).bind(reason).execute(pool).await?;
    Ok(())
}

fn parse_cursor(value: Option<String>) -> LedgerResult<i64> {
    match value {
        None => Ok(0),
        Some(raw) => raw
            .parse::<i64>()
            .ok()
            .filter(|h| *h >= 0)
            .ok_or(LedgerError::Provider("malformed_cursor")),
    }
}

fn validate_amount(amount: &BigInt) -> LedgerResult<()> {
    if amount < &BigInt::from(0) || amount >= &(BigInt::from(1) << 256) {
        return Err(LedgerError::Amount("amount outside uint256 range".into()));
    }
    Ok(())
}
