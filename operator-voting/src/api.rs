use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::balance_query::{freeze_heights, resolve_balance_height, resolve_chain};
use crate::blacklist::normalize_address;
use crate::config::VotingConfig;
use crate::crypto::{verify_eip191, verify_terra};
use crate::db;
use crate::error::{VotingError, VotingResult};
use crate::payload::parse_and_validate;
use crate::proposal_sections::{prepare_sections, summary_html};
use crate::rate_limit::{enforce_post_rate_limit, RateLimitState};

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub cfg: VotingConfig,
}

pub fn router(state: AppState) -> Router {
    let limits = RateLimitState::from_config(&state.cfg);
    Router::new()
        .route("/health", get(health))
        .route("/openapi.json", get(openapi))
        .route("/v1/register", post(register))
        .route("/v1/registration/{addr}", get(registration))
        .route("/v1/proposals", get(list_proposals).post(create_proposal))
        .route("/v1/proposals/{id}", get(get_proposal))
        .route("/v1/proposals/{id}/votes", post(cast_vote))
        .route("/v1/proposals/{id}/votes/{addr}", get(get_vote))
        .route("/v1/balances/{addr}", get(get_balance))
        .with_state(state)
        .layer(axum::middleware::from_fn_with_state(
            limits,
            enforce_post_rate_limit,
        ))
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    service: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health {
        ok: true,
        service: "operator-voting",
    })
}

async fn openapi() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "openapi": "3.0.3",
        "info": { "title": "operator-voting", "version": "0.1.0" },
        "paths": {
            "/v1/register": { "post": { "summary": "Register a wallet" } },
            "/v1/proposals": {
                "get": { "summary": "List proposals (includes summary TL;DR when sections exist)" },
                "post": { "summary": "Create a templated proposal (body_sections required)" }
            },
            "/v1/proposals/{id}": { "get": { "summary": "Proposal detail; body_sections or legacy body_html" } },
            "/v1/proposals/{id}/votes": { "post": { "summary": "Cast vote" } }
        }
    }))
}

#[derive(Debug, Deserialize)]
pub struct SignedRequest {
    pub chain: String,
    pub address: String,
    pub payload: String,
    pub signature: String,
    pub pubkey: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProposalRequest {
    #[serde(flatten)]
    pub signed: SignedRequest,
    pub title: String,
    /// Required for new creates. Legacy `body_html` is read-only.
    pub body_sections: Option<serde_json::Value>,
    #[serde(default)]
    pub body_html: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CastVoteRequest {
    #[serde(flatten)]
    pub signed: SignedRequest,
    pub choice: String,
}

fn verify_signed(req: &SignedRequest, message: &str) -> VotingResult<()> {
    match req.chain.as_str() {
        "terra" => {
            let pubkey = req
                .pubkey
                .as_deref()
                .ok_or_else(|| VotingError::BadRequest("terra pubkey required".into()))?;
            verify_terra(
                &normalize_address(&req.address),
                message,
                &req.signature,
                pubkey,
            )
        }
        "bsc" => verify_eip191(&normalize_address(&req.address), message, &req.signature),
        other => Err(VotingError::BadRequest(format!(
            "unsupported chain {other}; use terra or bsc"
        ))),
    }
}

fn deny_cross_scheme(chain: &str, payload_chain: &str) -> VotingResult<()> {
    if chain != payload_chain {
        return Err(VotingError::Unauthorized(
            "rejecting cross-scheme signature (ADR-36 vs EIP-191)".into(),
        ));
    }
    Ok(())
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<SignedRequest>,
) -> VotingResult<Json<serde_json::Value>> {
    let payload = parse_and_validate(
        &req.payload,
        "register",
        &req.address,
        &state.cfg.terra_chain_id,
        &state.cfg.evm_chain_id,
    )?;
    deny_cross_scheme(&req.chain, &payload.chain)?;
    verify_signed(&req, &req.payload)?;
    if state.cfg.blacklist.contains(&req.address) {
        return Err(VotingError::Forbidden("address is blacklisted".into()));
    }
    let hash = hex::encode(Sha256::digest(req.payload.as_bytes()));
    let sig_id = db::insert_signature(
        &state.pool,
        &payload.chain,
        &req.address,
        req.pubkey.as_deref(),
        &req.signature,
        &hash,
        "register",
    )
    .await?;
    db::insert_registration_intent(&state.pool, &payload.chain, &req.address, sig_id).await?;
    let existing = db::ledger_registration(&state.pool, &payload.chain, &req.address).await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "signature_id": sig_id,
        "pending": existing.is_none(),
        "registration": existing.map(|r| serde_json::json!({
            "chain": r.chain,
            "address": r.wallet_address,
            "registered_at_height": r.registered_at_height,
            "status": r.status,
        })),
    })))
}

async fn registration(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> VotingResult<Json<serde_json::Value>> {
    let terra = db::ledger_registration(&state.pool, "terra", &addr).await?;
    let bsc = db::ledger_registration(&state.pool, "bsc", &addr).await?;
    let pending_terra = db::pending_intent(&state.pool, "terra", &addr).await?;
    let pending_bsc = db::pending_intent(&state.pool, "bsc", &addr).await?;
    if terra.is_none() && bsc.is_none() && !pending_terra && !pending_bsc {
        return Err(VotingError::NotFound("not registered".into()));
    }
    Ok(Json(serde_json::json!({
        "terra": terra,
        "bsc": bsc,
        "pending": { "terra": pending_terra, "bsc": pending_bsc },
    })))
}

async fn list_proposals(
    State(state): State<AppState>,
) -> VotingResult<Json<Vec<serde_json::Value>>> {
    let rows = db::list_proposals(&state.pool).await?;
    let mut out = Vec::new();
    for p in rows {
        let tallies = db::tally(&state.pool, p.id).await?;
        let summary = p
            .body_sections
            .as_ref()
            .and_then(|j| j.as_object())
            .and_then(summary_html);
        out.push(serde_json::json!({
            "id": p.id,
            "chain": p.chain,
            "proposer": p.proposer,
            "title": p.title,
            "terra_height": p.terra_height,
            "bsc_block": p.bsc_block,
            "created_at": p.created_at,
            "status": p.status,
            "tally": tallies,
            "summary": summary,
        }));
    }
    Ok(Json(out))
}

async fn create_proposal(
    State(state): State<AppState>,
    Json(req): Json<CreateProposalRequest>,
) -> VotingResult<(StatusCode, Json<serde_json::Value>)> {
    if req.title.len() > 200 {
        return Err(VotingError::BadRequest("proposal too large".into()));
    }
    if req.body_html.as_ref().is_some_and(|s| !s.is_empty()) {
        return Err(VotingError::BadRequest(
            "new proposals must use body_sections; body_html is read-only".into(),
        ));
    }
    let sections = req
        .body_sections
        .as_ref()
        .ok_or_else(|| VotingError::BadRequest("body_sections is required".into()))?;
    let prepared = prepare_sections(sections)?;
    if state.cfg.blacklist.contains(&req.signed.address) {
        return Err(VotingError::Forbidden("address is blacklisted".into()));
    }
    let payload = parse_and_validate(
        &req.signed.payload,
        "propose",
        &req.signed.address,
        &state.cfg.terra_chain_id,
        &state.cfg.evm_chain_id,
    )?;
    deny_cross_scheme(&req.signed.chain, &payload.chain)?;
    if payload.title.as_deref() != Some(req.title.as_str()) {
        return Err(VotingError::Unauthorized("signed title mismatch".into()));
    }
    if payload.body_hash.as_deref() != Some(prepared.body_hash.as_str()) {
        return Err(VotingError::Unauthorized(
            "signed body hash mismatch".into(),
        ));
    }
    verify_signed(&req.signed, &req.signed.payload)?;

    let reg = db::ledger_registration(&state.pool, &payload.chain, &req.signed.address)
        .await?
        .ok_or_else(|| VotingError::Forbidden("register before proposing".into()))?;
    let (terra_h, bsc_b) = db::tip_heights(&state.pool).await?;
    let (terra_freeze, bsc_freeze) =
        freeze_heights(&payload.chain, terra_h, bsc_b, reg.registered_at_height);
    let height = if payload.chain == "terra" {
        terra_freeze
    } else {
        bsc_freeze
    };
    let bal = db::balance_at(&state.pool, &payload.chain, &req.signed.address, height).await?;
    if bal < state.cfg.min_proposal_raw {
        return Err(VotingError::Forbidden(
            "need at least 1000 CL8Y on this address's chain to propose".into(),
        ));
    }

    let hash = hex::encode(Sha256::digest(req.signed.payload.as_bytes()));
    db::insert_signature(
        &state.pool,
        &payload.chain,
        &req.signed.address,
        req.signed.pubkey.as_deref(),
        &req.signed.signature,
        &hash,
        "propose",
    )
    .await?;
    let sections_json = serde_json::Value::Object(prepared.sanitized.clone());
    let id = db::insert_proposal(
        &state.pool,
        &payload.chain,
        &req.signed.address,
        &req.title,
        &prepared.body_html,
        &prepared.canonical_json,
        &sections_json,
        terra_freeze,
        bsc_freeze,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": id,
            "terra_height": terra_freeze,
            "bsc_block": bsc_freeze,
        })),
    ))
}

async fn get_proposal(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> VotingResult<Json<serde_json::Value>> {
    let p = db::get_proposal(&state.pool, id)
        .await?
        .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    let tallies = db::tally(&state.pool, id).await?;
    let sections = p.body_sections.map(|j| j.0);
    Ok(Json(serde_json::json!({
        "id": p.id,
        "chain": p.chain,
        "proposer": p.proposer,
        "title": p.title,
        "body_html": p.body_html,
        "body_sections": sections,
        "terra_height": p.terra_height,
        "bsc_block": p.bsc_block,
        "created_at": p.created_at,
        "status": p.status,
        "tally": tallies,
        "advisory": true,
    })))
}

async fn cast_vote(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CastVoteRequest>,
) -> VotingResult<Json<serde_json::Value>> {
    if !matches!(req.choice.as_str(), "for" | "against" | "abstain") {
        return Err(VotingError::BadRequest(
            "choice must be for, against, or abstain".into(),
        ));
    }
    if state.cfg.blacklist.contains(&req.signed.address) {
        return Err(VotingError::Forbidden("address is blacklisted".into()));
    }
    let payload = parse_and_validate(
        &req.signed.payload,
        "vote",
        &req.signed.address,
        &state.cfg.terra_chain_id,
        &state.cfg.evm_chain_id,
    )?;
    deny_cross_scheme(&req.signed.chain, &payload.chain)?;
    if payload.proposal_id.as_deref() != Some(&id.to_string()) {
        return Err(VotingError::Unauthorized(
            "vote is for a different proposal".into(),
        ));
    }
    if payload.choice.as_deref() != Some(req.choice.as_str()) {
        return Err(VotingError::Unauthorized("signed choice mismatch".into()));
    }
    verify_signed(&req.signed, &req.signed.payload)?;

    let proposal = db::get_proposal(&state.pool, id)
        .await?
        .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    let _reg = db::ledger_registration(&state.pool, &payload.chain, &req.signed.address)
        .await?
        .ok_or_else(|| VotingError::Forbidden("register before voting".into()))?;
    let weight = db::snapshot_weight(&state.pool, id, &payload.chain, &req.signed.address).await?;
    if weight <= 0.into() {
        return Err(VotingError::Forbidden(
            "no snapshot weight at proposal freeze".into(),
        ));
    }
    let hash = hex::encode(Sha256::digest(req.signed.payload.as_bytes()));
    let sig_id = db::insert_signature(
        &state.pool,
        &payload.chain,
        &req.signed.address,
        req.signed.pubkey.as_deref(),
        &req.signed.signature,
        &hash,
        "vote",
    )
    .await?;
    db::insert_vote(
        &state.pool,
        id,
        &payload.chain,
        &req.signed.address,
        &req.choice,
        &weight,
        sig_id,
    )
    .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "weight": weight.to_string(),
        "snapshot": { "terra_height": proposal.terra_height, "bsc_block": proposal.bsc_block },
    })))
}

async fn get_vote(
    State(state): State<AppState>,
    Path((id, addr)): Path<(Uuid, String)>,
    Query(q): Query<ChainQuery>,
) -> VotingResult<Json<serde_json::Value>> {
    let chain = resolve_chain(&addr, q.chain.as_deref())?.to_string();
    match db::get_vote(&state.pool, id, &chain, &addr).await? {
        Some((choice, weight)) => Ok(Json(
            serde_json::json!({ "choice": choice, "weight": weight }),
        )),
        None => Err(VotingError::NotFound("vote".into())),
    }
}

#[derive(Debug, Deserialize)]
struct ChainQuery {
    chain: Option<String>,
    height: Option<i64>,
}

async fn get_balance(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    Query(q): Query<ChainQuery>,
) -> VotingResult<Json<serde_json::Value>> {
    let chain = resolve_chain(&addr, q.chain.as_deref())?;
    let reg = db::ledger_registration(&state.pool, chain, &addr).await?;
    let pending = if reg.is_some() {
        false
    } else {
        db::pending_intent(&state.pool, chain, &addr).await?
    };
    let (terra_h, bsc_b) = db::tip_heights(&state.pool).await?;
    let tip = if chain == "terra" { terra_h } else { bsc_b };
    let as_of_height =
        resolve_balance_height(q.height, tip, reg.as_ref().map(|r| r.registered_at_height));
    let amount = db::balance_at(&state.pool, chain, &addr, as_of_height).await?;
    // OV-B6: always emit registered/pending/as_of_height/initial_balance (issue #14).
    Ok(Json(serde_json::json!({
        "chain": chain,
        "address": normalize_address(&addr),
        "height": as_of_height,
        "as_of_height": as_of_height,
        "balance": amount.to_string(),
        "registered": reg.is_some(),
        "pending": pending,
        "initial_balance": reg.as_ref().map(|r| r.initial_balance.clone()),
    })))
}
