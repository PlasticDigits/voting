//! Shared wire vectors: immutable JSON bytes plus EIP-191 and ADR-36 digests.
use operator_voting::{
    crypto::{adr36_sign_doc_json, eip191_hash},
    payload::SignedPayload,
};
use serde_json::json;
use sha2::{Digest, Sha256};
fn main() {
    let mut vectors = Vec::new();
    let key = k256::ecdsa::SigningKey::from_slice(&[0x33; 32]).unwrap();
    let point = key.verifying_key().to_encoded_point(true);
    let terra_address =
        operator_voting::crypto::cosmos_address_from_pubkey(point.as_bytes(), "terra").unwrap();
    for (chain, address, chain_id) in [
        ("terra", terra_address.as_str(), "columbus-5"),
        ("bsc", "0x19e7e376e7c213b7e7e7e46cc70a5dd086daff2a", "56"),
    ] {
        for (purpose, nonce, revision) in [
            ("register", "11111111-1111-1111-1111-111111111111", None),
            ("amend", "22222222-2222-2222-2222-222222222222", Some(7)),
            ("open_vote", "33333333-3333-3333-3333-333333333333", Some(8)),
        ] {
            let payload = SignedPayload {
                version: 2,
                nonce: Some(nonce.into()),
                app: "cl8y-voting".into(),
                chain: chain.into(),
                chain_id: chain_id.into(),
                purpose: purpose.into(),
                address: address.into(),
                issued_at: 1_700_000_000,
                expires_at: 1_700_000_600,
                revision,
                title: revision.map(|_| "제안 🚀 e\u{0301} \"quoted\"\nreview".into()),
                body_hash: revision.map(|_| "a".repeat(64)),
                proposal_id: revision.map(|_| "44444444-4444-4444-4444-444444444444".into()),
                choice: None,
                prev_body_hash: if purpose == "amend" {
                    Some("b".repeat(64))
                } else {
                    None
                },
            };
            let canonical = payload.canonical_json().unwrap();
            let amino = adr36_sign_doc_json(address, &canonical);
            vectors.push(json!({"payload":payload,"canonical_json":canonical,"eip191_hash":hex::encode(eip191_hash(&canonical)),"adr36_doc":amino,"adr36_sha256":hex::encode(Sha256::digest(amino.as_bytes()))}));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"protocol_version":2,"vectors":vectors})).unwrap()
    );
}
