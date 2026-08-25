//! Apply `deploy/grants.sql` (O1 / L10) and assert the restricted role
//! can read balance functions + write `voting.*` but cannot write ledger ingest.

use sqlx::PgPool;

fn test_db_url() -> Option<String> {
    std::env::var("LEDGER_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()
        .filter(|s| !s.is_empty())
}

const GRANTS_SQL: &str = include_str!("../../deploy/grants.sql");

#[test]
fn grants_sql_is_least_privilege() {
    assert!(
        GRANTS_SQL.contains("REVOKE INSERT, UPDATE, DELETE, TRUNCATE"),
        "Coolify grants must revoke ledger writes"
    );
    for table in [
        "voting_registrations",
        "cl8y_balances",
        "cl8y_bsc_balances",
        "cl8y_cw20_transfers",
        "cl8y_bep20_transfers",
        "indexer_state",
    ] {
        assert!(GRANTS_SQL.contains(table), "grants.sql must mention {table}");
    }
    assert!(
        !GRANTS_SQL.contains("GRANT ALL ON SCHEMA voting"),
        "CREATE on schema voting is not least privilege (USAGE only)"
    );
    assert!(
        GRANTS_SQL.contains("current_database()"),
        "CONNECT grant must follow the database the file is applied to"
    );
    assert!(
        GRANTS_SQL.contains("public.voting_cl8y_balance_at"),
        "EXECUTE must target public.* (role voting + schema voting shadows search_path)"
    );
}

#[tokio::test]
async fn restricted_role_from_grants_sql_cannot_write_ledger() {
    let Some(url) = test_db_url() else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let _lock = voting_ledger::test_lock::hold_integration_db(&url)
        .await
        .expect("advisory lock");
    let admin = PgPool::connect(&url).await.unwrap();
    voting_ledger::db::migrate(&admin).await.unwrap();

    // Coolify applies this file with psql (DO blocks + multiple GRANTs).
    // sqlx pool execute / raw_sql is not that path and can stop after the first statement.
    apply_grants_sql(&url);

    let restricted_url = rewrite_user(&url, "operator_voting", "change-me-in-prod");
    let restricted = PgPool::connect(&restricted_url)
        .await
        .expect("operator_voting login from deploy/grants.sql");

    let read = sqlx::query_scalar::<_, String>("SELECT public.voting_cl8y_balance_at('terra1missing', 1)::text")
        .fetch_one(&restricted)
        .await;
    assert!(read.is_ok(), "restricted role must EXECUTE balance functions: {read:?}");

    let write_bal = sqlx::query(
        "INSERT INTO cl8y_balances (wallet_address, height, balance) VALUES ('x', 1, 1)",
    )
    .execute(&restricted)
    .await;
    assert!(write_bal.is_err(), "restricted role must not write cl8y_balances");

    let write_xfer = sqlx::query(
        "INSERT INTO cl8y_cw20_transfers (height, tx_hash, from_address, to_address, amount, action) VALUES (1, 'h', 'a', 'b', 1, 'transfer')",
    )
    .execute(&restricted)
    .await;
    assert!(write_xfer.is_err(), "restricted role must not write cl8y_cw20_transfers");

    let write_state = sqlx::query(
        "UPDATE indexer_state SET value = '9' WHERE key = 'last_indexed_height'",
    )
    .execute(&restricted)
    .await;
    assert!(write_state.is_err(), "restricted role must not write indexer_state");

    let write_reg = sqlx::query(
        "INSERT INTO voting_registrations (chain, wallet_address, registered_at_height, initial_balance) VALUES ('terra', 'terra1x', 1, 1)",
    )
    .execute(&restricted)
    .await;
    assert!(write_reg.is_err(), "restricted role must not insert voting_registrations");

    let sig_ok = sqlx::query(
        r#"
        INSERT INTO voting.signatures (id, chain, wallet_address, signature, payload_hash, purpose)
        VALUES (gen_random_uuid(), 'terra', 'terra1test', 'sig', 'hash', 'register')
        "#,
    )
    .execute(&restricted)
    .await;
    assert!(sig_ok.is_ok(), "restricted role must write voting.signatures: {sig_ok:?}");

    let create = sqlx::query("CREATE TABLE voting.should_not_exist (id int)")
        .execute(&restricted)
        .await;
    assert!(create.is_err(), "restricted role must not CREATE in schema voting");
}

fn apply_grants_sql(url: &str) {
    let output = std::process::Command::new("psql")
        .args(["-v", "ON_ERROR_STOP=1", "--no-psqlrc", "-X", "-d", url])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .expect("psql stdin")
                .write_all(GRANTS_SQL.as_bytes())?;
            child.wait_with_output()
        })
        .expect("psql must be on PATH to apply deploy/grants.sql (postgresql-client)");
    assert!(
        output.status.success(),
        "psql deploy/grants.sql failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn rewrite_user(url: &str, user: &str, pass: &str) -> String {
    if let Some(rest) = url
        .strip_prefix("postgres://")
        .or_else(|| url.strip_prefix("postgresql://"))
    {
        if let Some(at) = rest.find('@') {
            let after_at = &rest[at..];
            return format!("postgresql://{user}:{pass}{after_at}");
        }
    }
    url.to_string()
}
