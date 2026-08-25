use std::net::SocketAddr;

use axum::http::{HeaderValue, Method};
use operator_voting::api::{router, AppState};
use operator_voting::config::{VotingConfig, MAX_BODY_BYTES};
use operator_voting::db;
use operator_voting::VotingError;
use tokio::net::TcpListener;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), VotingError> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    let cfg = VotingConfig::from_env()?;
    let pool = db::connect(&cfg.database_url).await?;
    // Restricted prod role cannot apply ledger migrations (issue #7 / L10).
    if cfg.apply_migrations {
        db::migrate(&pool).await?;
    }

    let cors = if cfg.cors_origins.is_empty() {
        CorsLayer::new()
            .allow_origin(AllowOrigin::predicate(|_, _| true))
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers(tower_http::cors::Any)
    } else {
        let origins: Vec<HeaderValue> = cfg
            .cors_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers(tower_http::cors::Any)
    };

    let addr: SocketAddr = cfg
        .api_bind
        .parse()
        .map_err(|e| VotingError::InvalidConfig(format!("API_BIND: {e}")))?;
    let app = router(AppState { pool, cfg })
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES + 8 * 1024))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    tracing::info!(%addr, "operator-voting listening");
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| VotingError::InvalidConfig(e.to_string()))?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(|e| VotingError::InvalidConfig(e.to_string()))?;
    Ok(())
}
