//! Real API-process persistence and policy-rotation regression, on marked ephemeral PG only.
use chrono::Utc;
use k256::ecdsa::{signature::hazmat::PrehashSigner, SigningKey};
use operator_voting::{
    crypto::{adr36_sign_doc_json, cosmos_address_from_pubkey, eip191_hash},
    payload::SignedPayload,
    proposal_sections::prepare_sections,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct ApiProcess {
    child: Child,
    cwd: PathBuf,
    address: SocketAddr,
}
impl ApiProcess {
    fn spawn(url: &str, address: SocketAddr, committee: &str, blacklist: &str) -> Self {
        let cwd =
            std::env::temp_dir().join(format!("voting-process-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir(&cwd).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_operator-voting"))
            .current_dir(&cwd)
            .env_clear()
            .env("DATABASE_URL", url)
            .env("RUN_MODE", "test")
            .env("APPLY_MIGRATIONS", "false")
            .env("API_BIND", address.to_string())
            .env("CORS_ORIGINS", "http://127.0.0.1:5173")
            .env("TERRA_CHAIN_ID", "columbus-5")
            .env("EVM_CHAIN_ID", "56")
            .env("RATE_LIMIT_POST_PER_MINUTE", "0")
            .env("RATE_LIMIT_POST_BURST", "20")
            .env("VOTING_COMMITTEE_ADDRESSES", committee)
            .env("VOTING_BLACKLIST_ADDRESSES", blacklist)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start real API binary");
        Self {
            child,
            cwd,
            address,
        }
    }
    async fn wait_ready(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "API process exited before liveness"
            );
            if matches!(http(self.address, "GET", "/health", None), Ok((200, _))) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("API process failed to become live");
    }
    fn crash(&mut self) {
        self.child.kill().expect("SIGKILL API process");
        let status = self.child.wait().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(
                status.signal(),
                Some(9),
                "test must exercise an actual OS crash"
            );
        }
    }
}
impl Drop for ApiProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir(&self.cwd);
    }
}

fn http(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<(u16, Value), ()> {
    let mut socket =
        TcpStream::connect_timeout(&address, Duration::from_millis(200)).map_err(|_| ())?;
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| ())?;
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| ())?;
    let body = body.unwrap_or("");
    write!(socket,"{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len()).map_err(|_|())?;
    let mut response = String::new();
    socket.read_to_string(&mut response).map_err(|_| ())?;
    let (header, body) = response.split_once("\r\n\r\n").ok_or(())?;
    let status = header
        .split_whitespace()
        .nth(1)
        .ok_or(())?
        .parse()
        .map_err(|_| ())?;
    let value = serde_json::from_str(body).map_err(|_| ())?;
    Ok((status, value))
}
fn payload(chain: &str, purpose: &str, address: &str) -> SignedPayload {
    let issued = Utc::now().timestamp();
    SignedPayload {
        version: 2,
        nonce: Some(uuid::Uuid::new_v4().to_string()),
        revision: None,
        app: "cl8y-voting".into(),
        chain: chain.into(),
        chain_id: if chain == "terra" { "columbus-5" } else { "56" }.into(),
        purpose: purpose.into(),
        address: address.into(),
        issued_at: issued,
        expires_at: issued + 180,
        title: None,
        body_hash: None,
        proposal_id: None,
        choice: None,
        prev_body_hash: None,
    }
}
fn terra_envelope(key: &SigningKey, payload: &SignedPayload) -> Value {
    use base64::Engine;
    let raw = serde_json::to_string(payload).unwrap();
    let doc = adr36_sign_doc_json(&payload.address, &raw);
    let hash = Sha256::digest(doc.as_bytes());
    let signature: k256::ecdsa::Signature = key.sign_prehash(&hash).unwrap();
    let base64 = base64::engine::general_purpose::STANDARD;
    json!({"chain":"terra","address":payload.address,"payload":raw,"signature":base64.encode(signature.to_bytes()),"pubkey":base64.encode(key.verifying_key().to_encoded_point(true).as_bytes())})
}

#[tokio::test]
async fn sigkill_restart_replays_once_and_rejects_prepared_revoked_actors() {
    let Some(base_url) = voting_ledger::test_lock::test_database_url() else {
        return;
    };
    let _lock = voting_ledger::test_lock::hold_integration_db(&base_url)
        .await
        .unwrap();
    let admin = voting_ledger::test_lock::test_pool(&base_url)
        .await
        .unwrap();
    let database = format!("voting_test_process_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {database}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!(
        "COMMENT ON DATABASE {database} IS 'cl8y-voting-ephemeral-test'"
    ))
    .execute(&admin)
    .await
    .unwrap();
    let url = format!("{}/{}", base_url.rsplit_once('/').unwrap().0, database);
    let pool = voting_ledger::test_lock::test_pool(&url).await.unwrap();
    voting_ledger::db::migrate(&pool).await.unwrap();
    let key = SigningKey::from_slice(&[0x67u8; 32]).unwrap();
    let actor = cosmos_address_from_pubkey(
        key.verifying_key().to_encoded_point(true).as_bytes(),
        "terra",
    )
    .unwrap();
    sqlx::query("INSERT INTO public.voting_registrations(chain,wallet_address,registered_at_height,initial_balance,status,baseline_height,baseline_block_hash,baseline_verified) VALUES('terra',$1,1,5000000000000000000000,'active',1,'terra-1',true)").bind(&actor).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO public.ledger_checkpoints(chain,height,block_hash,observed_at,complete,canonical,network_tip) VALUES('terra',20,'terra-20',clock_timestamp(),true,true,20),('bsc',20,'bsc-20',clock_timestamp(),true,true,20)").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO public.indexer_state(key,value,updated_at) VALUES('intents_ok','true',clock_timestamp()) ON CONFLICT(key) DO UPDATE SET value='true',updated_at=clock_timestamp()").execute(&pool).await.unwrap();
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    let mut api = ApiProcess::spawn(&url, address, &actor, "");
    api.wait_ready().await;
    let text =
        "<p>This proposal contains sufficient visible text to meet every required minimum.</p>";
    let sections = json!({"context":"","problem":text,"solution":text,"pros_cons":text,"summary":text,"success_criteria":text});
    let prepared = prepare_sections(&sections).unwrap();
    let mut draft = payload("terra", "draft", &actor);
    draft.title = Some("Process persistence".into());
    draft.body_hash = Some(prepared.body_hash.clone());
    draft.expires_at = draft.issued_at + 2;
    let mut request = terra_envelope(&key, &draft);
    request["title"] = json!("Process persistence");
    request["body_sections"] = sections;
    let bytes = serde_json::to_string(&request).unwrap();
    let (status, accepted) =
        http(address, "POST", "/v1/proposals", Some(&bytes)).expect("accepted draft HTTP response");
    assert_eq!(status, 201, "draft should be accepted before crash");
    let id = accepted["id"].as_str().unwrap();
    let receipt = accepted["receipt_id"].as_str().unwrap();
    // This queued approval was prepared under the old committee but never submitted.
    let mut opening = payload("terra", "open_vote", &actor);
    opening.proposal_id = Some(id.into());
    opening.revision = Some(1);
    opening.title = Some("Process persistence".into());
    opening.body_hash = Some(prepared.body_hash);
    let queued_open = serde_json::to_string(&terra_envelope(&key, &opening)).unwrap();
    let blocked_key = SigningKey::from_slice(&[0x68u8; 32]).unwrap();
    let hash = sha3::Keccak256::digest(
        &blocked_key
            .verifying_key()
            .to_encoded_point(false)
            .as_bytes()[1..],
    );
    let blocked = format!("0x{}", hex::encode(&hash[12..]));
    let register = payload("bsc", "register", &blocked);
    let raw = serde_json::to_string(&register).unwrap();
    let (sig, recovery) = blocked_key
        .sign_prehash_recoverable(&eip191_hash(&raw))
        .unwrap();
    let mut signature = sig.to_bytes().to_vec();
    signature.push(recovery.to_byte() + 27);
    let queued_register=serde_json::to_string(&json!({"chain":"bsc","address":blocked,"payload":raw,"signature":format!("0x{}",hex::encode(signature)),"pubkey":null})).unwrap();
    api.crash();
    drop(api);
    // The same database survives process destruction; new config removes the reviewer and blacklists queued actor.
    let mut restarted = ApiProcess::spawn(&url, address, "", &blocked);
    restarted.wait_ready().await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let (status, retry) = http(address, "POST", "/v1/proposals", Some(&bytes)).unwrap();
    assert_eq!(
        status, 201,
        "accepted exact bytes remain retryable after original expiry"
    );
    assert_eq!(retry, accepted);
    let (status, export) = http(address, "GET", &format!("/v1/receipts/{receipt}"), None).unwrap();
    assert_eq!(status, 200);
    operator_voting::payload::verify_exported_receipt(&export).unwrap();
    let (status, denied) = http(
        address,
        "POST",
        &format!("/v1/proposals/{id}/open"),
        Some(&queued_open),
    )
    .unwrap();
    assert_eq!(status, 403);
    assert_eq!(denied["code"], "forbidden");
    let (status, denied) = http(address, "POST", "/v1/register", Some(&queued_register)).unwrap();
    assert_eq!(status, 403);
    assert_eq!(denied["code"], "forbidden");
    let signature_count: i64 = sqlx::query_scalar("SELECT count(*) FROM voting.signatures")
        .fetch_one(&pool)
        .await
        .unwrap();
    let receipt_count: i64 = sqlx::query_scalar("SELECT count(*) FROM voting.action_receipts")
        .fetch_one(&pool)
        .await
        .unwrap();
    let proposal_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM voting.proposals WHERE status='draft'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let intents: i64 = sqlx::query_scalar("SELECT count(*) FROM voting.registration_intents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        (signature_count, receipt_count, proposal_count, intents),
        (1, 1, 1, 0)
    );
    restarted.crash();
    drop(restarted);
    pool.close().await;
    sqlx::query(&format!("DROP DATABASE {database}"))
        .execute(&admin)
        .await
        .unwrap();
    println!("SIGKILL/restart: exact expired draft retry retained one signature, receipt and draft; prepared revoked committee/BSC-blacklist actions denied");
}
