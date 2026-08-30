use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
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
use voting_ledger::ingest::{self, INGEST_CHUNK, INGEST_STALE_GAP};
use voting_ledger::lcd::LcdClient;
use voting_ledger::register::process_pending_intents;
use voting_ledger::LedgerError;

/// Last `process_pending_intents` SQL result. Independent of ingest catch-up (#14 / L12).
static INTENTS_OK: AtomicBool = AtomicBool::new(true);

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
    /// False when `voting.registration_intents` SELECT/UPDATE failed (L12). Not LCD retry.
    intents_ok: bool,
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

    let ingest_cfg = cfg.clone();
    let ingest_pool = pool.clone();
    let ingest_lcd = lcd.clone();
    let ingest_bsc = bsc.clone();
    let intent_source = source;
    let intent_pool = pool.clone();
    let intent_interval = cfg.poll_interval_ms;
    tokio::spawn(async move {
        loop {
            match process_pending_intents(&intent_pool, &intent_source).await {
                Ok(_) => INTENTS_OK.store(true, Ordering::Relaxed),
                Err(e) => {
                    INTENTS_OK.store(false, Ordering::Relaxed);
                    tracing::error!(error = %e, "registration intent poll failed");
                }
            }
            tokio::time::sleep(Duration::from_millis(intent_interval)).await;
        }
    });
    tokio::spawn(async move {
        loop {
            if let Err(e) = ingest_once(&ingest_pool, &ingest_lcd, &ingest_bsc, &ingest_cfg).await {
                tracing::error!(error = %e, "ledger ingest poll failed");
            }
            tokio::time::sleep(Duration::from_millis(ingest_cfg.poll_interval_ms)).await;
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
        intents_ok: INTENTS_OK.load(Ordering::Relaxed),
    })
}

async fn ingest_once(
    pool: &sqlx::PgPool,
    lcd: &Option<LcdClient>,
    bsc: &BscClient,
    cfg: &LedgerConfig,
) -> Result<(), LedgerError> {
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
        if let Some(window) = ingest::ingest_window(last, tip, INGEST_CHUNK, INGEST_STALE_GAP) {
            if window.jumped_to_tip && last > 0 {
                tracing::warn!(
                    last,
                    tip,
                    "terra last_indexed_height is a stale non-zero cursor; jumping to tip (L1, no archive)"
                );
            }
            for h in window.start..=window.end {
                ingest::ingest_terra_height(pool, lcd, cfg, h).await?;
                if let Ok(hash) = lcd.block_hash(h).await {
                    db::set_state(pool, "last_indexed_block_hash", &hash).await?;
                }
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
        if let Some(window) = ingest::ingest_window(last, tip, INGEST_CHUNK, INGEST_STALE_GAP) {
            if window.jumped_to_tip && last > 0 {
                tracing::warn!(
                    last,
                    tip,
                    "bsc last_indexed_bsc_block is a stale non-zero cursor; jumping to tip (L1, no archive)"
                );
            }
            ingest::ingest_bsc_range(pool, bsc, cfg, window.start, window.end).await?;
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
