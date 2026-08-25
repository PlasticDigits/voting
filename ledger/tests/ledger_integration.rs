//! Postgres integration: register → transfer → balance_at (no archive header).
//! Run with `LEDGER_TEST_DATABASE_URL` (see docker-compose.test.yml).

use async_trait::async_trait;
use num_bigint::BigInt;
use sqlx::PgPool;
use voting_ledger::amount::human_to_raw;
use voting_ledger::db::{self, Chain};
use voting_ledger::parser::{apply_transfer, Cw20Transfer};
use voting_ledger::register::{register_wallet, LiveBalanceSource};
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
    let lock = voting_ledger::test_lock::hold_integration_db(&url).await.ok()?;
    let pool = PgPool::connect(&url).await.ok()?;
    db::migrate(&pool).await.ok()?;
    sqlx::query("TRUNCATE voting_registrations, cl8y_cw20_transfers, cl8y_balances, cl8y_bep20_transfers, cl8y_bsc_balances CASCADE")
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

    assert_eq!(db::balance_at(&pool, Chain::Terra, alice, 99).await.unwrap(), BigInt::from(0));
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, alice, 100).await.unwrap(),
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
        db::balance_at(&pool, Chain::Terra, alice, 110).await.unwrap(),
        human_to_raw(40)
    );
    // Bob is only a counterparty — not a voter until he registers (no backfill).
    assert_eq!(db::balance_at(&pool, Chain::Terra, bob, 110).await.unwrap(), BigInt::from(0));

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
    assert_eq!(db::balance_at(&pool, Chain::Terra, bob, 110).await.unwrap(), BigInt::from(0));

    db::rewind_terra(&pool, 100).await.unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, alice, 200).await.unwrap(),
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
        db::balance_at(&pool, Chain::Bsc, "0x3333333333333333333333333333333333333333", 1_000)
            .await
            .unwrap(),
        BigInt::from(0)
    );
    assert_eq!(db::balance_at(&pool, Chain::Terra, evm, 50).await.unwrap(), BigInt::from(0));
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, terra, 50).await.unwrap(),
        human_to_raw(9)
    );
}

#[test]
fn apply_transfer_math_unit() {
    let w = "terra1alice";
    let mut b = human_to_raw(100);
    b = apply_transfer(&b, w, w, "terra1bob", &human_to_raw(40));
    assert_eq!(b, human_to_raw(60));
}
