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
        "eth_call" => {
            if mode == 24 {
                f.tip.store(135, Ordering::Relaxed);
                serde_json::json!(format!("0x{:064x}", 0))
            } else {
                serde_json::json!(format!(
                    "0x{:064x}",
                    if mode == 6 && params[1] == "latest" {
                        90
                    } else {
                        100
                    }
                ))
            }
        }
        "eth_getLogs" => {
            if mode == 24 {
                return Json(
                    serde_json::json!({"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"execution history unavailable"}}),
                );
            }
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
                    26 => log["data"] = serde_json::json!(format!("0x{}", "f".repeat(64))),
                    _ => {}
                }
                if mode == 27 {
                    log["data"] = serde_json::json!(format!("0x{:064x}", 10));
                    log["logIndex"] = serde_json::json!("0x1");
                    let mut credit = log.clone();
                    credit["topics"][1] = serde_json::json!(format!("0x{}", "0".repeat(64)));
                    credit["topics"][2] =
                        serde_json::json!(format!("0x{:0>64}", EVM.trim_start_matches("0x")));
                    credit["logIndex"] = serde_json::json!("0x0");
                    logs.push(credit);
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

#[tokio::test]
async fn process_startup_recovery_and_registration_use_configured_native_terra_rpc() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let native_queries = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let lcd_queries = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = lcd_queries.clone();
    let lcd_app = Router::new().fallback(get(move |uri: OriginalUri| {
        let count = count.clone();
        async move {
            if uri.0.path().contains("/smart/") {
                count.fetch_add(1, Ordering::Relaxed);
                // The deployed LCD returns a valid balance without an observed-height header.
                Json(serde_json::json!({"data":{"balance":"999999999"}}))
            } else if uri.0.path().contains("/blocks/") {
                Json(serde_json::json!({
                    "block_id":{"hash":base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD, vec![0xAA;32]
                    )},
                    "sdk_block":{"header":{
                        "chain_id":"columbus-5","height":"110",
                        "time":chrono::Utc::now().to_rfc3339()
                    },"data":{"txs":null}}
                }))
            } else {
                Json(serde_json::json!({"pagination":null,"total":"0","tx_responses":[]}))
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let lcd_url = format!("http://{}", listener.local_addr().unwrap());
    let lcd_task = tokio::spawn(async move { axum::serve(listener, lcd_app).await.unwrap() });
    let count = native_queries.clone();
    let native_app = Router::new().fallback(get(move |uri: OriginalUri| {
        let count = count.clone();
        async move {
            let result = match uri.0.path() {
                "/status" => serde_json::json!({
                    "node_info":{"network":"columbus-5"},
                    "sync_info":{"catching_up":false}
                }),
                "/block" => serde_json::json!({
                    "block_id":{"hash":"A".repeat(64)},
                    "block":{"header":{
                        "chain_id":"columbus-5","height":"110",
                        "time":chrono::Utc::now().to_rfc3339()
                    },"data":{"txs":[]}}
                }),
                "/abci_query" => {
                    count.fetch_add(1, Ordering::Relaxed);
                    let url = reqwest::Url::parse(&format!("http://fixture{}", uri.0)).unwrap();
                    let params = url
                        .query_pairs()
                        .collect::<std::collections::HashMap<_, _>>();
                    assert_eq!(params["height"], "110");
                    assert_eq!(
                        params["path"],
                        "\"/cosmwasm.wasm.v1.Query/SmartContractState\""
                    );
                    assert!(params["data"].starts_with("0x0a"));
                    assert_eq!(params["prove"], "false");
                    let bytes = br#"{"balance":"90"}"#;
                    let mut proto = vec![10, bytes.len() as u8];
                    proto.extend_from_slice(bytes);
                    serde_json::json!({"response":{
                        "code":0,"height":"110",
                        "value":base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD, proto
                        )
                    }})
                }
                _ => panic!("unexpected native RPC path"),
            };
            Json(serde_json::json!({"jsonrpc":"2.0","id":-1,"result":result}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let native_url = format!("http://{}", listener.local_addr().unwrap());
    let native_task = tokio::spawn(async move { axum::serve(listener, native_app).await.unwrap() });
    let (bsc_url, _, bsc_task) = rpc_server().await;
    seed_registration(&pool, Chain::Terra, ALICE, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    sqlx::query("INSERT INTO cl8y_balances(wallet_address,height,balance) VALUES($1,100,100)")
        .bind(ALICE)
        .execute(&pool)
        .await
        .unwrap();
    db::degrade(&pool, Chain::Terra, "baseline_recovery_requested")
        .await
        .unwrap();
    // Actual database errors expose a safe SQLSTATE and never server-provided detail.
    let error = LedgerError::from(
        sqlx::query("DO $$ BEGIN RAISE EXCEPTION 'SENTINEL private credential' USING ERRCODE='23505'; END $$")
            .execute(&pool)
            .await
            .unwrap_err(),
    );
    assert_eq!(error.to_string(), "database error");
    assert_eq!(format!("{error:?}"), "database error (sqlstate=23505)");

    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reserve.local_addr().unwrap();
    drop(reserve);
    let http = reqwest::Client::new();
    let ready_url = format!("http://{address}/ready");
    let mut epoch = 0;
    for restart in 0..2 {
        let mut child = ChildGuard(
            std::process::Command::new(env!("CARGO_BIN_EXE_voting-ledger"))
                .env_clear()
                .env(
                    "DATABASE_URL",
                    std::env::var("LEDGER_TEST_DATABASE_URL").unwrap(),
                )
                .env("RUN_MODE", "prod")
                .env("TERRA_LCD_URL", &lcd_url)
                .env("TERRA_RPC_URLS", &native_url)
                .env("BSC_RPC_URLS", &bsc_url)
                .env("LEDGER_RECONCILE_LIVE_BASELINE", "true")
                .env("API_BIND", address.to_string())
                .env("POLL_INTERVAL_MS", "100")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "ledger startup exited"
                );
                if http
                    .get(&ready_url)
                    .send()
                    .await
                    .is_ok_and(|r| r.status().is_success())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        let checkpoint: (i64, i64, String, bool, bool) = sqlx::query_as(
            "SELECT height,epoch,block_hash,complete,canonical FROM ledger_checkpoints WHERE chain='terra'",
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(
            (checkpoint.0, checkpoint.2, checkpoint.3, checkpoint.4),
            (110, "A".repeat(64), true, true)
        );
        if restart == 0 {
            epoch = checkpoint.1;
            assert_eq!(native_queries.load(Ordering::Relaxed), 1);
            assert_eq!(
                db::balance_at(&pool, Chain::Terra, ALICE, 110)
                    .await
                    .unwrap(),
                BigInt::from(90)
            );
            let original: String = sqlx::query_scalar(
                "SELECT balance::text FROM cl8y_balances WHERE wallet_address=$1 AND height=100",
            )
            .bind(ALICE)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(original, "100");
            let signature = uuid::Uuid::new_v4();
            sqlx::query("INSERT INTO voting.signatures(id,chain,wallet_address,signature,payload_hash,purpose) VALUES($1,'terra','terra1entrypoint','synthetic','synthetic','register')")
                .bind(signature).execute(&pool).await.unwrap();
            let intent: uuid::Uuid = sqlx::query_scalar("INSERT INTO voting.registration_intents(chain,wallet_address,signature_id) VALUES('terra','terra1entrypoint',$1) RETURNING id")
                .bind(signature).fetch_one(&pool).await.unwrap();
            tokio::time::timeout(Duration::from_secs(8), async {
                loop {
                    let processed: bool = sqlx::query_scalar("SELECT processed_at IS NOT NULL FROM voting.registration_intents WHERE id=$1")
                        .bind(intent).fetch_one(&pool).await.unwrap();
                    if processed { break; }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }).await.unwrap();
            assert_eq!(
                db::balance_at(&pool, Chain::Terra, "terra1entrypoint", 110)
                    .await
                    .unwrap(),
                BigInt::from(90)
            );
            let verified: bool = sqlx::query_scalar("SELECT baseline_verified AND baseline_height=110 AND baseline_block_hash=$1 FROM voting_registrations WHERE wallet_address='terra1entrypoint'")
                .bind("A".repeat(64)).fetch_one(&pool).await.unwrap();
            assert!(verified);
        } else {
            assert_eq!(
                checkpoint.1, epoch,
                "restart must preserve the recovered epoch"
            );
        }
        assert_eq!(native_queries.load(Ordering::Relaxed), 2);
        assert_eq!(lcd_queries.load(Ordering::Relaxed), 0);
        assert!(http
            .get(&ready_url)
            .send()
            .await
            .unwrap()
            .status()
            .is_success());
        assert!(std::process::Command::new("kill")
            .arg("-TERM")
            .arg(child.0.id().to_string())
            .status()
            .unwrap()
            .success());
        tokio::time::timeout(Duration::from_secs(6), async {
            loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    assert!(status.success());
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
    }
    lcd_task.abort();
    native_task.abort();
    bsc_task.abort();
}

#[tokio::test]
async fn writer_pool_pins_every_connection_against_startup_search_path_shadow_tables() {
    use sqlx::Connection;
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    for (chain, wallet) in [(Chain::Terra, ALICE), (Chain::Bsc, EVM)] {
        seed_registration(&pool, chain, wallet, 100, &BigInt::from(100), None)
            .await
            .unwrap();
        sqlx::query("UPDATE public.voting_registrations SET baseline_height=100 WHERE chain=$1 AND wallet_address=$2")
            .bind(chain.as_str()).bind(wallet).execute(&pool).await.unwrap();
        let mut transaction = pool.begin().await.unwrap();
        db::lock_chain(&mut transaction, chain).await.unwrap();
        db::publish_checkpoint(
            &mut transaction,
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
        transaction.commit().await.unwrap();
    }
    let (proposal, signature) = signed_draft_fixture(&pool).await;
    sqlx::query("SELECT * FROM voting.freeze_proposal($1,$2)")
        .bind(proposal)
        .bind(signature)
        .execute(&pool)
        .await
        .unwrap();
    let frozen_before: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT chain,wallet_address,weight::text FROM voting.proposal_snapshots WHERE proposal_id=$1 ORDER BY chain,wallet_address",
    ).bind(proposal).fetch_all(&pool).await.unwrap();
    sqlx::query("CREATE TABLE voting.voting_registrations(chain text,wallet_address text,registered_at_height bigint,initial_balance numeric,status text)")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO voting.voting_registrations VALUES ('terra',$1,1,999,'active'),('terra','terra1shadowonly',1,888,'active')")
        .bind(ALICE).execute(&pool).await.unwrap();
    let mut url = reqwest::Url::parse(&std::env::var("LEDGER_TEST_DATABASE_URL").unwrap()).unwrap();
    url.query_pairs_mut()
        .append_pair("options", "-csearch_path=voting,public");
    let mut unsafe_connection = sqlx::PgConnection::connect(url.as_str()).await.unwrap();
    let shadow: String = sqlx::query_scalar(
        "SELECT initial_balance::text FROM voting_registrations WHERE wallet_address=$1",
    )
    .bind(ALICE)
    .fetch_one(&mut unsafe_connection)
    .await
    .unwrap();
    assert_eq!(
        shadow, "999",
        "startup options actually select the legacy shadow"
    );
    unsafe_connection.close().await.unwrap();

    let writer = db::connect(url.as_str()).await.unwrap();
    db::migrate(&writer).await.unwrap();
    assert_eq!(
        db::registered_set(&writer, Chain::Terra).await.unwrap(),
        vec![ALICE.to_string()]
    );
    assert_eq!(
        db::get_registration(&writer, Chain::Terra, ALICE)
            .await
            .unwrap()
            .unwrap()
            .initial_balance,
        "100"
    );
    assert!(db::insert_registration(
        &writer,
        Chain::Terra,
        "terra1freshpool",
        101,
        &BigInt::from(7),
        None
    )
    .await
    .unwrap());

    // Hold every default-pool slot simultaneously so these checks cannot reuse one connection.
    let mut connections = Vec::new();
    let mut process_ids = std::collections::HashSet::new();
    for _ in 0..10 {
        let mut connection = writer.acquire().await.unwrap();
        let path: String = sqlx::query_scalar("SHOW search_path")
            .fetch_one(&mut *connection)
            .await
            .unwrap();
        assert_eq!(path, "pg_catalog, public, pg_temp");
        let height: i64 = sqlx::query_scalar(
            "SELECT baseline_height FROM voting_registrations WHERE wallet_address=$1",
        )
        .bind(ALICE)
        .fetch_one(&mut *connection)
        .await
        .unwrap();
        assert_eq!(height, 100);
        let process: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *connection)
            .await
            .unwrap();
        assert!(process_ids.insert(process));
        connections.push(connection);
    }
    connections.pop().unwrap().close().await.unwrap();
    let mut replacement = writer.acquire().await.unwrap();
    let (path, process): (String, i32) =
        sqlx::query_as("SELECT current_setting('search_path'),pg_backend_pid()")
            .fetch_one(&mut *replacement)
            .await
            .unwrap();
    assert_eq!(path, "pg_catalog, public, pg_temp");
    assert!(
        !process_ids.contains(&process),
        "replacement uses a newly opened physical connection"
    );
    drop(replacement);
    drop(connections);
    let canonical_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM public.voting_registrations WHERE wallet_address='terra1freshpool'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let shadow_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM voting.voting_registrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(canonical_count, 1);
    assert_eq!(shadow_count, 2, "shadow history remains untouched");
    let frozen_after: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT chain,wallet_address,weight::text FROM voting.proposal_snapshots WHERE proposal_id=$1 ORDER BY chain,wallet_address",
    ).bind(proposal).fetch_all(&pool).await.unwrap();
    assert_eq!(frozen_after, frozen_before);
    assert_eq!(
        db::balance_at(&writer, Chain::Terra, ALICE, 100)
            .await
            .unwrap(),
        BigInt::from(100)
    );
    writer.close().await;
    sqlx::query("DROP TABLE voting.voting_registrations")
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn process_storage_inspection_precedes_migrations_and_never_starts_accounting() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    seed_registration(&pool, Chain::Terra, ALICE, 100, &BigInt::from(100), None)
        .await
        .unwrap();
    for chain in [Chain::Terra, Chain::Bsc] {
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
            if chain == Chain::Terra { 100 } else { 115 },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    let (proposal, signature) = signed_draft_fixture(&pool).await;
    sqlx::query("SELECT * FROM voting.freeze_proposal($1,$2)")
        .bind(proposal)
        .bind(signature)
        .execute(&pool)
        .await
        .unwrap();
    let frozen_before:Vec<(String,String)>=sqlx::query_as("SELECT wallet_address,weight::text FROM voting.proposal_snapshots WHERE proposal_id=$1 ORDER BY wallet_address")
        .bind(proposal).fetch_all(&pool).await.unwrap();
    sqlx::query("CREATE TABLE voting.voting_registrations(chain text,wallet_address text,registered_at_height bigint,initial_balance numeric,status text,private_sentinel text)").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO voting.voting_registrations VALUES('terra',$1,100,999,'active','SENTINEL_PRIVATE_COLUMN_VALUE')").bind(ALICE).execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE voting.indexer_state(key text,value text)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO voting.indexer_state VALUES('last_indexed_height','200'),('last_indexed_bsc_block','201'),('private_provider_url','SENTINEL_PRIVATE_STATE_VALUE')").execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE voting._sqlx_migrations(version bigint)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO voting._sqlx_migrations VALUES(13)")
        .execute(&pool)
        .await
        .unwrap();
    // If the entrypoint ran migrate first, it would create/replay tracking and fail on old objects.
    sqlx::query("ALTER TABLE public._sqlx_migrations RENAME TO inspection_saved_migrations")
        .execute(&pool)
        .await
        .unwrap();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = calls.clone();
    let app = Router::new().fallback(move || {
        let count = count.clone();
        async move {
            count.fetch_add(1, Ordering::Relaxed);
            Json(serde_json::json!({}))
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reserve.local_addr().unwrap();
    drop(reserve);
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_voting-ledger"))
        .kill_on_drop(true)
        .env_clear()
        .env(
            "DATABASE_URL",
            std::env::var("LEDGER_TEST_DATABASE_URL").unwrap(),
        )
        .env("RUN_MODE", "prod")
        .env("API_BIND", address.to_string())
        .env("TERRA_LCD_URL", &provider)
        .env("TERRA_RPC_URLS", &provider)
        .env("BSC_RPC_URLS", &provider)
        .env("LEDGER_INSPECT_STORAGE_ONLY", "true")
        .env("LEDGER_RECONCILE_LIVE_BASELINE", "true")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let http = reqwest::Client::new();
    let base = format!("http://{address}");
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            assert!(
                child.try_wait().unwrap().is_none(),
                "inspection process exited"
            );
            if http
                .get(format!("{base}/live"))
                .send()
                .await
                .is_ok_and(|r| r.status().is_success())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    for path in ["/ready", "/health"] {
        let response = http.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["ready"], false);
        assert_eq!(body["reason"], "storage_inspection");
    }
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        calls.load(Ordering::Relaxed),
        0,
        "no provider/registration/recovery workers start"
    );
    let canonical = db::connect(&std::env::var("LEDGER_TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    assert_eq!(db::indexed_terra_height(&canonical).await.unwrap(), 100);
    assert_eq!(db::indexed_bsc_block(&canonical).await.unwrap(), 100);
    canonical.close().await;
    let missing: bool =
        sqlx::query_scalar("SELECT pg_catalog.to_regclass('public._sqlx_migrations') IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        missing,
        "inspection never creates migration tracking or runs DDL"
    );
    let public: String = sqlx::query_scalar(
        "SELECT initial_balance::text FROM public.voting_registrations WHERE wallet_address=$1",
    )
    .bind(ALICE)
    .fetch_one(&pool)
    .await
    .unwrap();
    let shadow: String = sqlx::query_scalar(
        "SELECT initial_balance::text FROM voting.voting_registrations WHERE wallet_address=$1",
    )
    .bind(ALICE)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((public, shadow), ("100".into(), "999".into()));
    let frozen_after:Vec<(String,String)>=sqlx::query_as("SELECT wallet_address,weight::text FROM voting.proposal_snapshots WHERE proposal_id=$1 ORDER BY wallet_address")
        .bind(proposal).fetch_all(&pool).await.unwrap();
    assert_eq!(frozen_after, frozen_before);
    assert!(std::process::Command::new("kill")
        .arg("-TERM")
        .arg(child.id().unwrap().to_string())
        .status()
        .unwrap()
        .success());
    let output = tokio::time::timeout(Duration::from_secs(6), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let markers: Vec<_> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("VOTING_STORAGE_INVENTORY "))
        .collect();
    assert_eq!(markers.len(), 1);
    let inventory: serde_json::Value = serde_json::from_str(markers[0]).unwrap();
    assert_eq!(inventory["read_only"], true);
    let relations = inventory["relations"].as_array().unwrap();
    assert_eq!(relations.len(), 14);
    let find = |schema: &str, table: &str| {
        relations
            .iter()
            .find(|r| r["schema"] == schema && r["table"] == table)
            .unwrap()
    };
    assert_eq!(find("public", "voting_registrations")["row_count"], 1);
    assert_eq!(find("voting", "voting_registrations")["row_count"], 1);
    assert_eq!(
        find("public", "voting_registrations")["baseline_height_present"],
        true
    );
    assert_eq!(
        find("voting", "voting_registrations")["baseline_height_present"],
        false
    );
    assert_eq!(find("voting", "indexer_state")["last_indexed_height"], 200);
    assert_eq!(
        find("voting", "indexer_state")["last_indexed_bsc_block"],
        201
    );
    assert_eq!(find("public", "_sqlx_migrations")["exists"], false);
    assert_eq!(
        find("voting", "_sqlx_migrations")["migration_max_version"],
        13
    );
    let difference = inventory["comparisons"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["table"] == "voting_registrations")
        .unwrap();
    assert_eq!(difference["public_only_rows"], 1);
    assert_eq!(difference["voting_only_rows"], 1);
    for secret in [
        ALICE,
        "SENTINEL",
        "private_sentinel",
        "private_provider_url",
        &provider,
    ] {
        assert!(!stdout.contains(secret));
    }
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stderr.contains("SENTINEL"));
    sqlx::query(
        "DROP TABLE voting.voting_registrations,voting.indexer_state,voting._sqlx_migrations",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE public.inspection_saved_migrations RENAME TO _sqlx_migrations")
        .execute(&pool)
        .await
        .unwrap();
    // A normal existing tracker is also inspected without changing its version history.
    let completed = voting_ledger::storage::inventory(&pool).await.unwrap();
    assert!(completed
        .relations
        .iter()
        .find(|r| r.schema == "public" && r.table == "_sqlx_migrations")
        .unwrap()
        .migration_max_version
        .is_some());
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE public.cl8y_balances IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    let bounded = tokio::time::timeout(
        Duration::from_secs(3),
        voting_ledger::storage::inventory(&pool),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        bounded
            .relations
            .iter()
            .find(|r| r.schema == "public" && r.table == "cl8y_balances")
            .unwrap()
            .row_count
            .is_none(),
        "an unavailable count is null, never a false zero"
    );
    assert!(
        bounded
            .relations
            .iter()
            .find(|r| r.schema == "public" && r.table == "voting_registrations")
            .unwrap()
            .row_count
            .is_some(),
        "a bounded query failure does not poison the inventory transaction"
    );
    lock.rollback().await.unwrap();
    server.abort();
}

#[derive(Clone)]
struct TerraForkFixture {
    tip: Arc<std::sync::atomic::AtomicI64>,
    fork_after: Arc<std::sync::atomic::AtomicI64>,
    fail_hash: Arc<AtomicBool>,
    missing_from: Arc<std::sync::atomic::AtomicI64>,
    live_balance: Arc<std::sync::atomic::AtomicI64>,
}
impl TerraForkFixture {
    fn new(tip: i64) -> Self {
        Self {
            tip: Arc::new(std::sync::atomic::AtomicI64::new(tip)),
            fork_after: Arc::new(std::sync::atomic::AtomicI64::new(i64::MAX)),
            fail_hash: Arc::new(AtomicBool::new(false)),
            missing_from: Arc::new(std::sync::atomic::AtomicI64::new(i64::MAX)),
            live_balance: Arc::new(std::sync::atomic::AtomicI64::new(1000)),
        }
    }
    fn changed(&self, height: i64) -> bool {
        height > self.fork_after.load(Ordering::Relaxed)
    }
    fn hash(&self, height: i64) -> String {
        format!(
            "{:064X}",
            height + if self.changed(height) { 1_000_000 } else { 0 }
        )
    }
    fn bytes(&self, height: i64) -> String {
        format!("terra-indexed-fork:{height}:{}", self.changed(height))
    }
}
async fn terra_fork_http(
    State(state): State<TerraForkFixture>,
    uri: OriginalUri,
    request: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use sha2::{Digest, Sha256};
    let path = uri.0.path();
    if path.contains("/smart/") {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-cosmos-block-height",
            request.get("x-cosmos-block-height").unwrap().clone(),
        );
        return (headers,Json(serde_json::json!({"data":{"balance":state.live_balance.load(Ordering::Relaxed).to_string()}}))).into_response();
    }
    if path.contains("/blocks/") {
        let suffix = path.rsplit('/').next().unwrap();
        if suffix != "latest" && state.fail_hash.load(Ordering::Relaxed) {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "canonical hash unavailable",
            )
                .into_response();
        }
        let height = if suffix == "latest" {
            state.tip.load(Ordering::Relaxed)
        } else {
            suffix.parse::<i64>().unwrap()
        };
        return Json(serde_json::json!({"block_id":{"hash":state.hash(height)},"sdk_block":{
            "header":{"height":height.to_string(),"chain_id":"columbus-5","time":chrono::Utc::now().to_rfc3339()},
            "data":{"txs":[base64::Engine::encode(&base64::engine::general_purpose::STANDARD,state.bytes(height))]}
        }})).into_response();
    }
    let url = reqwest::Url::parse(&format!("http://fixture{}", uri.0)).unwrap();
    let height = url
        .query_pairs()
        .find(|(k, _)| k == "query")
        .unwrap()
        .1
        .split('=')
        .nth(1)
        .unwrap()
        .parse::<i64>()
        .unwrap();
    if height >= state.missing_from.load(Ordering::Relaxed) {
        return (
            axum::http::StatusCode::NOT_FOUND,
            "execution history unavailable",
        )
            .into_response();
    }
    Json(serde_json::json!({"pagination":null,"total":"1","tx_responses":[{
        "height":height.to_string(),"txhash":hex::encode_upper(Sha256::digest(state.bytes(height).as_bytes())),"code":0,"logs":[],
        "events":[{"type":"wasm","attributes":[
            {"key":"_contract_address","value":DEFAULT_CL8Y_CW20},
            {"key":"action","value":"transfer"},{"key":"from","value":ALICE},{"key":"to","value":"terra1bob"},
            {"key":"amount","value":if state.changed(height){"2"}else{"1"}}
        ]}]
    }]})).into_response()
}
async fn terra_fork_server(tip: i64) -> (String, TerraForkFixture, tokio::task::JoinHandle<()>) {
    let state = TerraForkFixture::new(tip);
    let app = Router::new()
        .fallback(get(terra_fork_http))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, state, server)
}
async fn seed_terra_fork_electorate(
    pool: &sqlx::PgPool,
    state: &TerraForkFixture,
    recipient: bool,
) {
    for (chain, wallet, balance) in [(Chain::Terra, ALICE, 1000), (Chain::Bsc, EVM, 1000)] {
        seed_registration(pool, chain, wallet, 100, &BigInt::from(balance), None)
            .await
            .unwrap();
    }
    if recipient {
        seed_registration(pool, Chain::Terra, "terra1bob", 100, &BigInt::from(0), None)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE public.voting_registrations SET baseline_height=100,baseline_block_hash=$1 WHERE chain='terra'")
        .bind(state.hash(100)).execute(pool).await.unwrap();
    for chain in [Chain::Terra, Chain::Bsc] {
        let mut tx = pool.begin().await.unwrap();
        db::lock_chain(&mut tx, chain).await.unwrap();
        db::publish_checkpoint(
            &mut tx,
            chain,
            100,
            &if chain == Chain::Terra {
                state.hash(100)
            } else {
                block_hash(100)
            },
            if chain == Chain::Terra { 100 } else { 115 },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    db::set_state(pool, "intents_ok", "true").await.unwrap();
}
async fn frozen_terra_power(pool: &sqlx::PgPool, proposal: uuid::Uuid) -> String {
    sqlx::query_scalar("SELECT weight::text FROM voting.proposal_snapshots WHERE proposal_id=$1 AND chain='terra' AND wallet_address=$2")
        .bind(proposal).bind(ALICE).fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn indexed_terra_one_multi_and_beyond_retained_horizon_forks_preserve_frozen_power() {
    for (end, ancestor) in [(104, Some(103)), (104, Some(102)), (230, None)] {
        let Some((pool, _lock)) = setup().await else {
            return;
        };
        let (url, state, server) = terra_fork_server(end).await;
        let lcd = LcdClient::new(vec![url.clone()]);
        let cfg = config(&url);
        seed_terra_fork_electorate(&pool, &state, true).await;
        for height in 101..=end {
            assert_eq!(
                ingest::ingest_terra_height(&pool, &lcd, &cfg, height)
                    .await
                    .unwrap(),
                1
            );
        }
        let original = 1000 - (end - 100);
        assert_eq!(
            db::balance_at(&pool, Chain::Terra, ALICE, end)
                .await
                .unwrap(),
            BigInt::from(original)
        );
        let (poll, signature) = signed_draft_fixture(&pool).await;
        sqlx::query("SELECT * FROM voting.freeze_proposal($1,$2)")
            .bind(poll)
            .bind(signature)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(frozen_terra_power(&pool, poll).await, original.to_string());
        state
            .fork_after
            .store(ancestor.unwrap_or(100), Ordering::Relaxed);
        if let Some(ancestor) = ancestor {
            ingest::maybe_rewind_terra(&pool, &lcd, end).await.unwrap();
            assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), ancestor);
            let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(remaining, ancestor - 100);
            assert_eq!(
                db::balance_at(&pool, Chain::Terra, ALICE, ancestor)
                    .await
                    .unwrap(),
                BigInt::from(1000 - (ancestor - 100))
            );
            for height in ancestor + 1..=end {
                ingest::ingest_terra_height(&pool, &lcd, &cfg, height)
                    .await
                    .unwrap();
            }
            ingest::ingest_terra_height(&pool, &lcd, &cfg, end)
                .await
                .unwrap();
            let expected = 1000 - (ancestor - 100) - 2 * (end - ancestor);
            assert_eq!(
                db::balance_at(&pool, Chain::Terra, ALICE, end)
                    .await
                    .unwrap(),
                BigInt::from(expected)
            );
            let recipient = db::balance_at(&pool, Chain::Terra, "terra1bob", end)
                .await
                .unwrap();
            assert_eq!(recipient + BigInt::from(expected), BigInt::from(1000));
            let checkpoint:(i64,String,bool,bool)=sqlx::query_as("SELECT height,block_hash,complete,canonical FROM ledger_checkpoints WHERE chain='terra'").fetch_one(&pool).await.unwrap();
            assert_eq!(checkpoint, (end, state.hash(end), true, true));
            let replayed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(replayed, end - 100);
        } else {
            let first: i64 = sqlx::query_scalar(
                "SELECT MIN(height) FROM ledger_canonical_blocks WHERE chain='terra'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                first, 102,
                "actual ancestor100 is outside retained128-block horizon"
            );
            let error = ingest::maybe_rewind_terra(&pool, &lcd, end)
                .await
                .unwrap_err();
            assert_eq!(error.to_string(), "provider: fork_beyond_horizon");
            assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), end);
            let state:(bool,bool,String)=sqlx::query_as("SELECT complete,canonical,degraded_reason FROM ledger_checkpoints WHERE chain='terra'").fetch_one(&pool).await.unwrap();
            assert_eq!(state, (false, false, "fork_beyond_horizon".into()));
            assert_eq!(
                db::balance_at(&pool, Chain::Terra, ALICE, end)
                    .await
                    .unwrap(),
                BigInt::from(original)
            );
            let retained: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(
                retained,
                end - 100,
                "unresolved fork does not rewrite accounting history"
            );
            assert_eq!(
                voting_ledger::health::readiness(&pool).await.0,
                axum::http::StatusCode::SERVICE_UNAVAILABLE
            );
            let (blocked, signature) = signed_draft_fixture(&pool).await;
            assert!(sqlx::query("SELECT * FROM voting.freeze_proposal($1,$2)")
                .bind(blocked)
                .bind(signature)
                .execute(&pool)
                .await
                .is_err());
        }
        let validity: String =
            sqlx::query_scalar("SELECT snapshot_validity FROM voting.proposals WHERE id=$1")
                .bind(poll)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(validity, "invalid");
        assert_eq!(frozen_terra_power(&pool, poll).await, original.to_string());
        server.abort();
    }
}

#[tokio::test]
async fn indexed_terra_canonical_hash_failure_keeps_checkpoint_then_recovers_same_epoch() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (url, state, server) = terra_fork_server(104).await;
    let lcd = LcdClient::new(vec![url.clone()]);
    let cfg = config(&url);
    seed_terra_fork_electorate(&pool, &state, false).await;
    for height in 101..=104 {
        ingest::ingest_terra_height(&pool, &lcd, &cfg, height)
            .await
            .unwrap();
    }
    let before: (i64, String, i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain='terra'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    state.fail_hash.store(true, Ordering::Relaxed);
    let absent = voting_ledger::bsc::BscClient::new(vec![]);
    assert!(voting_ledger::worker::poll_chain(
        &pool,
        &Some(lcd.clone()),
        &absent,
        &cfg,
        Chain::Terra
    )
    .await
    .is_err());
    let after: (i64, String, i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain='terra'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, before);
    // Mirror the supervisor's persisted failure classification without inventing a fork.
    db::degrade(&pool, Chain::Terra, "provider_or_accounting_failure")
        .await
        .unwrap();
    assert_eq!(
        voting_ledger::health::readiness(&pool).await.0,
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );
    state.fail_hash.store(false, Ordering::Relaxed);
    voting_ledger::worker::poll_chain(&pool, &Some(lcd), &absent, &cfg, Chain::Terra)
        .await
        .unwrap();
    let recovered:(i64,String,i64,bool,bool)=sqlx::query_as("SELECT height,block_hash,epoch,complete,canonical FROM ledger_checkpoints WHERE chain='terra'").fetch_one(&pool).await.unwrap();
    assert_eq!(recovered, (before.0, before.1, before.2, true, true));
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 104)
            .await
            .unwrap(),
        BigInt::from(996)
    );
    assert_eq!(
        voting_ledger::health::readiness(&pool).await.0,
        axum::http::StatusCode::OK
    );
    server.abort();
}

#[tokio::test]
async fn drained_holder_missing_terra_interval_requires_recovery_before_new_freeze() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (url, state, server) = terra_fork_server(100).await;
    let lcd = LcdClient::new(vec![url.clone()]);
    let cfg = config(&url);
    seed_terra_fork_electorate(&pool, &state, false).await;
    let (old, signature) = signed_draft_fixture(&pool).await;
    sqlx::query("SELECT * FROM voting.freeze_proposal($1,$2)")
        .bind(old)
        .bind(signature)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(frozen_terra_power(&pool, old).await, "1000");
    state.tip.store(110, Ordering::Relaxed);
    state.missing_from.store(101, Ordering::Relaxed);
    state.live_balance.store(0, Ordering::Relaxed);
    let absent = voting_ledger::bsc::BscClient::new(vec![]);
    assert!(voting_ledger::worker::poll_chain(
        &pool,
        &Some(lcd.clone()),
        &absent,
        &cfg,
        Chain::Terra
    )
    .await
    .is_err());
    assert_eq!(
        db::indexed_terra_height(&pool).await.unwrap(),
        100,
        "unavailable interval cannot jump to current head"
    );
    // The operator confirms history is unavailable and explicitly requests a new eligibility epoch.
    db::degrade(&pool, Chain::Terra, "lost_registered_history")
        .await
        .unwrap();
    let (new, signature) = signed_draft_fixture(&pool).await;
    assert!(sqlx::query("SELECT * FROM voting.freeze_proposal($1,$2)")
        .bind(new)
        .bind(signature)
        .execute(&pool)
        .await
        .is_err());
    let source = voting_ledger::bsc::CompositeBalanceSource {
        terra: Some(voting_ledger::bsc::LcdBalancePair {
            lcd: lcd.clone(),
            token: DEFAULT_CL8Y_CW20.into(),
        }),
        bsc: None,
    };
    voting_ledger::reconcile::reconcile_live_baseline(
        &pool,
        &source,
        &lcd,
        &absent,
        &cfg,
        Chain::Terra,
    )
    .await
    .unwrap();
    assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), 110);
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 110)
            .await
            .unwrap(),
        BigInt::from(0)
    );
    assert_eq!(
        voting_ledger::health::readiness(&pool).await.0,
        axum::http::StatusCode::OK
    );
    let heights: (i64, i64) = sqlx::query_as("SELECT * FROM voting.freeze_proposal($1,$2)")
        .bind(new)
        .bind(signature)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(heights, (110, 100));
    assert_eq!(
        frozen_terra_power(&pool, new).await,
        "0",
        "spent Terra power is excluded from the new electorate"
    );
    assert_eq!(
        frozen_terra_power(&pool, old).await,
        "1000",
        "old advisory weight remains immutable"
    );
    let epochs: Vec<i64> = sqlx::query_scalar(
        "SELECT terra_epoch FROM voting.proposals WHERE id=ANY($1) ORDER BY terra_epoch",
    )
    .bind(vec![old, new])
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(epochs, vec![1, 2]);
    server.abort();
}

#[tokio::test]
async fn drained_holder_missing_bsc_interval_requires_confirmed_recovery_before_new_freeze() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (url, mode, server) = rpc_server().await;
    let state = TerraForkFixture::new(100);
    seed_terra_fork_electorate(&pool, &state, false).await;
    let client = voting_ledger::bsc::BscClient::new(vec![url.clone()]);
    let cfg = config(&url);
    let (old, signature) = signed_draft_fixture(&pool).await;
    sqlx::query("SELECT * FROM voting.freeze_proposal($1,$2)")
        .bind(old)
        .bind(signature)
        .execute(&pool)
        .await
        .unwrap();
    let frozen = |id| {
        sqlx::query_scalar::<_, String>("SELECT weight::text FROM voting.proposal_snapshots WHERE proposal_id=$1 AND chain='bsc' AND wallet_address=$2")
        .bind(id).bind(EVM)
    };
    assert_eq!(frozen(old).fetch_one(&pool).await.unwrap(), "1000");
    mode.store(24, Ordering::Relaxed);
    assert!(
        voting_ledger::worker::poll_chain(&pool, &None, &client, &cfg, Chain::Bsc)
            .await
            .is_err()
    );
    assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 100);
    db::degrade(&pool, Chain::Bsc, "lost_registered_history")
        .await
        .unwrap();
    let (new, signature) = signed_draft_fixture(&pool).await;
    assert!(sqlx::query("SELECT * FROM voting.freeze_proposal($1,$2)")
        .bind(new)
        .bind(signature)
        .execute(&pool)
        .await
        .is_err());
    let source = voting_ledger::bsc::CompositeBalanceSource {
        terra: None,
        bsc: Some(voting_ledger::bsc::BscBalanceSource {
            client: client.clone(),
            token: DEFAULT_CL8Y_BEP20.into(),
        }),
    };
    voting_ledger::reconcile::reconcile_live_baseline(
        &pool,
        &source,
        &LcdClient::new(vec![]),
        &client,
        &cfg,
        Chain::Bsc,
    )
    .await
    .unwrap();
    let checkpoint:(i64,i64,i64,bool,bool)=sqlx::query_as("SELECT height,network_tip,epoch,complete,canonical FROM ledger_checkpoints WHERE chain='bsc'").fetch_one(&pool).await.unwrap();
    assert_eq!(
        checkpoint,
        (120, 135, 2, true, true),
        "current baseline becomes eligible only at15confirmedblocks"
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 120).await.unwrap(),
        BigInt::from(0)
    );
    assert_eq!(
        voting_ledger::health::readiness(&pool).await.0,
        axum::http::StatusCode::OK
    );
    let heights: (i64, i64) = sqlx::query_as("SELECT * FROM voting.freeze_proposal($1,$2)")
        .bind(new)
        .bind(signature)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(heights, (100, 120));
    assert_eq!(frozen(new).fetch_one(&pool).await.unwrap(), "0");
    assert_eq!(frozen(old).fetch_one(&pool).await.unwrap(), "1000");
    let epochs: Vec<i64> = sqlx::query_scalar(
        "SELECT bsc_epoch FROM voting.proposals WHERE id=ANY($1) ORDER BY bsc_epoch",
    )
    .bind(vec![old, new])
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(epochs, vec![1, 2]);
    server.abort();
}

const INGEST_GATE_CLASS: i32 = 9137;
async fn install_ingest_gate(pool: &sqlx::PgPool, chain: Chain, stage: i32) -> sqlx::PgConnection {
    use sqlx::Connection;
    let (events, balances) = match chain {
        Chain::Terra => ("cl8y_cw20_transfers", "cl8y_balances"),
        Chain::Bsc => ("cl8y_bep20_transfers", "cl8y_bsc_balances"),
    };
    sqlx::query("CREATE FUNCTION public.test_ingest_stage_gate() RETURNS trigger LANGUAGE plpgsql SET search_path=pg_catalog,public AS $$ BEGIN PERFORM pg_catalog.pg_advisory_xact_lock(9137,TG_ARGV[0]::integer); RETURN NEW; END $$")
        .execute(pool).await.unwrap();
    let table = if stage == 2 { balances } else { events };
    let trigger = if stage == 3 {
        format!("CREATE CONSTRAINT TRIGGER test_ingest_stage_gate AFTER INSERT ON public.{table} DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION public.test_ingest_stage_gate('{stage}')")
    } else {
        format!("CREATE TRIGGER test_ingest_stage_gate AFTER INSERT OR UPDATE ON public.{table} FOR EACH ROW EXECUTE FUNCTION public.test_ingest_stage_gate('{stage}')")
    };
    sqlx::query(&trigger).execute(pool).await.unwrap();
    let mut gate = sqlx::PgConnection::connect(&std::env::var("LEDGER_TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    sqlx::query("SELECT pg_catalog.pg_advisory_lock($1,$2)")
        .bind(INGEST_GATE_CLASS)
        .bind(stage)
        .execute(&mut gate)
        .await
        .unwrap();
    gate
}
async fn wait_advisory_query(pool: &sqlx::PgPool, class: i32, key: i32) -> String {
    tokio::time::timeout(Duration::from_secs(5),async{loop{
        let query:Option<String>=sqlx::query_scalar("SELECT a.query FROM pg_catalog.pg_locks l JOIN pg_catalog.pg_stat_activity a ON a.pid=l.pid WHERE l.locktype='advisory' AND l.classid::bigint=$1 AND l.objid::bigint=$2 AND NOT l.granted LIMIT 1")
            .bind(class as i64).bind(key as i64).fetch_optional(pool).await.unwrap();
        if let Some(query)=query {return query;}
        tokio::time::sleep(Duration::from_millis(10)).await;
    }}).await.unwrap()
}
async fn release_ingest_gate(gate: &mut sqlx::PgConnection, stage: i32) {
    let unlocked: bool = sqlx::query_scalar("SELECT pg_catalog.pg_advisory_unlock($1,$2)")
        .bind(INGEST_GATE_CLASS)
        .bind(stage)
        .fetch_one(gate)
        .await
        .unwrap();
    assert!(unlocked);
}
async fn remove_ingest_gate(pool: &sqlx::PgPool, chain: Chain, stage: i32) {
    let table = match (chain, stage) {
        (Chain::Terra, 2) => "cl8y_balances",
        (Chain::Bsc, 2) => "cl8y_bsc_balances",
        (Chain::Terra, _) => "cl8y_cw20_transfers",
        (Chain::Bsc, _) => "cl8y_bep20_transfers",
    };
    sqlx::query(&format!(
        "DROP TRIGGER test_ingest_stage_gate ON public.{table}"
    ))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("DROP FUNCTION public.test_ingest_stage_gate()")
        .execute(pool)
        .await
        .unwrap();
}
async fn ingest_test_block(
    pool: &sqlx::PgPool,
    chain: Chain,
    lcd: &LcdClient,
    bsc: &voting_ledger::bsc::BscClient,
    cfg: &LedgerConfig,
    height: i64,
) -> Result<usize, LedgerError> {
    match chain {
        Chain::Terra => ingest::ingest_terra_height(pool, lcd, cfg, height).await,
        Chain::Bsc => ingest::ingest_bsc_range(pool, bsc, cfg, height, height).await,
    }
}

#[tokio::test]
async fn actual_ingest_cancellation_after_insert_balance_and_during_commit_retries_once_both_chains(
) {
    for chain in [Chain::Terra, Chain::Bsc] {
        for stage in 1..=3 {
            let Some((pool, _lock)) = setup().await else {
                return;
            };
            let (url, state, terra_server) = terra_fork_server(101).await;
            let (rpc, _, bsc_server) = rpc_server().await;
            let lcd = LcdClient::new(vec![url.clone()]);
            let bsc = voting_ledger::bsc::BscClient::new(vec![rpc]);
            let cfg = config(&url);
            seed_terra_fork_electorate(&pool, &state, false).await;
            let wallet = if chain == Chain::Terra { ALICE } else { EVM };
            let table = if chain == Chain::Terra {
                "cl8y_cw20_transfers"
            } else {
                "cl8y_bep20_transfers"
            };
            let before: (i64, String, i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
                "SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain=$1",
            )
            .bind(chain.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
            let mut gate = install_ingest_gate(&pool, chain, stage).await;
            let task_pool = pool.clone();
            let task_lcd = lcd.clone();
            let task_bsc = bsc.clone();
            let task_cfg = cfg.clone();
            let task = tokio::spawn(async move {
                ingest_test_block(&task_pool, chain, &task_lcd, &task_bsc, &task_cfg, 101).await
            });
            let query = wait_advisory_query(&pool, INGEST_GATE_CLASS, stage).await;
            if stage == 3 {
                assert_eq!(query.trim(), "COMMIT");
            } else {
                assert!(query.contains(if stage == 1 {
                    table
                } else if chain == Chain::Terra {
                    "cl8y_balances"
                } else {
                    "cl8y_bsc_balances"
                }));
            }
            let hidden: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM public.{table}"))
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(hidden, 0);
            assert_eq!(
                db::balance_at(&pool, chain, wallet, 101).await.unwrap(),
                BigInt::from(1000)
            );
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
            release_ingest_gate(&mut gate, stage).await;
            // Acquiring the same accounting lock proves the canceled backend has settled.
            tokio::time::timeout(Duration::from_secs(5), async {
                let mut settled = pool.begin().await.unwrap();
                db::lock_chain(&mut settled, chain).await.unwrap();
                settled.rollback().await.unwrap();
            })
            .await
            .unwrap();
            let rows: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM public.{table}"))
                .fetch_one(&pool)
                .await
                .unwrap();
            assert!(rows == 0 || rows == 1);
            if stage != 3 {
                assert_eq!(
                    rows, 0,
                    "canceling an unfinished accounting statement rolls back"
                );
            }
            let after: (i64, String, i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
                "SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain=$1",
            )
            .bind(chain.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
            if rows == 0 {
                assert_eq!(after, before);
            } else {
                assert_eq!(after.0, 101);
                assert_eq!(after.2, before.2);
                assert_eq!(
                    after.1,
                    if chain == Chain::Terra {
                        state.hash(101)
                    } else {
                        block_hash(101)
                    }
                );
            }
            assert_eq!(
                db::balance_at(&pool, chain, wallet, 101).await.unwrap(),
                BigInt::from(1000 - rows)
            );
            let cursor = match chain {
                Chain::Terra => db::indexed_terra_height(&pool).await.unwrap(),
                Chain::Bsc => db::indexed_bsc_block(&pool).await.unwrap(),
            };
            assert_eq!(cursor, 100 + rows);
            let history: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM ledger_canonical_blocks WHERE chain=$1")
                    .bind(chain.as_str())
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(history, 1 + rows);
            println!(
                "canceled_ingest chain={} stage={stage} committed={}",
                chain.as_str(),
                rows == 1
            );
            remove_ingest_gate(&pool, chain, stage).await;
            for _ in 0..2 {
                ingest_test_block(&pool, chain, &lcd, &bsc, &cfg, 101)
                    .await
                    .unwrap();
            }
            let rows: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM public.{table}"))
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(rows, 1);
            assert_eq!(
                db::balance_at(&pool, chain, wallet, 101).await.unwrap(),
                BigInt::from(999)
            );
            let checkpoint: (i64, bool, bool) = sqlx::query_as(
                "SELECT height,complete,canonical FROM ledger_checkpoints WHERE chain=$1",
            )
            .bind(chain.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(checkpoint, (101, true, true));
            terra_server.abort();
            bsc_server.abort();
        }
    }
}

#[tokio::test]
async fn concurrent_partially_overlapping_actual_ingest_ranges_apply_distinct_events_once_both_chains(
) {
    for chain in [Chain::Terra, Chain::Bsc] {
        let Some((pool, _lock)) = setup().await else {
            return;
        };
        let (url, state, terra_server) = terra_fork_server(104).await;
        let (rpc, _, bsc_server) = rpc_server().await;
        let lcd = LcdClient::new(vec![url.clone()]);
        let bsc = voting_ledger::bsc::BscClient::new(vec![rpc]);
        let cfg = config(&url);
        seed_terra_fork_electorate(&pool, &state, false).await;
        let mut gate = install_ingest_gate(&pool, chain, 1).await;
        let a_pool = pool.clone();
        let a_lcd = lcd.clone();
        let a_bsc = bsc.clone();
        let a_cfg = cfg.clone();
        let a = tokio::spawn(async move {
            if chain == Chain::Bsc {
                ingest::ingest_bsc_range(&a_pool, &a_bsc, &a_cfg, 101, 103)
                    .await
                    .map(|_| ())
            } else {
                for h in 101..=103 {
                    ingest::ingest_terra_height(&a_pool, &a_lcd, &a_cfg, h).await?;
                }
                Ok(())
            }
        });
        wait_advisory_query(&pool, INGEST_GATE_CLASS, 1).await;
        let b_pool = pool.clone();
        let b_lcd = lcd.clone();
        let b_bsc = bsc.clone();
        let b_cfg = cfg.clone();
        let b = tokio::spawn(async move {
            if chain == Chain::Bsc {
                ingest::ingest_bsc_range(&b_pool, &b_bsc, &b_cfg, 102, 104)
                    .await
                    .map(|_| ())
            } else {
                for h in 102..=104 {
                    ingest::ingest_terra_height(&b_pool, &b_lcd, &b_cfg, h).await?;
                }
                Ok(())
            }
        });
        wait_advisory_query(&pool, 8136, if chain == Chain::Terra { 1 } else { 2 }).await;
        release_ingest_gate(&mut gate, 1).await;
        for outcome in [a.await.unwrap(), b.await.unwrap()] {
            if let Err(error) = outcome {
                assert!(
                    matches!(
                        error,
                        LedgerError::Provider("checkpoint_regression" | "noncontiguous_ingest")
                    ),
                    "unexpected {error:?}"
                );
            }
        }
        remove_ingest_gate(&pool, chain, 1).await;
        let cursor = if chain == Chain::Terra {
            db::indexed_terra_height(&pool).await.unwrap()
        } else {
            db::indexed_bsc_block(&pool).await.unwrap()
        };
        for height in cursor + 1..=104 {
            ingest_test_block(&pool, chain, &lcd, &bsc, &cfg, height)
                .await
                .unwrap();
        }
        let table = if chain == Chain::Terra {
            "cl8y_cw20_transfers"
        } else {
            "cl8y_bep20_transfers"
        };
        let rows: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM public.{table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 4);
        for height in 101..=104 {
            assert_eq!(
                db::balance_at(
                    &pool,
                    chain,
                    if chain == Chain::Terra { ALICE } else { EVM },
                    height
                )
                .await
                .unwrap(),
                BigInt::from(1000 - (height - 100))
            );
        }
        ingest_test_block(&pool, chain, &lcd, &bsc, &cfg, 104)
            .await
            .unwrap();
        assert_eq!(
            db::balance_at(
                &pool,
                chain,
                if chain == Chain::Terra { ALICE } else { EVM },
                104
            )
            .await
            .unwrap(),
            BigInt::from(996)
        );
        terra_server.abort();
        bsc_server.abort();
    }
}

#[tokio::test]
async fn reordered_new_ingest_blocks_reject_gaps_then_retry_canonical_order_both_chains() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (url, state, terra_server) = terra_fork_server(104).await;
    let (rpc, _, bsc_server) = rpc_server().await;
    let lcd = LcdClient::new(vec![url.clone()]);
    let bsc = voting_ledger::bsc::BscClient::new(vec![rpc]);
    let cfg = config(&url);
    seed_terra_fork_electorate(&pool, &state, false).await;
    for chain in [Chain::Terra, Chain::Bsc] {
        assert_eq!(
            ingest_test_block(&pool, chain, &lcd, &bsc, &cfg, 103)
                .await
                .unwrap_err()
                .to_string(),
            "provider: noncontiguous_ingest"
        );
        assert_eq!(
            db::balance_at(
                &pool,
                chain,
                if chain == Chain::Terra { ALICE } else { EVM },
                104
            )
            .await
            .unwrap(),
            BigInt::from(1000)
        );
        if chain == Chain::Bsc {
            ingest::ingest_bsc_range(&pool, &bsc, &cfg, 101, 104)
                .await
                .unwrap();
            ingest::ingest_bsc_range(&pool, &bsc, &cfg, 102, 104)
                .await
                .unwrap();
        } else {
            for h in 101..=104 {
                ingest::ingest_terra_height(&pool, &lcd, &cfg, h)
                    .await
                    .unwrap();
            }
        }
        for height in 101..=104 {
            assert_eq!(
                db::balance_at(
                    &pool,
                    chain,
                    if chain == Chain::Terra { ALICE } else { EVM },
                    height
                )
                .await
                .unwrap(),
                BigInt::from(1000 - (height - 100))
            );
        }
        let table = if chain == Chain::Terra {
            "cl8y_cw20_transfers"
        } else {
            "cl8y_bep20_transfers"
        };
        let rows: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM public.{table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 4);
    }
    terra_server.abort();
    bsc_server.abort();
}

struct ArrivalSource {
    state: TerraForkFixture,
    poison_seen: Arc<tokio::sync::Notify>,
}
#[async_trait::async_trait]
impl voting_ledger::register::LiveBalanceSource for ArrivalSource {
    async fn terra_live_balance(
        &self,
        wallet: &str,
    ) -> Result<voting_ledger::register::LiveBalanceSnapshot, LedgerError> {
        if wallet == "terra1poisonarrivals" {
            self.poison_seen.notify_one();
            std::future::pending::<()>().await;
        }
        let height = self.state.tip.load(Ordering::Relaxed);
        Ok(voting_ledger::register::LiveBalanceSnapshot {
            height,
            amount: BigInt::from(10),
            block_hash: self.state.hash(height),
        })
    }
    async fn bsc_live_balance(
        &self,
        _: &str,
    ) -> Result<voting_ledger::register::LiveBalanceSnapshot, LedgerError> {
        unreachable!()
    }
}
async fn insert_test_intent(pool: &sqlx::PgPool, wallet: &str) {
    let signature = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO voting.signatures(id,chain,wallet_address,signature,payload_hash,purpose) VALUES($1,'terra',$2,'synthetic','synthetic','register')").bind(signature).bind(wallet).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO voting.registration_intents(chain,wallet_address,signature_id) VALUES('terra',$1,$2)").bind(wallet).bind(signature).execute(pool).await.unwrap();
}

#[tokio::test]
async fn sustained_registration_arrivals_during_poison_timeout_keep_both_chains_progressing() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (terra, state, terra_server) = terra_fork_server(104).await;
    let tip = Arc::new(std::sync::atomic::AtomicI64::new(120));
    let app = Router::new()
        .fallback(axum::routing::post(rpc_fixture))
        .with_state(RpcFixture {
            bad: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            tip: tip.clone(),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc = format!("http://{}", listener.local_addr().unwrap());
    let bsc_server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let lcd = LcdClient::new(vec![terra.clone()]);
    let bsc = voting_ledger::bsc::BscClient::new(vec![rpc]);
    let cfg = config(&terra);
    seed_terra_fork_electorate(&pool, &state, false).await;
    insert_test_intent(&pool, "terra1poisonarrivals").await;
    insert_test_intent(&pool, "terra1earlyarrival").await;
    let source = Arc::new(ArrivalSource {
        state: state.clone(),
        poison_seen: Arc::new(tokio::sync::Notify::new()),
    });
    let work_pool = pool.clone();
    let work_source = source.clone();
    let intents = tokio::spawn(async move {
        loop {
            assert!(voting_ledger::register::supervised_intent_pass(
                &work_pool,
                work_source.as_ref(),
                Duration::from_secs(20)
            )
            .await
            .unwrap());
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    });
    tokio::time::timeout(Duration::from_secs(2), source.poison_seen.notified())
        .await
        .unwrap();
    let mut workers = Vec::new();
    for chain in [Chain::Terra, Chain::Bsc] {
        let work_pool = pool.clone();
        let work_lcd = lcd.clone();
        let work_bsc = bsc.clone();
        let work_cfg = cfg.clone();
        workers.push(tokio::spawn(async move {
            loop {
                voting_ledger::worker::poll_chain(
                    &work_pool,
                    &Some(work_lcd.clone()),
                    &work_bsc,
                    &work_cfg,
                    chain,
                )
                .await
                .unwrap();
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        }));
    }
    let producer_pool = pool.clone();
    let producer_state = state.clone();
    let producer_tip = tip.clone();
    let producer = tokio::spawn(async move {
        for n in 0..32 {
            if n % 8 == 0 {
                producer_state.tip.fetch_add(2, Ordering::Relaxed);
                producer_tip.fetch_add(2, Ordering::Relaxed);
            }
            insert_test_intent(&producer_pool, &format!("terra1arrival{n}")).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    tokio::time::timeout(Duration::from_secs(25),async{loop{
        assert!(!intents.is_finished(),"intent worker exited under sustained arrivals");
        assert!(workers.iter().all(|w|!w.is_finished()),"core worker exited under sustained arrivals");
        let complete:i64=sqlx::query_scalar("SELECT COUNT(*) FROM voting.registration_intents WHERE wallet_address<>'terra1poisonarrivals' AND processed_at IS NOT NULL").fetch_one(&pool).await.unwrap();
        if complete==33 && db::indexed_terra_height(&pool).await.unwrap()==112 && db::indexed_bsc_block(&pool).await.unwrap()==113 {break;}
        tokio::time::sleep(Duration::from_millis(50)).await;
    }}).await.unwrap();
    producer.await.unwrap();
    let poison:(i32,String,bool)=sqlx::query_as("SELECT attempts,last_error_class,processed_at IS NULL FROM voting.registration_intents WHERE wallet_address='terra1poisonarrivals'").fetch_one(&pool).await.unwrap();
    assert_eq!(poison, (1, "deadline".into(), true));
    assert_eq!(
        voting_ledger::health::readiness(&pool).await.0,
        axum::http::StatusCode::OK
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 112)
            .await
            .unwrap(),
        BigInt::from(988)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 113).await.unwrap(),
        BigInt::from(987)
    );
    let registered: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM voting_registrations WHERE wallet_address LIKE 'terra1%arrival%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(registered, 33);
    intents.abort();
    let _ = intents.await;
    for worker in workers {
        worker.abort();
        let _ = worker.await;
    }
    terra_server.abort();
    bsc_server.abort();
}

#[derive(Clone)]
struct PendingRpc {
    inner: RpcFixture,
    pending: Arc<tokio::sync::Notify>,
}
async fn pending_rpc_http(
    State(state): State<PendingRpc>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    if body["method"] == "eth_getLogs" && state.inner.bad.load(Ordering::Relaxed) == 25 {
        state.pending.notify_one();
        std::future::pending::<()>().await;
    }
    rpc_fixture(State(state.inner), Json(body)).await
}
fn launch_test_ledger(
    terra: &str,
    rpc: &str,
    bind: &str,
    database_url: &str,
) -> tokio::process::Child {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_voting-ledger"))
        .kill_on_drop(true)
        .env_clear()
        .env("DATABASE_URL", database_url)
        .env("RUN_MODE", "prod")
        .env("TERRA_LCD_URL", terra)
        .env("BSC_RPC_URLS", rpc)
        .env("API_BIND", bind)
        .env("POLL_INTERVAL_MS", "100")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap()
}
async fn wait_child_status(
    child: &mut tokio::process::Child,
    http: &reqwest::Client,
    base: &str,
    expected: reqwest::StatusCode,
) {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            assert!(child.try_wait().unwrap().is_none(), "ledger child exited");
            if http
                .get(format!("{base}/ready"))
                .send()
                .await
                .is_ok_and(|r| r.status() == expected)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
}
// Readiness permits bounded lag; fault fixtures need fully committed, exact tips.
async fn wait_child_cursors(
    child: &mut tokio::process::Child,
    pool: &sqlx::PgPool,
    terra: i64,
    bsc: i64,
) {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            assert!(
                child.try_wait().unwrap().is_none(),
                "ledger child exited before cursor barrier"
            );
            let observed = (
                db::indexed_terra_height(pool).await.unwrap(),
                db::indexed_bsc_block(pool).await.unwrap(),
            );
            assert!(
                observed.0 <= terra && observed.1 <= bsc,
                "cursor exceeded fixed fixture tip"
            );
            if observed == (terra, bsc) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
async fn terminate_child(child: &mut tokio::process::Child) {
    assert!(std::process::Command::new("kill")
        .arg("-TERM")
        .arg(child.id().unwrap().to_string())
        .status()
        .unwrap()
        .success());
    let exit = tokio::time::timeout(Duration::from_secs(6), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(exit.success());
}

#[tokio::test]
async fn process_sigterm_during_pending_bsc_rpc_rolls_back_and_restart_retries_funded_delta() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (terra, state, terra_server) = terra_fork_server(110).await;
    seed_terra_fork_electorate(&pool, &state, false).await;
    sqlx::query(
        "UPDATE ledger_checkpoints SET network_tip=CASE chain WHEN 'terra' THEN 110 ELSE 120 END",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mode = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let tip = Arc::new(std::sync::atomic::AtomicI64::new(120));
    let pending = Arc::new(tokio::sync::Notify::new());
    let app = Router::new()
        .fallback(axum::routing::post(pending_rpc_http))
        .with_state(PendingRpc {
            inner: RpcFixture {
                bad: mode.clone(),
                tip: tip.clone(),
            },
            pending: pending.clone(),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc = format!("http://{}", listener.local_addr().unwrap());
    let bsc_server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reserve.local_addr().unwrap();
    drop(reserve);
    let base = format!("http://{address}");
    let database = std::env::var("LEDGER_TEST_DATABASE_URL").unwrap();
    let http = reqwest::Client::new();
    let mut child = launch_test_ledger(&terra, &rpc, &address.to_string(), &database);
    wait_child_status(&mut child, &http, &base, reqwest::StatusCode::OK).await;
    wait_child_cursors(&mut child, &pool, 110, 105).await;
    assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), 110);
    assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 105);
    let epoch: i64 = sqlx::query_scalar("SELECT epoch FROM ledger_checkpoints WHERE chain='bsc'")
        .fetch_one(&pool)
        .await
        .unwrap();
    mode.store(25, Ordering::Relaxed);
    tip.store(121, Ordering::Relaxed);
    tokio::time::timeout(Duration::from_secs(3), pending.notified())
        .await
        .unwrap();
    terminate_child(&mut child).await;
    assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 105);
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 106).await.unwrap(),
        BigInt::from(995)
    );
    mode.store(0, Ordering::Relaxed);
    let mut restarted = launch_test_ledger(&terra, &rpc, &address.to_string(), &database);
    wait_child_status(&mut restarted, &http, &base, reqwest::StatusCode::OK).await;
    wait_child_cursors(&mut restarted, &pool, 110, 106).await;
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 106).await.unwrap(),
        BigInt::from(994)
    );
    let recovered: i64 =
        sqlx::query_scalar("SELECT epoch FROM ledger_checkpoints WHERE chain='bsc'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(recovered, epoch);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_bep20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 6);
    terminate_child(&mut restarted).await;
    terra_server.abort();
    bsc_server.abort();
}

#[tokio::test]
async fn process_database_session_loss_fails_readiness_then_restores_without_new_epoch() {
    use sqlx::Connection;
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (terra, state, terra_server) = terra_fork_server(110).await;
    let (rpc, _, bsc_server) = rpc_server().await;
    seed_terra_fork_electorate(&pool, &state, false).await;
    sqlx::query(
        "UPDATE ledger_checkpoints SET network_tip=CASE chain WHEN 'terra' THEN 110 ELSE 120 END",
    )
    .execute(&pool)
    .await
    .unwrap();
    let database = std::env::var("LEDGER_TEST_DATABASE_URL").unwrap();
    let name = voting_ledger::test_lock::validate_test_url(&database, true).unwrap();
    let quoted = name.replace('"', "\"\"");
    let mut admin_url = reqwest::Url::parse(&database).unwrap();
    admin_url.set_path("/postgres");
    let mut control = sqlx::PgConnection::connect(admin_url.as_str())
        .await
        .unwrap();
    let tag = format!("voting-db-fault-{}", uuid::Uuid::new_v4());
    assert!(tag.len() <= 63);
    let mut child_url = reqwest::Url::parse(&database).unwrap();
    child_url
        .query_pairs_mut()
        .append_pair("application_name", &tag);
    let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reserve.local_addr().unwrap();
    drop(reserve);
    let base = format!("http://{address}");
    let http = reqwest::Client::new();
    let mut child = launch_test_ledger(&terra, &rpc, &address.to_string(), child_url.as_str());
    wait_child_status(&mut child, &http, &base, reqwest::StatusCode::OK).await;
    wait_child_cursors(&mut child, &pool, 110, 105).await;
    let tagged: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pg_catalog.pg_stat_activity WHERE datname=$1 AND application_name=$2",
    )
    .bind(&name)
    .bind(&tag)
    .fetch_one(&mut control)
    .await
    .unwrap();
    assert!(tagged > 0, "the fault targets only tagged child sessions");
    let before: Vec<(String, i64, i64)> =
        sqlx::query_as("SELECT chain,height,epoch FROM ledger_checkpoints ORDER BY chain")
            .fetch_all(&pool)
            .await
            .unwrap();
    sqlx::query(&format!(
        "ALTER DATABASE \"{quoted}\" ALLOW_CONNECTIONS false"
    ))
    .execute(&mut control)
    .await
    .unwrap();
    // Only this marked database and this test child's sessions are disconnected.
    // Test-lock/observer sessions remain open; the control connection uses the maintenance DB.
    let disconnected=sqlx::query_scalar::<_,bool>("SELECT pg_catalog.pg_terminate_backend(pid) FROM pg_catalog.pg_stat_activity WHERE datname=$1 AND application_name=$2")
        .bind(&name).bind(&tag).fetch_all(&mut control).await;
    let unavailable = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(response) = http.get(format!("{base}/ready")).send().await {
                if response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE {
                    let body: serde_json::Value = response
                        .json()
                        .await
                        .map_err(|_| LedgerError::Provider("test_readiness_json"))?;
                    let live = http
                        .get(format!("{base}/live"))
                        .send()
                        .await
                        .map_err(|_| LedgerError::Provider("test_liveness_transport"))?;
                    return Ok::<_, LedgerError>(
                        body["database_ok"] == false && live.status().is_success(),
                    );
                }
            }
            if child.try_wait()?.is_some() {
                return Ok(false);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    // Always restore admission before checking fault assertions, including a failed observation.
    sqlx::query(&format!(
        "ALTER DATABASE \"{quoted}\" ALLOW_CONNECTIONS true"
    ))
    .execute(&mut control)
    .await
    .unwrap();
    assert!(disconnected.unwrap().iter().any(|killed| *killed));
    assert!(
        unavailable.unwrap().unwrap(),
        "running readiness must expose actual database failure"
    );
    let restart = if let Some(exit) = child.try_wait().unwrap() {
        assert!(
            !exit.success(),
            "a fatal DB progress failure requests supervisor restart"
        );
        child = launch_test_ledger(&terra, &rpc, &address.to_string(), child_url.as_str());
        true
    } else {
        false
    };
    println!("database_session_loss supervisor_restart={restart}");
    wait_child_status(&mut child, &http, &base, reqwest::StatusCode::OK).await;
    wait_child_cursors(&mut child, &pool, 110, 105).await;
    let after: Vec<(String, i64, i64)> =
        sqlx::query_as("SELECT chain,height,epoch FROM ledger_checkpoints ORDER BY chain")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(after, before);
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 110)
            .await
            .unwrap(),
        BigInt::from(990)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 105).await.unwrap(),
        BigInt::from(995)
    );
    let connections: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pg_catalog.pg_stat_activity WHERE datname=$1 AND application_name=$2",
    )
    .bind(&name)
    .bind(&tag)
    .fetch_one(&mut control)
    .await
    .unwrap();
    assert!(connections > 0);
    terminate_child(&mut child).await;
    control.close().await.unwrap();
    terra_server.abort();
    bsc_server.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn process_sigkill_after_event_balance_and_during_commit_recovers_exactly_once_both_chains() {
    use std::os::unix::process::ExitStatusExt;
    for chain in [Chain::Terra, Chain::Bsc] {
        for stage in 1..=3 {
            let Some((pool, _lock)) = setup().await else {
                return;
            };
            let (terra, state, terra_server) = terra_fork_server(101).await;
            let (rpc, _, bsc_server) = rpc_server().await;
            seed_terra_fork_electorate(&pool, &state, false).await;
            sqlx::query("UPDATE ledger_checkpoints SET network_tip=CASE chain WHEN 'terra' THEN 101 ELSE 120 END").execute(&pool).await.unwrap();
            let before: (i64, String, i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
                "SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain=$1",
            )
            .bind(chain.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
            let mut gate = install_ingest_gate(&pool, chain, stage).await;
            let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = reserve.local_addr().unwrap();
            drop(reserve);
            let base = format!("http://{address}");
            let database = std::env::var("LEDGER_TEST_DATABASE_URL").unwrap();
            let mut child = launch_test_ledger(&terra, &rpc, &address.to_string(), &database);
            let query = wait_advisory_query(&pool, INGEST_GATE_CLASS, stage).await;
            if stage == 3 {
                assert_eq!(query.trim(), "COMMIT");
            }
            child.kill().await.unwrap();
            assert_eq!(child.try_wait().unwrap().unwrap().signal(), Some(9));
            release_ingest_gate(&mut gate, stage).await;
            tokio::time::timeout(Duration::from_secs(5), async {
                let mut tx = pool.begin().await.unwrap();
                db::lock_chain(&mut tx, chain).await.unwrap();
                tx.rollback().await.unwrap();
            })
            .await
            .unwrap();
            let (table, wallet, last, total) = match chain {
                Chain::Terra => ("cl8y_cw20_transfers", ALICE, 101, 1),
                Chain::Bsc => ("cl8y_bep20_transfers", EVM, 105, 5),
            };
            let rows: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM public.{table}"))
                .fetch_one(&pool)
                .await
                .unwrap();
            assert!(
                rows == 0 || rows == total,
                "a crashed commit cannot publish a partial range"
            );
            if stage != 3 {
                assert_eq!(
                    rows, 0,
                    "unfinished statement must roll back after process death"
                );
            }
            let checkpoint: (i64, String, i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
                "SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain=$1",
            )
            .bind(chain.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
            if rows == 0 {
                assert_eq!(checkpoint, before);
            } else {
                assert_eq!((checkpoint.0, checkpoint.2), (last, before.2));
                assert_eq!(
                    checkpoint.1,
                    if chain == Chain::Terra {
                        state.hash(last)
                    } else {
                        block_hash(last)
                    }
                );
            }
            assert_eq!(
                db::balance_at(&pool, chain, wallet, last).await.unwrap(),
                BigInt::from(1000 - rows)
            );
            let cursor = match chain {
                Chain::Terra => db::indexed_terra_height(&pool).await.unwrap(),
                Chain::Bsc => db::indexed_bsc_block(&pool).await.unwrap(),
            };
            assert_eq!(cursor, if rows == 0 { 100 } else { last });
            let history: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM ledger_canonical_blocks WHERE chain=$1")
                    .bind(chain.as_str())
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(history, 1 + rows);
            println!(
                "process_sigkill chain={} stage={stage} durable_events={rows}",
                chain.as_str()
            );
            remove_ingest_gate(&pool, chain, stage).await;
            let mut restarted = launch_test_ledger(&terra, &rpc, &address.to_string(), &database);
            let http = reqwest::Client::new();
            wait_child_status(&mut restarted, &http, &base, reqwest::StatusCode::OK).await;
            wait_child_cursors(&mut restarted, &pool, 101, 105).await;
            let rows: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM public.{table}"))
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(rows, total);
            assert_eq!(
                db::balance_at(&pool, chain, wallet, last).await.unwrap(),
                BigInt::from(1000 - total)
            );
            let epoch: i64 =
                sqlx::query_scalar("SELECT epoch FROM ledger_checkpoints WHERE chain=$1")
                    .bind(chain.as_str())
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(epoch, before.2);
            let cfg = config(&terra);
            let lcd = LcdClient::new(vec![terra.clone()]);
            let bsc = voting_ledger::bsc::BscClient::new(vec![rpc.clone()]);
            if chain == Chain::Terra {
                ingest::ingest_terra_height(&pool, &lcd, &cfg, last)
                    .await
                    .unwrap();
            } else {
                ingest::ingest_bsc_range(&pool, &bsc, &cfg, 101, last)
                    .await
                    .unwrap();
            }
            assert_eq!(
                db::balance_at(&pool, chain, wallet, last).await.unwrap(),
                BigInt::from(1000 - total)
            );
            terminate_child(&mut restarted).await;
            terra_server.abort();
            bsc_server.abort();
        }
    }
}

#[derive(Clone)]
struct PendingTerra {
    inner: TerraForkFixture,
    blocked: Arc<AtomicBool>,
    pending: Arc<tokio::sync::Notify>,
}
async fn pending_terra_http(
    State(state): State<PendingTerra>,
    uri: OriginalUri,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    if uri.0.path().contains("/tx/") && state.blocked.load(Ordering::Relaxed) {
        state.pending.notify_one();
        std::future::pending::<()>().await;
    }
    terra_fork_http(State(state.inner), uri, headers).await
}
#[tokio::test]
async fn process_sigterm_during_pending_terra_lcd_keeps_bsc_progress_and_retries_funded_delta() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let state = TerraForkFixture::new(110);
    let blocked = Arc::new(AtomicBool::new(false));
    let pending = Arc::new(tokio::sync::Notify::new());
    let app = Router::new()
        .fallback(get(pending_terra_http))
        .with_state(PendingTerra {
            inner: state.clone(),
            blocked: blocked.clone(),
            pending: pending.clone(),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let terra = format!("http://{}", listener.local_addr().unwrap());
    let terra_server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (rpc, _, bsc_server) = rpc_server().await;
    seed_terra_fork_electorate(&pool, &state, false).await;
    sqlx::query(
        "UPDATE ledger_checkpoints SET network_tip=CASE chain WHEN 'terra' THEN 110 ELSE 120 END",
    )
    .execute(&pool)
    .await
    .unwrap();
    let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reserve.local_addr().unwrap();
    drop(reserve);
    let base = format!("http://{address}");
    let database = std::env::var("LEDGER_TEST_DATABASE_URL").unwrap();
    let http = reqwest::Client::new();
    let mut child = launch_test_ledger(&terra, &rpc, &address.to_string(), &database);
    wait_child_status(&mut child, &http, &base, reqwest::StatusCode::OK).await;
    wait_child_cursors(&mut child, &pool, 110, 105).await;
    let epoch: i64 = sqlx::query_scalar("SELECT epoch FROM ledger_checkpoints WHERE chain='terra'")
        .fetch_one(&pool)
        .await
        .unwrap();
    blocked.store(true, Ordering::Relaxed);
    state.tip.store(111, Ordering::Relaxed);
    tokio::time::timeout(Duration::from_secs(3), pending.notified())
        .await
        .unwrap();
    let sibling_observed: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT observed_at FROM ledger_checkpoints WHERE chain='bsc'")
            .fetch_one(&pool)
            .await
            .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let observed: chrono::DateTime<chrono::Utc> =
                sqlx::query_scalar("SELECT observed_at FROM ledger_checkpoints WHERE chain='bsc'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            if observed > sibling_observed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 105);
    terminate_child(&mut child).await;
    assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), 110);
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 111)
            .await
            .unwrap(),
        BigInt::from(990)
    );
    blocked.store(false, Ordering::Relaxed);
    let mut restarted = launch_test_ledger(&terra, &rpc, &address.to_string(), &database);
    wait_child_status(&mut restarted, &http, &base, reqwest::StatusCode::OK).await;
    wait_child_cursors(&mut restarted, &pool, 111, 105).await;
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 111)
            .await
            .unwrap(),
        BigInt::from(989)
    );
    let recovered: i64 =
        sqlx::query_scalar("SELECT epoch FROM ledger_checkpoints WHERE chain='terra'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(recovered, epoch);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 11);
    terminate_child(&mut restarted).await;
    terra_server.abort();
    bsc_server.abort();
}

#[tokio::test]
async fn maximum_uint256_bsc_http_event_persists_exact_debit_credit_and_replay() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (url, mode, server) = rpc_server().await;
    mode.store(26, Ordering::Relaxed);
    let client = voting_ledger::bsc::BscClient::new(vec![url.clone()]);
    let cfg = config(&url);
    let maximum = (BigInt::from(1) << 256) - BigInt::from(1);
    let recipient = "0x2222222222222222222222222222222222222222";
    seed_registration(&pool, Chain::Bsc, EVM, 100, &maximum, None)
        .await
        .unwrap();
    seed_registration(&pool, Chain::Bsc, recipient, 100, &BigInt::from(0), None)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    db::lock_chain(&mut tx, Chain::Bsc).await.unwrap();
    db::publish_checkpoint(&mut tx, Chain::Bsc, 100, &block_hash(100), 115)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for _ in 0..2 {
        assert_eq!(
            ingest::ingest_bsc_range(&pool, &client, &cfg, 101, 101)
                .await
                .unwrap(),
            1
        );
    }
    let amount: String = sqlx::query_scalar("SELECT amount::text FROM cl8y_bep20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(amount, maximum.to_string());
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 101).await.unwrap(),
        BigInt::from(0)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, recipient, 101)
            .await
            .unwrap(),
        maximum
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 100).await.unwrap(),
        maximum
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_bep20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 101);
    server.abort();
}

#[tokio::test]
async fn multiple_distinct_bsc_logs_in_one_block_sort_credit_before_debit_and_replay_once() {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (url, mode, server) = rpc_server().await;
    mode.store(27, Ordering::Relaxed);
    let client = voting_ledger::bsc::BscClient::new(vec![url.clone()]);
    let cfg = config(&url);
    let recipient = "0x2222222222222222222222222222222222222222";
    for wallet in [EVM, recipient] {
        seed_registration(&pool, Chain::Bsc, wallet, 100, &BigInt::from(0), None)
            .await
            .unwrap();
    }
    let mut tx = pool.begin().await.unwrap();
    db::lock_chain(&mut tx, Chain::Bsc).await.unwrap();
    db::publish_checkpoint(&mut tx, Chain::Bsc, 100, &block_hash(100), 115)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for _ in 0..2 {
        assert_eq!(
            ingest::ingest_bsc_range(&pool, &client, &cfg, 101, 101)
                .await
                .unwrap(),
            2
        );
    }
    let rows: Vec<(String, i32, String)> = sqlx::query_as(
        "SELECT tx_hash,log_index,amount::text FROM cl8y_bep20_transfers ORDER BY log_index",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (block_hash(101 + 10000), 0, "10".into()),
            (block_hash(101 + 10000), 1, "10".into())
        ]
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 101).await.unwrap(),
        BigInt::from(0)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, recipient, 101)
            .await
            .unwrap(),
        BigInt::from(10)
    );
    assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 101);
    server.abort();
}

#[tokio::test]
async fn failed_terra_execution_and_base64_wasm_attributes_ingest_atomically_through_http() {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let failed = "synthetic-terra-failed-execution:101";
    let successful = "synthetic-terra-successful-execution:101";
    let failed_hash = hex::encode_upper(Sha256::digest(failed.as_bytes()));
    let success_hash = hex::encode_upper(Sha256::digest(successful.as_bytes()));
    let expected_hash = success_hash.clone();
    let encode = |text: &str| base64::engine::general_purpose::STANDARD.encode(text);
    let events = serde_json::json!([{"type":"wasm","attributes":[
        {"key":encode("_contract_address"),"value":encode(DEFAULT_CL8Y_CW20)},
        {"key":encode("action"),"value":encode("transfer")},
        {"key":encode("from"),"value":encode(ALICE)},
        {"key":encode("to"),"value":encode("terra1bob")},
        {"key":encode("amount"),"value":encode("10")}
    ]}]);
    let response = serde_json::json!({"pagination":null,"total":"2","txs":[{"body":{}}],"tx_responses":[
        {"txhash":failed_hash,"height":"101","code":5,"logs":[],"events":[{"type":"wasm","attributes":[
            {"key":"_contract_address","value":DEFAULT_CL8Y_CW20},{"key":"action","value":"transfer"},
            {"key":"from","value":ALICE},{"key":"to","value":"terra1bob"},{"key":"amount","value":"999999"}
        ]}]},
        {"txhash":success_hash,"height":"101","code":0,"logs":[],"events":events}
    ]});
    let app=Router::new().fallback(get(move|uri:OriginalUri|{let response=response.clone();async move{
        if uri.0.path().contains("/blocks/") {
            Json(serde_json::json!({"block_id":{"hash":format!("{:064X}",101)},"sdk_block":{
                "header":{"height":"101","chain_id":"columbus-5","time":chrono::Utc::now().to_rfc3339()},
                "data":{"txs":[base64::engine::general_purpose::STANDARD.encode(failed),base64::engine::general_purpose::STANDARD.encode(successful)]}
            }}))
        } else {Json(response)}
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let state = TerraForkFixture::new(101);
    seed_terra_fork_electorate(&pool, &state, true).await;
    let lcd = LcdClient::new(vec![url.clone()]);
    let cfg = config(&url);
    for _ in 0..2 {
        assert_eq!(
            ingest::ingest_terra_height(&pool, &lcd, &cfg, 101)
                .await
                .unwrap(),
            1
        );
    }
    let event: (String, i32, String) =
        sqlx::query_as("SELECT tx_hash,msg_index,amount::text FROM cl8y_cw20_transfers")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event, (expected_hash, 0, "10".into()));
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 101)
            .await
            .unwrap(),
        BigInt::from(990)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, "terra1bob", 101)
            .await
            .unwrap(),
        BigInt::from(10)
    );
    let checkpoint: (i64, String, bool, bool) = sqlx::query_as(
        "SELECT height,block_hash,complete,canonical FROM ledger_checkpoints WHERE chain='terra'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(checkpoint, (101, state.hash(101), true, true));
    server.abort();
}

#[derive(Clone)]
struct DelayedHashRpc {
    chain_calls: Arc<std::sync::atomic::AtomicUsize>,
    active: Arc<std::sync::atomic::AtomicUsize>,
    maximum_active: Arc<std::sync::atomic::AtomicUsize>,
    fail_height: Arc<std::sync::atomic::AtomicI64>,
    invalid_hash: bool,
    include_log: bool,
    forked: bool,
}
impl DelayedHashRpc {
    fn new(include_log: bool) -> Self {
        Self {
            chain_calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            active: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            maximum_active: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            fail_height: Arc::new(std::sync::atomic::AtomicI64::new(0)),
            invalid_hash: false,
            include_log,
            forked: false,
        }
    }
}
async fn delayed_hash_server(state: DelayedHashRpc) -> (String, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .fallback(axum::routing::post(delayed_hash_rpc))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, server)
}
async fn delayed_hash_rpc(
    State(state): State<DelayedHashRpc>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let active = state.active.fetch_add(1, Ordering::SeqCst) + 1;
    state.maximum_active.fetch_max(active, Ordering::SeqCst);
    // Differing header delays force completion order to differ from height order.
    let delay = if body["method"] == "eth_getBlockByNumber" {
        let height = i64::from_str_radix(
            body["params"][0].as_str().unwrap().trim_start_matches("0x"),
            16,
        )
        .unwrap();
        20 + (height % 4) as u64 * 3
    } else {
        20
    };
    tokio::time::sleep(Duration::from_millis(delay)).await;
    let result = match body["method"].as_str().unwrap() {
        "eth_chainId" => {
            state.chain_calls.fetch_add(1, Ordering::SeqCst);
            serde_json::json!("0x38")
        }
        "eth_blockNumber" => serde_json::json!("0x93"), //147, finalized132
        "eth_getBlockByNumber" => {
            let height = i64::from_str_radix(
                body["params"][0].as_str().unwrap().trim_start_matches("0x"),
                16,
            )
            .unwrap();
            let fail = state.fail_height.load(Ordering::Relaxed) == height;
            serde_json::json!({"number":if fail && !state.invalid_hash {serde_json::json!(format!("0x{:x}",height-1))}else{body["params"][0].clone()},"hash":if fail && state.invalid_hash {"SENTINEL_BAD_HASH".into()}else{block_hash(height+if state.forked{100000}else{0})},"timestamp":format!("0x{:x}",chrono::Utc::now().timestamp())})
        }
        "eth_getLogs" => {
            if state.include_log {
                serde_json::json!([{
                    "address":DEFAULT_CL8Y_BEP20,"topics":[voting_ledger::parser::TRANSFER_TOPIC,format!("0x{:0>64}",EVM.trim_start_matches("0x")),format!("0x{:0>64}","2222222222222222222222222222222222222222")],
                    "data":format!("0x{:064x}",1),"blockNumber":"0x65","blockHash":block_hash(101+if state.forked{100000}else{0}),"transactionHash":block_hash(10101),"logIndex":"0x0","removed":false
                }])
            } else {
                serde_json::json!([])
            }
        }
        _ => panic!("unexpected capability request"),
    };
    state.active.fetch_sub(1, Ordering::SeqCst);
    Json(serde_json::json!({"jsonrpc":"2.0","id":1,"result":result}))
}
#[tokio::test]
async fn delayed_bsc_range_hashes_use_bounded_pinned_concurrency_and_constant_network_checks() {
    let state = DelayedHashRpc::new(false);
    let (url, server) = delayed_hash_server(state.clone()).await;
    let started = std::time::Instant::now();
    let observation = voting_ledger::bsc::BscClient::new(vec![url])
        .canonical_range(DEFAULT_CL8Y_BEP20, 101, 132, &[])
        .await
        .unwrap();
    let elapsed = started.elapsed();
    let chain_calls = state.chain_calls.load(Ordering::SeqCst);
    let concurrency = state.maximum_active.load(Ordering::SeqCst);
    println!(
        "hash_range elapsed_ms={} network_checks={chain_calls} maximum_active={concurrency}",
        elapsed.as_millis()
    );
    server.abort();
    assert_eq!(
        observation.hashes,
        (101..=132)
            .map(|height| (height, block_hash(height)))
            .collect::<Vec<_>>()
    );
    assert!(
        elapsed < Duration::from_millis(900),
        "processing must fit two 0.45s BSC blocks at modest RPC latency"
    );
    assert!(
        chain_calls <= 4,
        "validate network around the pinned observation, not once per historical header"
    );
    assert!((2..=voting_ledger::bsc::BSC_RANGE_RPC_CONCURRENCY).contains(&concurrency));
}

#[tokio::test]
async fn interior_hash_batch_failure_rolls_back_entire_ingest_and_pins_failover_observation() {
    for invalid_hash in [false, true] {
        let Some((pool, _lock)) = setup().await else {
            return;
        };
        let mut bad = DelayedHashRpc::new(true);
        bad.invalid_hash = invalid_hash;
        bad.forked = true;
        bad.fail_height.store(106, Ordering::Relaxed);
        let good = DelayedHashRpc::new(true);
        let (bad_url, bad_server) = delayed_hash_server(bad.clone()).await;
        let (good_url, good_server) = delayed_hash_server(good.clone()).await;
        let seed = TerraForkFixture::new(100);
        seed_terra_fork_electorate(&pool, &seed, false).await;
        let before: (i64, String, i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
            "SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain='bsc'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let failed = voting_ledger::bsc::BscClient::new(vec![bad_url.clone()]);
        let cfg = config(&bad_url);
        let error = ingest::ingest_bsc_range(&pool, &failed, &cfg, 101, 132)
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            if invalid_hash {
                "provider: invalid_block_hash"
            } else {
                "provider: wrong_block_number"
            }
        );
        assert!(!error.to_string().contains("SENTINEL"));
        let after: (i64, String, i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
            "SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain='bsc'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after, before);
        assert_eq!(db::indexed_bsc_block(&pool).await.unwrap(), 100);
        assert_eq!(
            db::balance_at(&pool, Chain::Bsc, EVM, 132).await.unwrap(),
            BigInt::from(1000)
        );
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_bep20_transfers")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 0);
        let hashes: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ledger_canonical_blocks WHERE chain='bsc'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(hashes, 1);
        let candidates =
            voting_ledger::bsc::BscClient::new(vec![bad_url.clone(), good_url.clone()]);
        let observation = candidates
            .canonical_range(DEFAULT_CL8Y_BEP20, 101, 132, &[])
            .await
            .unwrap();
        assert_eq!(observation.end_hash, block_hash(132));
        assert_eq!(
            observation.hashes,
            (101..=132).map(|h| (h, block_hash(h))).collect::<Vec<_>>()
        );
        assert_eq!(
            observation.source.block_hash(101).await.unwrap(),
            block_hash(101),
            "source remains the healthy provider, never the first provider's fork"
        );
        for _ in 0..2 {
            assert_eq!(
                ingest::ingest_bsc_range(&pool, &candidates, &cfg, 101, 132)
                    .await
                    .unwrap(),
                1
            );
        }
        assert_eq!(
            db::balance_at(&pool, Chain::Bsc, EVM, 132).await.unwrap(),
            BigInt::from(999)
        );
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_bep20_transfers")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 1);
        let checkpoint: (i64, String, bool, bool) = sqlx::query_as(
            "SELECT height,block_hash,complete,canonical FROM ledger_checkpoints WHERE chain='bsc'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(checkpoint, (132, block_hash(132), true, true));
        let hashes:Vec<(i64,String)>=sqlx::query_as("SELECT height,block_hash FROM ledger_canonical_blocks WHERE chain='bsc' AND height>100 ORDER BY height").fetch_all(&pool).await.unwrap();
        assert_eq!(
            hashes,
            (101..=132).map(|h| (h, block_hash(h))).collect::<Vec<_>>()
        );
        assert!(
            bad.maximum_active.load(Ordering::SeqCst)
                <= voting_ledger::bsc::BSC_RANGE_RPC_CONCURRENCY
        );
        assert!(
            good.maximum_active.load(Ordering::SeqCst)
                <= voting_ledger::bsc::BSC_RANGE_RPC_CONCURRENCY
        );
        bad_server.abort();
        good_server.abort();
    }
}

#[tokio::test]
async fn bsc_rpc_error_codes_preserve_safe_classes_without_provider_messages() {
    for (code, class) in [
        (-32005, "rpc_limit"),
        (-32601, "rpc_method_unavailable"),
        (-32602, "rpc_parameters_or_range"),
        (-32603, "rpc_internal_error"),
        (-99999, "rpc_error"),
    ] {
        let app=Router::new().fallback(axum::routing::post(move || async move {
            Json(serde_json::json!({"jsonrpc":"2.0","id":1,"error":{"code":code,"message":"SENTINEL_PRIVATE_PROVIDER_RESPONSE"}}))
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = voting_ledger::bsc::BscClient::new(vec![url]);
        assert_eq!(
            client.block_number().await.unwrap_err().to_string(),
            format!("provider: {class}")
        );
        assert_eq!(
            client.block_hash(101).await.unwrap_err().to_string(),
            format!("provider: {class}")
        );
        server.abort();
    }
}

async fn rpc_server_with_tip(
    height: i64,
) -> (
    String,
    Arc<std::sync::atomic::AtomicI64>,
    tokio::task::JoinHandle<()>,
) {
    let tip = Arc::new(std::sync::atomic::AtomicI64::new(height));
    let app = Router::new()
        .fallback(axum::routing::post(rpc_fixture))
        .with_state(RpcFixture {
            tip: tip.clone(),
            bad: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, tip, server)
}
#[tokio::test]
async fn cached_lower_head_fails_over_at_canonical_cursor_and_all_lower_publish_nothing_both_chains(
) {
    for chain in [Chain::Terra, Chain::Bsc] {
        let Some((pool, _lock)) = setup().await else {
            return;
        };
        let (primary, primary_state, primary_server) = terra_fork_server(103).await;
        let (secondary, secondary_state, secondary_server) = terra_fork_server(106).await;
        let (rpc_primary, _, rpc_primary_server) = rpc_server_with_tip(118).await;
        let (rpc_secondary, rpc_secondary_tip, rpc_secondary_server) =
            rpc_server_with_tip(121).await;
        seed_terra_fork_electorate(&pool, &secondary_state, false).await;
        let good_lcd = LcdClient::new(vec![secondary.clone()]);
        let cfg = config(&secondary);
        let good_bsc = voting_ledger::bsc::BscClient::new(vec![rpc_secondary.clone()]);
        for height in 101..=104 {
            ingest_test_block(&pool, chain, &good_lcd, &good_bsc, &cfg, height)
                .await
                .unwrap();
        }
        let lcd = LcdClient::new(vec![primary.clone(), secondary.clone()]);
        let bsc =
            voting_ledger::bsc::BscClient::new(vec![rpc_primary.clone(), rpc_secondary.clone()]);
        if chain == Chain::Terra {
            assert_eq!(
                lcd.latest_height().await.unwrap(),
                103,
                "default API still returns first valid head"
            );
            assert_eq!(lcd.latest_height_at_least(104).await.unwrap(), 106);
        } else {
            assert_eq!(bsc.block_number().await.unwrap(), 118);
            assert_eq!(bsc.block_number_at_least(119).await.unwrap(), 121);
        }
        voting_ledger::worker::poll_chain(&pool, &Some(lcd.clone()), &bsc, &cfg, chain)
            .await
            .unwrap();
        let cursor = match chain {
            Chain::Terra => db::indexed_terra_height(&pool).await.unwrap(),
            Chain::Bsc => db::indexed_bsc_block(&pool).await.unwrap(),
        };
        assert_eq!(cursor, 106);
        let wallet = if chain == Chain::Terra { ALICE } else { EVM };
        assert_eq!(
            db::balance_at(&pool, chain, wallet, 106).await.unwrap(),
            BigInt::from(994)
        );
        let before: (i64, String, i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
            "SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain=$1",
        )
        .bind(chain.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
        secondary_state.tip.store(105, Ordering::Relaxed);
        rpc_secondary_tip.store(120, Ordering::Relaxed);
        let error = voting_ledger::worker::poll_chain(&pool, &Some(lcd), &bsc, &cfg, chain)
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "provider: provider_head_regressed");
        let after: (i64, String, i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
            "SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain=$1",
        )
        .bind(chain.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after, before);
        let table = if chain == Chain::Terra {
            "cl8y_cw20_transfers"
        } else {
            "cl8y_bep20_transfers"
        };
        let rows: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM public.{table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 6);
        let hashes: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ledger_canonical_blocks WHERE chain=$1")
                .bind(chain.as_str())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(hashes, 7);
        assert_eq!(
            db::balance_at(&pool, chain, wallet, 106).await.unwrap(),
            BigInt::from(994)
        );
        // A real rewind can legitimately lower the admissible cursor; the worker
        // must use the newly canonical checkpoint, rather than its old106 cursor.
        if chain == Chain::Terra {
            primary_state.fork_after.store(104, Ordering::Relaxed);
            secondary_state.fork_after.store(104, Ordering::Relaxed);
            secondary_state.tip.store(105, Ordering::Relaxed);
            let lcd = LcdClient::new(vec![primary, secondary]);
            voting_ledger::worker::poll_chain(&pool, &Some(lcd), &bsc, &cfg, chain)
                .await
                .unwrap();
            assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), 105);
            assert_eq!(
                db::balance_at(&pool, chain, wallet, 105).await.unwrap(),
                BigInt::from(994)
            );
        }
        primary_server.abort();
        secondary_server.abort();
        rpc_primary_server.abort();
        rpc_secondary_server.abort();
    }
}

#[tokio::test]
async fn mutable_lcd_cache_bypass_avoids_false_refusals_and_incoherent_origin_still_rejects() {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    for cached_latest in [true, false] {
        let Some((pool, _lock)) = setup().await else {
            return;
        };
        let origin = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let missing_directives = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let immutable_bypassed = Arc::new(AtomicBool::new(false));
        let mode = origin.clone();
        let missing = missing_directives.clone();
        let immutable = immutable_bypassed.clone();
        let app=Router::new().fallback(get(move|uri:OriginalUri,headers:axum::http::HeaderMap| {
            let mode=mode.clone();let missing=missing.clone();let immutable=immutable.clone();
            async move {
                let path=uri.0.path();
                let latest=path.ends_with("/blocks/latest");let search=path.contains("/tx/");
                let bypass=headers.get("cache-control").and_then(|v|v.to_str().ok())==Some("no-cache, max-age=0")
                    && headers.get("pragma").and_then(|v|v.to_str().ok())==Some("no-cache");
                if (latest||search)&&!bypass{missing.fetch_add(1,Ordering::Relaxed);}
                if !latest&&!search&&(headers.contains_key("cache-control")||headers.contains_key("pragma")){immutable.store(true,Ordering::Relaxed);}
                let state=mode.load(Ordering::Relaxed);
                let bytes="cached-gateway-transfer:111";
                if path.contains("/blocks/") {
                    let height=if latest && (state==1||(cached_latest&&!bypass)){110}else{111};
                    Json(serde_json::json!({"block_id":{"hash":format!("{height:064X}")},"sdk_block":{
                        "header":{"height":height.to_string(),"chain_id":"columbus-5","time":chrono::Utc::now().to_rfc3339()},
                        "data":{"txs":[base64::engine::general_purpose::STANDARD.encode(bytes)]}
                    }}))
                } else if state==2 || (!cached_latest&&!bypass) {
                    Json(serde_json::json!({"pagination":null,"total":"0","tx_responses":[]}))
                } else {
                    Json(serde_json::json!({"pagination":null,"total":"1","tx_responses":[{
                        "height":"111","txhash":hex::encode_upper(Sha256::digest(bytes.as_bytes())),"code":0,"logs":[],
                        "events":[{"type":"wasm","attributes":[
                            {"key":"_contract_address","value":DEFAULT_CL8Y_CW20},{"key":"action","value":"transfer"},
                            {"key":"from","value":ALICE},{"key":"to","value":"terra1bob"},{"key":"amount","value":"1"}
                        ]}]
                    }]}))
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let seed = TerraForkFixture::new(110);
        seed_terra_fork_electorate(&pool, &seed, false).await;
        let mut tx = pool.begin().await.unwrap();
        db::lock_chain(&mut tx, Chain::Terra).await.unwrap();
        db::publish_checkpoint(&mut tx, Chain::Terra, 110, &format!("{:064X}", 110), 110)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let lcd = LcdClient::new(vec![url.clone()]);
        let cfg = config(&url);
        assert_eq!(
            ingest::ingest_terra_height(&pool, &lcd, &cfg, 111)
                .await
                .unwrap(),
            1
        );
        assert_eq!(missing_directives.load(Ordering::Relaxed), 0);
        assert!(
            !immutable_bypassed.load(Ordering::Relaxed),
            "immutable height/hash reads keep ordinary caching"
        );
        let before:(i64,String,i64,chrono::DateTime<chrono::Utc>)=sqlx::query_as("SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain='terra'").fetch_one(&pool).await.unwrap();
        for (state, class) in [
            (1, "future_terra_height"),
            (2, "incomplete_or_reordered_execution_set"),
        ] {
            origin.store(state, Ordering::Relaxed);
            let error = ingest::ingest_terra_height(&pool, &lcd, &cfg, 111)
                .await
                .unwrap_err();
            assert_eq!(error.to_string(), format!("provider: {class}"));
            let after:(i64,String,i64,chrono::DateTime<chrono::Utc>)=sqlx::query_as("SELECT height,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain='terra'").fetch_one(&pool).await.unwrap();
            assert_eq!(after, before);
            assert_eq!(db::indexed_terra_height(&pool).await.unwrap(), 111);
            assert_eq!(
                db::balance_at(&pool, Chain::Terra, ALICE, 111)
                    .await
                    .unwrap(),
                BigInt::from(999)
            );
            let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cl8y_cw20_transfers")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(events, 1);
        }
        server.abort();
    }
}

#[derive(Clone)]
struct MovingHeadRpc {
    started: std::time::Instant,
    mode: Arc<std::sync::atomic::AtomicUsize>,
    heads: Arc<tokio::sync::Mutex<Vec<i64>>>,
    ranges: Arc<tokio::sync::Mutex<Vec<(i64, i64)>>>,
}
impl MovingHeadRpc {
    fn new() -> Self {
        Self {
            started: std::time::Instant::now(),
            mode: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            heads: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            ranges: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }
    fn tip(&self) -> i64 {
        115 + (self.started.elapsed().as_millis() / 450) as i64
    }
}
async fn moving_head_rpc(
    State(state): State<MovingHeadRpc>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    tokio::time::sleep(Duration::from_millis(90)).await;
    let mode = state.mode.load(Ordering::Relaxed);
    let number = |v: &serde_json::Value| {
        i64::from_str_radix(v.as_str().unwrap().trim_start_matches("0x"), 16).unwrap()
    };
    let result = match body["method"].as_str().unwrap() {
        "eth_chainId" => serde_json::json!(if mode == 4 { "0x1" } else { "0x38" }),
        "eth_blockNumber" => {
            let tip = state.tip();
            state.heads.lock().await.push(tip);
            serde_json::json!(format!("0x{tip:x}"))
        }
        "eth_getBlockByNumber" => {
            let height = number(&body["params"][0]);
            serde_json::json!({"number":format!("0x{:x}",height-if mode==3 && height>=115{1}else{0}),"hash":block_hash(height+if mode==1{100000}else{0}),"timestamp":format!("0x{:x}",chrono::Utc::now().timestamp()-if mode==2{121}else{0})})
        }
        "eth_getLogs" => {
            let filter = &body["params"][0];
            let from = number(&filter["fromBlock"]);
            let to = number(&filter["toBlock"]);
            state.ranges.lock().await.push((from, to));
            serde_json::json!((from..=to).map(|height|serde_json::json!({"address":DEFAULT_CL8Y_BEP20,"topics":[voting_ledger::parser::TRANSFER_TOPIC,format!("0x{:0>64}",EVM.trim_start_matches("0x")),format!("0x{:0>64}","2222222222222222222222222222222222222222")],"data":format!("0x{:064x}",1),"blockNumber":format!("0x{height:x}"),"blockHash":block_hash(height),"transactionHash":block_hash(10000+height),"logIndex":"0x0","removed":false})).collect::<Vec<_>>())
        }
        _ => panic!("unexpected moving-head RPC method"),
    };
    Json(serde_json::json!({"jsonrpc":"2.0","id":1,"result":result}))
}
async fn moving_head_server(state: MovingHeadRpc) -> (String, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .fallback(axum::routing::post(moving_head_rpc))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    (
        url,
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }),
    )
}
#[tokio::test]
async fn moving_bsc_head_at_production_latency_commits_true_tip_within_unchanged_freshness_budget()
{
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let state = MovingHeadRpc::new();
    let (url, server) = moving_head_server(state.clone()).await;
    let client = voting_ledger::bsc::BscClient::new(vec![url.clone()]);
    let cfg = config(&url);
    seed_registration(&pool, Chain::Bsc, EVM, 100, &BigInt::from(1000), None)
        .await
        .unwrap();
    let future = "0x3333333333333333333333333333333333333333";
    db::insert_registration(&pool, Chain::Bsc, future, 131, &BigInt::from(0), None)
        .await
        .unwrap();
    sqlx::query("UPDATE voting_registrations SET baseline_block_hash=$1,baseline_verified=false WHERE wallet_address=$2").bind(block_hash(100131)).bind(future).execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    db::publish_checkpoint(&mut tx, Chain::Bsc, 100, &block_hash(100), 115)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    tokio::time::sleep(Duration::from_millis(450)).await;
    let mut previous = 100;
    for _ in 0..5 {
        let before = state.heads.lock().await.len();
        let started = std::time::Instant::now();
        let catch_up =
            voting_ledger::worker::poll_chain_progress(&pool, &None, &client, &cfg, Chain::Bsc)
                .await
                .unwrap();
        let (height,tip,complete,canonical):(i64,i64,bool,bool)=sqlx::query_as("SELECT height,network_tip,complete,canonical FROM ledger_checkpoints WHERE chain='bsc'").fetch_one(&pool).await.unwrap();
        let observed = state.heads.lock().await[before..].to_vec();
        assert_eq!(
            observed.len(),
            3,
            "outer selection, pinned plan and final checked head"
        );
        assert_eq!(
            height,
            (observed[1] - 15).min(previous + 32),
            "plan uses its own fresh pinned head"
        );
        assert_eq!(
            tip, observed[2],
            "publication must retain the actual final observed head"
        );
        assert!(
            (15..=17).contains(&(tip - height)),
            "true finalized lag {} exceeds unchanged policy",
            tip - height
        );
        assert!(height > previous && complete && canonical && !catch_up);
        assert_eq!(
            state.ranges.lock().await.last().copied(),
            Some((previous + 1, height))
        );
        assert_eq!(
            db::balance_at(&pool, Chain::Bsc, EVM, height)
                .await
                .unwrap(),
            BigInt::from(1100 - height)
        );
        let verified: bool = sqlx::query_scalar(
            "SELECT baseline_verified FROM voting_registrations WHERE wallet_address=$1",
        )
        .bind(future)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            !verified && height < 131,
            "future baseline outside actual selected end must remain pending"
        );
        println!(
            "moving_head poll_ms={} range={}-{} true_tip={tip} lag={}",
            started.elapsed().as_millis(),
            previous + 1,
            height,
            tip - height
        );
        previous = height;
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    server.abort();
}
#[tokio::test]
async fn final_bsc_same_source_recheck_after_sql_rejects_fork_stale_header_wrong_height_and_network_atomically(
) {
    for (mode, expected) in [
        (1, "bsc_changed_during_ingest"),
        (2, "stale_or_future_provider_head"),
        (3, "wrong_head_block"),
        (4, "wrong_bsc_network"),
    ] {
        let Some((pool, _lock)) = setup().await else {
            return;
        };
        let state = MovingHeadRpc::new();
        let (url, server) = moving_head_server(state.clone()).await;
        seed_registration(&pool, Chain::Bsc, EVM, 100, &BigInt::from(1000), None)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        db::publish_checkpoint(&mut tx, Chain::Bsc, 100, &block_hash(100), 115)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        // Advance the simulated starting time so101 is safely finalized before the pass.
        tokio::time::sleep(Duration::from_millis(450)).await;
        let before:(i64,i64,String,i64,chrono::DateTime<chrono::Utc>)=sqlx::query_as("SELECT height,network_tip,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain='bsc'").fetch_one(&pool).await.unwrap();
        let mut gate = install_ingest_gate(&pool, Chain::Bsc, 1).await;
        let p = pool.clone();
        let cfg = config(&url);
        let client = voting_ledger::bsc::BscClient::new(vec![url]);
        let write =
            tokio::spawn(
                async move { ingest::ingest_bsc_range(&p, &client, &cfg, 101, 101).await },
            );
        wait_advisory_query(&pool, INGEST_GATE_CLASS, 1).await;
        state.mode.store(mode, Ordering::Relaxed);
        release_ingest_gate(&mut gate, 1).await;
        let error = write.await.unwrap().unwrap_err();
        assert!(
            matches!(error,LedgerError::Provider(class) if class==expected),
            "expected {expected}, got {error:?}"
        );
        let after:(i64,i64,String,i64,chrono::DateTime<chrono::Utc>)=sqlx::query_as("SELECT height,network_tip,block_hash,epoch,observed_at FROM ledger_checkpoints WHERE chain='bsc'").fetch_one(&pool).await.unwrap();
        assert_eq!(after, before);
        assert_eq!(
            db::balance_at(&pool, Chain::Bsc, EVM, 101).await.unwrap(),
            BigInt::from(1000)
        );
        let events: i64 = sqlx::query_scalar("SELECT count(*) FROM cl8y_bep20_transfers")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(events, 0);
        let hashes: i64 =
            sqlx::query_scalar("SELECT count(*) FROM ledger_canonical_blocks WHERE chain='bsc'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(hashes, 1);
        remove_ingest_gate(&pool, Chain::Bsc, 1).await;
        server.abort();
    }
}
#[tokio::test]
async fn process_successful_terra_backlog_repolls_without_interval_sleep_and_keeps_sibling_and_intents_healthy(
) {
    let Some((pool, _lock)) = setup().await else {
        return;
    };
    let (terra, state, terra_server) = terra_fork_server(180).await;
    let (rpc, _tip, rpc_server) = rpc_server_with_tip(147).await;
    seed_terra_fork_electorate(&pool, &state, false).await;
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bind = socket.local_addr().unwrap().to_string();
    drop(socket);
    let started = std::time::Instant::now();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_voting-ledger"))
        .kill_on_drop(true)
        .env_clear()
        .env(
            "DATABASE_URL",
            std::env::var("LEDGER_TEST_DATABASE_URL").unwrap(),
        )
        .env("RUN_MODE", "prod")
        .env("TERRA_LCD_URL", &terra)
        .env("BSC_RPC_URLS", &rpc)
        .env("API_BIND", &bind)
        .env("POLL_INTERVAL_MS", "5000")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            assert!(child.try_wait().unwrap().is_none());
            if db::indexed_terra_height(&pool).await.unwrap() == 180
                && db::indexed_bsc_block(&pool).await.unwrap() == 132
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("successful backlog must repoll before the configured five-second sleep");
    assert!(started.elapsed() < Duration::from_secs(5));
    let http = reqwest::Client::new();
    wait_child_status(
        &mut child,
        &http,
        &format!("http://{bind}"),
        reqwest::StatusCode::OK,
    )
    .await;
    assert_eq!(
        db::balance_at(&pool, Chain::Terra, ALICE, 180)
            .await
            .unwrap(),
        BigInt::from(920)
    );
    assert_eq!(
        db::balance_at(&pool, Chain::Bsc, EVM, 132).await.unwrap(),
        BigInt::from(968)
    );
    assert_eq!(
        db::get_state(&pool, "intents_ok").await.unwrap(),
        Some("true".into())
    );
    terminate_child(&mut child).await;
    terra_server.abort();
    rpc_server.abort();
}
