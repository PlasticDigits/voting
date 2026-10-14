//! Recorded schema shape and injected HTTP failures, with isolated Postgres.
use axum::{
    extract::{OriginalUri, State},
    routing::get,
    Json, Router,
};
use num_bigint::BigInt;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use voting_ledger::{
    config::{LedgerConfig, RunMode, DEFAULT_CL8Y_BEP20, DEFAULT_CL8Y_CW20},
    db::{self, Chain},
    ingest,
    lcd::LcdClient,
    parser::{Bep20Transfer, Cw20Transfer},
    LedgerError,
};
const ALICE: &str = "terra1alice";
const EVM: &str = "0x1111111111111111111111111111111111111111";
async fn setup() -> Option<(sqlx::PgPool, voting_ledger::test_lock::IntegrationDbLock)> {
    let url = std::env::var("LEDGER_TEST_DATABASE_URL").ok()?;
    assert_eq!(
        std::env::var("LEDGER_TEST_ALLOW_DESTRUCTIVE").as_deref(),
        Ok("1")
    );
    assert!(url.contains("127.0.0.1:") && url.contains("/voting_test_"));
    let lock = voting_ledger::test_lock::hold_integration_db(&url)
        .await
        .unwrap();
    let pool = voting_ledger::test_lock::test_pool(&url).await.unwrap();
    db::migrate(&pool).await.unwrap();
    sqlx::query("TRUNCATE indexer_state,voting.registration_intents,voting.signatures,voting.proposals,voting_registrations,cl8y_cw20_transfers,cl8y_balances,cl8y_bep20_transfers,cl8y_bsc_balances,ledger_checkpoints,ledger_canonical_blocks CASCADE").execute(&pool).await.unwrap();
    Some((pool, lock))
}
fn terra(amount: u64) -> Cw20Transfer {
    Cw20Transfer {
        contract: DEFAULT_CL8Y_CW20.into(),
        from: ALICE.into(),
        to: "terra1bob".into(),
        amount: BigInt::from(amount),
        action: "transfer".into(),
    }
}
fn bsc(amount: u64, block: i64, index: i32) -> Bep20Transfer {
    Bep20Transfer {
        token: DEFAULT_CL8Y_BEP20.to_ascii_lowercase(),
        from: EVM.into(),
        to: "0x2222222222222222222222222222222222222222".into(),
        amount: BigInt::from(amount),
        bsc_block: block,
        tx_hash: format!("tx{index}"),
        log_index: index,
    }
}
#[tokio::test]
async fn replay_boundary_and_concurrent_writers_both_chains() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    for (chain, wallet) in [(Chain::Terra, ALICE), (Chain::Bsc, EVM)] {
        seed_registration(&pool, chain, wallet, 110, &BigInt::from(100), None)
            .await
            .unwrap();
    }
    for h in [109, 110, 111] {
        db::apply_cw20_transfers(&pool, &[terra(10)], h, &format!("h{h}"))
            .await
            .unwrap();
        db::apply_bep20_transfers(&pool, &[bsc(10, h, h as i32)])
            .await
            .unwrap();
    }
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 111)
            .await
            .unwrap(),
        BigInt::from(90)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 111).await.unwrap(),
        BigInt::from(90)
    );
    let transfers = [terra(10)];
    let (a, b) = tokio::join!(
        db::apply_cw20_transfers(&pool, &transfers, 111, "h111"),
        db::apply_cw20_transfers(&pool, &transfers, 111, "h111")
    );
    a.unwrap();
    b.unwrap();
    let transfers = [bsc(10, 111, 111)];
    let (a, b) = tokio::join!(
        db::apply_bep20_transfers(&pool, &transfers),
        db::apply_bep20_transfers(&pool, &transfers)
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 111)
            .await
            .unwrap(),
        BigInt::from(90)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 111).await.unwrap(),
        BigInt::from(90)
    );
    assert!(matches!(
        db::apply_cw20_transfers(&pool, &[terra(9)], 111, "h111").await,
        Err(LedgerError::Conflict(_))
    ));
    assert!(matches!(
        db::apply_bep20_transfers(&pool, &[bsc(9, 111, 111)]).await,
        Err(LedgerError::Conflict(_))
    ));
}
#[tokio::test]
async fn underflow_rolls_back_rows_and_checkpoints() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    seed_registration(&pool, Chain::Terra, ALICE, 100, &BigInt::from(5), None)
        .await
        .unwrap();
    assert!(
        db::apply_cw20_transfers(&pool, &[terra(1), terra(10)], 101, "bad")
            .await
            .is_err()
    );
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 101)
            .await
            .unwrap(),
        BigInt::from(5)
    );
}
#[derive(Clone)]
struct Fixture {
    fail_second: Arc<AtomicBool>,
}
async fn fixture(
    State(f): State<Fixture>,
    OriginalUri(uri): OriginalUri,
) -> Json<serde_json::Value> {
    if uri.path().contains("/blocks/") {
        return Json(
            serde_json::json!({"block_id":{"hash":"A".repeat(64)},"block":{"header":{"chain_id":"columbus-5","time":chrono::Utc::now().to_rfc3339(),"height":"110"},"data":{"txs":(0..101).map(|i|base64::Engine::encode(&base64::engine::general_purpose::STANDARD,format!("fixture:{i}"))).collect::<Vec<_>>()}}}),
        );
    }
    let second = uri.query().unwrap_or_default().contains("pagination.key")
        || uri.query().unwrap_or_default().contains("page=2");
    if second && f.fail_second.load(Ordering::Relaxed) {
        return Json(serde_json::json!({"txs":[]}));
    }
    let range = if second { 100..101 } else { 0..100 };
    let txs:Vec<_>=range.map(|i|serde_json::json!({"txhash":if i==0 && uri.path().contains("/report/") {"REAL_HASH".to_string()}else{physical_hash(i)},"height":"110","code":0,"logs":[],"events":[{"type":"wasm","attributes":[{"key":"_contract_address","value":DEFAULT_CL8Y_CW20},{"key":"action","value":"transfer"},{"key":"from","value":ALICE},{"key":"to","value":"terra1bob"},{"key":"amount","value":"1"}]}]})).collect();
    Json(
        serde_json::json!({"txs":[{"body":{}}],"tx_responses":txs,"pagination":{"next_key":if second {""}else{"a+/="},"total":"101"}}),
    )
}
async fn server() -> (String, Arc<AtomicBool>, tokio::task::JoinHandle<()>) {
    let fail_second = Arc::new(AtomicBool::new(false));
    let app = Router::new().fallback(get(fixture)).with_state(Fixture {
        fail_second: fail_second.clone(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, fail_second, task)
}
fn config(url: &str) -> LedgerConfig {
    LedgerConfig {
        database_url: String::new(),
        terra_lcd_urls: vec![url.into()],
        terra_chain_id: "columbus-5".into(),
        terra_rpc_urls: vec![],
        cl8y_token_address: DEFAULT_CL8Y_CW20.into(),
        bsc_cl8y_token_address: DEFAULT_CL8Y_BEP20.to_ascii_lowercase(),
        bsc_rpc_urls: vec![],
        run_mode: RunMode::Dev,
        api_bind: "127.0.0.1:0".into(),
        poll_interval_ms: 100,
    }
}
#[tokio::test]
async fn standard_real_hash_response_paginates_execution_events() {
    let (url, _, task) = server().await;
    let lcd = LcdClient::new(vec![format!("{url}/report")]);
    let txs = lcd.block_txs(110).await.unwrap();
    assert_eq!(txs.len(), 101);
    assert_eq!(txs[0].tx_hash(), "REAL_HASH");
    assert_eq!(txs[0].wasm_events().unwrap().len(), 1);
    task.abort();
}
#[tokio::test]
async fn failed_later_page_cannot_commit_any_event_or_cursor() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (url, fail, task) = server().await;
    let lcd = LcdClient::new(vec![url.clone()]);
    let cfg = config(&url);
    seed_registration(&pool, Chain::Terra, ALICE, 100, &BigInt::from(1000), None)
        .await
        .unwrap();
    fail.store(true, Ordering::Relaxed);
    assert!(ingest::ingest_terra_height(&pool, &lcd, &cfg, 110)
        .await
        .is_err());
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_checkpoints")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    fail.store(false, Ordering::Relaxed);
    assert_eq!(
        ingest::ingest_terra_height(&pool, &lcd, &cfg, 110)
            .await
            .unwrap(),
        101
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 110)
            .await
            .unwrap(),
        BigInt::from(899)
    );
    ingest::ingest_terra_height(&pool, &lcd, &cfg, 110)
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 110)
            .await
            .unwrap(),
        BigInt::from(899)
    );
    task.abort();
}
#[tokio::test]
async fn bounded_http_schema_failover_never_exposes_url() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/private?key=SENTINEL",
        listener.local_addr().unwrap()
    );
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().fallback(get(|| async { "invalid json" })),
        )
        .await
        .unwrap();
    });
    let (good, _, healthy) = server().await;
    let lcd = LcdClient::new(vec![url.clone(), good]);
    assert_eq!(lcd.latest_height().await.unwrap(), 110);
    let error = LcdClient::new(vec![url])
        .latest_height()
        .await
        .unwrap_err()
        .to_string();
    assert!(!error.contains("SENTINEL"));
    assert!(!error.contains("127.0.0.1"));
    task.abort();
    healthy.abort();
    tokio::time::sleep(Duration::from_millis(1)).await;
}
#[derive(Clone)]
struct RpcFixture {
    bad: Arc<std::sync::atomic::AtomicUsize>,
    tip: Arc<std::sync::atomic::AtomicI64>,
}
fn block_hash(h: i64) -> String {
    format!("0x{h:064x}")
}
async fn rpc_fixture(
    State(f): State<RpcFixture>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let mode = f.bad.load(Ordering::Relaxed);
    let method = body["method"].as_str().unwrap();
    let params = &body["params"];
    let result = match method {
        "eth_chainId" => serde_json::json!(if mode == 1 { "0x1" } else { "0x38" }),
        "eth_blockNumber" => serde_json::json!(format!(
            "0x{:x}",
            if mode == 11 {
                140
            } else if mode == 6 {
                f.tip.fetch_add(20, Ordering::Relaxed)
            } else {
                f.tip.load(Ordering::Relaxed)
            }
        )),
        "eth_getBlockByNumber" => {
            let h = i64::from_str_radix(params[0].as_str().unwrap().trim_start_matches("0x"), 16)
                .unwrap();
            if mode == 9 {
                return Json(
                    serde_json::json!({"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"unavailable"}}),
                );
            }
            serde_json::json!({"number":params[0],"timestamp":format!("0x{:x}",chrono::Utc::now().timestamp()-if mode==13{121}else{0}),"hash":block_hash(if mode==8 || (mode==7 && h>=103) || (mode==11 && h>=120) {h+100000}else{h})})
        }
        "eth_call" => serde_json::json!(format!(
            "0x{:064x}",
            if mode == 6 && params[1] == "latest" {
                90
            } else {
                100
            }
        )),
        "eth_getLogs" => {
            if mode == 18 {
                return Json(serde_json::json!({"jsonrpc":"2.0","id":1,"result":[]}));
            }
            let lo = i64::from_str_radix(
                params[0]["fromBlock"]
                    .as_str()
                    .unwrap()
                    .trim_start_matches("0x"),
                16,
            )
            .unwrap();
            let hi = i64::from_str_radix(
                params[0]["toBlock"]
                    .as_str()
                    .unwrap()
                    .trim_start_matches("0x"),
                16,
            )
            .unwrap();
            if hi - lo > 1 {
                return Json(
                    serde_json::json!({"jsonrpc":"2.0","id":1,"error":{"code":-32005,"message":"limit exceeded"}}),
                );
            }
            let mut logs = Vec::new();
            for h in lo..=hi {
                let mut log = serde_json::json!({"address":DEFAULT_CL8Y_BEP20,"topics":[voting_ledger::parser::TRANSFER_TOPIC,format!("0x{:0>64}",EVM.trim_start_matches("0x")),format!("0x{:0>64}","2222222222222222222222222222222222222222")],"data":format!("0x{:064x}",1),"blockNumber":format!("0x{h:x}"),"blockHash":block_hash(if mode==11 && h>=120 {h+100000}else{h}),"transactionHash":block_hash(h+10000),"logIndex":"0x0","removed":false});
                match mode {
                    2 => log["removed"] = serde_json::json!(true),
                    3 => {
                        log.as_object_mut().unwrap().remove("transactionHash");
                    }
                    4 => log["data"] = serde_json::json!(format!("0x{}", "f".repeat(65))),
                    5 => log["address"] = serde_json::json!(EVM),
                    15 => {
                        log.as_object_mut().unwrap().remove("blockHash");
                    }
                    16 => {
                        log.as_object_mut().unwrap().remove("blockNumber");
                    }
                    17 => log["topics"][1] = serde_json::json!(format!("0x{}", "f".repeat(64))),
                    _ => {}
                }
                if mode == 14 {
                    logs.push(log.clone());
                }
                logs.push(log);
            }
            logs.reverse();
            serde_json::json!(logs)
        }
        _ => panic!("unexpected method"),
    };
    Json(serde_json::json!({"jsonrpc":"2.0","id":1,"result":result}))
}
async fn rpc_server() -> (
    String,
    Arc<std::sync::atomic::AtomicUsize>,
    tokio::task::JoinHandle<()>,
) {
    let bad = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let app = Router::new()
        .fallback(axum::routing::post(rpc_fixture))
        .with_state(RpcFixture {
            bad: bad.clone(),
            tip: Arc::new(std::sync::atomic::AtomicI64::new(120)),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, bad, task)
}
#[tokio::test]
async fn bsc_http_to_database_adaptive_ranges_and_fail_closed_matrix() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (url, bad, task) = rpc_server().await;
    let bsc = voting_ledger::bsc::BscClient::new(vec![url.clone()]);
    let cfg = config(&url);
    seed_registration(&pool, Chain::Bsc, EVM, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    for mode in [1, 2, 3, 4, 5, 14, 15, 16, 17] {
        bad.store(mode, Ordering::Relaxed);
        assert!(
            ingest::ingest_bsc_range(&pool, &bsc, &cfg, 101, 104)
                .await
                .is_err(),
            "mode {mode}"
        );
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_bep20_transfers")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_checkpoints")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
    bad.store(0, Ordering::Relaxed);
    assert_eq!(
        ingest::ingest_bsc_range(&pool, &bsc, &cfg, 101, 104)
            .await
            .unwrap(),
        4
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 104).await.unwrap(),
        BigInt::from(96)
    );
    ingest::ingest_bsc_range(&pool, &bsc, &cfg, 101, 104)
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 104).await.unwrap(),
        BigInt::from(96)
    );
    task.abort();
}
#[tokio::test]
async fn maximum_uint256_and_coherent_live_bsc_snapshot() {
    let (url, _, task) = rpc_server().await;
    let bsc = voting_ledger::bsc::BscClient::new(vec![url]);
    let observed = bsc.live_snapshot(DEFAULT_CL8Y_BEP20, EVM).await.unwrap();
    let h = observed.height;
    let b = observed.amount;
    assert_eq!(h, 120);
    assert_eq!(b, BigInt::from(100));
    assert_eq!(
        voting_ledger::amount::parse_hex_uint256(&format!("0x{}", "f".repeat(64))).unwrap(),
        (BigInt::from(1) << 256) - BigInt::from(1)
    );
    assert!(voting_ledger::amount::parse_hex_uint256(&format!("0x{}", "f".repeat(65))).is_err());
    task.abort();
}
#[tokio::test]
async fn recovery_new_epoch_preserves_old_frozen_balances_and_boundary() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (url, bad, task) = rpc_server().await;
    let client = voting_ledger::bsc::BscClient::new(vec![url.clone()]);
    let source = voting_ledger::bsc::CompositeBalanceSource {
        terra: None,
        bsc: Some(voting_ledger::bsc::BscBalanceSource {
            client: client.clone(),
            token: DEFAULT_CL8Y_BEP20.into(),
        }),
    };
    let cfg = config(&url);
    let lcd = LcdClient::new(vec![]);
    seed_registration(&pool, Chain::Bsc, EVM, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    db::apply_bep20_transfers(&pool, &[bsc(10, 110, 1)])
        .await
        .unwrap();
    bad.store(6, Ordering::Relaxed);
    voting_ledger::reconcile::reconcile_live_baseline(
        &pool,
        &source,
        &lcd,
        &client,
        &cfg,
        Chain::Bsc,
    )
    .await
    .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 110).await.unwrap(),
        BigInt::from(90)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 120).await.unwrap(),
        BigInt::from(100)
    );
    db::apply_bep20_transfers(&pool, &[bsc(1, 119, 2), bsc(1, 120, 3), bsc(10, 121, 4)])
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 121).await.unwrap(),
        BigInt::from(90)
    );
    let reopened = sqlx::PgPool::connect(&std::env::var("LEDGER_TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    db::migrate(&reopened).await.unwrap();
    let state: (i64, bool, bool) = sqlx::query_as(
        "SELECT height,complete,canonical FROM ledger_checkpoints WHERE chain='bsc'",
    )
    .fetch_one(&reopened)
    .await
    .unwrap();
    assert_eq!(state, (120, true, true));
    assert_eq!(
        db::balance_at(&reopened, Chain::Bsc, EVM, 110)
            .await
            .unwrap(),
        BigInt::from(90)
    );
    reopened.close().await;
    task.abort();
}
#[tokio::test]
async fn multi_block_bsc_fork_rewinds_to_common_ancestor_and_orphans_poll() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (url, bad, task) = rpc_server().await;
    let client = voting_ledger::bsc::BscClient::new(vec![url.clone()]);
    let cfg = config(&url);
    seed_registration(&pool, Chain::Bsc, EVM, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    ingest::ingest_bsc_range(&pool, &client, &cfg, 101, 104)
        .await
        .unwrap();
    let id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO voting.proposals(id,chain,proposer,title,body_html,body_canonical,terra_height,bsc_block,status,opened_at) VALUES($1,'bsc',$2,'fork poll','body','body',100,104,'open',NOW())").bind(id).bind(EVM).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO voting.proposal_snapshots(proposal_id,chain,wallet_address,weight) VALUES($1,'bsc',$2,96)").bind(id).bind(EVM).execute(&pool).await.unwrap();
    bad.store(7, Ordering::Relaxed);
    ingest::maybe_rewind_bsc(&pool, &client, 104).await.unwrap();
    assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 102);
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 102).await.unwrap(),
        BigInt::from(98)
    );
    let status: String =
        sqlx::query_scalar("SELECT snapshot_validity FROM voting.proposals WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "invalid");
    let frozen: String = sqlx::query_scalar(
        "SELECT weight::text FROM voting.proposal_snapshots WHERE proposal_id=$1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(frozen, "96");
    bad.store(8, Ordering::Relaxed);
    assert!(ingest::maybe_rewind_bsc(&pool, &client, 102).await.is_err());
    let reason: String =
        sqlx::query_scalar("SELECT degraded_reason FROM ledger_checkpoints WHERE chain='bsc'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reason, "fork_beyond_horizon");
    task.abort();
}
#[tokio::test]
async fn either_chain_failure_does_not_prevent_sibling_poll() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (url, bad, rpc_task) = rpc_server().await;
    let bsc = voting_ledger::bsc::BscClient::new(vec![url.clone()]);
    let cfg = config(&url);
    assert!(
        voting_ledger::worker::poll_chain(&pool, &None, &bsc, &cfg, Chain::Terra)
            .await
            .is_err()
    );
    voting_ledger::worker::poll_chain(&pool, &None, &bsc, &cfg, Chain::Bsc)
        .await
        .unwrap();
    assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 105);
    let (terra, _, terra_task) = server().await;
    let lcd = Some(LcdClient::new(vec![terra]));
    bad.store(1, Ordering::Relaxed);
    assert!(
        voting_ledger::worker::poll_chain(&pool, &lcd, &bsc, &cfg, Chain::Bsc)
            .await
            .is_err()
    );
    voting_ledger::worker::poll_chain(&pool, &lcd, &bsc, &cfg, Chain::Terra)
        .await
        .unwrap();
    assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), 110);
    rpc_task.abort();
    terra_task.abort();
}
struct FairSource;
#[async_trait::async_trait]
impl voting_ledger::register::LiveBalanceSource for FairSource {
    async fn terra_live_balance(
        &self,
        address: &str,
    ) -> Result<voting_ledger::register::LiveBalanceSnapshot, LedgerError> {
        if address == "terra1poisoned" {
            tokio::time::sleep(Duration::from_secs(60)).await;
            return Err(LedgerError::Provider("poisoned"));
        }
        Ok(voting_ledger::register::LiveBalanceSnapshot {
            height: 100,
            amount: BigInt::from(10),
            block_hash: "A".repeat(64),
        })
    }
    async fn bsc_live_balance(
        &self,
        _: &str,
    ) -> Result<voting_ledger::register::LiveBalanceSnapshot, LedgerError> {
        unreachable!()
    }
}
#[tokio::test]
async fn poisoned_early_intent_cannot_starve_later_valid_holders() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    for i in 0..17 {
        let wallet = if i == 0 {
            "terra1poisoned".into()
        } else {
            format!("terra1valid{i}")
        };
        let sig = uuid::Uuid::new_v4();
        sqlx::query("INSERT INTO voting.signatures(id,chain,wallet_address,signature,payload_hash,purpose) VALUES($1,'terra',$2,'synthetic','synthetic','register')").bind(sig).bind(&wallet).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO voting.registration_intents(chain,wallet_address,signature_id,created_at) VALUES('terra',$1,$2,NOW()+make_interval(secs=>$3))").bind(wallet).bind(sig).bind(i as f64).execute(&pool).await.unwrap();
    }
    let result = tokio::time::timeout(
        Duration::from_secs(12),
        voting_ledger::register::process_pending_intents(&pool, &FairSource),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.len(), 15);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM voting_registrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 15);
    let result = voting_ledger::register::process_pending_intents(&pool, &FairSource)
        .await
        .unwrap();
    assert_eq!(result.len(), 1);
    let failed:(i32,String)=sqlx::query_as("SELECT attempts,last_error_class FROM voting.registration_intents WHERE wallet_address='terra1poisoned'").fetch_one(&pool).await.unwrap();
    assert_eq!(failed, (1, "deadline".into()));
}
#[tokio::test]
async fn readiness_fails_for_each_stale_degraded_missing_lagging_chain_and_database() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    db::set_state(&pool, "intents_ok", "true").await.unwrap();
    for chain in [Chain::Terra, Chain::Bsc] {
        let mut tx = pool.begin().await.unwrap();
        db::lock_chain(&mut tx, chain).await.unwrap();
        db::publish_checkpoint(
            &mut tx,
            chain,
            100,
            &block_hash(100),
            if chain == Chain::Bsc { 115 } else { 100 },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    assert_eq!(
        voting_ledger::health::readiness(&pool).await.0,
        axum::http::StatusCode::OK
    );
    for chain in ["terra", "bsc"] {
        for mutation in [
            "observed_at=NOW()-INTERVAL '121 seconds'",
            "complete=false",
            "canonical=false",
            "degraded_reason='injected'",
            "height=0",
            "network_tip=200",
        ] {
            let mut tx = pool.begin().await.unwrap();
            sqlx::query(&format!(
                "UPDATE ledger_checkpoints SET {mutation} WHERE chain=$1"
            ))
            .bind(chain)
            .execute(&mut *tx)
            .await
            .unwrap();
            tx.commit().await.unwrap();
            let (status, body) = voting_ledger::health::readiness(&pool).await;
            assert_eq!(
                status,
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "{chain} {mutation}"
            );
            assert_eq!(serde_json::to_value(body.0).unwrap()["database_ok"], true);
            sqlx::query("UPDATE ledger_checkpoints SET height=100,observed_at=NOW(),complete=true,canonical=true,degraded_reason=NULL,network_tip=$2 WHERE chain=$1").bind(chain).bind(if chain=="bsc"{115_i64}else{100_i64}).execute(&pool).await.unwrap();
        }
    }
    db::set_state(&pool, "intents_ok", "false").await.unwrap();
    assert_eq!(
        voting_ledger::health::readiness(&pool).await.0,
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );
    let unavailable = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://voting:voting@127.0.0.1:1/voting_test_absent")
        .unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        voting_ledger::health::readiness(&unavailable),
    )
    .await
    .unwrap();
    assert_eq!(result.0, axum::http::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        serde_json::to_value(result.1 .0).unwrap()["database_ok"],
        false
    );
    unavailable.close().await;
}
#[tokio::test]
async fn canceled_writer_rolls_back_event_balance_and_cursor_before_restart() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    seed_registration(&pool, Chain::Terra, ALICE, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    let (sent, received) = tokio::sync::oneshot::channel();
    let writer_pool = pool.clone();
    let task = tokio::spawn(async move {
        let mut tx = writer_pool.begin().await.unwrap();
        db::lock_chain(&mut tx, Chain::Terra).await.unwrap();
        db::persist_terra_leg(&mut tx, &terra(10), 101, "crash", 0)
            .await
            .unwrap();
        db::publish_checkpoint(&mut tx, Chain::Terra, 101, &"A".repeat(64), 101)
            .await
            .unwrap();
        sent.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    received.await.unwrap();
    task.abort();
    let _ = task.await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 101)
            .await
            .unwrap(),
        BigInt::from(100)
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_checkpoints")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    db::apply_cw20_transfers(&pool, &[terra(10)], 101, "crash")
        .await
        .unwrap();
    db::apply_cw20_transfers(&pool, &[terra(10)], 101, "crash")
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 101)
            .await
            .unwrap(),
        BigInt::from(90)
    );
}
#[tokio::test]
async fn explicit_startup_recovery_is_durable_and_idempotent_on_restart() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (url, bad, task) = rpc_server().await;
    let bsc = voting_ledger::bsc::BscClient::new(vec![url.clone()]);
    let source = voting_ledger::bsc::CompositeBalanceSource {
        terra: None,
        bsc: Some(voting_ledger::bsc::BscBalanceSource {
            client: bsc.clone(),
            token: DEFAULT_CL8Y_BEP20.into(),
        }),
    };
    let lcd = LcdClient::new(vec![]);
    let cfg = config(&url);
    seed_registration(&pool, Chain::Bsc, EVM, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    bad.store(1, Ordering::Relaxed);
    assert!(
        voting_ledger::reconcile::reconcile_if_required(&pool, &source, &lcd, &bsc, &cfg)
            .await
            .is_err()
    );
    let failed:bool=sqlx::query_scalar("SELECT NOT complete AND degraded_reason IS NOT NULL FROM ledger_checkpoints WHERE chain='bsc'").fetch_one(&pool).await.unwrap();
    assert!(failed);
    bad.store(6, Ordering::Relaxed);
    assert_eq!(
        voting_ledger::reconcile::reconcile_if_required(&pool, &source, &lcd, &bsc, &cfg)
            .await
            .unwrap(),
        1
    );
    let epoch: i64 = sqlx::query_scalar("SELECT epoch FROM ledger_checkpoints WHERE chain='bsc'")
        .fetch_one(&pool)
        .await
        .unwrap();
    bad.store(1, Ordering::Relaxed);
    assert_eq!(
        voting_ledger::reconcile::reconcile_if_required(&pool, &source, &lcd, &bsc, &cfg)
            .await
            .unwrap(),
        0
    );
    let after: i64 = sqlx::query_scalar("SELECT epoch FROM ledger_checkpoints WHERE chain='bsc'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(epoch, after);
    task.abort();
}
async fn modern_fixture(State(f): State<Fixture>, uri: OriginalUri) -> Json<serde_json::Value> {
    let mut response = fixture(State(f), uri).await.0;
    if response.get("tx_responses").is_some() {
        response["pagination"] = serde_json::Value::Null;
        response["total"] = serde_json::json!("101");
    }
    Json(response)
}
#[tokio::test]
async fn supported_numbered_pagination_and_recorded_empty_execution() {
    let app = Router::new()
        .fallback(get(modern_fixture))
        .with_state(Fixture {
            fail_second: Arc::new(AtomicBool::new(false)),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/report", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let rows = LcdClient::new(vec![url]).block_txs(110).await.unwrap();
    assert_eq!(rows.len(), 101);
    assert_eq!(rows[0].tx_hash(), "REAL_HASH");
    task.abort();
    let recorded: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/terra-supported-execution.json")).unwrap();
    let height = recorded["response"]["tx_responses"][0]["height"]
        .as_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();
    let execution = recorded["response"].clone();
    let empty: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/terra-supported-empty.json")).unwrap();
    let switch = Arc::new(AtomicBool::new(false));
    let flag = switch.clone();
    let app=Router::new().fallback(get(move |uri:OriginalUri|{let execution=execution.clone();let empty=empty.clone();let flag=flag.clone();async move {if uri.0.path().contains("/blocks/"){Json(serde_json::json!({"block_id":{"hash":"A".repeat(64)},"block":{"header":{"chain_id":"columbus-5","time":chrono::Utc::now().to_rfc3339(),"height":height.to_string()}}}))}else{Json(if flag.load(Ordering::Relaxed){empty}else{execution})}}}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let lcd = LcdClient::new(vec![url]);
    let rows = lcd.block_txs(height).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].tx_hash(),
        recorded["response"]["tx_responses"][0]["txhash"]
            .as_str()
            .unwrap()
    );
    assert!(rows[0].events.is_some() || rows[0].logs.is_some());
    switch.store(true, Ordering::Relaxed);
    assert!(lcd.block_txs(height).await.unwrap().is_empty());
    task.abort();
}
#[tokio::test]
async fn process_readiness_isolation_sigterm_and_restart() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (terra, _, terra_task) = server().await;
    let (rpc, bad, rpc_task) = rpc_server().await;
    let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = reserve.local_addr().unwrap();
    drop(reserve);
    let base = format!("http://{addr}");
    let launch = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_voting-ledger"))
            .env_clear()
            .env(
                "DATABASE_URL",
                std::env::var("LEDGER_TEST_DATABASE_URL").unwrap(),
            )
            .env("RUN_MODE", "prod")
            .env("TERRA_LCD_URL", &terra)
            .env("BSC_RPC_URLS", &rpc)
            .env("API_BIND", addr.to_string())
            .env("POLL_INTERVAL_MS", "100")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap()
    };
    let http = reqwest::Client::new();
    let wait_status = |expected| {
        let http = http.clone();
        let base = base.clone();
        async move {
            tokio::time::timeout(Duration::from_secs(6), async {
                loop {
                    if http
                        .get(format!("{base}/ready"))
                        .send()
                        .await
                        .is_ok_and(|r| r.status() == expected)
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await
            .unwrap();
        }
    };
    let mut child = launch();
    wait_status(reqwest::StatusCode::OK).await;
    assert_eq!(
        http.get(format!("{base}/live"))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::OK
    );
    let original_epoch: i64 =
        sqlx::query_scalar("SELECT epoch FROM ledger_checkpoints WHERE chain='bsc'")
            .fetch_one(&pool)
            .await
            .unwrap();
    bad.store(1, Ordering::Relaxed);
    wait_status(reqwest::StatusCode::SERVICE_UNAVAILABLE).await;
    assert_eq!(
        http.get(format!("{base}/live"))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::OK
    );
    assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), 110);
    bad.store(0, Ordering::Relaxed);
    wait_status(reqwest::StatusCode::OK).await;
    let recovered_epoch: i64 =
        sqlx::query_scalar("SELECT epoch FROM ledger_checkpoints WHERE chain='bsc'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(recovered_epoch, original_epoch);
    // Explicit canonical hash lookup failure must not refresh a stale head or stop its sibling.
    bad.store(9, Ordering::Relaxed);
    wait_status(reqwest::StatusCode::SERVICE_UNAVAILABLE).await;
    assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 105);
    assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), 110);
    assert_eq!(
        http.get(format!("{base}/live"))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::OK
    );
    bad.store(0, Ordering::Relaxed);
    wait_status(reqwest::StatusCode::OK).await;
    let recovered: i64 =
        sqlx::query_scalar("SELECT epoch FROM ledger_checkpoints WHERE chain='bsc'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(recovered, original_epoch);

    assert!(std::process::Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status()
        .unwrap()
        .success());
    let exited = tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert!(exited.success());
    assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 105);
    let mut child = launch();
    wait_status(reqwest::StatusCode::OK).await;
    assert!(std::process::Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status()
        .unwrap()
        .success());
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    terra_task.abort();
    rpc_task.abort();
}

async fn seed_registration(
    pool: &sqlx::PgPool,
    chain: Chain,
    wallet: &str,
    height: i64,
    amount: &BigInt,
    sig: Option<uuid::Uuid>,
) -> Result<bool, LedgerError> {
    let inserted = db::insert_registration(pool, chain, wallet, height, amount, sig).await?;
    sqlx::query("UPDATE voting_registrations SET baseline_block_hash=$3,baseline_verified=true WHERE chain=$1 AND wallet_address=$2")
        .bind(chain.as_str()).bind(wallet).bind(if chain==Chain::Terra{"A".repeat(64)}else{block_hash(height)}).execute(pool).await?;
    Ok(inserted)
}
#[tokio::test]
async fn unfinalized_registration_baseline_fork_cannot_publish_eligible_power() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (url, bad, task) = rpc_server().await;
    let bsc = voting_ledger::bsc::BscClient::new(vec![url.clone()]);
    let source = voting_ledger::bsc::CompositeBalanceSource {
        terra: None,
        bsc: Some(voting_ledger::bsc::BscBalanceSource {
            client: bsc.clone(),
            token: DEFAULT_CL8Y_BEP20.into(),
        }),
    };
    let cfg = config(&url);
    voting_ledger::worker::poll_chain(&pool, &None, &bsc, &cfg, Chain::Bsc)
        .await
        .unwrap();
    assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 105);
    voting_ledger::register::register_wallet(&pool, &source, Chain::Bsc, EVM, None)
        .await
        .unwrap();
    let baseline:(i64,String,bool)=sqlx::query_as("SELECT baseline_height,baseline_block_hash,baseline_verified FROM voting_registrations WHERE chain='bsc' AND wallet_address=$1").bind(EVM).fetch_one(&pool).await.unwrap();
    assert_eq!(baseline, (120, block_hash(120), false));
    bad.store(11, Ordering::Relaxed);
    assert!(
        voting_ledger::worker::poll_chain(&pool, &None, &bsc, &cfg, Chain::Bsc)
            .await
            .is_err()
    );
    assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 105);
    let degraded: (bool, bool, String) = sqlx::query_as(
        "SELECT complete,canonical,degraded_reason FROM ledger_checkpoints WHERE chain='bsc'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        degraded,
        (false, false, "registration_baseline_orphaned".into())
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_bep20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    task.abort();
}
#[tokio::test]
async fn stale_consistent_provider_heads_cannot_renew_freshness() {
    let (url, bad, rpc_task) = rpc_server().await;
    let bsc = voting_ledger::bsc::BscClient::new(vec![url]);
    bad.store(13, Ordering::Relaxed);
    assert!(bsc.block_number().await.is_err());
    assert_eq!(bsc.block_hash(120).await.unwrap(), block_hash(120));
    rpc_task.abort();
    let app=Router::new().fallback(get(||async {Json(serde_json::json!({"block_id":{"hash":"A".repeat(64)},"block":{"header":{"chain_id":"columbus-5","height":"110","time":(chrono::Utc::now()-chrono::Duration::seconds(121)).to_rfc3339()}}}))}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let lcd = LcdClient::new(vec![url]);
    assert!(lcd.latest_height().await.is_err());
    assert_eq!(lcd.block_hash(110).await.unwrap(), "A".repeat(64));
    task.abort();
}
#[tokio::test]
async fn terra_observed_live_height_and_bulk_recovery_binding() {
    let bound = Arc::new(AtomicBool::new(true));
    let flag = bound.clone();
    let head = Arc::new(std::sync::atomic::AtomicI64::new(110));
    let smart_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let app=Router::new().fallback(get(move |uri:OriginalUri,request:axum::http::HeaderMap| {let flag=flag.clone();let head=head.clone();let smart_calls=smart_calls.clone();async move {
        if uri.0.path().contains("/smart/") {
            let fallback=if smart_calls.fetch_add(1,Ordering::Relaxed)==0{"111".to_string()}else{(head.load(Ordering::Relaxed)-2).to_string()};let observed=request.get("x-cosmos-block-height").and_then(|h|h.to_str().ok()).unwrap_or(&fallback);let mut headers=axum::http::HeaderMap::new();if flag.load(Ordering::Relaxed){headers.insert("x-cosmos-block-height",observed.parse().unwrap());}
            (headers,Json(serde_json::json!({"data":{"balance":"100"}})))
        } else {
            let height=if uri.0.path().ends_with("/latest"){head.fetch_add(2,Ordering::Relaxed)}else{uri.0.path().rsplit('/').next().unwrap().parse().unwrap()};
            (axum::http::HeaderMap::new(),Json(serde_json::json!({"block_id":{"hash":"A".repeat(64)},"block":{"header":{"chain_id":"columbus-5","height":height.to_string(),"time":chrono::Utc::now().to_rfc3339()}}})))
        }
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let lcd = LcdClient::new(vec![url]);
    let observed = lcd.live_snapshot(DEFAULT_CL8Y_CW20, ALICE).await.unwrap();
    assert_eq!(observed.height, 114);
    assert_eq!(observed.amount, BigInt::from(100));
    assert_eq!(observed.block_hash, "A".repeat(64));
    let wallets = (0..50)
        .map(|i| format!("terra1wallet{i}"))
        .collect::<Vec<_>>();
    let bulk = lcd
        .live_balances(DEFAULT_CL8Y_CW20, &wallets)
        .await
        .unwrap();
    assert_eq!(bulk.balances.len(), 50);
    assert_eq!(bulk.height, 118);
    bound.store(false, Ordering::Relaxed);
    assert!(lcd.live_snapshot(DEFAULT_CL8Y_CW20, ALICE).await.is_err());
    assert!(lcd
        .live_balances(DEFAULT_CL8Y_CW20, &wallets)
        .await
        .is_err());
    task.abort();
}
#[tokio::test]
async fn rate_limit_timeout_and_oversized_http_responses_have_bounded_failover() {
    let unhealthy = Router::new().fallback(get(|uri: OriginalUri| async move {
        match uri.0.path() {
            "/large" => (
                axum::http::StatusCode::OK,
                "x".repeat(voting_ledger::transport::MAX_RESPONSE_BYTES + 1),
            ),
            "/slow" => {
                tokio::time::sleep(Duration::from_secs(30)).await;
                (axum::http::StatusCode::OK, "{}".into())
            }
            _ => (
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                "rate limited".into(),
            ),
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, unhealthy).await.unwrap();
    });
    let (good, _, good_task) = server().await;
    // The appended LCD path still routes to the same injected behavior.
    for prefix in ["limited", "large"] {
        let url = format!("{base}/{prefix}");
        let lcd = LcdClient::new(vec![url, good.clone()]);
        assert_eq!(lcd.latest_height().await.unwrap(), 110);
    }
    let response = reqwest::get(format!("{base}/large")).await.unwrap();
    assert!(
        voting_ledger::transport::decode::<serde_json::Value>(response)
            .await
            .is_err()
    );
    let slow = Router::new().fallback(get(|| async {
        tokio::time::sleep(Duration::from_secs(30)).await;
        "{}"
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let slow_task = tokio::spawn(async move {
        axum::serve(listener, slow).await.unwrap();
    });
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(10),
            LcdClient::new(vec![url, good]).latest_height()
        )
        .await
        .unwrap()
        .unwrap(),
        110
    );
    task.abort();
    slow_task.abort();
    good_task.abort();
}
#[tokio::test]
async fn mint_burn_credit_debit_self_replay_and_conservation_both_chains() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let bob = "terra1bob";
    let evm_bob = "0x2222222222222222222222222222222222222222";
    for (chain, wallet) in [
        (Chain::Terra, ALICE),
        (Chain::Terra, bob),
        (Chain::Bsc, EVM),
        (Chain::Bsc, evm_bob),
    ] {
        seed_registration(&pool, chain, wallet, 100, &BigInt::from(100), None)
            .await
            .unwrap();
    }
    let cases = [
        ("mint", "", ALICE, 5),
        ("burn", ALICE, "", 3),
        ("transfer", ALICE, ALICE, 7),
        ("transfer", ALICE, bob, 15),
        ("transfer", bob, ALICE, 9),
    ];
    let mut expected_a = 100_i64;
    let mut expected_b = 100_i64;
    for (i, (action, from, to, amount)) in cases.into_iter().enumerate() {
        let height = 101 + i as i64;
        let amount = BigInt::from(amount);
        let mut transfer = terra(0);
        transfer.action = action.into();
        transfer.from = from.into();
        transfer.to = to.into();
        transfer.amount = amount.clone();
        let hash = format!("case{i}");
        for _ in 0..2 {
            db::apply_cw20_transfers(&pool, &[transfer.clone()], height, &hash)
                .await
                .unwrap();
        }
        let evm = |a: &str| {
            if a.is_empty() {
                voting_ledger::parser::ZERO_EVM_ADDRESS.to_string()
            } else if a == ALICE {
                EVM.into()
            } else {
                evm_bob.into()
            }
        };
        let mut transfer = bsc(0, height, i as i32);
        transfer.from = evm(from);
        transfer.to = evm(to);
        transfer.amount = amount;
        for _ in 0..2 {
            db::apply_bep20_transfers(&pool, &[transfer.clone()])
                .await
                .unwrap();
        }
        if from != to {
            if from == ALICE {
                expected_a -= cases[i].3;
            }
            if to == ALICE {
                expected_a += cases[i].3;
            }
            if from == bob {
                expected_b -= cases[i].3;
            }
            if to == bob {
                expected_b += cases[i].3;
            }
        }
        for (chain, a, b) in [(Chain::Terra, ALICE, bob), (Chain::Bsc, EVM, evm_bob)] {
            let actual_a = db::balance_at(&pool, chain, a, height).await.unwrap();
            let actual_b = db::balance_at(&pool, chain, b, height).await.unwrap();
            assert_eq!(actual_a, BigInt::from(expected_a));
            assert_eq!(actual_b, BigInt::from(expected_b));
            assert_eq!(actual_a + actual_b, BigInt::from(expected_a + expected_b));
        }
    }
    for bad in [BigInt::from(-1), BigInt::from(1) << 256] {
        let mut transfer = terra(1);
        transfer.amount = bad.clone();
        assert!(db::apply_cw20_transfers(&pool, &[transfer], 106, "invalid")
            .await
            .is_err());
        let mut transfer = bsc(1, 106, 1000);
        transfer.amount = bad;
        assert!(db::apply_bep20_transfers(&pool, &[transfer]).await.is_err());
    }
    let maximum: BigInt = (BigInt::from(1) << 256) - BigInt::from(1);
    let mut overflow = terra(0);
    overflow.from = String::new();
    overflow.to = ALICE.into();
    overflow.action = "mint".into();
    overflow.amount = maximum.clone();
    assert!(
        db::apply_cw20_transfers(&pool, &[overflow], 106, "overflow")
            .await
            .is_err()
    );
    let mut overflow = bsc(0, 106, 1001);
    overflow.from = voting_ledger::parser::ZERO_EVM_ADDRESS.into();
    overflow.to = EVM.into();
    overflow.amount = maximum;
    assert!(db::apply_bep20_transfers(&pool, &[overflow]).await.is_err());
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 106)
            .await
            .unwrap(),
        BigInt::from(96)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 106).await.unwrap(),
        BigInt::from(96)
    );
}
struct PausingRegistration {
    ready: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl voting_ledger::register::LiveBalanceSource for PausingRegistration {
    async fn terra_live_balance(
        &self,
        _: &str,
    ) -> Result<voting_ledger::register::LiveBalanceSnapshot, LedgerError> {
        self.ready.notify_one();
        self.release.notified().await;
        Ok(voting_ledger::register::LiveBalanceSnapshot {
            height: 100,
            amount: BigInt::from(100),
            block_hash: "A".repeat(64),
        })
    }
    async fn bsc_live_balance(
        &self,
        _: &str,
    ) -> Result<voting_ledger::register::LiveBalanceSnapshot, LedgerError> {
        self.ready.notify_one();
        self.release.notified().await;
        Ok(voting_ledger::register::LiveBalanceSnapshot {
            height: 100,
            amount: BigInt::from(100),
            block_hash: block_hash(100),
        })
    }
}
struct FreshRegistration;
#[async_trait::async_trait]
impl voting_ledger::register::LiveBalanceSource for FreshRegistration {
    async fn terra_live_balance(
        &self,
        _: &str,
    ) -> Result<voting_ledger::register::LiveBalanceSnapshot, LedgerError> {
        Ok(voting_ledger::register::LiveBalanceSnapshot {
            height: 101,
            amount: BigInt::from(90),
            block_hash: "A".repeat(64),
        })
    }
    async fn bsc_live_balance(
        &self,
        _: &str,
    ) -> Result<voting_ledger::register::LiveBalanceSnapshot, LedgerError> {
        Ok(voting_ledger::register::LiveBalanceSnapshot {
            height: 101,
            amount: BigInt::from(90),
            block_hash: block_hash(101),
        })
    }
}
#[tokio::test]
async fn registration_racing_ingest_retries_overtaken_snapshot_and_excludes_boundary() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    seed_registration(
        &pool,
        Chain::Terra,
        "terra1bob",
        100,
        &BigInt::from(100),
        None,
    )
    .await
    .unwrap();
    let source = Arc::new(PausingRegistration {
        ready: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let reg_source = source.clone();
    let reg_pool = pool.clone();
    let register = tokio::spawn(async move {
        voting_ledger::register::register_wallet(
            &reg_pool,
            reg_source.as_ref(),
            Chain::Terra,
            ALICE,
            None,
        )
        .await
    });
    source.ready.notified().await;
    let mut tx = pool.begin().await.unwrap();
    db::lock_chain(&mut tx, Chain::Terra).await.unwrap();
    db::persist_terra_leg(&mut tx, &terra(10), 101, "race", 0)
        .await
        .unwrap();
    db::publish_checkpoint(&mut tx, Chain::Terra, 101, &"A".repeat(64), 101)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    source.release.notify_one();
    assert!(register.await.unwrap().is_err());
    assert!(db::get_registration(&pool, Chain::Terra, ALICE)
        .await
        .unwrap()
        .is_none());
    voting_ledger::register::register_wallet(&pool, &FreshRegistration, Chain::Terra, ALICE, None)
        .await
        .unwrap();
    db::apply_cw20_transfers(&pool, &[terra(10)], 101, "race")
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 101)
            .await
            .unwrap(),
        BigInt::from(90)
    );
    db::apply_cw20_transfers(&pool, &[terra(10)], 102, "after-race")
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 102)
            .await
            .unwrap(),
        BigInt::from(80)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, "terra1bob", 102)
            .await
            .unwrap(),
        BigInt::from(120)
    );
}
#[tokio::test]
async fn explicit_dev_localterra_client_rejects_default_mainnet_identity() {
    let app=Router::new().fallback(get(||async {Json(serde_json::json!({"block_id":{"hash":"A".repeat(64)},"block":{"header":{"chain_id":"localterra","height":"100","time":chrono::Utc::now().to_rfc3339()}}}))}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    assert!(LcdClient::new(vec![url.clone()])
        .latest_height()
        .await
        .is_err());
    assert_eq!(
        LcdClient::new_for_chain(vec![url], "localterra")
            .latest_height()
            .await
            .unwrap(),
        100
    );
    task.abort();
}
#[tokio::test]
async fn bsc_semantic_stale_or_wrong_event_primary_fails_over_to_coherent_secondary() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (first, bad, first_task) = rpc_server().await;
    let (second, _, second_task) = rpc_server().await;
    let bsc = voting_ledger::bsc::BscClient::new(vec![first, second.clone()]);
    let cfg = config(&second);
    bad.store(13, Ordering::Relaxed);
    assert_eq!(bsc.block_number().await.unwrap(), 120);
    seed_registration(&pool, Chain::Bsc, EVM, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    bad.store(5, Ordering::Relaxed);
    assert_eq!(
        ingest::ingest_bsc_range(&pool, &bsc, &cfg, 101, 104)
            .await
            .unwrap(),
        4
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 104).await.unwrap(),
        BigInt::from(96)
    );
    first_task.abort();
    second_task.abort();
}
async fn event_index_fixture(state: State<Fixture>, uri: OriginalUri) -> Json<serde_json::Value> {
    let mut response = fixture(state, uri).await.0;
    if let Some(executions) = response
        .get_mut("tx_responses")
        .and_then(serde_json::Value::as_array_mut)
    {
        for tx in executions {
            let event = serde_json::json!({"type":"wasm","attributes":[{"key":"_contract_address","value":DEFAULT_CL8Y_CW20},{"key":"action","value":"transfer"},{"key":"from","value":"terra1carol"},{"key":"to","value":"terra1dave"},{"key":"amount","value":"1"}]});
            tx["events"].as_array_mut().unwrap().insert(0, event);
        }
    }
    Json(response)
}
#[tokio::test]
async fn terra_source_event_indices_survive_later_registration_set_changes() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let app = Router::new()
        .fallback(get(event_index_fixture))
        .with_state(Fixture {
            fail_second: Arc::new(AtomicBool::new(false)),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let lcd = LcdClient::new(vec![url.clone()]);
    let cfg = config(&url);
    seed_registration(&pool, Chain::Terra, ALICE, 100, &BigInt::from(1000), None)
        .await
        .unwrap();
    ingest::ingest_terra_height(&pool, &lcd, &cfg, 110)
        .await
        .unwrap();
    let indices: Vec<i32> = sqlx::query_scalar("SELECT msg_index FROM cl8y_cw20_transfers")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(indices.iter().all(|i| *i == 1));
    seed_registration(
        &pool,
        Chain::Terra,
        "terra1carol",
        111,
        &BigInt::from(100),
        None,
    )
    .await
    .unwrap();
    ingest::ingest_terra_height(&pool, &lcd, &cfg, 110)
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 110)
            .await
            .unwrap(),
        BigInt::from(899)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, "terra1carol", 111)
            .await
            .unwrap(),
        BigInt::from(100)
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 202);
    task.abort();
}
#[tokio::test]
async fn forward_upgrade_archives_legacy_identity_conflicts_and_preserves_frozen_power() {
    let Some((admin, _lock)) = setup().await else {
        return;
    };
    let name = format!(
        "voting_test_ledger_upgrade_{}",
        uuid::Uuid::new_v4().simple()
    );
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!(
        "COMMENT ON DATABASE {name} IS 'cl8y-voting-ephemeral-test'"
    ))
    .execute(&admin)
    .await
    .unwrap();
    let mut url = reqwest::Url::parse(&std::env::var("LEDGER_TEST_DATABASE_URL").unwrap()).unwrap();
    url.set_path(&name);
    let pool = voting_ledger::test_lock::test_pool(url.as_str())
        .await
        .unwrap();
    let all = sqlx::migrate!("./migrations");
    let old = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            all.iter()
                .filter(|m| m.version < 20261014000001)
                .cloned()
                .collect(),
        ),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    let mut conn = pool.acquire().await.unwrap();
    sqlx::query("SET search_path TO public,pg_temp")
        .execute(&mut *conn)
        .await
        .unwrap();
    old.run(&mut *conn).await.unwrap();
    drop(conn);
    db::insert_registration(&pool, Chain::Terra, ALICE, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    sqlx::query("INSERT INTO cl8y_cw20_transfers(height,tx_hash,msg_index,from_address,to_address,amount,action) VALUES(101,'legacy',0,$1,'terra1bob',1,'transfer'),(101,'legacy',0,$1,'terra1carol',2,'transfer')").bind(ALICE).execute(&pool).await.unwrap();
    let proposal = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO voting.proposals(id,chain,proposer,title,body_html,body_canonical,terra_height,bsc_block,status,opened_at) VALUES($1,'terra',$2,'legacy','body','body',101,200,'open',NOW())").bind(proposal).bind(ALICE).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO voting.proposal_snapshots(proposal_id,chain,wallet_address,weight) VALUES($1,'terra',$2,97)").bind(proposal).bind(ALICE).execute(&pool).await.unwrap();
    db::migrate(&pool).await.unwrap();
    let archive: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_legacy_identity_conflicts")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(archive, 2);
    let transfers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(transfers, 0);
    let frozen: String = sqlx::query_scalar(
        "SELECT weight::text FROM voting.proposal_snapshots WHERE proposal_id=$1",
    )
    .bind(proposal)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(frozen, "97");
    let reason: String =
        sqlx::query_scalar("SELECT degraded_reason FROM ledger_checkpoints WHERE chain='terra'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reason, "legacy_electorate_requires_reconciliation");
    db::migrate(&pool).await.unwrap();
    let shadow: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('voting._sqlx_migrations')::text")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(shadow.is_none());
    pool.close().await;
    sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
        .execute(&admin)
        .await
        .unwrap();
}
#[tokio::test]
async fn bsc_moving_tip_snapshot_keeps_observed_block_amount_pair() {
    let (url, bad, task) = rpc_server().await;
    bad.store(6, Ordering::Relaxed);
    let client = voting_ledger::bsc::BscClient::new(vec![url]);
    let snapshot = client.live_snapshot(DEFAULT_CL8Y_BEP20, EVM).await.unwrap();
    assert_eq!(snapshot.height, 120);
    assert_eq!(snapshot.amount, BigInt::from(100));
    assert_eq!(snapshot.block_hash, block_hash(120));
    assert_eq!(client.block_number().await.unwrap(), 140);
    assert_eq!(
        client.balance_of(DEFAULT_CL8Y_BEP20, EVM).await.unwrap(),
        BigInt::from(90)
    );
    task.abort();
}
#[tokio::test]
async fn bsc_registration_racing_ingest_retries_overtaken_end_of_block_snapshot() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let bob = "0x2222222222222222222222222222222222222222";
    seed_registration(&pool, Chain::Bsc, bob, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    let source = Arc::new(PausingRegistration {
        ready: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let reg_source = source.clone();
    let reg_pool = pool.clone();
    let register = tokio::spawn(async move {
        voting_ledger::register::register_wallet(
            &reg_pool,
            reg_source.as_ref(),
            Chain::Bsc,
            EVM,
            None,
        )
        .await
    });
    source.ready.notified().await;
    let mut tx = pool.begin().await.unwrap();
    db::lock_chain(&mut tx, Chain::Bsc).await.unwrap();
    db::persist_bsc_leg(&mut tx, &bsc(10, 101, 0))
        .await
        .unwrap();
    db::publish_checkpoint(&mut tx, Chain::Bsc, 101, &block_hash(101), 116)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    source.release.notify_one();
    assert!(register.await.unwrap().is_err());
    assert!(db::get_registration(&pool, Chain::Bsc, EVM)
        .await
        .unwrap()
        .is_none());
    voting_ledger::register::register_wallet(&pool, &FreshRegistration, Chain::Bsc, EVM, None)
        .await
        .unwrap();
    db::apply_bep20_transfers(&pool, &[bsc(10, 101, 0)])
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 101).await.unwrap(),
        BigInt::from(90)
    );
    db::apply_bep20_transfers(&pool, &[bsc(10, 102, 1)])
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 102).await.unwrap(),
        BigInt::from(80)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, bob, 102).await.unwrap(),
        BigInt::from(120)
    );
}
#[tokio::test]
async fn deterministic_reference_ledger_conserves_raw_decimals_across_replay_both_chains() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let terra = (0..5)
        .map(|i| format!("terra1model{i}"))
        .collect::<Vec<_>>();
    let evm = (1..=5).map(|i| format!("0x{i:040x}")).collect::<Vec<_>>();
    let mut model = vec![voting_ledger::amount::human_to_raw(1000); 5];
    for (chain, wallets) in [(Chain::Terra, &terra), (Chain::Bsc, &evm)] {
        for wallet in wallets {
            seed_registration(&pool, chain, wallet, 100, &model[0], None)
                .await
                .unwrap();
        }
    }
    let mut random = 0x36_37_38_39_u64;
    for i in 0..40 {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        let from = random as usize % 5;
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        let to = random as usize % 5;
        let amount = BigInt::from(random % 1_000_000_000_000_000_000_u64);
        model[from] -= &amount;
        model[to] += &amount;
        let height = 101 + i as i64;
        let transfer = Cw20Transfer {
            contract: DEFAULT_CL8Y_CW20.into(),
            from: terra[from].clone(),
            to: terra[to].clone(),
            amount: amount.clone(),
            action: "transfer".into(),
        };
        let hash = format!("reference{i}");
        for _ in 0..2 {
            db::apply_cw20_transfers(&pool, &[transfer.clone()], height, &hash)
                .await
                .unwrap();
        }
        let mut transfer = bsc(0, height, i);
        transfer.from = evm[from].clone();
        transfer.to = evm[to].clone();
        transfer.amount = amount;
        for _ in 0..2 {
            db::apply_bep20_transfers(&pool, &[transfer.clone()])
                .await
                .unwrap();
        }
        for (chain, wallets) in [(Chain::Terra, &terra), (Chain::Bsc, &evm)] {
            let mut total = BigInt::from(0);
            for (wallet, expected) in wallets.iter().zip(&model) {
                let actual = db::balance_at(&pool, chain, wallet, height).await.unwrap();
                assert_eq!(&actual, expected, "case {i}");
                total += actual;
            }
            assert_eq!(total, voting_ledger::amount::human_to_raw(5000));
        }
    }
}
async fn signed_draft_fixture(pool: &sqlx::PgPool) -> (uuid::Uuid, uuid::Uuid) {
    db::set_state(pool, "intents_ok", "true").await.unwrap();
    let id = uuid::Uuid::new_v4();
    let sig = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO voting.proposals(id,chain,proposer,title,body_html,body_canonical,status) VALUES($1,'bsc',$2,'race freeze','body','body','draft')").bind(id).bind(EVM).execute(pool).await.unwrap();
    let hash: String = sqlx::query_scalar("SELECT encode(sha256(convert_to('body','UTF8')),'hex')")
        .fetch_one(pool)
        .await
        .unwrap();
    let payload =
        serde_json::json!({"revision":1,"title":"race freeze","body_hash":hash}).to_string();
    sqlx::query("INSERT INTO voting.signatures(id,chain,wallet_address,signature,payload_hash,purpose,signed_payload,proposal_id,revision) VALUES($1,'bsc',$2,'synthetic','synthetic','open_vote',$3,$4,1)").bind(sig).bind(EVM).bind(payload).bind(id).execute(pool).await.unwrap();
    (id, sig)
}
#[tokio::test]
async fn freeze_racing_reconciliation_and_beyond_horizon_fork_uses_atomic_source_state() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    for (chain, wallet) in [(Chain::Terra, ALICE), (Chain::Bsc, EVM)] {
        seed_registration(&pool, chain, wallet, 100, &BigInt::from(100), None)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        db::lock_chain(&mut tx, chain).await.unwrap();
        db::publish_checkpoint(
            &mut tx,
            chain,
            100,
            &if chain == Chain::Terra {
                "A".repeat(64)
            } else {
                block_hash(100)
            },
            if chain == Chain::Bsc { 115 } else { 100 },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    let (id, sig) = signed_draft_fixture(&pool).await;
    let mut repair = pool.begin().await.unwrap();
    db::lock_chain(&mut repair, Chain::Terra).await.unwrap();
    db::lock_chain(&mut repair, Chain::Bsc).await.unwrap();
    let freeze_pool = pool.clone();
    let freeze = tokio::spawn(async move {
        sqlx::query_as::<_, (i64, i64)>("SELECT * FROM voting.freeze_proposal($1,$2)")
            .bind(id)
            .bind(sig)
            .fetch_one(&freeze_pool)
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!freeze.is_finished());
    db::persist_terra_leg(&mut repair, &terra(10), 101, "freeze race", 0)
        .await
        .unwrap();
    db::persist_bsc_leg(&mut repair, &bsc(20, 101, 0))
        .await
        .unwrap();
    db::publish_checkpoint(&mut repair, Chain::Terra, 101, &"A".repeat(64), 101)
        .await
        .unwrap();
    db::publish_checkpoint(&mut repair, Chain::Bsc, 101, &block_hash(101), 116)
        .await
        .unwrap();
    repair.commit().await.unwrap();
    assert_eq!(freeze.await.unwrap().unwrap(), (101, 101));
    let weights: Vec<String> = sqlx::query_scalar(
        "SELECT weight::text FROM voting.proposal_snapshots WHERE proposal_id=$1 ORDER BY chain",
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(weights, vec!["80", "90"]);
    let (next, sig) = signed_draft_fixture(&pool).await;
    let mut gate = pool.begin().await.unwrap();
    db::lock_chain(&mut gate, Chain::Terra).await.unwrap();
    let freeze_pool = pool.clone();
    let freeze = tokio::spawn(async move {
        sqlx::query_as::<_, (i64, i64)>("SELECT * FROM voting.freeze_proposal($1,$2)")
            .bind(next)
            .bind(sig)
            .fetch_one(&freeze_pool)
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!freeze.is_finished());
    let (url, bad, task) = rpc_server().await;
    bad.store(8, Ordering::Relaxed);
    let bsc = voting_ledger::bsc::BscClient::new(vec![url]);
    assert!(ingest::maybe_rewind_bsc(&pool, &bsc, 101).await.is_err());
    gate.commit().await.unwrap();
    assert!(freeze.await.unwrap().is_err());
    let validity: String =
        sqlx::query_scalar("SELECT snapshot_validity FROM voting.proposals WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(validity, "invalid");
    let unchanged: Vec<String> = sqlx::query_scalar(
        "SELECT weight::text FROM voting.proposal_snapshots WHERE proposal_id=$1 ORDER BY chain",
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(unchanged, weights);
    let next_status: String = sqlx::query_scalar("SELECT status FROM voting.proposals WHERE id=$1")
        .bind(next)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(next_status, "draft");
    task.abort();
}
#[tokio::test]
async fn terra_explicit_recovery_preserves_old_power_and_moves_strict_baseline() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let app=Router::new().fallback(get(|uri:OriginalUri,request:axum::http::HeaderMap|async move{if uri.0.path().contains("/smart/"){let mut headers=axum::http::HeaderMap::new();headers.insert("x-cosmos-block-height",request.get("x-cosmos-block-height").unwrap().clone());(headers,Json(serde_json::json!({"data":{"balance":"100"}})))}else{(axum::http::HeaderMap::new(),Json(serde_json::json!({"block_id":{"hash":"A".repeat(64)},"block":{"header":{"chain_id":"columbus-5","height":"110","time":chrono::Utc::now().to_rfc3339()}}})))}}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let lcd = LcdClient::new(vec![url.clone()]);
    let source = voting_ledger::bsc::CompositeBalanceSource {
        terra: Some(voting_ledger::bsc::LcdBalancePair {
            lcd: lcd.clone(),
            token: DEFAULT_CL8Y_CW20.into(),
        }),
        bsc: None,
    };
    let cfg = config(&url);
    let bsc = voting_ledger::bsc::BscClient::new(vec![]);
    seed_registration(&pool, Chain::Terra, ALICE, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    db::apply_cw20_transfers(&pool, &[terra(10)], 105, "old power")
        .await
        .unwrap();
    voting_ledger::reconcile::reconcile_live_baseline(
        &pool,
        &source,
        &lcd,
        &bsc,
        &cfg,
        Chain::Terra,
    )
    .await
    .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 105)
            .await
            .unwrap(),
        BigInt::from(90)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 110)
            .await
            .unwrap(),
        BigInt::from(100)
    );
    db::apply_cw20_transfers(&pool, &[terra(5)], 110, "boundary")
        .await
        .unwrap();
    db::apply_cw20_transfers(&pool, &[terra(10)], 111, "after baseline")
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 111)
            .await
            .unwrap(),
        BigInt::from(90)
    );
    let baseline:(i64,bool,String)=sqlx::query_as("SELECT baseline_height,baseline_verified,baseline_block_hash FROM voting_registrations WHERE chain='terra'").fetch_one(&pool).await.unwrap();
    assert_eq!(baseline, (110, true, "A".repeat(64)));
    task.abort();
}

fn physical_hash(i: usize) -> String {
    use sha2::{Digest, Sha256};
    hex::encode_upper(Sha256::digest(format!("fixture:{i}").as_bytes()))
}
#[tokio::test]
async fn native_terra_abci_body_binding_prevents_fork_window_and_normalizes_sdk_hash() {
    let fork = Arc::new(AtomicBool::new(false));
    let switch = fork.clone();
    let app=Router::new().fallback(get(move |uri:OriginalUri|{let switch=switch.clone();async move{let path=uri.0.path();let result=if path=="/status"{serde_json::json!({"node_info":{"network":"columbus-5"},"sync_info":{"catching_up":false}})}else if path=="/block"{serde_json::json!({"block_id":{"hash":if switch.load(Ordering::Relaxed){"B".repeat(64)}else{"A".repeat(64)}},"block":{"header":{"chain_id":"columbus-5","height":"110","time":chrono::Utc::now().to_rfc3339()},"data":{"txs":[]}}})}else{
        let url=reqwest::Url::parse(&format!("http://fixture{}",uri.0)).unwrap();let params=url.query_pairs().collect::<std::collections::HashMap<_,_>>();assert_eq!(params["height"],"110");assert_eq!(params["path"],"\"/cosmwasm.wasm.v1.Query/SmartContractState\"");assert!(params["data"].starts_with("0x0a"));let was_forked=switch.swap(true,Ordering::Relaxed);let json=serde_json::json!({"balance":if was_forked{"90"}else{"100"}}).to_string();let mut proto=vec![10,json.len() as u8];proto.extend_from_slice(json.as_bytes());serde_json::json!({"response":{"code":0,"height":"110","value":base64::Engine::encode(&base64::engine::general_purpose::STANDARD,proto)}})
    };Json(serde_json::json!({"jsonrpc":"2.0","id":-1,"result":result}))}}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let lcd = LcdClient::new(vec![]).with_rpc_urls(vec![url]);
    let snapshot = lcd.live_snapshot(DEFAULT_CL8Y_CW20, ALICE).await.unwrap();
    assert_eq!(snapshot.height, 110);
    assert_eq!(snapshot.amount, BigInt::from(90));
    assert_eq!(snapshot.block_hash, "B".repeat(64));
    let bulk = lcd
        .live_balances(DEFAULT_CL8Y_CW20, &[ALICE.into(), "terra1bob".into()])
        .await
        .unwrap();
    assert_eq!(bulk.balances, vec![BigInt::from(90); 2]);
    assert_eq!(bulk.block_hash, "B".repeat(64));
    let encoded =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, vec![0xBB; 32]);
    assert_eq!(
        voting_ledger::lcd::normalize_block_hash(&encoded).unwrap(),
        snapshot.block_hash
    );
    task.abort();
}
#[tokio::test]
async fn supervised_intent_pass_bounds_sql_row_lock_and_stale_progress() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let sig = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO voting.signatures(id,chain,wallet_address,signature,payload_hash,purpose) VALUES($1,'terra','terra1valid','synthetic','synthetic','register')").bind(sig).execute(&pool).await.unwrap();
    let id:uuid::Uuid=sqlx::query_scalar("INSERT INTO voting.registration_intents(chain,wallet_address,signature_id) VALUES('terra','terra1valid',$1) RETURNING id").bind(sig).fetch_one(&pool).await.unwrap();
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM voting.registration_intents WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    let success = voting_ledger::register::supervised_intent_pass(
        &pool,
        &FairSource,
        Duration::from_millis(100),
    )
    .await
    .unwrap();
    assert!(!success);
    assert_eq!(
        db::get_state(&pool, "intents_ok").await.unwrap().as_deref(),
        Some("false")
    );
    let (url, _, task) = rpc_server().await;
    let bsc = voting_ledger::bsc::BscClient::new(vec![url.clone()]);
    voting_ledger::worker::poll_chain(&pool, &None, &bsc, &config(&url), Chain::Bsc)
        .await
        .unwrap();
    assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 105);
    lock.rollback().await.unwrap();
    sqlx::query("UPDATE voting.registration_intents SET retry_after=NOW() WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(voting_ledger::register::supervised_intent_pass(
        &pool,
        &FairSource,
        Duration::from_secs(2)
    )
    .await
    .unwrap());
    let processed: bool = sqlx::query_scalar(
        "SELECT processed_at IS NOT NULL FROM voting.registration_intents WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(processed);
    sqlx::query(
        "UPDATE indexer_state SET updated_at=NOW()-INTERVAL '121 seconds' WHERE key='intents_ok'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (_, body) = voting_ledger::health::readiness(&pool).await;
    let value = serde_json::to_value(body.0).unwrap();
    assert_eq!(value["intents_ok"], false);
    assert!(value["intent_progress_age_seconds"].as_i64().unwrap() >= 121);
    task.abort();
}
#[tokio::test]
async fn physical_block_manifest_detects_missing_and_reordered_search_executions() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let mode = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let flag = mode.clone();
    let app = Router::new().fallback(get(move |uri: OriginalUri| {
        let flag = flag.clone();
        async move {
            let mut body = fixture(
                State(Fixture {
                    fail_second: Arc::new(AtomicBool::new(false)),
                }),
                uri,
            )
            .await
            .0;
            if let Some(rows) = body
                .get_mut("tx_responses")
                .and_then(serde_json::Value::as_array_mut)
            {
                match flag.load(Ordering::Relaxed) {
                    0 => {
                        rows.clear();
                        body["pagination"] = serde_json::Value::Null;
                        body["total"] = serde_json::json!("0");
                    }
                    1 => rows.reverse(),
                    _ => {}
                }
            }
            Json(body)
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let lcd = LcdClient::new(vec![url.clone()]);
    let cfg = config(&url);
    seed_registration(&pool, Chain::Terra, ALICE, 100, &BigInt::from(1000), None)
        .await
        .unwrap();
    for failure in [0, 1] {
        mode.store(failure, Ordering::Relaxed);
        assert!(ingest::ingest_terra_height(&pool, &lcd, &cfg, 110)
            .await
            .is_err());
        assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), 0);
        assert_eq!(
            db::balance_at(&pool, Chain::Terra, ALICE, 110)
                .await
                .unwrap(),
            BigInt::from(1000)
        );
    }
    mode.store(2, Ordering::Relaxed);
    ingest::ingest_terra_height(&pool, &lcd, &cfg, 110)
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 110)
            .await
            .unwrap(),
        BigInt::from(899)
    );
    task.abort();
}
#[tokio::test]
async fn logs_and_raw_metadata_representations_apply_same_physical_wasm_event_once() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let raw = Arc::new(AtomicBool::new(false));
    let flag = raw.clone();
    let app=Router::new().fallback(get(move |uri:OriginalUri|{let flag=flag.clone();async move{if uri.0.path().contains("/blocks/"){return Json(serde_json::json!({"block_id":{"hash":"A".repeat(64)},"sdk_block":{"header":{"height":"110","chain_id":"columbus-5","time":chrono::Utc::now().to_rfc3339()},"data":{"txs":[base64::Engine::encode(&base64::engine::general_purpose::STANDARD,"fixture:0")]}}}));}let wasm=serde_json::json!({"type":"wasm","attributes":[{"key":"_contract_address","value":DEFAULT_CL8Y_CW20},{"key":"action","value":"transfer"},{"key":"from","value":ALICE},{"key":"to","value":"terra1bob"},{"key":"amount","value":"10"}]});let native=|kind:&str|serde_json::json!({"type":kind,"attributes":[]});let mut raw_wasm=wasm.clone();raw_wasm["attributes"].as_array_mut().unwrap().push(serde_json::json!({"key":"msg_index","value":"0"}));let tx=if flag.load(Ordering::Relaxed){serde_json::json!({"txhash":physical_hash(0).to_ascii_lowercase(),"height":"110","code":0,"logs":[],"events":[native("tx"),native("fee"),native("message"),native("execute"),raw_wasm]})}else{serde_json::json!({"txhash":physical_hash(0),"height":"110","code":0,"logs":[{"events":[native("execute"),wasm]}],"events":[native("tx"),native("fee"),native("message"),native("execute"),raw_wasm]})};Json(serde_json::json!({"tx_responses":[tx],"pagination":null,"total":"1"}))}}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let lcd = LcdClient::new(vec![url.clone()]);
    let cfg = config(&url);
    seed_registration(&pool, Chain::Terra, ALICE, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    ingest::ingest_terra_height(&pool, &lcd, &cfg, 110)
        .await
        .unwrap();
    raw.store(true, Ordering::Relaxed);
    ingest::ingest_terra_height(&pool, &lcd, &cfg, 110)
        .await
        .unwrap();
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 110)
            .await
            .unwrap(),
        BigInt::from(90)
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    task.abort();
}
#[tokio::test]
async fn aggregate_block_memory_budget_rejects_many_large_valid_pages_before_cursor() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = calls.clone();
    let app=Router::new().fallback(get(move |uri:OriginalUri|{let count=count.clone();async move{if uri.0.path().contains("/blocks/"){return Json(serde_json::json!({"block_id":{"hash":"A".repeat(64)},"block":{"header":{"height":"110","chain_id":"columbus-5","time":chrono::Utc::now().to_rfc3339()},"data":{"txs":[]}}}));}count.fetch_add(1,Ordering::Relaxed);let url=reqwest::Url::parse(&format!("http://fixture{}",uri.0)).unwrap();let page=url.query_pairs().find(|(key,_)|key=="page").unwrap().1.parse::<usize>().unwrap();let rows=((page-1)*100..page*100).map(|i|serde_json::json!({"txhash":physical_hash(i),"height":"110","code":0,"events":[{"type":"wasm","attributes":[{"key":"_contract_address","value":"terra1other"},{"key":"memo","value":"x".repeat(4096)}]}]})).collect::<Vec<_>>();Json(serde_json::json!({"tx_responses":rows,"pagination":null,"total":"20000"}))}}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let lcd = LcdClient::new(vec![url.clone()]);
    let err = ingest::ingest_terra_height(&pool, &lcd, &config(&url), 110)
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "provider: block_memory_budget");
    assert!(calls.load(Ordering::Relaxed) < 200);
    assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), 0);
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_checkpoints")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    task.abort();
}
#[derive(Clone)]
struct FetchBarrier {
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
async fn barrier_terra(
    State(state): State<FetchBarrier>,
    uri: OriginalUri,
) -> Json<serde_json::Value> {
    if uri.0.path().contains("/blocks/") {
        return Json(
            serde_json::json!({"block_id":{"hash":"A".repeat(64)},"block":{"header":{"height":"101","chain_id":"columbus-5","time":chrono::Utc::now().to_rfc3339()},"data":{"txs":[base64::Engine::encode(&base64::engine::general_purpose::STANDARD,"fixture:0")]}}}),
        );
    }
    state.started.notify_one();
    state.release.notified().await;
    Json(
        serde_json::json!({"tx_responses":[{"txhash":physical_hash(0),"height":"101","code":0,"events":[{"type":"wasm","attributes":[{"key":"_contract_address","value":DEFAULT_CL8Y_CW20},{"key":"action","value":"transfer"},{"key":"from","value":ALICE},{"key":"to","value":"terra1bob"},{"key":"amount","value":"10"}]}]}],"pagination":null,"total":"1"}),
    )
}
async fn barrier_bsc(
    State(state): State<FetchBarrier>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let result = match body["method"].as_str().unwrap() {
        "eth_chainId" => serde_json::json!("0x38"),
        "eth_blockNumber" => serde_json::json!("0x74"),
        "eth_getBlockByNumber" => {
            let h = i64::from_str_radix(
                body["params"][0].as_str().unwrap().trim_start_matches("0x"),
                16,
            )
            .unwrap();
            serde_json::json!({"number":body["params"][0],"hash":block_hash(h),"timestamp":format!("0x{:x}",chrono::Utc::now().timestamp())})
        }
        "eth_getLogs" => {
            state.started.notify_one();
            state.release.notified().await;
            serde_json::json!([{"address":DEFAULT_CL8Y_BEP20,"topics":[voting_ledger::parser::TRANSFER_TOPIC,format!("0x{:0>64}",EVM.trim_start_matches("0x")),format!("0x{:0>64}","2222222222222222222222222222222222222222")],"data":format!("0x{:064x}",10),"blockNumber":"0x65","blockHash":block_hash(101),"transactionHash":block_hash(10001),"logIndex":"0x0","removed":false}])
        }
        _ => panic!("unexpected method"),
    };
    Json(serde_json::json!({"jsonrpc":"2.0","id":1,"result":result}))
}
#[tokio::test]
async fn registration_cannot_insert_into_stale_registered_set_during_http_fetch_both_chains() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    for chain in [Chain::Terra, Chain::Bsc] {
        let (wallet, bob) = if chain == Chain::Terra {
            (ALICE, "terra1bob")
        } else {
            (EVM, "0x2222222222222222222222222222222222222222")
        };
        seed_registration(&pool, chain, bob, 100, &BigInt::from(100), None)
            .await
            .unwrap();
        let mut checkpoint = pool.begin().await.unwrap();
        db::lock_chain(&mut checkpoint, chain).await.unwrap();
        db::publish_checkpoint(
            &mut checkpoint,
            chain,
            100,
            &if chain == Chain::Terra {
                "A".repeat(64)
            } else {
                block_hash(100)
            },
            if chain == Chain::Terra { 100 } else { 115 },
        )
        .await
        .unwrap();
        checkpoint.commit().await.unwrap();
        let state = FetchBarrier {
            started: Arc::new(tokio::sync::Notify::new()),
            release: Arc::new(tokio::sync::Notify::new()),
        };
        let app = if chain == Chain::Terra {
            Router::new()
                .fallback(get(barrier_terra))
                .with_state(state.clone())
        } else {
            Router::new()
                .fallback(axum::routing::post(barrier_bsc))
                .with_state(state.clone())
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let ingest_pool = pool.clone();
        let cfg = config(&url);
        let ingest = tokio::spawn(async move {
            match chain {
                Chain::Terra => {
                    ingest::ingest_terra_height(&ingest_pool, &LcdClient::new(vec![url]), &cfg, 101)
                        .await
                }
                Chain::Bsc => {
                    ingest::ingest_bsc_range(
                        &ingest_pool,
                        &voting_ledger::bsc::BscClient::new(vec![url]),
                        &cfg,
                        101,
                        101,
                    )
                    .await
                }
            }
        });
        state.started.notified().await;
        let source = Arc::new(PausingRegistration {
            ready: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        });
        source.release.notify_one();
        let reg_source = source.clone();
        let reg_pool = pool.clone();
        let register = tokio::spawn(async move {
            voting_ledger::register::register_wallet(
                &reg_pool,
                reg_source.as_ref(),
                chain,
                wallet,
                None,
            )
            .await
        });
        source.ready.notified().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!register.is_finished());
        assert!(db::get_registration(&pool, chain, wallet)
            .await
            .unwrap()
            .is_none());
        state.release.notify_one();
        ingest.await.unwrap().unwrap();
        assert!(register.await.unwrap().is_err());
        voting_ledger::register::register_wallet(&pool, &FreshRegistration, chain, wallet, None)
            .await
            .unwrap();
        assert_eq!(
            db::balance_at(&pool, chain, wallet, 101).await.unwrap(),
            BigInt::from(90)
        );
        assert_eq!(
            db::balance_at(&pool, chain, bob, 101).await.unwrap(),
            BigInt::from(110)
        );
        server.abort();
    }
}
#[derive(Clone)]
struct RpcHttpFault {
    inner: RpcFixture,
}
async fn rpc_http_fault(
    State(state): State<RpcHttpFault>,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if body["method"] == "eth_getLogs" {
        match state.inner.bad.load(Ordering::Relaxed) {
            20 => {
                return (axum::http::StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response()
            }
            21 => return (axum::http::StatusCode::OK, "malformed successful JSON").into_response(),
            22 => {
                tokio::time::sleep(Duration::from_secs(30)).await;
                return "delayed response".into_response();
            }
            _ => {}
        }
    }
    rpc_fixture(State(state.inner), Json(body))
        .await
        .into_response()
}
#[tokio::test]
async fn bsc_http_429_timeout_malformed_json_and_empty_logs_preserve_atomic_progress() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let mode = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let app = Router::new()
        .fallback(axum::routing::post(rpc_http_fault))
        .with_state(RpcHttpFault {
            inner: RpcFixture {
                bad: mode.clone(),
                tip: Arc::new(std::sync::atomic::AtomicI64::new(120)),
            },
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = voting_ledger::bsc::BscClient::new(vec![url.clone()]);
    let cfg = config(&url);
    seed_registration(&pool, Chain::Bsc, EVM, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    db::lock_chain(&mut tx, Chain::Bsc).await.unwrap();
    db::publish_checkpoint(&mut tx, Chain::Bsc, 100, &block_hash(100), 115)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for (offset, fault) in [20, 21, 22].into_iter().enumerate() {
        let height = 101 + offset as i64;
        let before: (i64, String, i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
            "SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain='bsc'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        mode.store(fault, Ordering::Relaxed);
        assert!(
            tokio::time::timeout(
                Duration::from_secs(10),
                ingest::ingest_bsc_range(&pool, &client, &cfg, height, height)
            )
            .await
            .unwrap()
            .is_err(),
            "fault {fault}"
        );
        let after: (i64, String, i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
            "SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain='bsc'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after, before, "fault {fault}");
        assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), height - 1);
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_bep20_transfers")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, offset as i64);
        assert_eq!(
            db::balance_at(&pool, Chain::Bsc, EVM, height)
                .await
                .unwrap(),
            BigInt::from(100 - offset)
        );
        mode.store(0, Ordering::Relaxed);
        assert_eq!(
            ingest::ingest_bsc_range(&pool, &client, &cfg, height, height)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            db::balance_at(&pool, Chain::Bsc, EVM, height)
                .await
                .unwrap(),
            BigInt::from(99 - offset)
        );
    }
    mode.store(18, Ordering::Relaxed);
    assert_eq!(
        ingest::ingest_bsc_range(&pool, &client, &cfg, 104, 104)
            .await
            .unwrap(),
        0
    );
    let checkpoint: (i64, String, i64, bool, bool) = sqlx::query_as(
        "SELECT height,block_hash,epoch,complete,canonical FROM ledger_checkpoints WHERE chain='bsc'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(checkpoint, (104, block_hash(104), 1, true, true));
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 104).await.unwrap(),
        BigInt::from(97)
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_bep20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 3);
    server.abort();
}
#[tokio::test]
async fn valid_empty_terra_block_advances_bound_cursor_without_invented_events() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let app = Router::new().fallback(get(|uri: OriginalUri| async move {
        if uri.0.path().contains("/blocks/") {
            Json(serde_json::json!({
                "block_id": {
                    "hash": base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        vec![0xAA; 32],
                    )
                },
                "sdk_block": {
                    "header": {
                        "height": "110", "chain_id": "columbus-5",
                        "time": chrono::Utc::now().to_rfc3339()
                    },
                    "data": {"txs": null}
                }
            }))
        } else {
            Json(
                serde_json::from_str::<serde_json::Value>(include_str!(
                    "fixtures/terra-supported-empty.json"
                ))
                .unwrap(),
            )
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    seed_registration(&pool, Chain::Terra, ALICE, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    assert_eq!(
        ingest::ingest_terra_height(
            &pool,
            &LcdClient::new(vec![url.clone()]),
            &config(&url),
            110
        )
        .await
        .unwrap(),
        0
    );
    let checkpoint: (i64, String, bool, bool) = sqlx::query_as(
        "SELECT height,block_hash,complete,canonical FROM ledger_checkpoints WHERE chain='terra'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(checkpoint, (110, "A".repeat(64), true, true));
    assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), 110);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 110)
            .await
            .unwrap(),
        BigInt::from(100)
    );
    server.abort();
}
#[tokio::test]
async fn repeated_terra_pagination_key_stops_after_two_pages_without_progress() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let pages = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = pages.clone();
    let app = Router::new().fallback(get(move |uri: OriginalUri| {
        let count = count.clone();
        async move {
            let mut body = fixture(
                State(Fixture {
                    fail_second: Arc::new(AtomicBool::new(false)),
                }),
                uri,
            )
            .await
            .0;
            if body.get("tx_responses").is_some() {
                count.fetch_add(1, Ordering::Relaxed);
                body["pagination"]["total"] = serde_json::json!("1000");
                body["pagination"]["next_key"] = serde_json::json!("repeated+/=");
            }
            Json(body)
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    seed_registration(&pool, Chain::Terra, ALICE, 100, &BigInt::from(1000), None)
        .await
        .unwrap();
    let error = ingest::ingest_terra_height(
        &pool,
        &LcdClient::new(vec![url.clone()]),
        &config(&url),
        110,
    )
    .await
    .unwrap_err();
    assert_eq!(error.to_string(), "provider: repeated_tx_page");
    assert_eq!(pages.load(Ordering::Relaxed), 2);
    assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), 0);
    let checkpoints: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_checkpoints")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(checkpoints, 0);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 110)
            .await
            .unwrap(),
        BigInt::from(1000)
    );
    server.abort();
}
