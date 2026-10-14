use std::net::SocketAddr;

use operator_voting::api::AppState;
use operator_voting::application::application;
use operator_voting::config::VotingConfig;
use operator_voting::db;
use operator_voting::VotingError;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    if run().await.is_err() {
        tracing::error!("operator-voting startup failed; check validated configuration, database connectivity and restricted role");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), VotingError> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    let cfg = VotingConfig::from_env()?;
    let pool = db::connect(&cfg.database_url).await?;
    if cfg.run_mode == "prod" {
        db::validate_production_privileges(&pool).await?;
    }
    // Restricted prod role cannot apply ledger migrations (issue #7 / L10).
    if cfg.apply_migrations {
        db::migrate(&pool).await?;
    }

    let addr: SocketAddr = cfg
        .api_bind
        .parse()
        .map_err(|e| VotingError::InvalidConfig(format!("API_BIND: {e}")))?;
    let app = application(AppState { pool, cfg })?;

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
