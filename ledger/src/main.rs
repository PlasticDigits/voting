use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use std::{net::SocketAddr, time::Duration};
use tokio::{net::TcpListener, task::JoinSet};
use tracing_subscriber::EnvFilter;
use voting_ledger::{
    bsc::{BscBalanceSource, BscClient, CompositeBalanceSource, LcdBalancePair},
    config::LedgerConfig,
    db::{self, Chain},
    lcd::LcdClient,
    register::supervised_intent_pass,
    LedgerError,
};
#[derive(Clone)]
struct AppState {
    pool: sqlx::PgPool,
}
#[tokio::main]
async fn main() -> Result<(), LedgerError> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();
    let cfg = LedgerConfig::from_env()?;
    let inspect = voting_ledger::storage::requested_from_env()?;
    let pool = db::connect(&cfg.database_url).await?;
    if inspect {
        return inspect_storage_only(pool, &cfg.api_bind).await;
    }
    db::migrate(&pool).await?;
    let lcd = if cfg.terra_lcd_urls.is_empty() {
        None
    } else {
        Some(
            LcdClient::new_for_chain(cfg.terra_lcd_urls.clone(), &cfg.terra_chain_id)
                .with_rpc_urls(cfg.terra_rpc_urls.clone()),
        )
    };
    let bsc = BscClient::new(cfg.bsc_rpc_urls.clone());
    let source = CompositeBalanceSource {
        terra: lcd.clone().map(|lcd| LcdBalancePair {
            lcd,
            token: cfg.cl8y_token_address.clone(),
        }),
        bsc: if bsc.enabled() {
            Some(BscBalanceSource {
                client: bsc.clone(),
                token: cfg.bsc_cl8y_token_address.clone(),
            })
        } else {
            None
        },
    };
    if std::env::args().any(|a| a == "--reconcile-live-baseline") {
        let lcd = lcd
            .as_ref()
            .ok_or(LedgerError::Provider("no_terra_provider"))?;
        for chain in [Chain::Terra, Chain::Bsc] {
            voting_ledger::reconcile::reconcile_live_baseline(
                &pool, &source, lcd, &bsc, &cfg, chain,
            )
            .await?;
        }
        return Ok(());
    }
    let requested = match std::env::var("LEDGER_RECONCILE_LIVE_BASELINE").as_deref() {
        Ok("true" | "1") => true,
        Err(_) | Ok("false" | "0") => false,
        _ => {
            return Err(LedgerError::InvalidConfig(
                "LEDGER_RECONCILE_LIVE_BASELINE must be true or false".into(),
            ))
        }
    };
    if requested {
        let lcd = lcd
            .as_ref()
            .ok_or(LedgerError::Provider("no_terra_provider"))?;
        voting_ledger::reconcile::reconcile_if_required(&pool, &source, lcd, &bsc, &cfg).await?;
    }
    let mut workers = JoinSet::new();
    for chain in [Chain::Terra, Chain::Bsc] {
        let cfg = cfg.clone();
        let pool = pool.clone();
        let lcd = lcd.clone();
        let bsc = bsc.clone();
        workers.spawn(async move {
            let mut failures = 0_u64;
            loop {
                let result = tokio::time::timeout(
                    Duration::from_secs(60),
                    voting_ledger::worker::poll_chain(&pool, &lcd, &bsc, &cfg, chain),
                )
                .await;
                match result {
                    Ok(Ok(())) => failures = 0,
                    failure => {
                        failures = failures.saturating_add(1);
                        let provider_class = match &failure {
                            Ok(Err(error)) => provider_error_class(error),
                            _ => None,
                        };
                        let reason = match failure {
                            Err(_) => "worker_deadline",
                            Ok(Err(LedgerError::Conflict(_))) => "accounting_corruption",
                            Ok(Err(LedgerError::Amount(_))) => "accounting_invalid_amount",
                            _ => "provider_or_accounting_failure",
                        };
                        db::degrade(&pool, chain, reason).await?;
                        tracing::warn!(
                            chain = chain.as_str(),
                            reason,
                            provider_class = provider_class.unwrap_or("none"),
                            "chain worker degraded"
                        );
                    }
                }
                // Bounded exponential retry with deterministic per-chain jitter.
                let delay = cfg
                    .poll_interval_ms
                    .saturating_mul(1_u64 << failures.min(4))
                    .min(60_000)
                    + if chain == Chain::Bsc { 137 } else { 53 };
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            #[allow(unreachable_code)]
            Ok::<(), LedgerError>(())
        });
    }
    let intent_pool = pool.clone();
    let interval = cfg.poll_interval_ms;
    workers.spawn(async move {
        loop {
            if !supervised_intent_pass(&intent_pool, &source, Duration::from_secs(60)).await? {
                tracing::warn!("registration queue deadline or database failure");
            }
            tokio::time::sleep(Duration::from_millis(interval)).await;
        }
        #[allow(unreachable_code)]
        Ok::<(), LedgerError>(())
    });
    let app = Router::new()
        .route(
            "/live",
            get(|| async { Json(serde_json::json!({"ok":true})) }),
        )
        .route("/health", get(readiness))
        .route("/ready", get(readiness))
        .with_state(AppState { pool: pool.clone() });
    let addr: SocketAddr = cfg
        .api_bind
        .parse()
        .map_err(|_| LedgerError::InvalidConfig("invalid API_BIND".into()))?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr,"voting-ledger listening");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
    });
    let fatal = tokio::select! {
        _=shutdown_signal()=>false,
        _=supervise_workers(&mut workers)=>true,
    };
    let _ = shutdown_tx.send(());
    workers.abort_all();
    while workers.join_next().await.is_some() {}
    let _ = tokio::time::timeout(Duration::from_secs(5), server).await;
    let _ = tokio::time::timeout(Duration::from_secs(3), pool.close()).await;
    if fatal {
        return Err(LedgerError::Provider("supervised_worker_exit"));
    }
    Ok(())
}
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("signal handler");
        tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn inspect_storage_only(pool: sqlx::PgPool, bind: &str) -> Result<(), LedgerError> {
    let inventory = voting_ledger::storage::inventory(&pool).await?;
    let json = serde_json::to_string(&inventory)
        .map_err(|_| LedgerError::Provider("storage_inspection_serialization"))?;
    println!("VOTING_STORAGE_INVENTORY {json}");
    let unavailable = || async {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "ok": false, "ready": false, "reason": "storage_inspection"
            })),
        )
    };
    let app = Router::new()
        .route(
            "/live",
            get(|| async { Json(serde_json::json!({"ok": true})) }),
        )
        .route("/ready", get(unavailable))
        .route("/health", get(unavailable));
    let addr: SocketAddr = bind
        .parse()
        .map_err(|_| LedgerError::InvalidConfig("invalid API_BIND".into()))?;
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    let _ = tokio::time::timeout(Duration::from_secs(3), pool.close()).await;
    Ok(())
}
async fn readiness(
    State(state): State<AppState>,
) -> (StatusCode, Json<voting_ledger::health::Readiness>) {
    voting_ledger::health::readiness(&state.pool).await
}

async fn supervise_workers(workers: &mut JoinSet<Result<(), LedgerError>>) {
    let exit = workers.join_next().await;
    tracing::error!(
        panicked = exit.as_ref().is_some_and(|r| r.is_err()),
        "ledger worker exited; shutting down for supervisor restart"
    );
}
fn provider_error_class(error: &LedgerError) -> Option<&'static str> {
    match error {
        LedgerError::Provider(class) => Some(*class),
        _ => None,
    }
}
#[cfg(test)]
mod supervisor_tests {
    use super::*;
    #[test]
    fn provider_diagnostics_expose_only_static_provider_labels() {
        assert_eq!(
            provider_error_class(&LedgerError::Provider("bsc_transport")),
            Some("bsc_transport")
        );
        for error in [
            LedgerError::Conflict("SENTINEL_PRIVATE_ERROR".into()),
            LedgerError::BscRpc("SENTINEL_PRIVATE_RESPONSE".into()),
            LedgerError::Database(sqlx::Error::Protocol("SENTINEL_PRIVATE_DATABASE".into())),
        ] {
            assert_eq!(provider_error_class(&error), None);
        }
    }
    #[tokio::test]
    async fn panic_or_exit_notifies_fatal_supervisor() {
        let mut workers = JoinSet::new();
        workers.spawn(async {
            panic!("injected worker panic");
            #[allow(unreachable_code)]
            Ok::<(), LedgerError>(())
        });
        tokio::time::timeout(Duration::from_secs(1), supervise_workers(&mut workers))
            .await
            .unwrap();
        let mut workers = JoinSet::new();
        workers.spawn(async { Err(LedgerError::Provider("injected fatal exit")) });
        tokio::time::timeout(Duration::from_secs(1), supervise_workers(&mut workers))
            .await
            .unwrap();
    }
}
