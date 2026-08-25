//! POST IP/QPS limiter (`governor`). GET and OPTIONS are unrestricted.
//!
//! Invariants (cross-link: [`docs/OPERATOR_VOTING.md`](../../docs/OPERATOR_VOTING.md)):
//! - **O-RL1** — every POST (`/v1/register`, `/v1/proposals`, votes, …) shares one per-IP quota.
//! - **O-RL2** — GET `/health` and other reads are not QPS-limited.
//! - **O-RL3** — the 64 KiB body cap (`MAX_BODY_BYTES`) is independent of this layer.
//! - **O-RL4** — `X-Forwarded-For` / `X-Real-IP` are trusted only when configured (Coolify).
//! - **O-RL5** — counters are in-process per replica; Coolify scale-out does not share quota.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;

use axum::extract::connect_info::ConnectInfo;
use axum::extract::Request;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use governor::clock::{Clock, DefaultClock};
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};

use crate::config::VotingConfig;

pub type IpLimiter = RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>;

#[derive(Clone)]
pub struct RateLimitState {
    limiter: Option<Arc<IpLimiter>>,
    trust_forwarded: bool,
}

impl RateLimitState {
    pub fn from_config(cfg: &VotingConfig) -> Self {
        Self::new(
            cfg.rate_limit_post_per_minute,
            cfg.rate_limit_post_burst,
            cfg.rate_limit_trust_forwarded,
        )
    }

    pub fn new(per_minute: u32, burst: u32, trust_forwarded: bool) -> Self {
        let limiter = NonZeroU32::new(per_minute).map(|pm| {
            let burst = NonZeroU32::new(burst).unwrap_or(pm);
            let quota = Quota::per_minute(pm).allow_burst(burst);
            Arc::new(RateLimiter::keyed(quota))
        });
        Self {
            limiter,
            trust_forwarded,
        }
    }
}

pub async fn enforce_post_rate_limit(
    axum::extract::State(state): axum::extract::State<RateLimitState>,
    request: Request,
    next: Next,
) -> Response {
    if request.method() != Method::POST {
        return next.run(request).await;
    }
    let Some(limiter) = &state.limiter else {
        return next.run(request).await;
    };
    let ip = client_ip(
        request.headers(),
        request.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0),
        state.trust_forwarded,
    );
    match limiter.check_key(&ip) {
        Ok(()) => next.run(request).await,
        Err(not_until) => {
            let wait = not_until.wait_time_from(DefaultClock::default().now());
            let secs = wait.as_secs().clamp(1, 120);
            (
                StatusCode::TOO_MANY_REQUESTS,
                [(
                    axum::http::header::RETRY_AFTER,
                    secs.to_string().parse().unwrap_or_else(|_| {
                        axum::http::HeaderValue::from_static("60")
                    }),
                )],
                Json(serde_json::json!({ "error": "rate limited" })),
            )
                .into_response()
        }
    }
}

pub fn client_ip(headers: &HeaderMap, connect: Option<SocketAddr>, trust_forwarded: bool) -> IpAddr {
    if trust_forwarded {
        if let Some(ip) = forwarded_ip(headers) {
            return ip;
        }
    }
    connect
        .map(|addr| addr.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

fn forwarded_ip(headers: &HeaderMap) -> Option<IpAddr> {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            if let Ok(ip) = first.trim().parse::<IpAddr>() {
                return Some(ip);
            }
        }
    }
    headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::{get, post};
    use axum::Router;
    use tower::ServiceExt;

    fn app(state: RateLimitState) -> Router {
        Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/v1/register", post(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state,
                enforce_post_rate_limit,
            ))
    }

    async fn post_from(app: Router, ip: &str) -> StatusCode {
        let resp = app
            .oneshot(
                Request::post("/v1/register")
                    .header("x-forwarded-for", ip)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        resp.status()
    }

    async fn post_response(app: Router, ip: &str) -> axum::http::Response<Body> {
        app.oneshot(
            Request::post("/v1/register")
                .header("x-forwarded-for", ip)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn posts_over_burst_return_429() {
        let state = RateLimitState::new(2, 2, true);
        let router = app(state);
        assert_eq!(post_from(router.clone(), "203.0.113.9").await, StatusCode::OK);
        assert_eq!(post_from(router.clone(), "203.0.113.9").await, StatusCode::OK);
        let limited = post_response(router.clone(), "203.0.113.9").await;
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry = limited
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok());
        assert!(retry.is_some(), "O-RL: 429 must include Retry-After");
        // Different client is a different bucket (O-RL1 keyed by IP).
        assert_eq!(post_from(router.clone(), "198.51.100.7").await, StatusCode::OK);
        // GET stays open (O-RL2).
        let health = router
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn untrusted_forwarded_header_is_ignored() {
        let state = RateLimitState::new(1, 1, false);
        let router = app(state);
        assert_eq!(post_from(router.clone(), "203.0.113.1").await, StatusCode::OK);
        // Spoofed XFF must not open a new bucket when trust is off (O-RL4).
        assert_eq!(
            post_from(router, "203.0.113.2").await,
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[test]
    fn parses_first_forwarded_hop() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.10, 10.0.0.1".parse().unwrap());
        assert_eq!(
            forwarded_ip(&headers).unwrap(),
            "203.0.113.10".parse::<IpAddr>().unwrap()
        );
    }
}
