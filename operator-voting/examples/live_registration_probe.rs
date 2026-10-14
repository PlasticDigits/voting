//! Explicit live API probe with two fresh unfunded keys held only in memory.
//! Sends offchain registration signatures, never blockchain transactions.
use base64::Engine;
use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature, SigningKey};
use operator_voting::{
    config::APP_NAME,
    crypto::{adr36_sign_doc_json, cosmos_address_from_pubkey, eip191_hash},
    payload::SignedPayload,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sha3::Keccak256;
use std::{io::Read, process::Stdio, time::Duration};

const API: &str = "https://operator.vote.cl8y.com";

fn request(method: &str, path: &str, body: Option<&Value>) -> Result<(u16, Value), ()> {
    let mut child = std::process::Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--max-time",
            "20",
            "--config",
            "-",
            "--request",
            method,
            "--write-out",
            "\n%{http_code}",
        ])
        .arg(format!("{API}{path}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ())?;
    let mut config = String::from("header = \"Content-Type: application/json\"\n");
    if let Some(body) = body {
        config.push_str(&format!(
            "data = {}\n",
            serde_json::to_string(&body.to_string()).map_err(|_| ())?
        ));
    }
    use std::io::Write;
    child
        .stdin
        .take()
        .ok_or(())?
        .write_all(config.as_bytes())
        .map_err(|_| ())?;
    let output = child.wait_with_output().map_err(|_| ())?;
    if !output.status.success() {
        return Err(());
    }
    let text = String::from_utf8(output.stdout).map_err(|_| ())?;
    let (body, status) = text.rsplit_once('\n').ok_or(())?;
    Ok((
        status.parse().map_err(|_| ())?,
        serde_json::from_str(body).map_err(|_| ())?,
    ))
}

fn key() -> Result<SigningKey, ()> {
    for _ in 0..8 {
        let mut material = [0_u8; 32];
        std::fs::File::open("/dev/urandom")
            .map_err(|_| ())?
            .read_exact(&mut material)
            .map_err(|_| ())?;
        let result = SigningKey::from_bytes((&material).into());
        material.fill(0);
        if let Ok(key) = result {
            return Ok(key);
        }
    }
    Err(())
}

fn probe(chain: &str) -> Result<Value, ()> {
    let key = key()?;
    let compressed = key.verifying_key().to_encoded_point(true);
    let address = if chain == "terra" {
        cosmos_address_from_pubkey(compressed.as_bytes(), "terra").map_err(|_| ())?
    } else {
        let point = key.verifying_key().to_encoded_point(false);
        format!(
            "0x{}",
            hex::encode(&Keccak256::digest(&point.as_bytes()[1..])[12..])
        )
    };
    let now = chrono::Utc::now().timestamp();
    let payload = SignedPayload {
        version: 2,
        nonce: Some(uuid::Uuid::new_v4().to_string()),
        revision: None,
        app: APP_NAME.into(),
        chain: chain.into(),
        chain_id: if chain == "terra" { "columbus-5" } else { "56" }.into(),
        purpose: "register".into(),
        address: address.clone(),
        issued_at: now,
        expires_at: now + 600,
        title: None,
        body_hash: None,
        proposal_id: None,
        choice: None,
        prev_body_hash: None,
    };
    let message = payload.canonical_json().map_err(|_| ())?;
    let (signature, pubkey) = if chain == "terra" {
        let digest = Sha256::digest(adr36_sign_doc_json(&address, &message).as_bytes());
        let signature: Signature = key.sign_prehash(&digest).map_err(|_| ())?;
        (
            base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
            Some(base64::engine::general_purpose::STANDARD.encode(compressed.as_bytes())),
        )
    } else {
        let (signature, recovery) = key
            .sign_prehash_recoverable(&eip191_hash(&message))
            .map_err(|_| ())?;
        let mut bytes = signature.to_bytes().to_vec();
        bytes.push(recovery.to_byte() + 27);
        (format!("0x{}", hex::encode(bytes)), None)
    };
    let body = json!({"chain":chain,"address":address,"payload":message,"signature":signature,"pubkey":pubkey});
    let (status, accepted) = request("POST", "/v1/register", Some(&body))?;
    if status != 200 {
        return Err(());
    }
    let (retry_status, retried) = request("POST", "/v1/register", Some(&body))?;
    if retry_status != 200 || accepted != retried {
        return Err(());
    }
    let receipt = accepted["receipt_id"].as_str().ok_or(())?;
    let (receipt_status, exported) = request("GET", &format!("/v1/receipts/{receipt}"), None)?;
    if receipt_status != 200 || operator_voting::payload::verify_exported_receipt(&exported).is_err() {
        return Err(());
    }
    for _ in 0..60 {
        let (balance_status, balance) = request(
            "GET",
            &format!("/v1/balances/{address}?chain={chain}"),
            None,
        )?;
        if balance_status == 200
            && balance["registered"] == true
            && balance["snapshot_eligible"] == true
        {
            if balance["balance"] != "0" {
                return Err(());
            }
            return Ok(
                json!({"chain":chain,"registered":true,"snapshot_eligible":true,
                "balance":"0","receipt_verified":true,"idempotent_retry":true,
                "receipt_id":receipt,"wallet_hash":hex::encode(Sha256::digest(address.as_bytes()))}),
            );
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    Err(())
}

fn main() {
    if !std::env::args().any(|arg| arg == "--live-writes") {
        println!("Pass --live-writes to register one fresh unfunded address on each chain.");
        return;
    }
    for chain in ["terra", "bsc"] {
        match probe(chain) {
            Ok(result) => println!("{result}"),
            Err(()) => {
                println!("{}", json!({"chain":chain,"passed":false}));
                std::process::exit(1);
            }
        }
    }
}
