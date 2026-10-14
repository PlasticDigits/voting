//! Ledger `/health` diagnostics for indexer lag vs live registration snapshots.
//!
//! Issue [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9): a process that is
//! up (`ok`) is not the same as a cursor that can serve a just-registered wallet.
//! Issue [#14](https://gitlab.com/PlasticDigits/voting/-/issues/14): `intents_ok` is
//! the last `voting.registration_intents` query result (L12). HTTP 200 stays
//! liveness so Coolify does not bounce the poller on boot.

/// True when at least one active registration's live snapshot height is still
/// ahead of `last_indexed_*`. GET `/v1/balances` clamps independently (OV-B1).
pub fn indexer_behind_registration(cursor: i64, max_registered_at: i64) -> bool {
    max_registered_at > 0 && cursor < max_registered_at
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_registrations_is_not_behind() {
        assert!(!indexer_behind_registration(0, 0));
        assert!(!indexer_behind_registration(10, 0));
    }

    #[test]
    fn cursor_zero_with_a_snapshot_is_behind() {
        assert!(indexer_behind_registration(0, 100));
    }

    #[test]
    fn cursor_below_register_is_behind() {
        assert!(indexer_behind_registration(50, 100));
        assert!(!indexer_behind_registration(100, 100));
        assert!(!indexer_behind_registration(120, 100));
    }
}

use crate::LedgerError;
use axum::{http::StatusCode, Json};
use serde::Serialize;
use std::time::Duration;
#[derive(Serialize)]
pub struct Readiness {
    ok: bool,
    database_ok: bool,
    chains: Vec<ChainHealth>,
    intent_backlog: i64,
    intent_oldest_seconds: i64,
    intents_ok: bool,
    baselines_ok: bool,
    intent_progress_age_seconds: Option<i64>,
}
#[derive(Serialize, sqlx::FromRow)]
pub struct ChainHealth {
    chain: String,
    height: i64,
    network_tip: i64,
    lag: i64,
    age_seconds: Option<i64>,
    complete: bool,
    canonical: bool,
    degraded_reason: Option<String>,
}
pub async fn readiness(pool: &sqlx::PgPool) -> (StatusCode, Json<Readiness>) {
    let result=tokio::time::timeout(Duration::from_secs(2),async {
        let chains:Vec<ChainHealth>=sqlx::query_as("SELECT chain,height,network_tip,network_tip-height AS lag,EXTRACT(EPOCH FROM NOW()-observed_at)::bigint AS age_seconds,complete,canonical,degraded_reason FROM ledger_checkpoints ORDER BY chain").fetch_all(pool).await?;
        let (backlog,oldest):(i64,i64)=sqlx::query_as("SELECT COUNT(*),COALESCE(EXTRACT(EPOCH FROM NOW()-MIN(created_at))::bigint,0) FROM voting.registration_intents WHERE processed_at IS NULL").fetch_one(pool).await?;
        let progress:Option<(String,i64)>=sqlx::query_as("SELECT value,EXTRACT(EPOCH FROM NOW()-updated_at)::bigint FROM indexer_state WHERE key='intents_ok'").fetch_optional(pool).await?;
        let intent_progress_age_seconds=progress.as_ref().map(|(_,age)|*age);
        let intents_ok=progress.is_some_and(|(value,age)|value=="true" && (0..=120).contains(&age));
        let baselines_ok:bool=sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM voting_registrations r JOIN ledger_checkpoints c ON c.chain=r.chain WHERE r.status='active' AND COALESCE(r.baseline_height,r.registered_at_height)<=c.height AND (NOT r.baseline_verified OR COALESCE(r.baseline_block_hash,'')=''))").fetch_one(pool).await?;
        Ok::<_,LedgerError>((chains,backlog,oldest,intents_ok,baselines_ok,intent_progress_age_seconds))
    }).await.unwrap_or(Err(LedgerError::Provider("readiness_database_deadline")));
    match result {
        Ok((chains, backlog, oldest, intents_ok, baselines_ok, intent_progress_age_seconds)) => {
            let ok = chains.len() == 2
                && chains.iter().all(|c| {
                    c.height > 0
                        && c.complete
                        && c.canonical
                        && c.degraded_reason.is_none()
                        && c.age_seconds.is_some_and(|a| (0..=120).contains(&a))
                        && c.lag >= 0
                        && c.lag <= if c.chain == "bsc" { 17 } else { 2 }
                })
                && intents_ok
                && baselines_ok;
            (
                if ok {
                    StatusCode::OK
                } else {
                    StatusCode::SERVICE_UNAVAILABLE
                },
                Json(Readiness {
                    ok,
                    database_ok: true,
                    chains,
                    intent_backlog: backlog,
                    intent_oldest_seconds: oldest,
                    intents_ok,
                    baselines_ok,
                    intent_progress_age_seconds,
                }),
            )
        }
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(Readiness {
                ok: false,
                database_ok: false,
                chains: vec![],
                intent_backlog: 0,
                intent_oldest_seconds: 0,
                intents_ok: false,
                baselines_ok: false,
                intent_progress_age_seconds: None,
            }),
        ),
    }
}

/// Source time, not request completion time, proves the provider is observing a live head.
pub fn validate_head_time(timestamp: i64) -> crate::error::LedgerResult<()> {
    let age = chrono::Utc::now().timestamp().saturating_sub(timestamp);
    if !(-5..=120).contains(&age) {
        return Err(LedgerError::Provider("stale_or_future_provider_head"));
    }
    Ok(())
}
