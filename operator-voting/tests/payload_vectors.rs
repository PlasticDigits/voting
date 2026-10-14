use operator_voting::{
    crypto::{adr36_sign_doc_json, eip191_hash},
    payload::SignedPayload,
};
use sha2::{Digest, Sha256};
#[test]
fn shared_v2_payload_wire_and_signature_digests_match() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../protocol/fixtures/payload-v2.json")).unwrap();
    assert_eq!(fixture["protocol_version"], 2);
    for vector in fixture["vectors"].as_array().unwrap() {
        let payload: SignedPayload = serde_json::from_value(vector["payload"].clone()).unwrap();
        let canonical = payload.canonical_json().unwrap();
        assert_eq!(canonical, vector["canonical_json"]);
        assert_eq!(hex::encode(eip191_hash(&canonical)), vector["eip191_hash"]);
        let amino = adr36_sign_doc_json(&payload.address, &canonical);
        assert_eq!(amino, vector["adr36_doc"]);
        assert_eq!(
            hex::encode(Sha256::digest(amino.as_bytes())),
            vector["adr36_sha256"]
        );
        if payload.revision.is_some() {
            assert!(
                canonical.find("\"expires_at\"").unwrap() < canonical.find("\"revision\"").unwrap()
            );
            assert!(canonical.find("\"revision\"").unwrap() < canonical.find("\"title\"").unwrap());
        }
    }
}
