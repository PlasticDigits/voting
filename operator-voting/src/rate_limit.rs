//! Bounded, expiring mutation quotas. Forwarded identities require explicit peer CIDRs.
//! Single-replica enforcement: see skills/AGENTS_PROTOCOL_SECURITY.md.
use crate::config::VotingConfig;
use axum::{
    extract::{connect_info::ConnectInfo, Request, State},
    http::{HeaderMap, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use ipnet::IpNet;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const MAX_BUCKETS: usize = 10_000;
const IDLE_TTL: Duration = Duration::from_secs(600);
#[derive(Clone, Debug)]
pub struct RateLimitState {
    buckets: Arc<Mutex<HashMap<String, Bucket>>>,
    per_minute: u32,
    burst: u32,
    trusted_peers: Vec<IpNet>,
    observed_ingress: Arc<AtomicBool>,
}
#[derive(Debug)]
struct Bucket {
    tokens: f64,
    at: Instant,
}
impl RateLimitState {
    pub fn from_config(cfg: &VotingConfig) -> Self {
        let mut state = Self::new(
            cfg.rate_limit_post_per_minute,
            cfg.rate_limit_post_burst,
            false,
        );
        if cfg.rate_limit_trust_forwarded {
            state.trusted_peers = cfg.trusted_proxy_cidrs.clone();
        }
        state
    }
    /// The legacy trust boolean never authorizes an unverified peer.
    pub fn new(per_minute: u32, burst: u32, _trust_forwarded: bool) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            per_minute,
            burst: burst.max(1),
            trusted_peers: vec![],
            observed_ingress: Arc::new(AtomicBool::new(false)),
        }
    }
    fn check(&self, key: String, now: Instant) -> Result<(), u64> {
        if self.per_minute == 0 {
            return Ok(());
        }
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        // Only expired entries are removed; live buckets cannot be reset by flooding unique identities.
        buckets.retain(|_, b| now.duration_since(b.at) < IDLE_TTL);
        let key = if buckets.contains_key(&key) || buckets.len() < MAX_BUCKETS {
            key
        } else {
            "overflow".into()
        };
        let bucket = buckets.entry(key).or_insert(Bucket {
            tokens: self.burst as f64,
            at: now,
        });
        bucket.tokens = (bucket.tokens
            + now.duration_since(bucket.at).as_secs_f64() * self.per_minute as f64 / 60.0)
            .min(self.burst as f64);
        bucket.at = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            Err(((1.0 - bucket.tokens) * 60.0 / self.per_minute as f64)
                .ceil()
                .max(1.0) as u64)
        }
    }
}

pub async fn enforce_post_rate_limit(
    State(state): State<RateLimitState>,
    request: Request,
    next: Next,
) -> Response {
    if !matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) {
        return next.run(request).await;
    }
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0);
    // One private transport peer per process supports exact proxy enrollment.
    // Never log forwarded identities, public client addresses, URLs or bodies.
    if let Some(peer) = peer {
        let private = match peer.ip() {
            IpAddr::V4(ip) => ip.is_private() && !ip.is_loopback(),
            IpAddr::V6(ip) => ip.is_unique_local(),
        };
        if private && !state.observed_ingress.swap(true, Ordering::Relaxed) {
            tracing::info!(transport_peer = %peer.ip(), "voting ingress peer observed");
        }
    }
    let ip = client_ip(request.headers(), peer, &state.trusted_peers);
    if let Err(wait) = state.check(format!("ip:{ip}"), Instant::now()) {
        return limited(wait);
    }
    next.run(request).await
}

fn limited(wait: u64) -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(axum::http::header::RETRY_AFTER, wait.to_string())],
        Json(serde_json::json!({"error":"rate limited", "code":"rate_limited"})),
    )
        .into_response()
}

pub fn client_ip(headers: &HeaderMap, connect: Option<SocketAddr>, peers: &[IpNet]) -> IpAddr {
    let Some(connect) = connect else {
        return IpAddr::V4(Ipv4Addr::LOCALHOST);
    };
    let peer = connect.ip();
    if !peers.iter().any(|cidr| cidr.contains(&peer)) {
        return peer;
    }
    let Some(raw) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) else {
        return peer;
    };
    if raw.len() > 2048 || headers.get_all("x-forwarded-for").iter().count() != 1 {
        return peer;
    }
    let hops: Option<Vec<IpAddr>> = raw.split(',').map(|s| s.trim().parse().ok()).collect();
    let Some(hops) = hops else {
        return peer;
    };
    if hops.len() > 16 {
        return peer;
    }
    // Walk from the authenticated peer toward the client; ignore all attacker-controlled hops to its left.
    hops.into_iter()
        .rev()
        .find(|ip| !peers.iter().any(|cidr| cidr.contains(ip)))
        .unwrap_or(peer)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forwarded_chains_require_authenticated_peers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "198.51.100.1, 203.0.113.7, 10.0.0.2".parse().unwrap(),
        );
        let peers = vec!["10.0.0.0/24".parse().unwrap()];
        assert_eq!(
            client_ip(&headers, Some("10.0.0.3:443".parse().unwrap()), &peers),
            "203.0.113.7".parse::<IpAddr>().unwrap()
        );
        assert_eq!(
            client_ip(&headers, Some("203.0.113.9:443".parse().unwrap()), &peers),
            "203.0.113.9".parse::<IpAddr>().unwrap()
        );
        headers.insert("x-forwarded-for", "garbage".parse().unwrap());
        assert_eq!(
            client_ip(&headers, Some("10.0.0.3:443".parse().unwrap()), &peers),
            "10.0.0.3".parse::<IpAddr>().unwrap()
        );
    }
    #[test]
    fn absent_peer_metadata_never_authorizes_forwarded_identity() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.7".parse().unwrap());
        let peers = vec!["127.0.0.0/8".parse().unwrap()];
        assert_eq!(
            client_ip(&headers, None, &peers),
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        );
    }

    #[test]
    fn ipv6_and_hostile_forwarding_are_bounded() {
        let peers = vec!["fd00::/64".parse().unwrap()];
        let peer = Some("[fd00::1]:443".parse().unwrap());
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "2001:db8::1234, fd00::2".parse().unwrap(),
        );
        assert_eq!(
            client_ip(&headers, peer, &peers),
            "2001:db8::1234".parse::<IpAddr>().unwrap()
        );
        headers.append("x-forwarded-for", "2001:db8::9999".parse().unwrap());
        assert_eq!(client_ip(&headers, peer, &peers), peer.unwrap().ip());
        for raw in [
            "x".repeat(2049),
            vec!["2001:db8::1"; 17].join(","),
            "2001:db8::1, garbage".into(),
        ] {
            headers.clear();
            headers.insert("x-forwarded-for", raw.parse().unwrap());
            assert_eq!(client_ip(&headers, peer, &peers), peer.unwrap().ip());
        }
    }

    #[tokio::test]
    async fn unverified_envelopes_cannot_charge_victim_wallets() {
        use axum::{body::Body, routing::post, Router};
        use tower::ServiceExt;
        let mut cfg = VotingConfig::for_tests("unused", "");
        cfg.rate_limit_post_burst = 1;
        let state = RateLimitState::from_config(&cfg);
        let app = Router::new()
            .route("/v1/register", post(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                enforce_post_rate_limit,
            ));
        for i in 1..=3 {
            let mut req = Request::post("/v1/register").header("content-type","application/json")
                .body(Body::from(r#"{"chain":"terra","address":"terra1victim","payload":"forged","signature":"bad"}"#)).unwrap();
            req.extensions_mut().insert(ConnectInfo(
                format!("203.0.113.{i}:1234").parse::<SocketAddr>().unwrap(),
            ));
            assert_eq!(
                app.clone().oneshot(req).await.unwrap().status(),
                StatusCode::OK
            );
        }
        assert!(
            state
                .buckets
                .lock()
                .unwrap()
                .keys()
                .all(|key| key.starts_with("ip:")),
            "unauthenticated envelopes must not create wallet buckets"
        );
    }

    #[test]
    fn buckets_are_bounded_expire_and_recover() {
        let state = RateLimitState::new(60, 1, false);
        let now = Instant::now();
        assert_eq!(state.check("one".into(), now), Ok(()));
        assert_eq!(state.check("one".into(), now), Err(1));
        assert_eq!(
            state.check("one".into(), now + Duration::from_secs(1)),
            Ok(())
        );
        for i in 0..MAX_BUCKETS + 100 {
            let _ = state.check(i.to_string(), now + Duration::from_secs(1));
        }
        assert!(state.buckets.lock().unwrap().len() <= MAX_BUCKETS + 1);
        assert!(state
            .check("fresh".into(), now + IDLE_TTL + Duration::from_secs(2))
            .is_ok());
        assert_eq!(state.buckets.lock().unwrap().len(), 1);
    }
}
