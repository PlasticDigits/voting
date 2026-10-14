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
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    let mut cfg = VotingConfig::for_tests("unused", "");
    cfg.rate_limit_post_burst = burst;
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
    assert_eq!(value["min_proposal_raw"], "1000000000000000000000");
    assert_eq!(value["protocol_version"], 2);
    let response=app(5).oneshot(Request::post("/v1/canonicalize").header("content-type","application/json").body(Body::from(r#"{"kind":"comment","body_html":"<div><p onclick='x()'>safe 🚀</p><a href='javascript:x()'>link</a></div>"}"#)).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(!value["body_html"].as_str().unwrap().contains("onclick"));
    assert!(!value["body_html"].as_str().unwrap().contains("javascript:"));
    assert_eq!(
        value["body_hash"],
        operator_voting::payload::body_hash(value["body_html"].as_str().unwrap())
    );
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
