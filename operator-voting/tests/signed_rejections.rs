//! Cryptographic/domain attacks through the exact production middleware stack.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use k256::ecdsa::{Signature, SigningKey};
use num_bigint::BigUint;
use operator_voting::{
    api::AppState,
    config::VotingConfig,
    crypto::{eip191_hash, recover_evm_address},
    payload::SignedPayload,
};
use serde_json::{json, Value};
use tower::ServiceExt;

fn sign(key: &SigningKey, payload: &SignedPayload) -> (String, String) {
    let raw = payload.canonical_json().unwrap();
    let (signature, recovery) = key.sign_prehash_recoverable(&eip191_hash(&raw)).unwrap();
    let mut bytes = signature.to_bytes().to_vec();
    bytes.push(recovery.to_byte() + 27);
    (raw, format!("0x{}", hex::encode(bytes)))
}
async fn rejected(app: axum::Router, path: &str, body: Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "attack must be an authentication error"
    );
    assert!(response.headers().contains_key("x-request-id"));
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(body["code"].is_string());
    assert!(body["request_id"].is_string());
}
#[tokio::test]
async fn high_s_wrong_domain_cross_scheme_purpose_and_proposal_are_rejected_without_records() {
    let Some(url) = voting_ledger::test_lock::test_database_url() else {
        return;
    };
    let _lock = voting_ledger::test_lock::hold_integration_db(&url)
        .await
        .unwrap();
    let pool = voting_ledger::test_lock::test_pool(&url).await.unwrap();
    voting_ledger::db::migrate(&pool).await.unwrap();
    let counts = || {
        sqlx::query_as::<_,(i64,i64,i64)>("SELECT (SELECT count(*) FROM voting.signatures),(SELECT count(*) FROM voting.action_receipts),(SELECT count(*) FROM voting.registration_intents)")
    };
    let before = counts().fetch_one(&pool).await.unwrap();
    let key = SigningKey::from_slice(&[0x71; 32]).unwrap();
    let now = chrono::Utc::now().timestamp();
    let mut payload = SignedPayload {
        version: 2,
        nonce: Some(uuid::Uuid::new_v4().to_string()),
        app: "cl8y-voting".into(),
        chain: "bsc".into(),
        chain_id: "56".into(),
        purpose: "register".into(),
        address: "0x0000000000000000000000000000000000000000".into(),
        issued_at: now,
        expires_at: now + 180,
        revision: None,
        title: None,
        body_hash: None,
        proposal_id: None,
        choice: None,
        prev_body_hash: None,
    };
    let (raw, signature) = sign(&key, &payload);
    payload.address = recover_evm_address(&raw, &signature).unwrap();
    let (raw, signature) = sign(&key, &payload);
    let app = operator_voting::application::application(AppState {
        pool: pool.clone(),
        cfg: VotingConfig::for_tests(&url, ""),
    })
    .unwrap();
    let request = |raw: String, signature: String| json!({"chain":"bsc","address":payload.address,"payload":raw,"signature":signature});
    // A real malleated signature: same r, n-s, parity flipped. Normalization
    // recovers the exact original low-s signature, proving this is not random junk.
    let mut high = hex::decode(signature.trim_start_matches("0x")).unwrap();
    let n = BigUint::parse_bytes(
        b"fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141",
        16,
    )
    .unwrap();
    let high_s = (n - BigUint::from_bytes_be(&high[32..64])).to_bytes_be();
    high[32..64].fill(0);
    high[64 - high_s.len()..64].copy_from_slice(&high_s);
    high[64] = 27 + ((high[64] - 27) ^ 1);
    let normalized = Signature::from_slice(&high[..64])
        .unwrap()
        .normalize_s()
        .unwrap();
    assert_eq!(
        &normalized.to_bytes()[..],
        &hex::decode(signature.trim_start_matches("0x")).unwrap()[..64]
    );
    rejected(
        app.clone(),
        "/v1/register",
        request(raw.clone(), format!("0x{}", hex::encode(high))),
    )
    .await;
    let mut cross = request(raw.clone(), signature.clone());
    cross["chain"] = json!("terra");
    rejected(app.clone(), "/v1/register", cross).await;
    let mut wrong = payload.clone();
    wrong.chain_id = "31337".into();
    let (raw, sig) = sign(&key, &wrong);
    rejected(app.clone(), "/v1/register", request(raw, sig)).await;
    wrong = payload.clone();
    wrong.purpose = "vote".into();
    wrong.proposal_id = Some(uuid::Uuid::new_v4().to_string());
    wrong.choice = Some("yes".into());
    let (raw, sig) = sign(&key, &wrong);
    rejected(
        app.clone(),
        "/v1/register",
        request(raw.clone(), sig.clone()),
    )
    .await;
    let different = uuid::Uuid::new_v4();
    let mut vote = request(raw, sig);
    vote["choice"] = json!("yes");
    rejected(app, &format!("/v1/proposals/{different}/votes"), vote).await;
    assert_eq!(
        counts().fetch_one(&pool).await.unwrap(),
        before,
        "rejected attacks must not consume signatures, receipts, or queue entries"
    );
}
