//! Production application factory: CLI and middleware integration tests share this stack.
use crate::{
    api::{router, AppState},
    config::{validate_networks, validate_origins, validate_prod_guards, MAX_BODY_BYTES},
    VotingError,
};
use axum::{
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Json, Router,
};
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer, trace::TraceLayer};

pub fn application(state: AppState) -> Result<Router, VotingError> {
    validate_origins(&state.cfg.cors_origins)?;
    validate_networks(
        &state.cfg.run_mode,
        &state.cfg.terra_chain_id,
        &state.cfg.evm_chain_id,
    )?;
    validate_prod_guards(
        &state.cfg.run_mode,
        &state.cfg.cors_origins,
        state.cfg.rate_limit_post_per_minute,
        state.cfg.apply_migrations,
    )?;
    let origins: Vec<HeaderValue> = state
        .cfg
        .cors_origins
        .iter()
        .map(|o| o.parse().expect("validated origin"))
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::OPTIONS])
        .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::ACCEPT])
        .expose_headers([
            axum::http::header::RETRY_AFTER,
            axum::http::HeaderName::from_static("x-request-id"),
        ]);
    let cfg = state.cfg.clone();
    Ok(router(state)
        .merge(crate::contracts::router(cfg.clone()))
        .layer(middleware::from_fn_with_state(
            crate::rate_limit::RateLimitState::from_config(&cfg),
            crate::rate_limit::enforce_post_rate_limit,
        ))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES + 8 * 1024))
        .layer(cors)
        .layer(middleware::from_fn_with_state(
            cfg.cors_origins,
            enforce_origin,
        ))
        .layer(middleware::from_fn(correlate_errors))
        .layer(TraceLayer::new_for_http().make_span_with(
            |request: &Request| tracing::info_span!("http_request", method = %request.method()),
        )))
}
async fn enforce_origin(
    State(origins): State<Vec<String>>,
    request: Request,
    next: Next,
) -> Response {
    if let Some(origin) = request.headers().get(axum::http::header::ORIGIN) {
        let allowed = origin
            .to_str()
            .ok()
            .map(|o| origins.iter().any(|known| known == o))
            .unwrap_or(false);
        if !allowed {
            return (
                StatusCode::FORBIDDEN,
                Json(
                    serde_json::json!({"error":"origin not allowed", "code":"origin_not_allowed"}),
                ),
            )
                .into_response();
        }
    }
    if request.method() == Method::OPTIONS {
        if let Some(method) = request.headers().get("access-control-request-method") {
            if !matches!(method.to_str(), Ok("GET" | "POST" | "PUT")) {
                return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"method not allowed", "code":"cors_method_not_allowed"}))).into_response();
            }
        }
        if let Some(headers) = request.headers().get("access-control-request-headers") {
            if !headers
                .to_str()
                .ok()
                .map(|s| {
                    s.split(',').all(|h| {
                        matches!(
                            h.trim().to_ascii_lowercase().as_str(),
                            "content-type" | "accept"
                        )
                    })
                })
                .unwrap_or(false)
            {
                return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"headers not allowed", "code":"cors_headers_not_allowed"}))).into_response();
            }
        }
    }
    next.run(request).await
}
async fn correlate_errors(request: Request, next: Next) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    let response = next.run(request).await;
    let (mut parts, body) = response.into_parts();
    parts
        .headers
        .insert("x-request-id", request_id.parse().unwrap());
    if !parts.status.is_client_error() && !parts.status.is_server_error() {
        return Response::from_parts(parts, body);
    }
    let bytes = axum::body::to_bytes(body, 128 * 1024)
        .await
        .unwrap_or_default();
    let mut value = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({"error": "request rejected"}));
    if value.get("code").is_none() {
        let code = match parts.status.as_u16() {
            400 => "bad_request",
            404 => "not_found",
            405 => "method_not_allowed",
            413 => "body_too_large",
            415 => "unsupported_media_type",
            422 => "invalid_request",
            429 => "rate_limited",
            503 => "service_unavailable",
            _ => "request_rejected",
        };
        value["code"] = serde_json::json!(code);
        value["error"] = serde_json::json!("request rejected");
    }
    value["request_id"] = serde_json::json!(request_id);
    parts.headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    parts.headers.remove(axum::http::header::CONTENT_LENGTH);
    tracing::warn!(
        request_id,
        status = parts.status.as_u16(),
        "API request rejected"
    );
    Response::from_parts(
        parts,
        axum::body::Body::from(serde_json::to_vec(&value).unwrap()),
    )
}
