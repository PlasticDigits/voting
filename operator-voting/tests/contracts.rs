#[test]
fn generated_document_covers_actual_domain_routes_and_typed_statuses() {
    let actual = operator_voting::contracts::openapi();
    let artifact: serde_json::Value =
        serde_json::from_str(include_str!("../../protocol/openapi.json")).unwrap();
    assert_eq!(
        actual, artifact,
        "regenerate OpenAPI after changing a handler, DTO or middleware response"
    );
    for (route, method, success) in [
        ("/health", "get", "200"),
        ("/openapi.json", "get", "200"),
        ("/v1/proposals/{id}/weights/{addr}", "get", "200"),
        ("/ready", "get", "200"),
        ("/v1/register", "post", "200"),
        ("/v1/registration/{addr}", "get", "200"),
        ("/v1/proposals", "get", "200"),
        ("/v1/proposals", "post", "201"),
        ("/v1/proposals/{id}", "get", "200"),
        ("/v1/proposals/{id}/sections", "put", "200"),
        ("/v1/proposals/{id}/comments", "get", "200"),
        ("/v1/proposals/{id}/comments", "post", "201"),
        ("/v1/proposals/{id}/analysis", "get", "200"),
        ("/v1/proposals/{id}/analysis", "post", "201"),
        ("/v1/proposals/{id}/open", "post", "200"),
        ("/v1/proposals/{id}/votes", "post", "200"),
        ("/v1/proposals/{id}/votes/{addr}", "get", "200"),
        ("/v1/balances/{addr}", "get", "200"),
        ("/v1/receipts/{id}", "get", "200"),
        ("/v1/metadata", "get", "200"),
        ("/v1/canonicalize", "post", "200"),
    ] {
        let operation = &actual["paths"][route][method];
        assert!(operation.is_object(), "{method} {route}");
        assert!(
            operation["responses"][success]["content"]["application/json"]["schema"].is_object(),
            "typed success {method} {route}"
        );
        for status in ["403", "413", "422", "500"] {
            assert_eq!(
                operation["responses"][status]["content"]["application/json"]["schema"]["$ref"],
                "#/components/schemas/ErrorBody",
                "{route} {status}"
            );
        }
    }
    assert_eq!(
        actual["components"]["schemas"]["VoteChoice"]["enum"],
        serde_json::json!(["for", "against", "abstain"])
    );
    assert_eq!(
        actual["components"]["schemas"]["SnapshotWeight"]["properties"]["weight"]["type"],
        "string"
    );
}
