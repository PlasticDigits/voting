//! Cross-binary lock for Postgres integration tests.
//!
//! `cargo test --workspace --tests` runs crate test binaries in parallel.
//! Those binaries `TRUNCATE` the same tables. Hold this lock for the whole
//! test so one binary cannot wipe another's rows (issue #7 verify flake).
//!
//! Host `127.0.0.1:5433` (docker-proxy) can hang or RST after a long session.
//! Connect calls here use an 8s timeout and a few retries so a dead proxy
//! fails closed instead of blocking `cargo test` indefinitely.

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};
use std::str::FromStr;
use std::time::Duration;

/// Session-scoped `pg_advisory_lock`. Dropping the connection releases it.
pub struct IntegrationDbLock {
    _conn: PgConnection,
}

const LOCK_KEY: i64 = 739_001;
pub const TEST_DATABASE_MARKER: &str = "cl8y-voting-ephemeral-test";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// Offline refusal happens before opening a socket. Only explicit disposable
/// loopback databases are accepted; application DATABASE_URL is never used.
pub fn validate_test_url(url: &str, destructive_opt_in: bool) -> Result<String, sqlx::Error> {
    let refuse = || sqlx::Error::Protocol("unsafe integration database configuration".into());
    if !destructive_opt_in {
        return Err(refuse());
    }
    let opts = PgConnectOptions::from_str(url).map_err(|_| refuse())?;
    if !matches!(opts.get_host(), "127.0.0.1" | "localhost" | "::1")
        && !(opts.get_host() == "postgres"
            && std::env::var("CI").as_deref() == Ok("true")
            && std::env::var("LEDGER_TEST_ALLOW_CI_SERVICE").as_deref() == Ok("1"))
    {
        return Err(refuse());
    }
    let name = opts.get_database().ok_or_else(refuse)?;
    if !name.starts_with("voting_test_") || name.len() <= 12 {
        return Err(refuse());
    }
    Ok(name.to_owned())
}

pub fn test_database_url() -> Option<String> {
    match std::env::var("LEDGER_TEST_DATABASE_URL") {
        Ok(url) => {
            validate_test_url(
                &url,
                std::env::var("LEDGER_TEST_ALLOW_DESTRUCTIVE").as_deref() == Ok("1"),
            )
            .expect("explicit guarded ephemeral test database required");
            Some(url)
        }
        Err(_) if std::env::var("LEDGER_TEST_REQUIRED").as_deref() == Ok("1") => {
            panic!("required integration job needs LEDGER_TEST_DATABASE_URL")
        }
        Err(_) => None,
    }
}

fn guarded_name(url: &str) -> Result<String, sqlx::Error> {
    validate_test_url(
        url,
        std::env::var("LEDGER_TEST_ALLOW_DESTRUCTIVE").as_deref() == Ok("1"),
    )
}

fn timed_out() -> sqlx::Error {
    sqlx::Error::Protocol("postgres connect timed out (8s)".into())
}

async fn connect_and_lock(url: &str) -> Result<IntegrationDbLock, sqlx::Error> {
    let expected = guarded_name(url)?;
    let mut conn = tokio::time::timeout(CONNECT_TIMEOUT, PgConnection::connect(url))
        .await
        .map_err(|_| timed_out())??;
    let (actual,marker): (String,String) = sqlx::query_as("SELECT current_database(),COALESCE(shobj_description(oid,'pg_database'),'') FROM pg_database WHERE datname=current_database()")
        .fetch_one(&mut conn).await?;
    if actual != expected || marker != TEST_DATABASE_MARKER {
        return Err(sqlx::Error::Protocol(
            "integration database identity mismatch".into(),
        ));
    }
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
            Err(sqlx::Error::Protocol(message))
                if message == "integration database identity mismatch" =>
            {
                return Err(sqlx::Error::Protocol(message))
            }
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
    let expected = guarded_name(url)?;
    let pool = tokio::time::timeout(
        CONNECT_TIMEOUT,
        PgPoolOptions::new()
            .acquire_timeout(CONNECT_TIMEOUT)
            .connect(url),
    )
    .await
    .map_err(|_| timed_out())??;
    let (actual,marker): (String,String) = sqlx::query_as("SELECT current_database(),COALESCE(shobj_description(oid,'pg_database'),'') FROM pg_database WHERE datname=current_database()")
        .fetch_one(&pool).await?;
    if actual != expected || marker != TEST_DATABASE_MARKER {
        pool.close().await;
        return Err(sqlx::Error::Protocol(
            "integration database identity mismatch".into(),
        ));
    }
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destructive_guard_refuses_offline() {
        for url in [
            "postgres://user:secret@prod.cl8y.com/voting_test_x",
            "postgres://u:p@127.0.0.1/voting",
            "postgres://u:p@127.0.0.1/voting_test_",
            "invalid",
        ] {
            assert!(validate_test_url(url, true).is_err());
        }
        let safe = "postgres://u:p@127.0.0.1/voting_test_isolated";
        assert!(validate_test_url(safe, false).is_err());
        assert_eq!(
            validate_test_url(safe, true).unwrap(),
            "voting_test_isolated"
        );
    }
}
