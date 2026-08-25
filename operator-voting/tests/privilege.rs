//! Restricted role can read balance functions and cannot write ledger tables.

use sqlx::PgPool;

fn test_db_url() -> Option<String> {
    std::env::var("LEDGER_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()
        .filter(|s| !s.is_empty())
}

#[tokio::test]
async fn restricted_role_cannot_write_ledger() {
    let Some(url) = test_db_url() else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let _lock = voting_ledger::test_lock::hold_integration_db(&url)
        .await
        .expect("advisory lock");
    let admin = PgPool::connect(&url).await.unwrap();
    voting_ledger::db::migrate(&admin).await.unwrap();
    operator_voting::db::migrate(&admin).await.unwrap();

    sqlx::query(
        r#"
        DO $$
        BEGIN
          IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'operator_voting_test') THEN
            CREATE ROLE operator_voting_test LOGIN PASSWORD 'operator_voting_test';
          END IF;
        END
        $$;
        "#,
    )
    .execute(&admin)
    .await
    .unwrap();

    sqlx::query("GRANT USAGE ON SCHEMA public TO operator_voting_test")
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("GRANT USAGE ON SCHEMA voting TO operator_voting_test")
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO operator_voting_test")
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("GRANT ALL ON ALL TABLES IN SCHEMA voting TO operator_voting_test")
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON voting_registrations, cl8y_balances, cl8y_bsc_balances FROM operator_voting_test")
        .execute(&admin)
        .await
        .unwrap();

    let restricted_url = rewrite_user(&url, "operator_voting_test", "operator_voting_test");
    let restricted = match PgPool::connect(&restricted_url).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skip privilege login: {e}");
            return;
        }
    };

    let read = sqlx::query_scalar::<_, String>("SELECT voting_cl8y_balance_at('terra1missing', 1)::text")
        .fetch_one(&restricted)
        .await;
    assert!(read.is_ok(), "{read:?}");

    let write = sqlx::query(
        "INSERT INTO cl8y_balances (wallet_address, height, balance) VALUES ('x', 1, 1)",
    )
    .execute(&restricted)
    .await;
    assert!(write.is_err(), "restricted role must not write ledger tables");
}

fn rewrite_user(url: &str, user: &str, pass: &str) -> String {
    // postgresql://user:pass@host/db → swap user/pass
    if let Some(rest) = url.strip_prefix("postgres://").or_else(|| url.strip_prefix("postgresql://")) {
        if let Some(at) = rest.find('@') {
            let after_at = &rest[at..];
            return format!("postgresql://{user}:{pass}{after_at}");
        }
    }
    url.to_string()
}
