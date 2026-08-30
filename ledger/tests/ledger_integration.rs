//! Postgres integration: register → transfer → balance_at (no archive header).
//! Run with `LEDGER_TEST_DATABASE_URL` (see docker-compose.test.yml).

use async_trait::async_trait;
use num_bigint::BigInt;
use sqlx::PgPool;
use voting_ledger::amount::human_to_raw;
use voting_ledger::db::{self, Chain};
use voting_ledger::parser::{apply_transfer, Cw20Transfer};
use voting_ledger::register::{process_pending_intents, register_wallet, LiveBalanceSource};
use voting_ledger::LedgerError;

struct MockLive {
    terra: (i64, BigInt),
    bsc: (i64, BigInt),
}

#[async_trait]
impl LiveBalanceSource for MockLive {
    async fn terra_live_balance(&self, _address: &str) -> Result<(i64, BigInt), LedgerError> {
        Ok(self.terra.clone())
    }
    async fn bsc_live_balance(&self, _address: &str) -> Result<(i64, BigInt), LedgerError> {
        Ok(self.bsc.clone())
    }
}

fn test_db_url() -> Option<String> {
    std::env::var("LEDGER_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()
        .filter(|s| !s.is_empty())
}

async fn setup() -> Option<(PgPool, voting_ledger::test_lock::IntegrationDbLock)> {
    let url = test_db_url()?;
    let lock = voting_ledger::test_lock::hold_integration_db(&url)
        .await
        .ok()?;
    let pool = PgPool::connect(&url).await.ok()?;
    db::migrate(&pool).await.ok()?;
    sqlx::query("TRUNCATE voting_registrations, cl8y_cw20_transfers, cl8y_balances, cl8y_bep20_transfers, cl8y_bsc_balances, voting.registration_intents, voting.signatures CASCADE")
        .execute(&pool)
        .await
        .ok()?;
    Some((pool, lock))
}

#[tokio::test]
async fn register_transfer_balance_at_and_no_backfill() {
    let Some((pool, _lock)) = setup().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let alice = "terra1alice00000000000000000000000000000000";
    let bob = "terra1bob0000000000000000000000000000000000";
    let src = MockLive {
        terra: (100, human_to_raw(50)),
        bsc: (1, human_to_raw(0)),
    };
    let out = register_wallet(&pool, &src, Chain::Terra, alice, None)
        .await
        .unwrap();
    assert!(out.inserted);
    assert_eq!(out.height, 100);

    let again = register_wallet(&pool, &src, Chain::Terra, alice, None)
        .await
        .unwrap();
    assert!(!again.inserted);
    assert_eq!(again.initial_balance, out.initial_balance);

    assert_eq!(
        db::balance_at(&pool, Chain::Terra, alice, 99)
            .await
            .unwrap(),
        BigInt::from(0)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, alice, 100)
            .await
            .unwrap(),
        human_to_raw(50)
    );

    let t1 = Cw20Transfer {
        contract: "terra16wtml2q66g82fdkx66tap0qjkahqwp4lwq3ngtygacg5q0kzycgqvhpax3".into(),
        from: alice.into(),
        to: bob.into(),
        amount: human_to_raw(10),
        action: "transfer".into(),
    };
    db::apply_cw20_transfers(&pool, &[t1], 110, "hash1")
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, alice, 110)
            .await
            .unwrap(),
        human_to_raw(40)
    );
    // Bob is only a counterparty — not a voter until he registers (no backfill).
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, bob, 110).await.unwrap(),
        BigInt::from(0)
    );

    let src_bob = MockLive {
        terra: (200, human_to_raw(7)),
        bsc: (1, human_to_raw(0)),
    };
    register_wallet(&pool, &src_bob, Chain::Terra, bob, None)
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, bob, 200).await.unwrap(),
        human_to_raw(7)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, bob, 110).await.unwrap(),
        BigInt::from(0)
    );

    db::rewind_terra(&pool, 100).await.unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, alice, 200)
            .await
            .unwrap(),
        human_to_raw(50)
    );
}

#[tokio::test]
async fn bsc_register_and_isolation() {
    let Some((pool, _lock)) = setup().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let evm = "0x1111111111111111111111111111111111111111";
    let terra = "terra1alice00000000000000000000000000000000";
    let src = MockLive {
        terra: (50, human_to_raw(9)),
        bsc: (1_000, human_to_raw(80)),
    };
    register_wallet(&pool, &src, Chain::Bsc, evm, None)
        .await
        .unwrap();
    register_wallet(&pool, &src, Chain::Terra, terra, None)
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, evm, 1_000).await.unwrap(),
        human_to_raw(80)
    );
    assert_eq!(
        db::balance_at(
            &pool,
            Chain::Bsc,
            "0x3333333333333333333333333333333333333333",
            1_000
        )
        .await
        .unwrap(),
        BigInt::from(0)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, evm, 50).await.unwrap(),
        BigInt::from(0)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, terra, 50)
            .await
            .unwrap(),
        human_to_raw(9)
    );
}

struct FailLive;

#[async_trait]
impl LiveBalanceSource for FailLive {
    async fn terra_live_balance(&self, _address: &str) -> Result<(i64, BigInt), LedgerError> {
        Err(LedgerError::Lcd("timeout".into()))
    }
    async fn bsc_live_balance(&self, _address: &str) -> Result<(i64, BigInt), LedgerError> {
        Err(LedgerError::BscRpc("timeout".into()))
    }
}

#[tokio::test]
async fn live_balance_failure_does_not_invent_zero() {
    let Some((pool, _lock)) = setup().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let alice = "terra1alice00000000000000000000000000000000";
    let err = register_wallet(&pool, &FailLive, Chain::Terra, alice, None)
        .await
        .unwrap_err();
    assert!(matches!(err, LedgerError::Lcd(_)));
    assert!(db::get_registration(&pool, Chain::Terra, alice)
        .await
        .unwrap()
        .is_none());
}

async fn enqueue_intent(pool: &PgPool, chain: &str, wallet: &str) -> sqlx::types::Uuid {
    let sig_id = sqlx::query_scalar::<_, sqlx::types::Uuid>(
        r#"
        INSERT INTO voting.signatures (id, chain, wallet_address, signature, payload_hash, purpose)
        VALUES (gen_random_uuid(), $1, $2, 'sig', 'hash', 'register')
        RETURNING id
        "#,
    )
    .bind(chain)
    .bind(wallet)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query_scalar::<_, sqlx::types::Uuid>(
        r#"
        INSERT INTO voting.registration_intents (chain, wallet_address, signature_id)
        VALUES ($1, $2, $3)
        RETURNING id
        "#,
    )
    .bind(chain)
    .bind(wallet)
    .bind(sig_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn intent_processed(pool: &PgPool, id: sqlx::types::Uuid) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT processed_at IS NOT NULL FROM voting.registration_intents WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn pending_intent_becomes_snapshot_without_inventing_zero() {
    let Some((pool, _lock)) = setup().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let terra = "terra1alice00000000000000000000000000000000";
    let evm = "0x1111111111111111111111111111111111111111";
    let terra_id = enqueue_intent(&pool, "terra", terra).await;
    let bsc_id = enqueue_intent(&pool, "bsc", evm).await;
    let src = MockLive {
        terra: (100, human_to_raw(50)),
        bsc: (1_000, human_to_raw(80)),
    };
    let out = process_pending_intents(&pool, &src).await.unwrap();
    assert_eq!(out.len(), 2);
    assert!(intent_processed(&pool, terra_id).await);
    assert!(intent_processed(&pool, bsc_id).await);
    assert_eq!(
        db::get_registration(&pool, Chain::Terra, terra)
            .await
            .unwrap()
            .unwrap()
            .initial_balance,
        human_to_raw(50).to_string()
    );
    assert_eq!(
        db::get_registration(&pool, Chain::Bsc, evm)
            .await
            .unwrap()
            .unwrap()
            .initial_balance,
        human_to_raw(80).to_string()
    );

    sqlx::query("UPDATE voting.registration_intents SET processed_at = NULL WHERE id = $1")
        .bind(terra_id)
        .execute(&pool)
        .await
        .unwrap();
    let out2 = process_pending_intents(&pool, &src).await.unwrap();
    assert_eq!(out2.len(), 1);
    assert!(!out2[0].inserted);
    assert_eq!(out2[0].initial_balance, human_to_raw(50).to_string());
    assert!(intent_processed(&pool, terra_id).await);
}

#[tokio::test]
async fn pending_intent_survives_live_balance_failure() {
    let Some((pool, _lock)) = setup().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let alice = "terra1alice00000000000000000000000000000000";
    let id = enqueue_intent(&pool, "terra", alice).await;
    let out = process_pending_intents(&pool, &FailLive).await.unwrap();
    assert!(out.is_empty());
    assert!(!intent_processed(&pool, id).await);
    assert!(db::get_registration(&pool, Chain::Terra, alice)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn pending_intents_permission_failure_is_not_empty_queue() {
    let Some((pool, _lock)) = setup().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let url = test_db_url().unwrap();
    let dbname: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"
        DO $$
        BEGIN
          IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'voting_l12_denied') THEN
            CREATE ROLE voting_l12_denied LOGIN PASSWORD 'l12-denied';
          END IF;
        END
        $$;
        "#,
    )
    .execute(&pool)
    .await
    .expect("create denied role");
    sqlx::query(&format!(
        "GRANT CONNECT ON DATABASE \"{dbname}\" TO voting_l12_denied"
    ))
    .execute(&pool)
    .await
    .expect("grant connect");
    sqlx::query("GRANT USAGE ON SCHEMA voting TO voting_l12_denied")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("REVOKE ALL ON TABLE voting.registration_intents FROM voting_l12_denied")
        .execute(&pool)
        .await
        .ok();

    let denied_url = rewrite_user(&url, "voting_l12_denied", "l12-denied");
    let denied = PgPool::connect(&denied_url)
        .await
        .expect("denied role login");
    let err = db::pending_intents(&denied).await.unwrap_err();
    assert!(
        matches!(err, LedgerError::Database(_)),
        "permission/query failure must not look like an empty queue: {err:?}"
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

#[test]
fn apply_transfer_math_unit() {
    let w = "terra1alice";
    let mut b = human_to_raw(100);
    b = apply_transfer(&b, w, w, "terra1bob", &human_to_raw(40));
    assert_eq!(b, human_to_raw(60));
}
