use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use http_body_util::BodyExt;
use k256::ecdsa::SigningKey;
use operator_voting::api::AppState;
use operator_voting::blacklist::parse_committee;
use operator_voting::config::{VotingConfig, APP_NAME};
use operator_voting::crypto::{cosmos_address_from_pubkey, eip191_hash, verify_terra};
use operator_voting::db;
use operator_voting::payload::{body_hash, SignedPayload};
use operator_voting::proposal_sections::{
    analysis_hash, prepare_sections, sanitize_analysis_sections, sanitize_proposal_sections,
    AnalysisSections, ProposalSections,
};
use serde_json::Value;
use sha3::Digest;
use sqlx::PgPool;
use tower::ServiceExt;

fn router(state: AppState) -> axum::Router {
    operator_voting::application::application(state).expect("production application factory")
}

fn test_db_url() -> Option<String> {
    voting_ledger::test_lock::test_database_url()
}

async fn setup_pool() -> Option<(PgPool, voting_ledger::test_lock::IntegrationDbLock)> {
    let url = test_db_url()?;
    let lock = voting_ledger::test_lock::hold_integration_db(&url)
        .await
        .unwrap_or_else(|e| panic!("advisory lock: {e}"));
    let pool = voting_ledger::test_lock::test_pool(&url)
        .await
        .unwrap_or_else(|e| panic!("test postgres: {e}"));
    voting_ledger::db::migrate(&pool)
        .await
        .unwrap_or_else(|e| panic!("ledger migrate: {e}"));
    db::migrate(&pool)
        .await
        .unwrap_or_else(|e| panic!("operator-voting migrate: {e}"));
    sqlx::query(
        "TRUNCATE voting.votes, voting.proposal_snapshots, voting.proposal_comments, voting.proposal_analysis, voting.proposals, voting.registration_intents, voting.signatures, voting_registrations, cl8y_balances, cl8y_bsc_balances, cl8y_cw20_transfers, cl8y_bep20_transfers, public.ledger_checkpoints, public.ledger_canonical_blocks CASCADE",
    )
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("truncate: {e}"));
    sqlx::query(
        "UPDATE indexer_state SET value = '0' WHERE key IN ('last_indexed_height', 'last_indexed_bsc_block')",
    )
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("reset tips: {e}"));
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
    for (chain, height) in [("terra", terra.max(1)), ("bsc", bsc.max(1))] {
        sqlx::query("INSERT INTO public.ledger_checkpoints(chain,height,block_hash,network_tip,observed_at,epoch,complete,canonical) VALUES($1,$2,$3,$2,now(),1,true,true) ON CONFLICT(chain) DO UPDATE SET height=$2,network_tip=$2,block_hash=$3,observed_at=now(),complete=true,canonical=true,degraded_reason=NULL")
        .bind(chain).bind(height).bind(format!("{chain}-{height}")).execute(pool).await.unwrap();
    }
    sqlx::query("UPDATE public.voting_registrations SET baseline_height=COALESCE(baseline_height,registered_at_height),baseline_verified=true,baseline_block_hash=chain||'-'||registered_at_height::text WHERE status='active'").execute(pool).await.unwrap();
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
    use k256::ecdsa::signature::hazmat::PrehashSigner;
    let doc = operator_voting::crypto::adr36_sign_doc_json(
        &cosmos_address_from_pubkey(
            sk.verifying_key().to_encoded_point(true).as_bytes(),
            "terra",
        )
        .unwrap(),
        message,
    );
    let hash = sha2::Sha256::digest(doc.as_bytes());
    let sig: k256::ecdsa::Signature = sk.sign_prehash(&hash).unwrap();
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
        version: 2,
        nonce: Some(uuid::Uuid::new_v4().to_string()),
        revision: if matches!(purpose, "amend" | "analyze" | "open_vote") {
            Some(1)
        } else {
            None
        },
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
        prev_body_hash: None,
    }
}

fn sample_sections() -> ProposalSections {
    fn p(label: &str) -> String {
        format!("<p>{label} needs forty visible characters in this fixture text.</p>")
    }
    ProposalSections {
        problem: p("problem"),
        context: String::new(),
        solution: p("solution"),
        pros_cons: p("tradeoffs"),
        summary: p("summary"),
        success_criteria: p("success"),
    }
}

fn sections_body_hash(sections: &ProposalSections) -> String {
    let sanitized = sanitize_proposal_sections(sections);
    let value = serde_json::to_value(&sanitized).expect("serialize proposal sections");
    match prepare_sections(&value) {
        Ok(prepared) => prepared.body_hash,
        // Invalid fixtures still need a signed `body_hash` field. The API
        // rejects on OV-S1 minima before comparing hashes (`api.rs` create).
        Err(_) => body_hash("{}"),
    }
}

fn valid_sections() -> Value {
    serde_json::to_value(sample_sections()).unwrap()
}

fn draft_payload(
    chain: &str,
    chain_id: &str,
    addr: &str,
    title: &str,
    sections: &Value,
) -> SignedPayload {
    let mut p = payload(chain, chain_id, "draft", addr);
    p.title = Some(title.into());
    p.body_hash = Some(
        prepare_sections(sections)
            .map(|prepared| prepared.body_hash)
            .unwrap_or_else(|_| body_hash("{}")),
    );
    p
}

fn sample_analysis() -> AnalysisSections {
    fn p(label: &str) -> String {
        format!("<p>{label} needs forty visible characters in this fixture text.</p>")
    }
    AnalysisSections {
        what: p("what"),
        benefits: p("benefits"),
        risks: p("risks"),
        short_term: p("short"),
        long_term: p("long"),
    }
}

fn cfg_committee(pool_url: &str, committee: &str) -> VotingConfig {
    let mut c = cfg(pool_url, "");
    c.committee = parse_committee(committee).expect("committee");
    c
}

async fn call(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let method = req.method().to_string();
    let path = req.uri().to_string();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    capture_response(&method, &path, status, &json);
    (status, json)
}

/// Optional local contract evidence. Response bodies contain only synthetic test
/// wallet/public receipt data; request signatures/credentials are not logged.
fn capture_response(method: &str, path: &str, status: StatusCode, body: &Value) {
    let Ok(pathname) = std::env::var("VOTING_CONTRACT_CAPTURE_PATH") else {
        return;
    };
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _lock = LOCK.lock().unwrap();
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(pathname)
        .expect("open optional synthetic contract capture");
    let mut bytes = serde_json::to_vec(
        &serde_json::json!({"method":method,"path":path,"status":status.as_u16(),"body":body}),
    )
    .unwrap();
    bytes.push(b'\n');
    use std::io::Write;
    file.write_all(&bytes)
        .expect("write optional synthetic contract capture");
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
    set_tip(&pool, 20, 1).await;

    let state = AppState {
        pool: pool.clone(),
        cfg: cfg_committee(&url, &addr),
    };
    let app = router(state);
    let sections = sample_sections();

    let mut p = payload("terra", "columbus-5", "draft", &addr);
    p.title = Some("Test poll".into());
    p.body_hash = Some(sections_body_hash(&sections));
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
                    "sections": sections,
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["status"], "draft");
    assert!(created["terra_height"].is_null());
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
    assert_eq!(status, StatusCode::FORBIDDEN, "{voted}");

    let mut open = payload("terra", "columbus-5", "open_vote", &addr);
    open.title = Some("Test poll".into());
    open.proposal_id = Some(id.clone());
    open.body_hash = Some(sections_body_hash(&sections));
    let oraw = serde_json::to_string(&open).unwrap();
    let (status, opened) = call(
        app.clone(),
        Request::post(format!("/v1/proposals/{id}/open"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "chain": "terra",
                    "address": addr,
                    "payload": oraw,
                    "signature": sign_terra(&sk, &oraw),
                    "pubkey": pubkey,
                    "terra_height": 999_999,
                    "bsc_block": 9_999_999,
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    assert_eq!(opened["terra_height"], 20);
    assert_eq!(opened["bsc_block"], 1);

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
    assert_eq!(voted["snapshot"]["terra_height"], 20);

    let (status, replay) = call(
        app.clone(),
        Request::post(format!("/v1/proposals/{id}/votes"))
            .header("content-type", "application/json")
            .body(Body::from(vote_body))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay, voted);

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
    let sections = sample_sections();
    let mut p = payload("bsc", "56", "draft", &evm);
    p.title = Some("x".into());
    p.body_hash = Some(sections_body_hash(&sections));
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
                    "sections": sections,
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
    let mut sections = sample_sections();
    sections.problem = r#"<p>ok needs forty visible characters in this fixture text.</p><script>alert(1)</script>"#.into();
    let mut p = payload("terra", "columbus-5", "draft", &addr);
    p.title = Some("xss".into());
    p.body_hash = Some(sections_body_hash(&sections));
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
                    "sections": sections,
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
    assert!(html.contains("ok"));
    assert_eq!(detail["status"], "draft");
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
    let resp = app.clone().oneshot(post()).await.expect("third POST");
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        resp.headers()
            .get(axum::http::header::RETRY_AFTER)
            .is_some(),
        "O-RL: 429 must include Retry-After"
    );
    let (health, body) = call(app, Request::get("/health").body(Body::empty()).unwrap()).await;
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
    let sections = sample_sections();
    let mut p = payload("terra", "columbus-5", "draft", &addr);
    p.title = Some("Lag".into());
    p.body_hash = Some(sections_body_hash(&sections));
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
                    "sections": sections,
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert!(created["terra_height"].is_null());
    assert_eq!(created["status"], "draft");
}

fn terra_wallet_seed(seed: u8) -> (String, String, SigningKey) {
    let sk = SigningKey::from_bytes((&[seed; 32]).into()).unwrap();
    let vk = k256::ecdsa::VerifyingKey::from(&sk);
    let compressed = vk.to_encoded_point(true);
    let address = cosmos_address_from_pubkey(compressed.as_bytes(), "terra").unwrap();
    let pubkey = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        compressed.as_bytes(),
    );
    (address, pubkey, sk)
}

async fn post_json(app: axum::Router, path: &str, body: Value) -> (StatusCode, Value) {
    call(
        app,
        Request::post(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

async fn put_json(app: axum::Router, path: &str, body: Value) -> (StatusCode, Value) {
    call(
        app,
        Request::put(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn draft_comment_amend_open_and_register_before_freeze() {
    let Some((pool, _lock)) = setup_pool().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let url = test_db_url().unwrap();
    let (proposer, pubkey, sk) = terra_wallet();
    let (commenter, cpub, csk) = terra_wallet_seed(0x22);
    let (committee, kpub, ksk) = terra_wallet_seed(0x23);
    insert_reg(&pool, "terra", &proposer, 10, 2000).await;
    insert_reg(&pool, "terra", &committee, 10, 50).await;
    set_tip(&pool, 20, 0).await;

    let app = router(AppState {
        pool: pool.clone(),
        cfg: cfg_committee(&url, &committee),
    });
    let sections = sample_sections();

    let mut missing = sample_sections();
    missing.pros_cons = "<p> </p>".into();
    let mut p = payload("terra", "columbus-5", "draft", &proposer);
    p.title = Some("Idea".into());
    p.body_hash = Some(sections_body_hash(&missing));
    let raw = serde_json::to_string(&p).unwrap();
    let (status, body) = post_json(
        app.clone(),
        "/v1/proposals",
        serde_json::json!({
            "chain": "terra",
            "address": proposer,
            "payload": raw,
            "signature": sign_terra(&sk, &raw),
            "pubkey": pubkey,
            "title": "Idea",
            "sections": missing,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let mut propose_blob = payload("terra", "columbus-5", "propose", &proposer);
    propose_blob.title = Some("Idea".into());
    propose_blob.body_hash = Some(sections_body_hash(&sections));
    let praw = serde_json::to_string(&propose_blob).unwrap();
    let (status, body) = post_json(
        app.clone(),
        "/v1/proposals",
        serde_json::json!({
            "chain": "terra",
            "address": proposer,
            "payload": praw,
            "signature": sign_terra(&sk, &praw),
            "pubkey": pubkey,
            "title": "Idea",
            "sections": sections,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    let mut draft = payload("terra", "columbus-5", "draft", &proposer);
    draft.title = Some("Idea".into());
    draft.body_hash = Some(sections_body_hash(&sections));
    let draw = serde_json::to_string(&draft).unwrap();
    let (status, created) = post_json(
        app.clone(),
        "/v1/proposals",
        serde_json::json!({
            "chain": "terra",
            "address": proposer,
            "payload": draw,
            "signature": sign_terra(&sk, &draw),
            "pubkey": pubkey,
            "title": "Idea",
            "sections": sections,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap().to_string();

    let comment_html = "<p>needs a second opinion</p>";
    let mut cmt = payload("terra", "columbus-5", "comment", &commenter);
    cmt.proposal_id = Some(id.clone());
    cmt.body_hash = Some(operator_voting::payload::body_hash(comment_html));
    let craw = serde_json::to_string(&cmt).unwrap();
    let (status, denied) = post_json(
        app.clone(),
        &format!("/v1/proposals/{id}/comments"),
        serde_json::json!({
            "chain": "terra",
            "address": commenter,
            "payload": craw,
            "signature": sign_terra(&csk, &craw),
            "pubkey": cpub,
            "body_html": comment_html,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{denied}");

    insert_reg(&pool, "terra", &commenter, 25, 1500).await;
    let (status, posted) = post_json(
        app.clone(),
        &format!("/v1/proposals/{id}/comments"),
        serde_json::json!({
            "chain": "terra",
            "address": commenter,
            "payload": craw,
            "signature": sign_terra(&csk, &craw),
            "pubkey": cpub,
            "body_html": comment_html,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{posted}");

    let mut empty_p = payload("terra", "columbus-5", "comment", &commenter);
    empty_p.proposal_id = Some(id.clone());
    empty_p.body_hash = Some(operator_voting::payload::body_hash("<p></p>"));
    let eraw = serde_json::to_string(&empty_p).unwrap();
    let (status, empty_body) = post_json(
        app.clone(),
        &format!("/v1/proposals/{id}/comments"),
        serde_json::json!({
            "chain": "terra",
            "address": commenter,
            "payload": eraw,
            "signature": sign_terra(&csk, &eraw),
            "pubkey": cpub,
            "body_html": "<p></p>",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{empty_body}");

    let mut amended = sample_sections();
    amended.solution =
        "<p>updated solution needs forty visible characters in this fixture.</p>".into();
    let mut amend = payload("terra", "columbus-5", "amend", &commenter);
    amend.proposal_id = Some(id.clone());
    amend.title = Some("Idea".into());
    amend.body_hash = Some(sections_body_hash(&amended));
    amend.prev_body_hash = Some(sections_body_hash(&sections));
    let araw = serde_json::to_string(&amend).unwrap();
    let (status, other_amend) = put_json(
        app.clone(),
        &format!("/v1/proposals/{id}/sections"),
        serde_json::json!({
            "chain": "terra",
            "address": commenter,
            "payload": araw,
            "signature": sign_terra(&csk, &araw),
            "pubkey": cpub,
            "title": "Idea",
            "sections": amended,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{other_amend}");

    let mut pamend = payload("terra", "columbus-5", "amend", &proposer);
    pamend.proposal_id = Some(id.clone());
    pamend.title = Some("Idea".into());
    pamend.body_hash = Some(sections_body_hash(&amended));
    pamend.prev_body_hash = Some("deadbeef".into());
    let stale = serde_json::to_string(&pamend).unwrap();
    let (status, stale_body) = put_json(
        app.clone(),
        &format!("/v1/proposals/{id}/sections"),
        serde_json::json!({
            "chain": "terra",
            "address": proposer,
            "payload": stale,
            "signature": sign_terra(&sk, &stale),
            "pubkey": pubkey,
            "title": "Idea",
            "sections": amended,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale_body}");

    pamend.prev_body_hash = Some(sections_body_hash(&sections));
    let ok_amend = serde_json::to_string(&pamend).unwrap();
    let (status, amended_ok) = put_json(
        app.clone(),
        &format!("/v1/proposals/{id}/sections"),
        serde_json::json!({
            "chain": "terra",
            "address": proposer,
            "payload": ok_amend,
            "signature": sign_terra(&sk, &ok_amend),
            "pubkey": pubkey,
            "title": "Idea",
            "sections": amended,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{amended_ok}");

    let analysis = sample_analysis();
    let mut aprop = payload("terra", "columbus-5", "analyze", &proposer);
    aprop.proposal_id = Some(id.clone());
    aprop.body_hash = Some(analysis_hash(&sanitize_analysis_sections(&analysis)));
    let anraw = serde_json::to_string(&aprop).unwrap();
    let (status, proposer_analysis) = post_json(
        app.clone(),
        &format!("/v1/proposals/{id}/analysis"),
        serde_json::json!({
            "chain": "terra",
            "address": proposer,
            "payload": anraw,
            "signature": sign_terra(&sk, &anraw),
            "pubkey": pubkey,
            "sections": analysis,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{proposer_analysis}");

    let mut acom = payload("terra", "columbus-5", "analyze", &committee);
    acom.revision = Some(2);
    acom.proposal_id = Some(id.clone());
    acom.body_hash = Some(analysis_hash(&sanitize_analysis_sections(&analysis)));
    let acraw = serde_json::to_string(&acom).unwrap();
    let (status, analysis_ok) = post_json(
        app.clone(),
        &format!("/v1/proposals/{id}/analysis"),
        serde_json::json!({
            "chain": "terra",
            "address": committee,
            "payload": acraw,
            "signature": sign_terra(&ksk, &acraw),
            "pubkey": kpub,
            "sections": analysis,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{analysis_ok}");

    let mut fake_open = payload("terra", "columbus-5", "open_vote", &proposer);
    fake_open.proposal_id = Some(id.clone());
    fake_open.body_hash = Some(sections_body_hash(&amended));
    let fraw = serde_json::to_string(&fake_open).unwrap();
    let (status, fake) = post_json(
        app.clone(),
        &format!("/v1/proposals/{id}/open"),
        serde_json::json!({
            "chain": "terra",
            "address": proposer,
            "payload": fraw,
            "signature": sign_terra(&sk, &fraw),
            "pubkey": pubkey,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{fake}");

    set_tip(&pool, 40, 0).await;
    let mut open = payload("terra", "columbus-5", "open_vote", &committee);
    open.title = Some("Idea".into());
    open.revision = Some(2);
    open.proposal_id = Some(id.clone());
    open.body_hash = Some(sections_body_hash(&amended));
    let oraw = serde_json::to_string(&open).unwrap();
    let (status, opened) = post_json(
        app.clone(),
        &format!("/v1/proposals/{id}/open"),
        serde_json::json!({
            "chain": "terra",
            "address": committee,
            "payload": oraw,
            "signature": sign_terra(&ksk, &oraw),
            "pubkey": kpub,
            "terra_height": 1,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    assert_eq!(opened["terra_height"], 40);

    let (status, again) = post_json(
        app.clone(),
        &format!("/v1/proposals/{id}/open"),
        serde_json::json!({
            "chain": "terra",
            "address": committee,
            "payload": oraw,
            "signature": sign_terra(&ksk, &oraw),
            "pubkey": kpub,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");

    let mut late_amend = payload("terra", "columbus-5", "amend", &proposer);
    late_amend.proposal_id = Some(id.clone());
    late_amend.title = Some("Idea".into());
    late_amend.body_hash = Some(sections_body_hash(&amended));
    late_amend.prev_body_hash = Some(sections_body_hash(&amended));
    let lraw = serde_json::to_string(&late_amend).unwrap();
    let (status, late) = put_json(
        app.clone(),
        &format!("/v1/proposals/{id}/sections"),
        serde_json::json!({
            "chain": "terra",
            "address": proposer,
            "payload": lraw,
            "signature": sign_terra(&sk, &lraw),
            "pubkey": pubkey,
            "title": "Idea",
            "sections": amended,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{late}");

    let mut vote = payload("terra", "columbus-5", "vote", &commenter);
    vote.proposal_id = Some(id.clone());
    vote.choice = Some("for".into());
    let vraw = serde_json::to_string(&vote).unwrap();
    let (status, voted) = post_json(
        app.clone(),
        &format!("/v1/proposals/{id}/votes"),
        serde_json::json!({
            "chain": "terra",
            "address": commenter,
            "payload": vraw,
            "signature": sign_terra(&csk, &vraw),
            "pubkey": cpub,
            "choice": "for",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{voted}");
    assert_eq!(voted["weight"], cl8y_raw(1500));

    let (evm, esk) = evm_wallet();
    insert_reg(&pool, "bsc", &evm, 5, 2000).await;
    set_tip(&pool, 40, 80).await;
    let mut bsc_draft = payload("bsc", "56", "draft", &evm);
    bsc_draft.title = Some("BSC idea".into());
    bsc_draft.body_hash = Some(sections_body_hash(&sections));
    let braw = serde_json::to_string(&bsc_draft).unwrap();
    let (status, bsc_created) = post_json(
        app,
        "/v1/proposals",
        serde_json::json!({
            "chain": "bsc",
            "address": evm,
            "payload": braw,
            "signature": sign_evm(&esk, &braw),
            "title": "BSC idea",
            "sections": sections,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{bsc_created}");
    assert_eq!(bsc_created["status"], "draft");
}

#[tokio::test]
async fn below_1000_cannot_draft() {
    let Some((pool, _lock)) = setup_pool().await else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let url = test_db_url().unwrap();
    let (addr, pubkey, sk) = terra_wallet();
    insert_reg(&pool, "terra", &addr, 10, 999).await;
    set_tip(&pool, 20, 0).await;
    let app = app(pool, &url);
    let sections = sample_sections();
    let mut p = payload("terra", "columbus-5", "draft", &addr);
    p.title = Some("Nope".into());
    p.body_hash = Some(sections_body_hash(&sections));
    let raw = serde_json::to_string(&p).unwrap();
    let (status, body) = post_json(
        app,
        "/v1/proposals",
        serde_json::json!({
            "chain": "terra",
            "address": addr,
            "payload": raw,
            "signature": sign_terra(&sk, &raw),
            "pubkey": pubkey,
            "title": "Nope",
            "sections": sections,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
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
    let p = draft_payload(chain, chain_id, addr, title, sections);
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
    assert!(body["error"].is_string());

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

    let mut p = payload("terra", "columbus-5", "draft", &addr);
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
    let mut p = payload("terra", "columbus-5", "draft", &addr);
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

fn envelope(chain: &str, address: &str, key: &SigningKey, payload: &SignedPayload) -> Value {
    let raw = serde_json::to_string(payload).unwrap();
    let (signature, pubkey) = if chain == "terra" {
        (
            sign_terra(key, &raw),
            Some(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                key.verifying_key().to_encoded_point(true).as_bytes(),
            )),
        )
    } else {
        (sign_evm(key, &raw), None)
    };
    serde_json::json!({"chain":chain,"address":address,"payload":raw,"signature":signature,"pubkey":pubkey})
}

async fn verified_export(app: axum::Router, response: &Value) {
    let receipt = response["receipt_id"].as_str().unwrap();
    let (status, export) = call(
        app,
        Request::get(format!("/v1/receipts/{receipt}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{export}");
    operator_voting::payload::verify_exported_receipt(&export).unwrap();
    let mut tampered = export.clone();
    tampered["signature"]["signed_payload"] = Value::String("{}".into());
    assert!(operator_voting::payload::verify_exported_receipt(&tampered).is_err());
    if !export["revision"].is_null()
        && matches!(
            export["signature"]["purpose"].as_str(),
            Some("draft" | "amend" | "open_vote")
        )
    {
        let mut tampered = export.clone();
        tampered["revision"]["title"] = Value::String("unapproved title".into());
        assert!(operator_voting::payload::verify_exported_receipt(&tampered).is_err());
    }
    let mut tampered = export.clone();
    tampered["signature"]["revision"] = serde_json::json!(999);
    assert!(operator_voting::payload::verify_exported_receipt(&tampered).is_err());
    let mut tampered = export.clone();
    tampered["receipt"]["accepted_at"] = serde_json::json!("1970-01-01T00:00:00Z");
    assert!(operator_voting::payload::verify_exported_receipt(&tampered).is_err());
    match export["signature"]["purpose"].as_str().unwrap() {
        "draft" | "amend" | "open_vote" => {
            let mut tampered = export.clone();
            tampered["revision"]["body_sections"]["problem"] = serde_json::json!(
                "<p>Different content that still satisfies the required visible minimum.</p>"
            );
            assert!(operator_voting::payload::verify_exported_receipt(&tampered).is_err());
        }
        "comment" | "analyze" => {
            let mut tampered = export.clone();
            tampered["receipt"]["content"]["id"] = serde_json::json!(uuid::Uuid::new_v4());
            assert!(operator_voting::payload::verify_exported_receipt(&tampered).is_err());
            let mut tampered = export.clone();
            tampered["receipt"]["content"]["body_html"] =
                serde_json::json!("<p>Altered displayed content</p>");
            assert!(operator_voting::payload::verify_exported_receipt(&tampered).is_err());
        }
        _ => (),
    }
}

#[tokio::test]
async fn transactional_dual_chain_revisions_receipts_and_concurrency() {
    let Some((pool, _lock)) = setup_pool().await else {
        return;
    };
    for chain in ["terra", "bsc"] {
        let chain_id = if chain == "terra" { "columbus-5" } else { "56" };
        let (proposer, key) = if chain == "terra" {
            let (a, _, k) = terra_wallet();
            (a, k)
        } else {
            evm_wallet()
        };
        let reviewer_key = SigningKey::from_slice(&[0x55u8; 32]).unwrap();
        let reviewer = if chain == "terra" {
            cosmos_address_from_pubkey(
                reviewer_key
                    .verifying_key()
                    .to_encoded_point(true)
                    .as_bytes(),
                "terra",
            )
            .unwrap()
        } else {
            let digest = sha3::Keccak256::digest(
                &reviewer_key
                    .verifying_key()
                    .to_encoded_point(false)
                    .as_bytes()[1..],
            );
            format!("0x{}", hex::encode(&digest[12..]))
        };
        let mut configuration = cfg(&test_db_url().unwrap(), "");
        configuration.committee = parse_committee(&format!("{proposer},{reviewer}")).unwrap();
        let app = router(AppState {
            pool: pool.clone(),
            cfg: configuration,
        });
        let (status, document) = call(
            app.clone(),
            Request::get("/openapi.json").body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(document["paths"]["/v1/proposals/{id}/sections"]["put"].is_object());
        let (status, health) = call(
            app.clone(),
            Request::get("/health").body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(health["ok"], true);
        let p = payload(chain, chain_id, "register", &proposer);
        let (status, registered) = post_json(
            app.clone(),
            "/v1/register",
            envelope(chain, &proposer, &key, &p),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{registered}");
        verified_export(app.clone(), &registered).await;
        insert_reg(&pool, chain, &proposer, 10, 5000).await;
        insert_reg(&pool, chain, &reviewer, 10, 2000).await;
        set_tip(&pool, 20, 20).await;
        let sections = valid_sections();
        let hash = prepare_sections(&sections).unwrap().body_hash;
        let p = draft_payload(chain, chain_id, &proposer, "Original title", &sections);
        let mut request = envelope(chain, &proposer, &key, &p);
        request["title"] = serde_json::json!("Original title");
        request["body_sections"] = sections.clone();
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let a = {
            let app = app.clone();
            let request = request.clone();
            let b = barrier.clone();
            tokio::spawn(async move {
                b.wait().await;
                post_json(app, "/v1/proposals", request).await
            })
        };
        let b = {
            let app = app.clone();
            let b = barrier.clone();
            tokio::spawn(async move {
                b.wait().await;
                post_json(app, "/v1/proposals", request).await
            })
        };
        let a = a.await.unwrap();
        let b = b.await.unwrap();
        assert_eq!(a.0, StatusCode::CREATED, "{}", a.1);
        assert_eq!(a, b);
        verified_export(app.clone(), &a.1).await;
        let id = a.1["id"].as_str().unwrap().to_owned();
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM voting.proposals WHERE id=$1")
            .bind(uuid::Uuid::parse_str(&id).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 1);
        let mut changes = Vec::new();
        for title in ["Title A", "Title B"] {
            let mut p = payload(chain, chain_id, "amend", &proposer);
            p.proposal_id = Some(id.clone());
            p.title = Some(title.into());
            p.body_hash = Some(hash.clone());
            p.prev_body_hash = Some(hash.clone());
            p.revision = Some(1);
            let mut req = envelope(chain, &proposer, &key, &p);
            req["title"] = serde_json::json!(title);
            req["body_sections"] = sections.clone();
            changes.push(req);
        }
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let mut tasks = Vec::new();
        for request in changes {
            let app = app.clone();
            let path = format!("/v1/proposals/{id}/sections");
            let b = barrier.clone();
            tasks.push(tokio::spawn(async move {
                b.wait().await;
                put_json(app, &path, request).await
            }));
        }
        let mut statuses = Vec::new();
        for task in tasks {
            let (status, response) = task.await.unwrap();
            statuses.push(status);
            if status == StatusCode::OK {
                verified_export(app.clone(), &response).await;
            }
        }
        assert_eq!(statuses.iter().filter(|s| **s == StatusCode::OK).count(), 1);
        assert_eq!(
            statuses
                .iter()
                .filter(|s| **s == StatusCode::CONFLICT)
                .count(),
            1
        );
        // Title-only amendment invalidates a review of revision 1 even with unchanged section hash.
        let mut old = payload(chain, chain_id, "open_vote", &reviewer);
        old.proposal_id = Some(id.clone());
        old.title = Some("Original title".into());
        old.body_hash = Some(hash.clone());
        old.revision = Some(1);
        let (status, _) = post_json(
            app.clone(),
            &format!("/v1/proposals/{id}/open"),
            envelope(chain, &reviewer, &reviewer_key, &old),
        )
        .await;
        assert!(matches!(
            status,
            StatusCode::CONFLICT | StatusCode::UNAUTHORIZED
        ));
        // Parallel analysis quota is checked while holding the proposal lock.
        let mut tasks = Vec::new();
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(7));
        for _ in 0..7 {
            let mut p = payload(chain, chain_id, "analyze", &reviewer);
            p.proposal_id = Some(id.clone());
            p.revision = Some(2);
            p.body_hash = Some(prepare_analysis_for_test());
            let mut req = envelope(chain, &reviewer, &reviewer_key, &p);
            req["sections"] = serde_json::to_value(sample_analysis()).unwrap();
            let b = barrier.clone();
            let app = app.clone();
            let path = format!("/v1/proposals/{id}/analysis");
            tasks.push(tokio::spawn(async move {
                b.wait().await;
                post_json(app, &path, req).await
            }));
        }
        let mut accepted = 0;
        for task in tasks {
            let (status, response) = task.await.unwrap();
            if status == StatusCode::CREATED {
                accepted += 1;
                verified_export(app.clone(), &response).await;
            } else {
                assert_eq!(status, StatusCode::FORBIDDEN, "{response}");
            }
        }
        assert_eq!(accepted, 5);
        let (_, detail) = call(
            app.clone(),
            Request::get(format!("/v1/proposals/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        let mut opening = payload(chain, chain_id, "open_vote", &reviewer);
        opening.proposal_id = Some(id.clone());
        opening.title = detail["title"].as_str().map(str::to_owned);
        opening.body_hash = Some(hash.clone());
        opening.revision = Some(2);
        let mut amend = payload(chain, chain_id, "amend", &proposer);
        amend.proposal_id = Some(id.clone());
        amend.title = Some("Racing title".into());
        amend.body_hash = Some(hash.clone());
        amend.prev_body_hash = Some(hash.clone());
        amend.revision = Some(2);
        let mut amend = envelope(chain, &proposer, &key, &amend);
        amend["title"] = serde_json::json!("Racing title");
        amend["body_sections"] = sections.clone();
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let open_task = {
            let app = app.clone();
            let req = envelope(chain, &reviewer, &reviewer_key, &opening);
            let b = barrier.clone();
            let path = format!("/v1/proposals/{id}/open");
            tokio::spawn(async move {
                b.wait().await;
                post_json(app, &path, req).await
            })
        };
        let amend_task = {
            let app = app.clone();
            let b = barrier.clone();
            let path = format!("/v1/proposals/{id}/sections");
            tokio::spawn(async move {
                b.wait().await;
                put_json(app, &path, amend).await
            })
        };
        let opened_race = open_task.await.unwrap();
        let amended_race = amend_task.await.unwrap();
        assert_eq!(
            [opened_race.0, amended_race.0]
                .iter()
                .filter(|s| **s == StatusCode::OK)
                .count(),
            1
        );
        if amended_race.0 == StatusCode::OK {
            verified_export(app.clone(), &amended_race.1).await;
            opening.revision = Some(3);
            opening.title = Some("Racing title".into());
            opening.nonce = Some(uuid::Uuid::new_v4().to_string());
        }
        let opening = envelope(chain, &reviewer, &reviewer_key, &opening);
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let mut tasks = Vec::new();
        for _ in 0..2 {
            let app = app.clone();
            let req = opening.clone();
            let b = barrier.clone();
            let path = format!("/v1/proposals/{id}/open");
            tasks.push(tokio::spawn(async move {
                b.wait().await;
                post_json(app, &path, req).await
            }));
        }
        let first = tasks.remove(0).await.unwrap();
        let second = tasks.remove(0).await.unwrap();
        assert_eq!(first.0, StatusCode::OK, "{}", first.1);
        assert_eq!(first, second);
        verified_export(app.clone(), &first.1).await;
        let mut p = payload(chain, chain_id, "analyze", &reviewer);
        p.proposal_id = Some(id.clone());
        p.revision = Some(2);
        p.body_hash = Some(prepare_analysis_for_test());
        let mut req = envelope(chain, &reviewer, &reviewer_key, &p);
        req["sections"] = serde_json::to_value(sample_analysis()).unwrap();
        let (status, _) =
            post_json(app.clone(), &format!("/v1/proposals/{id}/analysis"), req).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let mut voting = payload(chain, chain_id, "vote", &proposer);
        voting.proposal_id = Some(id.clone());
        voting.choice = Some("for".into());
        let mut req = envelope(chain, &proposer, &key, &voting);
        req["choice"] = serde_json::json!("for");
        let (status, voted) =
            post_json(app.clone(), &format!("/v1/proposals/{id}/votes"), req).await;
        assert_eq!(status, StatusCode::OK, "{voted}");
        verified_export(app.clone(), &voted).await;
        let mut comment = payload(chain, chain_id, "comment", &reviewer);
        comment.proposal_id = Some(id.clone());
        comment.body_hash = Some(body_hash("<p>Receipt reviewed</p>"));
        let mut req = envelope(chain, &reviewer, &reviewer_key, &comment);
        req["body_html"] = serde_json::json!("<p>Receipt reviewed</p>");
        let (status, commented) = post_json(
            app.clone(),
            &format!("/v1/proposals/{id}/comments"),
            req.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{commented}");
        verified_export(app.clone(), &commented).await;
        let (status, retry) =
            post_json(app.clone(), &format!("/v1/proposals/{id}/comments"), req).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(retry, commented);
        // Same nonce and altered signed bytes cannot be consumed again.
        comment.body_hash = Some(body_hash("<p>Altered</p>"));
        let mut req = envelope(chain, &reviewer, &reviewer_key, &comment);
        req["body_html"] = serde_json::json!("<p>Altered</p>");
        let (status, _) =
            post_json(app.clone(), &format!("/v1/proposals/{id}/comments"), req).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let mut tasks = Vec::new();
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(21));
        for _ in 0..21 {
            let mut p = payload(chain, chain_id, "comment", &reviewer);
            p.proposal_id = Some(id.clone());
            p.body_hash = Some(body_hash("<p>Quota</p>"));
            let mut req = envelope(chain, &reviewer, &reviewer_key, &p);
            req["body_html"] = serde_json::json!("<p>Quota</p>");
            let app = app.clone();
            let path = format!("/v1/proposals/{id}/comments");
            let b = barrier.clone();
            tasks.push(tokio::spawn(async move {
                b.wait().await;
                post_json(app, &path, req).await
            }));
        }
        let mut accepted = 0;
        for task in tasks {
            let (status, response) = task.await.unwrap();
            if status == StatusCode::CREATED {
                accepted += 1;
            } else {
                assert_eq!(status, StatusCode::FORBIDDEN, "{response}");
            }
        }
        assert_eq!(accepted, 19);
        for (suffix, expected_count) in [("comments", 20usize), ("analysis", 5usize)] {
            let (status, records) = call(
                app.clone(),
                Request::get(format!("/v1/proposals/{id}/{suffix}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{records}");
            assert_eq!(records.as_array().unwrap().len(), expected_count);
            assert!(records
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["chain"] == chain));
        }
        let (status, recorded_vote) = call(
            app.clone(),
            Request::get(format!("/v1/proposals/{id}/votes/{proposer}?chain={chain}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{recorded_vote}");
        assert_eq!(recorded_vote["choice"], "for");
        assert_eq!(recorded_vote["weight"], voted["weight"]);
        let (status, frozen) = call(
            app.clone(),
            Request::get(format!(
                "/v1/proposals/{id}/weights/{proposer}?chain={chain}"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(frozen["eligible"], true);
        assert_eq!(frozen["weight"], voted["weight"]);

        // Independent analysis racing opening either commits before freeze or is rejected.
        let p = draft_payload(chain, chain_id, &proposer, "Analyze race", &sections);
        let mut req = envelope(chain, &proposer, &key, &p);
        req["title"] = serde_json::json!("Analyze race");
        req["body_sections"] = sections.clone();
        let (status, new_draft) = post_json(app.clone(), "/v1/proposals", req).await;
        assert_eq!(status, StatusCode::CREATED, "{new_draft}");
        let race_id = new_draft["id"].as_str().unwrap().to_owned();
        let mut analysis = payload(chain, chain_id, "analyze", &reviewer);
        analysis.proposal_id = Some(race_id.clone());
        analysis.body_hash = Some(prepare_analysis_for_test());
        let mut req = envelope(chain, &reviewer, &reviewer_key, &analysis);
        req["sections"] = serde_json::to_value(sample_analysis()).unwrap();
        let mut opening = payload(chain, chain_id, "open_vote", &reviewer);
        opening.proposal_id = Some(race_id.clone());
        opening.title = Some("Analyze race".into());
        opening.body_hash = Some(hash.clone());
        let open_req = envelope(chain, &reviewer, &reviewer_key, &opening);
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let analyze_task = {
            let app = app.clone();
            let path = format!("/v1/proposals/{race_id}/analysis");
            let b = barrier.clone();
            tokio::spawn(async move {
                b.wait().await;
                post_json(app, &path, req).await
            })
        };
        let open_task = {
            let app = app.clone();
            let path = format!("/v1/proposals/{race_id}/open");
            let b = barrier.clone();
            tokio::spawn(async move {
                b.wait().await;
                post_json(app, &path, open_req).await
            })
        };
        let analyzed = analyze_task.await.unwrap();
        let opened = open_task.await.unwrap();
        assert_eq!(opened.0, StatusCode::OK, "{}", opened.1);
        assert!(
            matches!(analyzed.0, StatusCode::CREATED | StatusCode::CONFLICT),
            "{}",
            analyzed.1
        );
        if analyzed.0 == StatusCode::CREATED {
            verified_export(app.clone(), &analyzed.1).await;
        }
    }
    let orphans:i64=sqlx::query_scalar("SELECT count(*) FROM voting.signatures s LEFT JOIN voting.action_receipts r ON r.signature_id=s.id WHERE s.signed_payload IS NOT NULL AND r.signature_id IS NULL").fetch_one(&pool).await.unwrap();
    assert_eq!(
        orphans, 0,
        "rejected actions must leave no partial signature rows"
    );
}

#[tokio::test]
async fn accepted_retry_survives_router_restart_and_expiry() {
    let Some((pool, _lock)) = setup_pool().await else {
        return;
    };
    let url = test_db_url().unwrap();
    let (addr, _, key) = terra_wallet();
    let mut p = payload("terra", "columbus-5", "register", &addr);
    p.expires_at = p.issued_at + 2;
    let request = envelope("terra", &addr, &key, &p);
    let app = router(AppState {
        pool: pool.clone(),
        cfg: cfg(&url, ""),
    });
    let (status, first) = post_json(app, "/v1/register", request.clone()).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let restarted = router(AppState {
        pool: pool.clone(),
        cfg: cfg(&url, ""),
    });
    let (status, retried) = post_json(restarted, "/v1/register", request).await;
    assert_eq!(status, StatusCode::OK, "{retried}");
    assert_eq!(first, retried);
    let signatures: i64 = sqlx::query_scalar("SELECT count(*) FROM voting.signatures")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(signatures, 1);
}

fn prepare_analysis_for_test() -> String {
    operator_voting::proposal_sections::prepare_analysis(&sample_analysis())
        .unwrap()
        .2
}

#[tokio::test]
async fn verified_wallet_action_quota_is_shared_and_replays_are_free() {
    let Some((pool, _lock)) = setup_pool().await else {
        return;
    };
    let url = test_db_url().unwrap();
    let (actor, pubkey, key) = terra_wallet();
    let mut cfg = cfg(&url, "");
    cfg.rate_limit_post_per_minute = 2;
    cfg.rate_limit_post_burst = 20;
    let first = router(AppState {
        pool: pool.clone(),
        cfg: cfg.clone(),
    });
    let second = router(AppState {
        pool: pool.clone(),
        cfg,
    });
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(3));
    let mut tasks = Vec::new();
    for i in 0..3 {
        let p = payload("terra", "columbus-5", "register", &actor);
        let request = envelope("terra", &actor, &key, &p);
        let app = if i % 2 == 0 {
            first.clone()
        } else {
            second.clone()
        };
        let b = barrier.clone();
        tasks.push(tokio::spawn(async move {
            b.wait().await;
            let result = post_json(app, "/v1/register", request.clone()).await;
            (request, result)
        }));
    }
    let mut accepted = Vec::new();
    for task in tasks {
        let (request, (status, response)) = task.await.unwrap();
        if status == StatusCode::OK {
            accepted.push((request, response));
        } else {
            assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{response}");
            assert_eq!(response["code"], "rate_limited");
        }
    }
    assert_eq!(accepted.len(), 2);
    let (request, original) = &accepted[0];
    let (status, replay) = post_json(second.clone(), "/v1/register", request.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&replay, original);
    // Case-varied envelope still maps to the authenticated normalized wallet bucket.
    let p = payload("terra", "columbus-5", "register", &actor);
    let mut request = envelope("terra", &actor, &key, &p);
    request["address"] = serde_json::json!(actor.to_ascii_uppercase());
    let raw = serde_json::to_string(&request).unwrap();
    let response = second
        .clone()
        .oneshot(
            Request::post("/v1/register")
                .header("content-type", "application/json")
                .body(Body::from(raw))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key("retry-after"));
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    capture_response("POST", "/v1/register", StatusCode::TOO_MANY_REQUESTS, &body);
    // An unsigned claimed address cannot burn the victim's verified action quota.
    let attacker = serde_json::json!({"chain":"terra","address":actor,"payload":"{}","signature":"bogus","pubkey":pubkey});
    let (status, _) = post_json(first, "/v1/register", attacker).await;
    assert_ne!(status, StatusCode::TOO_MANY_REQUESTS);
    let signatures: i64 = sqlx::query_scalar("SELECT count(*) FROM voting.signatures")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(signatures, 2);
}

#[tokio::test]
async fn transaction_wait_past_signed_ttl_rolls_back_before_acceptance() {
    let Some((pool, _lock)) = setup_pool().await else {
        return;
    };
    let url = test_db_url().unwrap();
    let (actor, _, key) = terra_wallet();
    insert_reg(&pool, "terra", &actor, 10, 5000).await;
    set_tip(&pool, 20, 20).await;
    let app = router(AppState {
        pool: pool.clone(),
        cfg: cfg(&url, ""),
    });
    let mut blocker = pool.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock(845001)")
        .execute(&mut *blocker)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION voting.test_acceptance_barrier() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(845001); RETURN NEW; END $$; CREATE TRIGGER test_acceptance_barrier BEFORE INSERT ON voting.proposal_revisions FOR EACH ROW EXECUTE FUNCTION voting.test_acceptance_barrier();").execute(&pool).await.unwrap();
    let sections = valid_sections();
    let mut p = draft_payload("terra", "columbus-5", &actor, "TTL barrier", &sections);
    p.expires_at = p.issued_at + 2;
    let expires = p.expires_at;
    let mut request = envelope("terra", &actor, &key, &p);
    request["title"] = serde_json::json!("TTL barrier");
    request["body_sections"] = sections;
    let task = tokio::spawn(async move { post_json(app, "/v1/proposals", request).await });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    loop {
        let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event='advisory' AND query LIKE 'INSERT INTO voting.proposal_revisions%')").fetch_one(&pool).await.unwrap();
        if waiting {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "action did not reach post-mutation acceptance barrier"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    while Utc::now().timestamp() <= expires {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    sqlx::query("SELECT pg_advisory_unlock(845001)")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let (status, response) = task.await.unwrap();
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "expired action must not commit: {response}"
    );
    for table in [
        "voting.signatures",
        "voting.proposals",
        "voting.proposal_revisions",
        "voting.action_receipts",
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "expired mutation left {table} state");
    }
    sqlx::raw_sql("DROP TRIGGER test_acceptance_barrier ON voting.proposal_revisions; DROP FUNCTION voting.test_acceptance_barrier();").execute(&pool).await.unwrap();
}

#[tokio::test]
async fn readiness_and_freeze_refuse_only_eligible_unverified_baselines() {
    let Some((pool, _lock)) = setup_pool().await else {
        return;
    };
    let url = test_db_url().unwrap();
    let (actor, _, key) = terra_wallet();
    insert_reg(&pool, "terra", &actor, 10, 5000).await;
    set_tip(&pool, 20, 20).await;
    let mut cfg = cfg(&url, "");
    cfg.committee = parse_committee(&actor).unwrap();
    let app = router(AppState {
        pool: pool.clone(),
        cfg,
    });
    let sections = valid_sections();
    let p = draft_payload("terra", "columbus-5", &actor, "Baseline guard", &sections);
    let mut request = envelope("terra", &actor, &key, &p);
    request["title"] = serde_json::json!("Baseline guard");
    request["body_sections"] = sections.clone();
    let (status, draft) = post_json(app.clone(), "/v1/proposals", request).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = draft["id"].as_str().unwrap();
    let (status, _) = call(
        app.clone(),
        Request::get("/ready").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    sqlx::query("INSERT INTO public.voting_registrations(chain,wallet_address,registered_at_height,initial_balance,status,baseline_height,baseline_verified) VALUES('bsc','eligible_unverified',10,1,'active',10,false)").execute(&pool).await.unwrap();
    let (status, _) = call(
        app.clone(),
        Request::get("/ready").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let mut open = payload("terra", "columbus-5", "open_vote", &actor);
    open.proposal_id = Some(id.into());
    open.title = Some("Baseline guard".into());
    open.body_hash = Some(prepare_sections(&sections).unwrap().body_hash);
    let (status, _) = post_json(
        app.clone(),
        &format!("/v1/proposals/{id}/open"),
        envelope("terra", &actor, &key, &open),
    )
    .await;
    assert!(
        status.is_server_error(),
        "uncertifiedeligiblebaseline must blockfreeze"
    );
    sqlx::query("UPDATE public.voting_registrations SET baseline_verified=true,baseline_block_hash='bsc-10' WHERE wallet_address='eligible_unverified'").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO public.voting_registrations(chain,wallet_address,registered_at_height,initial_balance,status,baseline_height,baseline_verified) VALUES('bsc','0x2222222222222222222222222222222222222222',100,1,'active',100,false)").execute(&pool).await.unwrap();
    let (status, _) = call(
        app.clone(),
        Request::get("/ready").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "above-cutoffunverifiedbaseline cannot haltallopening"
    );
    let (status, opened) = post_json(
        app.clone(),
        &format!("/v1/proposals/{id}/open"),
        envelope("terra", &actor, &key, &open),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    let weight:String=sqlx::query_scalar("SELECT weight::text FROM voting.proposal_snapshots WHERE proposal_id=$1 AND wallet_address='0x2222222222222222222222222222222222222222'").bind(uuid::Uuid::parse_str(id).unwrap()).fetch_one(&pool).await.unwrap();
    assert_eq!(weight, "0");
    sqlx::query("UPDATE public.voting_registrations SET baseline_verified=true,baseline_block_hash='bsc-100' WHERE wallet_address='0x2222222222222222222222222222222222222222'").execute(&pool).await.unwrap();
    sqlx::query("UPDATE public.ledger_checkpoints SET height=100,network_tip=115,block_hash='bsc-100',observed_at=clock_timestamp() WHERE chain='bsc'").execute(&pool).await.unwrap();
    let (status, current) = call(
        app.clone(),
        Request::get("/v1/balances/0x2222222222222222222222222222222222222222?chain=bsc")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current["snapshot_eligible"], true);
    let (status, frozen) = call(
        app.clone(),
        Request::get(format!(
            "/v1/proposals/{id}/weights/0x2222222222222222222222222222222222222222?chain=bsc"
        ))
        .body(Body::empty())
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(frozen["eligible"], false);
    assert_eq!(
        frozen["weight"], "0",
        "futureadmission must not rewriteolderfrozenpower"
    );
    let (status, missing) = call(
        app.clone(),
        Request::get(format!(
            "/v1/proposals/{id}/weights/0x1111111111111111111111111111111111111111?chain=bsc"
        ))
        .body(Body::empty())
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(missing["eligible"], false);
    assert_eq!(missing["weight"], "0");
    let (status, _) = call(
        app,
        Request::get(format!(
            "/v1/proposals/{}/weights/0x1111111111111111111111111111111111111111?chain=bsc",
            uuid::Uuid::new_v4()
        ))
        .body(Body::empty())
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bsc_registration_waits_for_finalized_snapshot_admission() {
    let Some((pool, _lock)) = setup_pool().await else {
        return;
    };
    let url = test_db_url().unwrap();
    let (actor, _) = evm_wallet();
    insert_reg(&pool, "bsc", &actor, 100, 5000).await;
    sqlx::query("UPDATE public.voting_registrations SET baseline_height=100,baseline_block_hash='bsc-100',baseline_verified=false WHERE chain='bsc' AND wallet_address=$1").bind(&actor).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO public.ledger_checkpoints(chain,height,block_hash,observed_at,complete,canonical,network_tip) VALUES('bsc',85,'bsc-85',clock_timestamp(),true,true,100)").execute(&pool).await.unwrap();
    let app = router(AppState {
        pool: pool.clone(),
        cfg: cfg(&url, ""),
    });
    let path = format!("/v1/balances/{actor}?chain=bsc");
    let (status, balance) = call(
        app.clone(),
        Request::get(&path).body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(balance["registered"], true);
    assert_eq!(balance["balance"], cl8y_raw(5000));
    assert_eq!(balance["snapshot_eligible"], false);
    sqlx::query("UPDATE public.voting_registrations SET baseline_verified=true WHERE chain='bsc' AND wallet_address=$1").bind(&actor).execute(&pool).await.unwrap();
    let (_, balance) = call(
        app.clone(),
        Request::get(&path).body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(
        balance["snapshot_eligible"], false,
        "verifiedfuturebaseline remainsabovefinalizedcutoff"
    );
    sqlx::query("UPDATE public.ledger_checkpoints SET height=100,block_hash='bsc-100',network_tip=115,observed_at=clock_timestamp() WHERE chain='bsc'").execute(&pool).await.unwrap();
    let (status, balance) = call(app, Request::get(&path).body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(balance["snapshot_eligible"], true);
}
