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
    sqlx::migrate!("./migrations").run(pool).await?;
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
    .execute(pool)
    .await?;
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
    Ok(row.map(|(chain, wallet_address, registered_at_height, initial_balance, status)| {
        Registration {
            chain,
            wallet_address,
            registered_at_height,
            initial_balance,
            status,
        }
    }))
}

pub async fn max_registered_height(pool: &PgPool, chain: Chain) -> LedgerResult<i64> {
    let v: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT MAX(registered_at_height)
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

pub async fn balance_at(pool: &PgPool, chain: Chain, wallet: &str, height: i64) -> LedgerResult<BigInt> {
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
    for (i, t) in transfers.iter().enumerate() {
        persist_terra_leg(&mut tx, t, height, tx_hash, i as i32).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn persist_terra_leg(
    tx: &mut Transaction<'_, Postgres>,
    t: &Cw20Transfer,
    height: i64,
    tx_hash: &str,
    msg_index: i32,
) -> LedgerResult<()> {
    sqlx::query(
        r#"
        INSERT INTO cl8y_cw20_transfers
            (height, tx_hash, msg_index, from_address, to_address, amount, action)
        VALUES ($1, $2, $3, $4, $5, $6::numeric, $7)
        ON CONFLICT (tx_hash, msg_index, from_address, to_address, amount) DO NOTHING
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

    for wallet in unique_nonempty(&[&t.from, &t.to]) {
        if !is_registered_in_tx(tx, Chain::Terra, &wallet).await? {
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
    for t in transfers {
        persist_bsc_leg(&mut tx, t).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn persist_bsc_leg(
    tx: &mut Transaction<'_, Postgres>,
    t: &Bep20Transfer,
) -> LedgerResult<()> {
    sqlx::query(
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

    for wallet in unique_nonempty(&[&t.from, &t.to]) {
        if !is_registered_in_tx(tx, Chain::Bsc, &wallet).await? {
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
) -> LedgerResult<bool> {
    let found: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM voting_registrations WHERE chain = $1 AND wallet_address = $2 AND status = 'active'",
    )
    .bind(chain.as_str())
    .bind(wallet)
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
    let v: Option<String> =
        sqlx::query_scalar("SELECT value FROM indexer_state WHERE key = $1")
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

async fn set_state_in_tx(
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
    Ok(get_state(pool, "last_indexed_height")
        .await?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0))
}

pub async fn indexed_bsc_block(pool: &PgPool) -> LedgerResult<i64> {
    Ok(get_state(pool, "last_indexed_bsc_block")
        .await?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0))
}

pub async fn mark_intent_processed(pool: &PgPool, id: uuid::Uuid) -> LedgerResult<()> {
    sqlx::query("UPDATE voting.registration_intents SET processed_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
        .or_else(|e| {
            // voting schema may not exist until operator-voting migrations run.
            if e.to_string().contains("voting.registration_intents") {
                Ok(())
            } else {
                Err(LedgerError::Database(e))
            }
        })
}

pub async fn pending_intents(pool: &PgPool) -> LedgerResult<Vec<(uuid::Uuid, String, String, Option<uuid::Uuid>)>> {
    let rows = sqlx::query_as::<_, (uuid::Uuid, String, String, Option<uuid::Uuid>)>(
        r#"
        SELECT id, chain, wallet_address, signature_id
        FROM voting.registration_intents
        WHERE processed_at IS NULL
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await;
    match rows {
        Ok(v) => Ok(v),
        Err(e) if e.to_string().contains("voting.registration_intents") => Ok(vec![]),
        Err(e) => Err(e.into()),
    }
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
