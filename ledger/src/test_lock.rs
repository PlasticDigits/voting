//! Cross-binary lock for Postgres integration tests.
//!
//! `cargo test --workspace --tests` runs crate test binaries in parallel.
//! Those binaries `TRUNCATE` the same tables. Hold this lock for the whole
//! test so one binary cannot wipe another's rows (issue #7 verify flake).

use sqlx::{Connection, PgConnection};

/// Session-scoped `pg_advisory_lock`. Dropping the connection releases it.
pub struct IntegrationDbLock {
    _conn: PgConnection,
}

const LOCK_KEY: i64 = 739_001;

pub async fn hold_integration_db(url: &str) -> Result<IntegrationDbLock, sqlx::Error> {
    let mut conn = PgConnection::connect(url).await?;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(LOCK_KEY)
        .execute(&mut conn)
        .await?;
    Ok(IntegrationDbLock { _conn: conn })
}
