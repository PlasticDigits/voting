use std::net::SocketAddr;
use std::time::Duration;

use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use voting_ledger::bsc::{BscBalanceSource, BscClient, CompositeBalanceSource, LcdBalancePair};
use voting_ledger::config::LedgerConfig;
use voting_ledger::db;
use voting_ledger::db::Chain;
use voting_ledger::health::indexer_behind_registration;
use voting_ledger::lcd::LcdClient;
use voting_ledger::ingest;
use voting_ledger::register::process_pending_intents;
use voting_ledger::LedgerError;

#[derive(Clone)]
struct AppState {
    pool: sqlx::PgPool,
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    terra_height: i64,
    bsc_block: i64,
    /// Indexer cursors are at or past every active registration snapshot.
    caught_up: bool,
    terra_behind_registration: bool,
    bsc_behind_registration: bool,
}

#[tokio::main]
async fn main() -> Result<(), LedgerError> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    let cfg = LedgerConfig::from_env()?;
    let pool = db::connect(&cfg.database_url).await?;
    db::migrate(&pool).await?;

    let lcd = if cfg.terra_lcd_urls.is_empty() {
        None
    } else {
        Some(LcdClient::new(cfg.terra_lcd_urls.clone()))
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

    let poll_cfg = cfg.clone();
    let poll_pool = pool.clone();
    let poll_lcd = lcd.clone();
    let poll_bsc = bsc.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = poll_once(&poll_pool, &poll_lcd, &poll_bsc, &source, &poll_cfg).await {
                tracing::error!(error = %e, "ledger poll failed");
            }
            tokio::time::sleep(Duration::from_millis(poll_cfg.poll_interval_ms)).await;
        }
    });

    let app = Router::new()
        .route("/health", get(health))
        .with_state(AppState { pool });
    let addr: SocketAddr = cfg
        .api_bind
        .parse()
        .map_err(|e| LedgerError::InvalidConfig(format!("API_BIND: {e}")))?;
    tracing::info!(%addr, "voting-ledger listening");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    let terra_height = db::indexed_terra_height(&state.pool).await.unwrap_or(0);
    let bsc_block = db::indexed_bsc_block(&state.pool).await.unwrap_or(0);
    let terra_max = db::max_registered_height(&state.pool, Chain::Terra)
        .await
        .unwrap_or(0);
    let bsc_max = db::max_registered_height(&state.pool, Chain::Bsc)
        .await
        .unwrap_or(0);
    let terra_behind_registration = indexer_behind_registration(terra_height, terra_max);
    let bsc_behind_registration = indexer_behind_registration(bsc_block, bsc_max);
    let caught_up = !terra_behind_registration && !bsc_behind_registration;
    Json(Health {
        ok: true,
        terra_height,
        bsc_block,
        caught_up,
        terra_behind_registration,
        bsc_behind_registration,
    })
}

async fn poll_once(
    pool: &sqlx::PgPool,
    lcd: &Option<LcdClient>,
    bsc: &BscClient,
    source: &CompositeBalanceSource,
    cfg: &LedgerConfig,
) -> Result<(), LedgerError> {
    process_pending_intents(pool, source).await?;

    if let Some(lcd) = lcd {
        let last = db::indexed_terra_height(pool).await?;
        let max_reg = db::max_registered_height(pool, Chain::Terra).await?;
        if indexer_behind_registration(last, max_reg) {
            tracing::warn!(
                last,
                max_reg,
                "terra last_indexed_height is behind a live registration snapshot; GET /v1/balances clamps to registered_at_height until ingest catches up"
            );
        }
        ingest::maybe_rewind_terra(pool, lcd, last).await?;
        let last = db::indexed_terra_height(pool).await?;
        let tip = lcd.latest_height().await?;
        let start = if last == 0 { tip } else { last + 1 };
        for h in start..=tip {
            ingest::ingest_terra_height(pool, lcd, cfg, h).await?;
            if let Ok(hash) = lcd.block_hash(h).await {
                db::set_state(pool, "last_indexed_block_hash", &hash).await?;
            }
        }
        let after = db::indexed_terra_height(pool).await?;
        if after == 0 && max_reg > 0 {
            tracing::error!(
                max_reg,
                "terra last_indexed_height is still 0 after ingest; balances and /health.caught_up will look stalled"
            );
        }
    }

    if bsc.enabled() {
        let last = db::indexed_bsc_block(pool).await?;
        let max_reg = db::max_registered_height(pool, Chain::Bsc).await?;
        if indexer_behind_registration(last, max_reg) {
            tracing::warn!(
                last,
                max_reg,
                "bsc last_indexed_bsc_block is behind a live registration snapshot; GET /v1/balances clamps to registered_at_height until ingest catches up"
            );
        }
        let tip = bsc.block_number().await?;
        let start = if last == 0 { tip } else { last + 1 };
        // Cap catch-up window to avoid huge eth_getLogs.
        let end = tip.min(start + 2_000);
        if start <= end {
            ingest::ingest_bsc_range(pool, bsc, cfg, start, end).await?;
        }
        let after = db::indexed_bsc_block(pool).await?;
        if after == 0 && max_reg > 0 {
            tracing::error!(
                max_reg,
                "bsc last_indexed_bsc_block is still 0 after ingest; balances and /health.caught_up will look stalled"
            );
        }
    }
    Ok(())
}
