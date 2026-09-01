//! Cross-binary lock for Postgres integration tests.
//!
//! `cargo test --workspace --tests` runs crate test binaries in parallel.
//! Those binaries `TRUNCATE` the same tables. Hold this lock for the whole
//! test so one binary cannot wipe another's rows (issue #7 verify flake).
//!
//! Host `127.0.0.1:5433` (docker-proxy) can hang or RST after a long session.
//! Connect calls here use an 8s timeout and a few retries so a dead proxy
//! fails closed instead of blocking `cargo test` indefinitely.

use sqlx::postgres::PgPoolOptions;
use sqlx::{Connection, PgConnection, PgPool};
use std::time::Duration;

/// Session-scoped `pg_advisory_lock`. Dropping the connection releases it.
pub struct IntegrationDbLock {
    _conn: PgConnection,
}

const LOCK_KEY: i64 = 739_001;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

fn timed_out() -> sqlx::Error {
    sqlx::Error::Protocol("postgres connect timed out (8s)".into())
}

async fn connect_and_lock(url: &str) -> Result<IntegrationDbLock, sqlx::Error> {
    let mut conn = tokio::time::timeout(CONNECT_TIMEOUT, PgConnection::connect(url))
        .await
        .map_err(|_| timed_out())??;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(LOCK_KEY)
        .execute(&mut conn)
        .await?;
    Ok(IntegrationDbLock { _conn: conn })
}

/// Retry connect: docker-proxy on the published 5433 port can RST after a long
/// migrate/truncate session. The lock itself is still required (cross-binary TRUNCATE).
pub async fn hold_integration_db(url: &str) -> Result<IntegrationDbLock, sqlx::Error> {
    let mut last = None;
    for attempt in 0..5 {
        match connect_and_lock(url).await {
            Ok(lock) => return Ok(lock),
            Err(err) => {
                last = Some(err);
                tokio::time::sleep(Duration::from_millis(250 * (attempt + 1) as u64)).await;
            }
        }
    }
    Err(last.expect("at least one connect attempt"))
}

/// Test pool with the same connect timeout as [`hold_integration_db`].
pub async fn test_pool(url: &str) -> Result<PgPool, sqlx::Error> {
    tokio::time::timeout(
        CONNECT_TIMEOUT,
        PgPoolOptions::new()
            .acquire_timeout(CONNECT_TIMEOUT)
            .connect(url),
    )
    .await
    .map_err(|_| timed_out())?
}
