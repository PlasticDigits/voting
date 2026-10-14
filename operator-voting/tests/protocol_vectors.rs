use operator_voting::proposal_sections::{prepare_sections, visible_len};
#[test]
fn shared_canonical_vectors_match_authoritative_server() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../protocol/fixtures/canonical-v2.json")).unwrap();
    for vector in fixture["vectors"].as_array().unwrap() {
        let actual = prepare_sections(&vector["raw"]).unwrap();
        assert_eq!(actual.canonical_json, vector["canonical_json"]);
        assert_eq!(actual.body_hash, vector["body_hash"]);
        assert_eq!(
            serde_json::Value::Object(actual.sanitized.clone()),
            vector["sections"]
        );
        assert_eq!(
            visible_len(actual.sanitized["summary"].as_str().unwrap()),
            vector["visible_length"].as_u64().unwrap() as usize
        );
    }
}
