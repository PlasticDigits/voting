//! Bounded, read-only metadata inspection before migrations or accounting workers.
use crate::{error::LedgerResult, LedgerError};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::time::Duration;

const TABLES: &[(&str, &[&str])] = &[
    ("indexer_state", &["key", "value", "updated_at"]),
    (
        "voting_registrations",
        &[
            "chain",
            "wallet_address",
            "registered_at_height",
            "registered_at_time",
            "initial_balance",
            "status",
            "signature_id",
            "baseline_height",
            "baseline_block_hash",
            "baseline_epoch",
            "baseline_verified",
        ],
    ),
    (
        "cl8y_cw20_transfers",
        &[
            "id",
            "height",
            "tx_hash",
            "msg_index",
            "from_address",
            "to_address",
            "amount",
            "action",
        ],
    ),
    ("cl8y_balances", &["wallet_address", "height", "balance"]),
    (
        "cl8y_bep20_transfers",
        &[
            "id",
            "bsc_block",
            "tx_hash",
            "log_index",
            "from_address",
            "to_address",
            "amount",
        ],
    ),
    (
        "cl8y_bsc_balances",
        &["wallet_address", "bsc_block", "balance"],
    ),
    ("_sqlx_migrations", &["version", "success", "checksum"]),
];

pub fn requested(raw: Option<&str>) -> LedgerResult<bool> {
    match raw {
        None | Some("false" | "0") => Ok(false),
        Some("true" | "1") => Ok(true),
        _ => Err(LedgerError::InvalidConfig(
            "LEDGER_INSPECT_STORAGE_ONLY must be true or false".into(),
        )),
    }
}

pub fn requested_from_env() -> LedgerResult<bool> {
    match std::env::var("LEDGER_INSPECT_STORAGE_ONLY") {
        Ok(raw) => requested(Some(&raw)),
        Err(std::env::VarError::NotPresent) => requested(None),
        Err(_) => Err(LedgerError::InvalidConfig(
            "LEDGER_INSPECT_STORAGE_ONLY must be true or false".into(),
        )),
    }
}

#[derive(Debug, Serialize)]
pub struct Inventory {
    pub read_only: bool,
    pub relations: Vec<RelationInventory>,
    pub comparisons: Vec<Comparison>,
}

#[derive(Debug, Serialize)]
pub struct RelationInventory {
    pub schema: &'static str,
    pub table: &'static str,
    pub exists: bool,
    pub row_count: Option<i64>,
    pub baseline_height_present: bool,
    pub last_indexed_height: Option<i64>,
    pub last_indexed_bsc_block: Option<i64>,
    pub migration_max_version: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct Comparison {
    pub table: &'static str,
    pub common_columns: Vec<&'static str>,
    pub public_only_rows: Option<i64>,
    pub voting_only_rows: Option<i64>,
}

// A failed bounded query cannot poison the surrounding read-only inventory transaction.
// None means unavailable/timed out, never zero or proof of equality.
async fn bounded_row(
    tx: &mut Transaction<'_, Postgres>,
    query: &str,
) -> LedgerResult<Option<sqlx::postgres::PgRow>> {
    sqlx::query("SAVEPOINT inventory_query")
        .execute(&mut **tx)
        .await?;
    let row = sqlx::query(query).fetch_one(&mut **tx).await;
    if row.is_err() {
        sqlx::query("ROLLBACK TO SAVEPOINT inventory_query")
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query("RELEASE SAVEPOINT inventory_query")
        .execute(&mut **tx)
        .await?;
    Ok(row.ok())
}

async fn inspect(pool: &PgPool) -> LedgerResult<Inventory> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='1s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL lock_timeout='500ms'")
        .execute(&mut *tx)
        .await?;
    let read_only: String = sqlx::query_scalar("SHOW transaction_read_only")
        .fetch_one(&mut *tx)
        .await?;
    if read_only != "on" {
        return Err(LedgerError::Provider("storage_inspection_not_readonly"));
    }
    let mut inventory = Inventory {
        read_only: true,
        relations: Vec::new(),
        comparisons: Vec::new(),
    };
    let mut columns = std::collections::HashMap::new();
    for schema in ["public", "voting"] {
        for &(table, _) in TABLES {
            // Both identifier components come from the fixed allowlist above.
            let name = format!("{schema}.{table}");
            let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname=$2 AND c.relkind IN ('r','p'))")
                .bind(schema).bind(table).fetch_one(&mut *tx).await?;
            let names: Vec<String> = if exists {
                sqlx::query_scalar("SELECT attname::text FROM pg_catalog.pg_attribute WHERE attrelid=pg_catalog.to_regclass($1) AND attnum>0 AND NOT attisdropped")
                    .bind(&name).fetch_all(&mut *tx).await?
            } else {
                Vec::new()
            };
            let mut relation = RelationInventory {
                schema,
                table,
                exists,
                row_count: None,
                baseline_height_present: names.iter().any(|n| n == "baseline_height"),
                last_indexed_height: None,
                last_indexed_bsc_block: None,
                migration_max_version: None,
            };
            if exists {
                relation.row_count =
                    bounded_row(&mut tx, &format!("SELECT COUNT(*) AS value FROM {name}"))
                        .await?
                        .map(|r| r.try_get("value"))
                        .transpose()?;
                if table == "indexer_state"
                    && names.iter().any(|n| n == "key")
                    && names.iter().any(|n| n == "value")
                {
                    for (key, output) in [
                        ("last_indexed_height", &mut relation.last_indexed_height),
                        (
                            "last_indexed_bsc_block",
                            &mut relation.last_indexed_bsc_block,
                        ),
                    ] {
                        if let Some(row) = bounded_row(
                            &mut tx,
                            &format!("SELECT value FROM {name} WHERE key='{key}' LIMIT 1"),
                        )
                        .await?
                        {
                            // Emit only validated nonnegative numeric cursors, never arbitrary state values.
                            let value: String = row.try_get("value")?;
                            *output = value.parse::<i64>().ok().filter(|n| *n >= 0);
                        }
                    }
                }
                if table == "_sqlx_migrations" && names.iter().any(|n| n == "version") {
                    relation.migration_max_version = bounded_row(
                        &mut tx,
                        &format!("SELECT MAX(version)::bigint AS value FROM {name}"),
                    )
                    .await?
                    .map(|r| r.try_get("value"))
                    .transpose()?
                    .flatten();
                }
            }
            columns.insert((schema, table), names);
            inventory.relations.push(relation);
        }
    }
    for &(table, allowed) in TABLES {
        let left = &columns[&("public", table)];
        let right = &columns[&("voting", table)];
        let common: Vec<_> = allowed
            .iter()
            .copied()
            .filter(|c| left.iter().any(|n| n == c) && right.iter().any(|n| n == c))
            .collect();
        if common.is_empty() {
            continue;
        }
        let projection = common
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(",");
        let query=format!("SELECT (SELECT COUNT(*) FROM (SELECT {projection} FROM public.{table} EXCEPT ALL SELECT {projection} FROM voting.{table}) a) AS public_only, (SELECT COUNT(*) FROM (SELECT {projection} FROM voting.{table} EXCEPT ALL SELECT {projection} FROM public.{table}) b) AS voting_only");
        let counts = bounded_row(&mut tx, &query).await?;
        inventory.comparisons.push(Comparison {
            table,
            common_columns: common,
            public_only_rows: counts
                .as_ref()
                .map(|r| r.try_get("public_only"))
                .transpose()?,
            voting_only_rows: counts
                .as_ref()
                .map(|r| r.try_get("voting_only"))
                .transpose()?,
        });
    }
    tx.rollback().await?;
    Ok(inventory)
}

pub async fn inventory(pool: &PgPool) -> LedgerResult<Inventory> {
    tokio::time::timeout(Duration::from_secs(45), inspect(pool))
        .await
        .map_err(|_| LedgerError::Provider("storage_inspection_deadline"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspection_flag_is_explicit_and_strict() {
        assert!(!requested(None).unwrap());
        for raw in ["false", "0"] {
            assert!(!requested(Some(raw)).unwrap());
        }
        for raw in ["true", "1"] {
            assert!(requested(Some(raw)).unwrap());
        }
        for raw in ["", "yes", "TRUE", "true "] {
            assert!(requested(Some(raw)).is_err());
        }
    }
}
