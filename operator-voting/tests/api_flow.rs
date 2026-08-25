use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use http_body_util::BodyExt;
use k256::ecdsa::signature::Signer;
use k256::ecdsa::SigningKey;
use operator_voting::api::{router, AppState};
use operator_voting::blacklist::parse_blacklist;
use operator_voting::config::{VotingConfig, APP_NAME};
use operator_voting::crypto::{cosmos_address_from_pubkey, eip191_hash, verify_terra};
use sha3::Digest;
use operator_voting::db;
use operator_voting::payload::SignedPayload;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

fn test_db_url() -> Option<String> {
    std::env::var("LEDGER_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()
        .filter(|s| !s.is_empty())
}

async fn setup_pool() -> Option<PgPool> {
    let url = test_db_url()?;
    let pool = PgPool::connect(&url).await.ok()?;
    voting_ledger::db::migrate(&pool).await.ok()?;
    db::migrate(&pool).await.ok()?;
    sqlx::query(
        "TRUNCATE voting.votes, voting.proposal_snapshots, voting.proposals, voting.registration_intents, voting.signatures, voting_registrations, cl8y_balances, cl8y_bsc_balances, cl8y_cw20_transfers, cl8y_bep20_transfers CASCADE",
    )
    .execute(&pool)
    .await
    .ok()?;
    Some(pool)
}

fn cfg(pool_url: &str, blacklist: &str) -> VotingConfig {
    VotingConfig {
        database_url: pool_url.into(),
        blacklist: parse_blacklist(blacklist).unwrap(),
        min_proposal_raw: num_bigint::BigInt::from(1000u64) * num_bigint::BigInt::from(10u64).pow(18),
        cors_origins: vec!["http://127.0.0.1:5173".into()],
        api_bind: "127.0.0.1:0".into(),
        terra_chain_id: "columbus-5".into(),
        evm_chain_id: "56".into(),
        run_mode: "dev".into(),
    }
}

fn terra_wallet() -> (String, String, SigningKey) {
    let sk = SigningKey::from_bytes((&[0x21u8; 32]).into()).unwrap();
    let vk = k256::ecdsa::VerifyingKey::from(&sk);
    let compressed = vk.to_encoded_point(true);
    let address = cosmos_address_from_pubkey(compressed.as_bytes(), "terra").unwrap();
    let pubkey = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, compressed.as_bytes());
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

async fn call(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

#[tokio::test]
async fn terra_register_propose_vote_and_replay() {
    let Some(pool) = setup_pool().await else {
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

    let mut p = payload("terra", "columbus-5", "propose", &addr);
    p.title = Some("Test poll".into());
    let body = "<p>hello</p>";
    p.body_hash = Some(operator_voting::payload::body_hash(body));
    let raw = serde_json::to_string(&p).unwrap();
    let sig = sign_terra(&sk, &raw);
    let (status, created) = call(
        app.clone(),
        Request::post("/v1/proposals")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({
                "chain": "terra",
                "address": addr,
                "payload": raw,
                "signature": sig,
                "pubkey": pubkey,
                "title": "Test poll",
                "body_html": body,
            }).to_string()))
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
            .body(Body::from(serde_json::json!({
                "chain": "terra",
                "address": addr,
                "payload": rraw,
                "signature": sign_terra(&sk, &rraw),
                "pubkey": pubkey,
                "choice": "for",
            }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn blacklist_and_threshold_and_evm() {
    let Some(pool) = setup_pool().await else {
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
    p.body_hash = Some(operator_voting::payload::body_hash("<p>x</p>"));
    let raw = serde_json::to_string(&p).unwrap();
    let (status, body) = call(
        app,
        Request::post("/v1/proposals")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({
                "chain": "bsc",
                "address": evm,
                "payload": raw,
                "signature": sign_evm(&sk, &raw),
                "title": "x",
                "body_html": "<p>x</p>",
            }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}

#[tokio::test]
async fn xss_stripped_on_create() {
    let Some(pool) = setup_pool().await else {
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
    let dirty = r#"<p>ok</p><script>alert(1)</script>"#;
    let mut p = payload("terra", "columbus-5", "propose", &addr);
    p.title = Some("xss".into());
    p.body_hash = Some(operator_voting::payload::body_hash(dirty));
    let raw = serde_json::to_string(&p).unwrap();
    let app = router(AppState {
        pool,
        cfg: cfg(&url, ""),
    });
    let (status, created) = call(
        app.clone(),
        Request::post("/v1/proposals")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({
                "chain": "terra",
                "address": addr,
                "payload": raw,
                "signature": sign_terra(&sk, &raw),
                "pubkey": pubkey,
                "title": "xss",
                "body_html": dirty,
            }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap();
    let (status, detail) = call(
        app,
        Request::get(format!("/v1/proposals/{id}")).body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let html = detail["body_html"].as_str().unwrap();
    assert!(!html.contains("script"));
    assert!(html.contains("ok"));
}
