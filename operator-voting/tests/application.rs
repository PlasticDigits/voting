//! Tests the deployed application stack, not a naked API router.
use axum::{
    body::Body,
    extract::connect_info::ConnectInfo,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use operator_voting::{api::AppState, application::application, VotingConfig};
use tower::ServiceExt;
fn app(burst: u32) -> axum::Router {
    let mut cfg = VotingConfig::for_tests("unused", "");
    cfg.rate_limit_post_burst = burst;
    app_with_config(cfg)
}
fn app_with_config(cfg: VotingConfig) -> axum::Router {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    application(AppState { pool, cfg }).unwrap()
}
fn req(method: &str, origin: &str) -> Request<Body> {
    let mut r = Request::builder()
        .method(method)
        .uri("/v1/proposals/00000000-0000-0000-0000-000000000000/sections")
        .header("origin", origin)
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    r.extensions_mut().insert(ConnectInfo(
        "203.0.113.8:1234".parse::<std::net::SocketAddr>().unwrap(),
    ));
    r
}
#[tokio::test]
async fn production_put_preflight_and_strict_cors() {
    let mut r = req("OPTIONS", "http://127.0.0.1:5173");
    r.headers_mut()
        .insert("access-control-request-method", "PUT".parse().unwrap());
    r.headers_mut().insert(
        "access-control-request-headers",
        "content-type".parse().unwrap(),
    );
    let response = app(5).oneshot(r).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "http://127.0.0.1:5173"
    );
    assert!(response.headers()["access-control-allow-methods"]
        .to_str()
        .unwrap()
        .contains("PUT"));
    for origin in ["https://evil.example", "null"] {
        assert_eq!(
            app(5).oneshot(req("PUT", origin)).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
    let mut r = req("OPTIONS", "http://127.0.0.1:5173");
    r.headers_mut()
        .insert("access-control-request-method", "DELETE".parse().unwrap());
    assert_eq!(
        app(5).oneshot(r).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let mut r = req("OPTIONS", "http://127.0.0.1:5173");
    r.headers_mut()
        .insert("access-control-request-method", "PUT".parse().unwrap());
    r.headers_mut().insert(
        "access-control-request-headers",
        "content-type,x-unapproved-header".parse().unwrap(),
    );
    assert_eq!(
        app(5).oneshot(r).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let response = app(5)
        .oneshot(req("PUT", "http://127.0.0.1:5173"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY); // reaches the actual amend JSON extractor.
    assert!(response
        .headers()
        .contains_key("access-control-allow-origin"));
    assert!(response.headers().contains_key("x-request-id"));
}
#[tokio::test]
async fn put_spoofing_burst_and_body_limit() {
    let router = app(1);
    let mut first = req("PUT", "http://127.0.0.1:5173");
    first
        .headers_mut()
        .insert("x-forwarded-for", "198.51.100.1".parse().unwrap());
    assert_eq!(
        router.clone().oneshot(first).await.unwrap().status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut second = req("PUT", "http://127.0.0.1:5173");
    second
        .headers_mut()
        .insert("x-forwarded-for", "198.51.100.2".parse().unwrap());
    let response = router.oneshot(second).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key("retry-after"));
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(body.contains("request_id"));
    let huge = Request::post("/v1/canonicalize")
        .header("content-type", "application/json")
        .body(Body::from("x".repeat(80 * 1024)))
        .unwrap();
    assert_eq!(
        app(5).oneshot(huge).await.unwrap().status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
}
#[tokio::test]
async fn configured_metadata_and_canonicalization_are_real_routes() {
    let response = app(5)
        .oneshot(Request::get("/v1/metadata").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    capture_response("GET", "/v1/metadata", StatusCode::OK, &value);
    assert_eq!(value["min_proposal_raw"], "1000000000000000000000");
    assert_eq!(value["protocol_version"], 2);
    let response=app(5).oneshot(Request::post("/v1/canonicalize").header("content-type","application/json").body(Body::from(r#"{"kind":"comment","body_html":"<div><p onclick='x()'>safe 🚀</p><a href='javascript:x()'>link</a></div>"}"#)).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    capture_response("POST", "/v1/canonicalize", StatusCode::OK, &value);
    assert!(!value["body_html"].as_str().unwrap().contains("onclick"));
    assert!(!value["body_html"].as_str().unwrap().contains("javascript:"));
    assert_eq!(
        value["body_hash"],
        operator_voting::payload::body_hash(value["body_html"].as_str().unwrap())
    );
}

#[tokio::test]
async fn metadata_reports_distinctive_configured_threshold_in_production() {
    let mut cfg = VotingConfig::for_tests("unused", "");
    cfg.run_mode = "production".into();
    cfg.apply_migrations = false;
    cfg.cors_origins = vec!["https://vote.cl8y.com".into()];
    cfg.min_proposal_raw =
        num_bigint::BigInt::from(4321u64) * num_bigint::BigInt::from(10u64).pow(18);
    let expected = cfg.min_proposal_raw.to_string();
    let response = app_with_config(cfg)
        .oneshot(Request::get("/v1/metadata").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(value["min_proposal_raw"], expected);
    assert_eq!(value["decimals"], 18);
    assert_eq!(value["protocol_version"], 2);
    capture_response("GET", "/v1/metadata", StatusCode::OK, &value);
}

/// Public fixture key; genuine chain-specific signatures over the exact request bytes.
fn signed_registration(chain: &str, chain_id: &str, version: Option<u32>) -> serde_json::Value {
    use base64::Engine;
    use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature, SigningKey};
    use operator_voting::crypto::{
        adr36_sign_doc_json, cosmos_address_from_pubkey, eip191_hash, recover_evm_address,
        verify_adr36, verify_eip191,
    };
    use sha2::{Digest, Sha256};

    let key = SigningKey::from_slice(&[0x11u8; 32]).unwrap();
    let pubkey = key.verifying_key().to_encoded_point(true);
    let address = if chain == "terra" {
        cosmos_address_from_pubkey(pubkey.as_bytes(), "terra").unwrap()
    } else {
        let (signature, recovery) = key.sign_prehash_recoverable(&eip191_hash("actor")).unwrap();
        let mut bytes = signature.to_bytes().to_vec();
        bytes.push(recovery.to_byte() + 27);
        recover_evm_address("actor", &hex::encode(bytes)).unwrap()
    };
    let now = chrono::Utc::now().timestamp();
    let mut payload = serde_json::json!({
        "nonce": uuid::Uuid::new_v4().to_string(),
        "app": "cl8y-voting", "chain": chain, "chain_id": chain_id,
        "purpose": "register", "address": address,
        "issued_at": now, "expires_at": now + 180,
    });
    if let Some(version) = version {
        payload["version"] = serde_json::json!(version);
    }
    let raw = payload.to_string();
    let mut request = serde_json::json!({"chain":chain,"address":address,"payload":raw});
    if chain == "terra" {
        let hash = Sha256::digest(adr36_sign_doc_json(&address, &raw).as_bytes());
        let signature: Signature = key.sign_prehash(&hash).unwrap();
        let signature = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
        let pubkey = base64::engine::general_purpose::STANDARD.encode(pubkey.as_bytes());
        verify_adr36(&address, &raw, &signature, &pubkey).unwrap();
        request["pubkey"] = serde_json::json!(pubkey);
        request["signature"] = serde_json::json!(signature);
    } else {
        let (signature, recovery) = key.sign_prehash_recoverable(&eip191_hash(&raw)).unwrap();
        let mut bytes = signature.to_bytes().to_vec();
        bytes.push(recovery.to_byte() + 27);
        let signature = format!("0x{}", hex::encode(bytes));
        verify_eip191(&address, &raw, &signature).unwrap();
        request["signature"] = serde_json::json!(signature);
    }
    request
}

#[tokio::test]
async fn production_factory_rejects_signed_development_domains_and_unsupported_versions() {
    // An unreachable lazy pool ensures rejection occurs before any transaction starts.
    for mode in ["prod", "production"] {
        let mut cfg = VotingConfig::for_tests("unused", "");
        cfg.run_mode = mode.into();
        cfg.apply_migrations = false;
        cfg.cors_origins = vec!["https://vote.cl8y.com".into()];
        let router = app_with_config(cfg);
        for (chain, development, mainnet) in [
            ("terra", "localterra", "columbus-5"),
            ("bsc", "31337", "56"),
        ] {
            let cases = [
                (
                    development,
                    Some(2),
                    StatusCode::UNAUTHORIZED,
                    "unauthorized",
                ),
                (mainnet, Some(1), StatusCode::BAD_REQUEST, "bad_request"),
                (mainnet, Some(99), StatusCode::BAD_REQUEST, "bad_request"),
                (mainnet, None, StatusCode::BAD_REQUEST, "bad_request"),
            ];
            for (chain_id, version, status, code) in cases {
                let body = signed_registration(chain, chain_id, version);
                let response = tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    router.clone().oneshot(
                        Request::post("/v1/register")
                            .header("origin", "https://vote.cl8y.com")
                            .header("content-type", "application/json")
                            .body(Body::from(body.to_string()))
                            .unwrap(),
                    ),
                )
                .await
                .expect("domain/version rejection must not wait on the database")
                .unwrap();
                assert_eq!(
                    response.status(),
                    status,
                    "{mode} {chain} {chain_id} {version:?}"
                );
                let request_id = response.headers()["x-request-id"]
                    .to_str()
                    .unwrap()
                    .to_owned();
                assert_eq!(
                    response.headers()["access-control-allow-origin"],
                    "https://vote.cl8y.com"
                );
                let value: serde_json::Value = serde_json::from_slice(
                    &response.into_body().collect().await.unwrap().to_bytes(),
                )
                .unwrap();
                assert_eq!(value["code"], code);
                assert_eq!(value["request_id"], request_id);
                capture_response("POST", "/v1/register", status, &value);
            }
        }
    }
}
#[tokio::test]
async fn sentinel_database_errors_are_never_public() {
    use axum::response::IntoResponse;
    let response = operator_voting::VotingError::Database(sqlx::Error::Protocol(
        "https://user:SENTINEL@db.example/?api_key=SENTINEL SQL SECRET".into(),
    ))
    .into_response();
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(!body.contains("SENTINEL"));
    assert!(body.contains("internal_error"));
}

#[tokio::test]
async fn extractor_routing_and_content_rejections_have_safe_correlated_contracts() {
    let cases = [
        (
            "POST",
            "/v1/register",
            "application/json",
            "{",
            "bad_request",
            400,
        ),
        (
            "GET",
            "/v1/proposals/not-a-uuid",
            "application/json",
            "",
            "bad_request",
            400,
        ),
        (
            "GET",
            "/v1/balances/0x1111111111111111111111111111111111111111?height=bad",
            "application/json",
            "",
            "bad_request",
            400,
        ),
        (
            "POST",
            "/v1/register",
            "text/plain",
            "{}",
            "unsupported_media_type",
            415,
        ),
        (
            "GET",
            "/missing?secret=SENTINEL",
            "application/json",
            "",
            "not_found",
            404,
        ),
        (
            "PUT",
            "/health",
            "application/json",
            "{}",
            "method_not_allowed",
            405,
        ),
    ];
    for (method, path, content_type, body, code, status) in cases {
        let response = app(50)
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", content_type)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status, "{path}");
        let request_id = response.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .to_string();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(!String::from_utf8_lossy(&bytes).contains("SENTINEL"));
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["code"], code, "{path}");
        assert_eq!(value["request_id"], request_id);
    }
}

/// Optional synthetic response evidence for external JSON Schema validation.
fn capture_response(method: &str, path: &str, status: StatusCode, body: &serde_json::Value) {
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
async fn canonical_proposal_and_analysis_responses_match_generated_contract() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../protocol/fixtures/canonical-v2.json")).unwrap();
    let text =
        "<p>This independent analysis has enough visible detail for the required minimum.</p>";
    for body in [
        serde_json::json!({"kind":"proposal","sections":fixture["vectors"][0]["raw"]}),
        serde_json::json!({"kind":"analysis","sections":{"benefits":text,"long_term":text,"risks":text,"short_term":text,"what":text}}),
    ] {
        let response = app(5)
            .oneshot(
                Request::post("/v1/canonicalize")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value: serde_json::Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(
            value["body_hash"],
            operator_voting::payload::body_hash(value["canonical_json"].as_str().unwrap())
        );
        capture_response("POST", "/v1/canonicalize", StatusCode::OK, &value);
    }
}
