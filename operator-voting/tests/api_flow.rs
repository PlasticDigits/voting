use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use http_body_util::BodyExt;
use k256::ecdsa::signature::Signer;
use k256::ecdsa::SigningKey;
use operator_voting::api::{router, AppState};
use operator_voting::config::{VotingConfig, APP_NAME};
use operator_voting::crypto::{cosmos_address_from_pubkey, eip191_hash, verify_terra};
use operator_voting::db;
use operator_voting::payload::SignedPayload;
use serde_json::Value;
use sha3::Digest;
use sqlx::PgPool;
use tower::ServiceExt;

fn test_db_url() -> Option<String> {
    std::env::var("LEDGER_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()
        .filter(|s| !s.is_empty())
}

async fn setup_pool() -> Option<(PgPool, voting_ledger::test_lock::IntegrationDbLock)> {
    let url = test_db_url()?;
    let lock = voting_ledger::test_lock::hold_integration_db(&url)
        .await
        .ok()?;
    let pool = PgPool::connect(&url).await.ok()?;
    voting_ledger::db::migrate(&pool).await.ok()?;
    db::migrate(&pool).await.ok()?;
    sqlx::query(
        "TRUNCATE voting.votes, voting.proposal_snapshots, voting.proposals, voting.registration_intents, voting.signatures, voting_registrations, cl8y_balances, cl8y_bsc_balances, cl8y_cw20_transfers, cl8y_bep20_transfers CASCADE",
    )
    .execute(&pool)
    .await
    .ok()?;
    sqlx::query(
        "UPDATE indexer_state SET value = '0' WHERE key IN ('last_indexed_height', 'last_indexed_bsc_block')",
    )
    .execute(&pool)
    .await
    .ok()?;
    Some((pool, lock))
}

fn cl8y_raw(human: u64) -> String {
    (num_bigint::BigInt::from(human) * num_bigint::BigInt::from(10u64).pow(18)).to_string()
}

async fn set_tip(pool: &PgPool, terra: i64, bsc: i64) {
    sqlx::query(
        "INSERT INTO indexer_state (key, value) VALUES ('last_indexed_height', $1) ON CONFLICT (key) DO UPDATE SET value = $1",
    )
    .bind(terra.to_string())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO indexer_state (key, value) VALUES ('last_indexed_bsc_block', $1) ON CONFLICT (key) DO UPDATE SET value = $1",
    )
    .bind(bsc.to_string())
    .execute(pool)
    .await
    .unwrap();
}

fn cfg(pool_url: &str, blacklist: &str) -> VotingConfig {
    VotingConfig::for_tests(pool_url, blacklist)
}

fn terra_wallet() -> (String, String, SigningKey) {
    let sk = SigningKey::from_bytes((&[0x21u8; 32]).into()).unwrap();
    let vk = k256::ecdsa::VerifyingKey::from(&sk);
    let compressed = vk.to_encoded_point(true);
    let address = cosmos_address_from_pubkey(compressed.as_bytes(), "terra").unwrap();
    let pubkey = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        compressed.as_bytes(),
    );
    (address, pubkey, sk)
}

fn sign_terra(sk: &SigningKey, message: &str) -> String {
    let sig: k256::ecdsa::Signature = sk.sign(message.as_bytes());
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, sig.to_bytes())
}

fn evm_wallet() -> (String, SigningKey) {
    let sk = SigningKey::from_slice(&[0x42u8; 32]).unwrap();
    let vk = k256::ecdsa::VerifyingKey::from(&sk);
    let pubkey_bytes = vk.to_encoded_point(false);
    let mut hasher = sha3::Keccak256::new();
    sha3::Digest::update(&mut hasher, &pubkey_bytes.as_bytes()[1..]);
    let digest: [u8; 32] = sha3::Digest::finalize(hasher).into();
    (format!("0x{}", hex::encode(&digest[12..])), sk)
}

fn sign_evm(sk: &SigningKey, message: &str) -> String {
    let hash = eip191_hash(message);
    let (sig, recid) = sk.sign_prehash_recoverable(&hash).unwrap();
    let mut bytes = [0u8; 65];
    bytes[..64].copy_from_slice(&sig.to_bytes());
    bytes[64] = recid.to_byte() + 27;
    format!("0x{}", hex::encode(bytes))
}

fn payload(chain: &str, chain_id: &str, purpose: &str, address: &str) -> SignedPayload {
    let now = Utc::now().timestamp();
    SignedPayload {
        app: APP_NAME.into(),
        chain: chain.into(),
        chain_id: chain_id.into(),
        purpose: purpose.into(),
        address: address.into(),
        issued_at: now,
        expires_at: now + 180,
        title: None,
        body_hash: None,
        proposal_id: None,
        choice: None,
    }
}

fn min_section(ch: char) -> String {
    format!("<p>{}</p>", ch.to_string().repeat(40))
}

fn valid_sections() -> Value {
    serde_json::json!({
        "problem": min_section('p'),
        "context": "",
        "solution": min_section('s'),
        "pros_cons": min_section('t'),
        "summary": min_section('m'),
        "success_criteria": min_section('c'),
    })
}

fn sections_hash(sections: &Value) -> String {
    operator_voting::proposal_sections::prepare_sections(sections)
        .map(|p| p.body_hash)
        .unwrap_or_else(|_| "00".repeat(32))
}

fn propose_payload(
    chain: &str,
    chain_id: &str,
    address: &str,
    title: &str,
    sections: &Value,
) -> SignedPayload {
    let mut p = payload(chain, chain_id, "propose", address);
    p.title = Some(title.into());
    p.body_hash = Some(sections_hash(sections));
    p
}

async fn call(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

#[tokio::test]
async fn terra_register_propose_vote_and_replay() {
    let Some((pool, _lock)) = setup_pool().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let url = test_db_url().unwrap();
    let (addr, pubkey, sk) = terra_wallet();
    verify_terra(&addr, "ping", &sign_terra(&sk, "ping"), &pubkey).unwrap();

    sqlx::query(
        r#"
        INSERT INTO voting_registrations (chain, wallet_address, registered_at_height, initial_balance)
        VALUES ('terra', $1, 10, $2::numeric)
        "#,
    )
    .bind(&addr)
    .bind((num_bigint::BigInt::from(5000u64) * num_bigint::BigInt::from(10u64).pow(18)).to_string())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO indexer_state (key, value) VALUES ('last_indexed_height', '20') ON CONFLICT (key) DO UPDATE SET value = '20'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let state = AppState {
        pool: pool.clone(),
        cfg: cfg(&url, ""),
    };
    let app = router(state);

    let sections = valid_sections();
    let p = propose_payload("terra", "columbus-5", &addr, "Test poll", &sections);
    let raw = serde_json::to_string(&p).unwrap();
    let sig = sign_terra(&sk, &raw);
    let (status, created) = call(
        app.clone(),
        Request::post("/v1/proposals")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "chain": "terra",
                    "address": addr,
                    "payload": raw,
                    "signature": sig,
                    "pubkey": pubkey,
                    "title": "Test poll",
                    "body_sections": sections,
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap().to_string();

    let mut vote = payload("terra", "columbus-5", "vote", &addr);
    vote.proposal_id = Some(id.clone());
    vote.choice = Some("for".into());
    let vraw = serde_json::to_string(&vote).unwrap();
    let vsig = sign_terra(&sk, &vraw);
    let vote_body = serde_json::json!({
        "chain": "terra",
        "address": addr,
        "payload": vraw,
        "signature": vsig,
        "pubkey": pubkey,
        "choice": "for",
    })
    .to_string();
    let (status, voted) = call(
        app.clone(),
        Request::post(format!("/v1/proposals/{id}/votes"))
            .header("content-type", "application/json")
            .body(Body::from(vote_body.clone()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{voted}");
    assert_eq!(voted["weight"], "5000000000000000000000");

    let (status, replay) = call(
        app.clone(),
        Request::post(format!("/v1/proposals/{id}/votes"))
            .header("content-type", "application/json")
            .body(Body::from(vote_body))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{replay}");

    let mut wrong = payload("terra", "columbus-5", "register", &addr);
    wrong.purpose = "register".into();
    let rraw = serde_json::to_string(&wrong).unwrap();
    let (status, _) = call(
        app,
        Request::post(format!("/v1/proposals/{id}/votes"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "chain": "terra",
                    "address": addr,
                    "payload": rraw,
                    "signature": sign_terra(&sk, &rraw),
                    "pubkey": pubkey,
                    "choice": "for",
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn blacklist_and_threshold_and_evm() {
    let Some((pool, _lock)) = setup_pool().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let url = test_db_url().unwrap();
    let (evm, sk) = evm_wallet();
    sqlx::query(
        r#"
        INSERT INTO voting_registrations (chain, wallet_address, registered_at_height, initial_balance)
        VALUES ('bsc', $1, 5, $2::numeric)
        "#,
    )
    .bind(&evm)
    .bind((num_bigint::BigInt::from(999u64) * num_bigint::BigInt::from(10u64).pow(18)).to_string())
    .execute(&pool)
    .await
    .unwrap();

    let state = AppState {
        pool: pool.clone(),
        cfg: cfg(&url, &evm),
    };
    let app = router(state);
    let mut p = payload("bsc", "56", "propose", &evm);
    p.title = Some("x".into());
    p.body_hash = Some(sections_hash(&valid_sections()));
    let raw = serde_json::to_string(&p).unwrap();
    let (status, body) = call(
        app,
        Request::post("/v1/proposals")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "chain": "bsc",
                    "address": evm,
                    "payload": raw,
                    "signature": sign_evm(&sk, &raw),
                    "title": "x",
                    "body_sections": valid_sections(),
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}

#[tokio::test]
async fn xss_stripped_on_create() {
    let Some((pool, _lock)) = setup_pool().await else {
        return;
    };
    let url = test_db_url().unwrap();
    let (addr, pubkey, sk) = terra_wallet();
    sqlx::query(
        r#"
        INSERT INTO voting_registrations (chain, wallet_address, registered_at_height, initial_balance)
        VALUES ('terra', $1, 1, $2::numeric)
        "#,
    )
    .bind(&addr)
    .bind((num_bigint::BigInt::from(2000u64) * num_bigint::BigInt::from(10u64).pow(18)).to_string())
    .execute(&pool)
    .await
    .unwrap();
    let dirty_section = format!(
        "<p>{}</p><script>alert(1)</script><img src=x onerror=\"alert(2)\">",
        "x".repeat(40)
    );
    let mut sections = valid_sections();
    sections["problem"] = serde_json::json!(dirty_section);
    let mut p = payload("terra", "columbus-5", "propose", &addr);
    p.title = Some("xss".into());
    p.body_hash = Some(sections_hash(&sections));
    let raw = serde_json::to_string(&p).unwrap();
    let app = router(AppState {
        pool,
        cfg: cfg(&url, ""),
    });
    let (status, created) = call(
        app.clone(),
        Request::post("/v1/proposals")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "chain": "terra",
                    "address": addr,
                    "payload": raw,
                    "signature": sign_terra(&sk, &raw),
                    "pubkey": pubkey,
                    "title": "xss",
                    "body_sections": sections,
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap();
    let (status, detail) = call(
        app.clone(),
        Request::get(format!("/v1/proposals/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let html = detail["body_html"].as_str().unwrap();
    assert!(!html.contains("script"));
    assert!(!html.contains("onerror"));
    assert!(!html.contains("alert"));
    let problem = detail["body_sections"]["problem"].as_str().unwrap();
    assert!(!problem.contains("script"));
    assert!(problem.contains("xxxxxxxx"));
    let (status, listed) = call(
        app,
        Request::get("/v1/proposals").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let summary = listed[0]["summary"].as_str().unwrap();
    assert!(!summary.contains("script"));
}

fn app(pool: PgPool, url: &str) -> axum::Router {
    router(AppState {
        pool,
        cfg: cfg(url, ""),
    })
}

/// O-RL: the production `router()` composition (AppState + rate-limit layer) must 429.
#[tokio::test]
async fn router_post_burst_returns_429_with_retry_after() {
    let Some((pool, _lock)) = setup_pool().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let url = test_db_url().unwrap();
    let mut limited_cfg = cfg(&url, "");
    limited_cfg.rate_limit_post_per_minute = 2;
    limited_cfg.rate_limit_post_burst = 2;
    limited_cfg.rate_limit_trust_forwarded = true;
    let app = router(AppState {
        pool,
        cfg: limited_cfg,
    });
    let post = || {
        Request::post("/v1/register")
            .header("content-type", "application/json")
            .header("x-forwarded-for", "203.0.113.50")
            .body(Body::from(r#"{"chain":"terra"}"#))
            .unwrap()
    };
    let (first, _) = call(app.clone(), post()).await;
    let (second, _) = call(app.clone(), post()).await;
    assert!(
        first.is_client_error() && first != StatusCode::TOO_MANY_REQUESTS,
        "first POST should reach the handler, got {first}"
    );
    assert!(
        second.is_client_error() && second != StatusCode::TOO_MANY_REQUESTS,
        "second POST should reach the handler, got {second}"
    );
    let resp = app
        .clone()
        .oneshot(post())
        .await
        .expect("third POST");
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        resp.headers().get(axum::http::header::RETRY_AFTER).is_some(),
        "O-RL: 429 must include Retry-After"
    );
    let (health, body) = call(
        app,
        Request::get("/health").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(health, StatusCode::OK, "{body}");
}

async fn insert_reg(pool: &PgPool, chain: &str, wallet: &str, height: i64, human: u64) {
    sqlx::query(
        r#"
        INSERT INTO voting_registrations (chain, wallet_address, registered_at_height, initial_balance)
        VALUES ($1, $2, $3, $4::numeric)
        "#,
    )
    .bind(chain)
    .bind(wallet)
    .bind(height)
    .bind(cl8y_raw(human))
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn unregistered_balance_is_flagged_not_a_live_zero() {
    let Some((pool, _lock)) = setup_pool().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let url = test_db_url().unwrap();
    let (addr, _, _) = terra_wallet();
    let app = app(pool, &url);

    let (status, body) = call(
        app.clone(),
        Request::get(format!("/v1/balances/{addr}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["registered"], false);
    assert_eq!(body["pending"], false);
    assert_eq!(body["balance"], "0");
    assert!(body["initial_balance"].is_null());

    let (status, _) = call(
        app,
        Request::get(format!("/v1/registration/{addr}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pending_intent_is_visible_before_ledger_row() {
    let Some((pool, _lock)) = setup_pool().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let url = test_db_url().unwrap();
    let (addr, pubkey, sk) = terra_wallet();
    let app = app(pool, &url);
    let p = payload("terra", "columbus-5", "register", &addr);
    let raw = serde_json::to_string(&p).unwrap();
    let (status, registered) = call(
        app.clone(),
        Request::post("/v1/register")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "chain": "terra",
                    "address": addr,
                    "payload": raw,
                    "signature": sign_terra(&sk, &raw),
                    "pubkey": pubkey,
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{registered}");
    assert_eq!(registered["pending"], true);

    let (status, lookup) = call(
        app.clone(),
        Request::get(format!("/v1/registration/{addr}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{lookup}");
    assert_eq!(lookup["pending"]["terra"], true);
    assert!(lookup["terra"].is_null());

    let (status, bal) = call(
        app,
        Request::get(format!("/v1/balances/{addr}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{bal}");
    assert_eq!(bal["registered"], false);
    assert_eq!(bal["pending"], true);
    assert_eq!(bal["balance"], "0");
}

#[tokio::test]
async fn default_balance_clamps_when_tip_lags_register() {
    let Some((pool, _lock)) = setup_pool().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let url = test_db_url().unwrap();
    let (addr, _, _) = terra_wallet();
    insert_reg(&pool, "terra", &addr, 100, 3540).await;

    let app = app(pool.clone(), &url);

    set_tip(&pool, 0, 0).await;
    let (status, body) = call(
        app.clone(),
        Request::get(format!("/v1/balances/{addr}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["registered"], true);
    assert_eq!(body["balance"], cl8y_raw(3540));
    assert_eq!(body["as_of_height"], 100);
    assert_eq!(body["initial_balance"], cl8y_raw(3540));

    set_tip(&pool, 50, 0).await;
    let (status, body) = call(
        app.clone(),
        Request::get(format!("/v1/balances/{addr}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["balance"], cl8y_raw(3540));
    assert_eq!(body["as_of_height"], 100);

    let (status, historical) = call(
        app.clone(),
        Request::get(format!("/v1/balances/{addr}?height=50"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical}");
    assert_eq!(historical["balance"], "0");
    assert_eq!(historical["as_of_height"], 50);

    set_tip(&pool, 120, 0).await;
    let (status, caught_up) = call(
        app.clone(),
        Request::get(format!("/v1/balances/{addr}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{caught_up}");
    assert_eq!(caught_up["balance"], cl8y_raw(3540));
    assert_eq!(caught_up["as_of_height"], 120);

    sqlx::query(
        "INSERT INTO cl8y_balances (wallet_address, height, balance) VALUES ($1, 110, $2::numeric)",
    )
    .bind(&addr)
    .bind(cl8y_raw(3000))
    .execute(&pool)
    .await
    .unwrap();
    let (status, after_xfer) = call(
        app.clone(),
        Request::get(format!("/v1/balances/{addr}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{after_xfer}");
    assert_eq!(after_xfer["balance"], cl8y_raw(3000));

    let (status, reject) = call(
        app,
        Request::get(format!("/v1/balances/{addr}?chain=bsc"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{reject}");
}

#[tokio::test]
async fn bsc_default_balance_clamps_when_tip_lags_register() {
    let Some((pool, _lock)) = setup_pool().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let url = test_db_url().unwrap();
    let (evm, _) = evm_wallet();
    insert_reg(&pool, "bsc", &evm, 80, 2000).await;
    set_tip(&pool, 0, 40).await;

    let app = app(pool, &url);
    let (status, body) = call(
        app.clone(),
        Request::get(format!("/v1/balances/{evm}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["chain"], "bsc");
    assert_eq!(body["registered"], true);
    assert_eq!(body["balance"], cl8y_raw(2000));
    assert_eq!(body["as_of_height"], 80);

    let (status, historical) = call(
        app.clone(),
        Request::get(format!("/v1/balances/{evm}?height=40"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical}");
    assert_eq!(historical["balance"], "0");

    let (status, reject) = call(
        app,
        Request::get(format!("/v1/balances/{evm}?chain=terra"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{reject}");
}

#[tokio::test]
async fn propose_succeeds_when_tip_lags_register() {
    let Some((pool, _lock)) = setup_pool().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let url = test_db_url().unwrap();
    let (addr, pubkey, sk) = terra_wallet();
    insert_reg(&pool, "terra", &addr, 100, 1500).await;
    set_tip(&pool, 50, 0).await;

    let app = app(pool, &url);
    let sections = valid_sections();
    let p = propose_payload("terra", "columbus-5", &addr, "Lag", &sections);
    let raw = serde_json::to_string(&p).unwrap();
    let (status, created) = call(
        app,
        Request::post("/v1/proposals")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "chain": "terra",
                    "address": addr,
                    "payload": raw,
                    "signature": sign_terra(&sk, &raw),
                    "pubkey": pubkey,
                    "title": "Lag",
                    "body_sections": sections,
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["terra_height"], 100);
}

async fn post_propose(
    app: axum::Router,
    chain: &str,
    chain_id: &str,
    addr: &str,
    pubkey: Option<&str>,
    sign: impl Fn(&str) -> String,
    title: &str,
    sections: &Value,
    extra: serde_json::Value,
) -> (StatusCode, Value) {
    let p = propose_payload(chain, chain_id, addr, title, sections);
    let raw = serde_json::to_string(&p).unwrap();
    let mut body = serde_json::json!({
        "chain": chain,
        "address": addr,
        "payload": raw,
        "signature": sign(&raw),
        "title": title,
        "body_sections": sections,
    });
    if let Some(pk) = pubkey {
        body["pubkey"] = serde_json::json!(pk);
    }
    if let Some(map) = extra.as_object() {
        for (k, v) in map {
            body[k] = v.clone();
        }
    }
    call(
        app,
        Request::post("/v1/proposals")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn templated_propose_happy_path_and_legacy_get() {
    let Some((pool, _lock)) = setup_pool().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let url = test_db_url().unwrap();
    let (addr, pubkey, sk) = terra_wallet();
    insert_reg(&pool, "terra", &addr, 10, 2000).await;
    set_tip(&pool, 20, 0).await;
    let app = app(pool.clone(), &url);
    let sections = valid_sections();
    let (status, created) = post_propose(
        app.clone(),
        "terra",
        "columbus-5",
        &addr,
        Some(&pubkey),
        |m| sign_terra(&sk, m),
        "Template",
        &sections,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap();

    let (status, detail) = call(
        app.clone(),
        Request::get(format!("/v1/proposals/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["body_sections"]["context"], "");
    assert!(detail["body_html"]
        .as_str()
        .unwrap()
        .contains("Summary (TL;DR)"));
    assert_eq!(detail["body_sections"]["summary"], sections["summary"]);

    let (status, listed) = call(
        app.clone(),
        Request::get("/v1/proposals").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed[0]["summary"], sections["summary"]);

    sqlx::query(
        r#"
        INSERT INTO voting.proposals
            (id, chain, proposer, title, body_html, body_canonical, terra_height, bsc_block)
        VALUES ('22222222-2222-2222-2222-222222222222', 'terra', $1, 'Legacy', '<p>old body</p>', '<p>old body</p>', 1, 1)
        "#,
    )
    .bind(&addr)
    .execute(&pool)
    .await
    .unwrap();
    let (status, legacy) = call(
        app,
        Request::get("/v1/proposals/22222222-2222-2222-2222-222222222222")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{legacy}");
    assert!(legacy["body_sections"].is_null());
    assert_eq!(legacy["body_html"], "<p>old body</p>");
}

#[tokio::test]
async fn templated_propose_rejects_empty_sections_and_body_html() {
    let Some((pool, _lock)) = setup_pool().await else {
        return;
    };
    let url = test_db_url().unwrap();
    let (addr, pubkey, sk) = terra_wallet();
    insert_reg(&pool, "terra", &addr, 10, 2000).await;
    let app = app(pool, &url);

    for empty in ["", "   ", "<p></p>", "<p><br></p>"] {
        let mut sections = valid_sections();
        sections["pros_cons"] = serde_json::json!(empty);
        let (status, body) = post_propose(
            app.clone(),
            "terra",
            "columbus-5",
            &addr,
            Some(&pubkey),
            |m| sign_terra(&sk, m),
            "Bad",
            &sections,
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{empty:?} {body}");
    }

    let (status, body) = call(
        app.clone(),
        Request::post("/v1/proposals")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "chain": "terra",
                    "address": addr,
                    "payload": "{}",
                    "signature": "x",
                    "pubkey": pubkey,
                    "title": "Old",
                    "body_html": "<p>freeform</p>",
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["error"].as_str().unwrap().contains("body_sections"));

    let sections = valid_sections();
    let (status, body) = post_propose(
        app,
        "terra",
        "columbus-5",
        &addr,
        Some(&pubkey),
        |m| sign_terra(&sk, m),
        "Both",
        &sections,
        serde_json::json!({ "body_html": "<p>freeform</p>" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn templated_propose_hash_and_summary_bounds() {
    let Some((pool, _lock)) = setup_pool().await else {
        return;
    };
    let url = test_db_url().unwrap();
    let (addr, pubkey, sk) = terra_wallet();
    insert_reg(&pool, "terra", &addr, 10, 2000).await;
    let app = app(pool, &url);
    let sections = valid_sections();

    let mut p = payload("terra", "columbus-5", "propose", &addr);
    p.title = Some("Hash".into());
    p.body_hash = Some(operator_voting::payload::body_hash("<p>freeform</p>"));
    let raw = serde_json::to_string(&p).unwrap();
    let (status, body) = call(
        app.clone(),
        Request::post("/v1/proposals")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "chain": "terra",
                    "address": addr,
                    "payload": raw,
                    "signature": sign_terra(&sk, &raw),
                    "pubkey": pubkey,
                    "title": "Hash",
                    "body_sections": sections,
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    let dirty = format!("<p>{}</p><script>alert(1)</script>", "x".repeat(40));
    let mut dirty_sections = valid_sections();
    dirty_sections["problem"] = serde_json::json!(dirty);
    let mut unsorted = serde_json::Map::new();
    for key in [
        "summary",
        "success_criteria",
        "pros_cons",
        "solution",
        "problem",
        "context",
    ] {
        unsorted.insert(key.to_string(), dirty_sections[key].clone());
    }
    let dirty_canonical = serde_json::to_string(&Value::Object(unsorted)).unwrap();
    let mut p = payload("terra", "columbus-5", "propose", &addr);
    p.title = Some("Dirty".into());
    p.body_hash = Some(operator_voting::payload::body_hash(&dirty_canonical));
    let raw = serde_json::to_string(&p).unwrap();
    let (status, body) = call(
        app.clone(),
        Request::post("/v1/proposals")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "chain": "terra",
                    "address": addr,
                    "payload": raw,
                    "signature": sign_terra(&sk, &raw),
                    "pubkey": pubkey,
                    "title": "Dirty",
                    "body_sections": dirty_sections,
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    let mut over = valid_sections();
    over["summary"] = serde_json::json!(format!("<p>{}</p>", "s".repeat(501)));
    let (status, body) = post_propose(
        app.clone(),
        "terra",
        "columbus-5",
        &addr,
        Some(&pubkey),
        |m| sign_terra(&sk, m),
        "Long",
        &over,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let mut huge = valid_sections();
    huge["problem"] = serde_json::json!(format!("<p>{}</p>", "a".repeat(65 * 1024)));
    let (status, body) = post_propose(
        app,
        "terra",
        "columbus-5",
        &addr,
        Some(&pubkey),
        |m| sign_terra(&sk, m),
        "Huge",
        &huge,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn bsc_templated_propose() {
    let Some((pool, _lock)) = setup_pool().await else {
        return;
    };
    let url = test_db_url().unwrap();
    let (evm, sk) = evm_wallet();
    insert_reg(&pool, "bsc", &evm, 5, 1500).await;
    set_tip(&pool, 0, 10).await;
    let app = app(pool, &url);
    let sections = valid_sections();
    let (status, created) = post_propose(
        app.clone(),
        "bsc",
        "56",
        &evm,
        None,
        |m| sign_evm(&sk, m),
        "BSC template",
        &sections,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap();
    let (status, detail) = call(
        app,
        Request::get(format!("/v1/proposals/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["chain"], "bsc");
    assert!(detail["body_sections"].is_object());
}
