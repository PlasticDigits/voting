use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::balance_query::{resolve_balance_height, resolve_chain};
use crate::blacklist::normalize_address;
use crate::config::{VotingConfig, MAX_COMMENTS_PER_WALLET_PER_PROPOSAL, MAX_COMMENT_BYTES};
use crate::crypto::{verify_eip191, verify_terra};
use crate::db;
use crate::error::{VotingError, VotingResult};
use crate::html::sanitize_proposal_html;
use crate::payload::{body_hash, parse_and_validate};
use crate::proposal_sections::{
    prepare_analysis, prepare_sections, render_analysis_html, summary_html, visible_len,
    AnalysisSections,
};
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
        .route("/v1/proposals", get(list_proposals).post(create_draft))
        .route("/v1/proposals/{id}", get(get_proposal))
        .route("/v1/proposals/{id}/sections", put(amend_sections))
        .route(
            "/v1/proposals/{id}/comments",
            get(list_comments).post(add_comment),
        )
        .route(
            "/v1/proposals/{id}/analysis",
            get(list_analysis).post(attach_analysis),
        )
        .route("/v1/proposals/{id}/open", post(open_vote))
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
            "/v1/proposals": { "get": { "summary": "List drafts and open polls" }, "post": { "summary": "Create a draft (purpose=draft)" } },
            "/v1/proposals/{id}/sections": { "put": { "summary": "Proposer amends draft sections" } },
            "/v1/proposals/{id}/comments": { "post": { "summary": "Signed review comment" } },
            "/v1/proposals/{id}/analysis": { "post": { "summary": "Committee independent analysis" } },
            "/v1/proposals/{id}/open": { "post": { "summary": "Committee opens voting and freezes the snapshot" } },
            "/v1/proposals/{id}/votes": { "post": { "summary": "Cast vote (status=open only)" } }
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
pub struct CreateDraftRequest {
    #[serde(flatten)]
    pub signed: SignedRequest,
    pub title: String,
    #[serde(alias = "sections")]
    pub body_sections: Option<serde_json::Value>,
    #[serde(default)]
    pub body_html: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AmendRequest {
    #[serde(flatten)]
    pub signed: SignedRequest,
    pub title: String,
    #[serde(alias = "sections")]
    pub body_sections: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct CommentRequest {
    #[serde(flatten)]
    pub signed: SignedRequest,
    pub body_html: String,
}

#[derive(Debug, Deserialize)]
pub struct AnalysisRequest {
    #[serde(flatten)]
    pub signed: SignedRequest,
    pub sections: AnalysisSections,
}

#[derive(Debug, Deserialize)]
pub struct OpenRequest {
    #[serde(flatten)]
    pub signed: SignedRequest,
    /// Ignored. Freeze heights come from indexer_state only.
    pub terra_height: Option<i64>,
    pub bsc_block: Option<i64>,
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

fn deny_blacklisted(cfg: &VotingConfig, address: &str) -> VotingResult<()> {
    if cfg.blacklist.contains(address) {
        return Err(VotingError::Forbidden("address is blacklisted".into()));
    }
    Ok(())
}

fn require_committee(cfg: &VotingConfig, address: &str) -> VotingResult<()> {
    if !cfg.committee.contains(address) {
        return Err(VotingError::Forbidden(
            "only VOTING_COMMITTEE_ADDRESSES may perform this action".into(),
        ));
    }
    Ok(())
}

async fn insert_purpose_signature(
    pool: &sqlx::PgPool,
    chain: &str,
    req: &SignedRequest,
    purpose: &str,
) -> VotingResult<Uuid> {
    let hash = hex::encode(Sha256::digest(req.payload.as_bytes()));
    db::insert_signature(
        pool,
        chain,
        &req.address,
        req.pubkey.as_deref(),
        &req.signature,
        &hash,
        purpose,
    )
    .await
}

fn proposal_summary(p: &db::ProposalRow) -> Option<String> {
    p.body_sections
        .as_ref()
        .and_then(|s| s.0.as_object())
        .and_then(summary_html)
}

fn proposal_list_json(p: &db::ProposalRow, tallies: Vec<db::TallyRow>) -> serde_json::Value {
    serde_json::json!({
        "id": p.id,
        "chain": p.chain,
        "proposer": p.proposer,
        "title": p.title,
        "summary": proposal_summary(p),
        "terra_height": p.terra_height,
        "bsc_block": p.bsc_block,
        "created_at": p.created_at,
        "opened_at": p.opened_at,
        "status": p.status,
        "tally": tallies,
    })
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
    deny_blacklisted(&state.cfg, &req.address)?;
    let sig_id = insert_purpose_signature(&state.pool, &payload.chain, &req, "register").await?;
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

#[derive(Debug, Deserialize)]
struct ListQuery {
    status: Option<String>,
}

async fn list_proposals(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> VotingResult<Json<Vec<serde_json::Value>>> {
    if let Some(status) = q.status.as_deref() {
        if status != "draft" && status != "open" {
            return Err(VotingError::BadRequest(
                "status must be draft or open".into(),
            ));
        }
    }
    let rows = db::list_proposals(&state.pool, q.status.as_deref()).await?;
    let mut out = Vec::new();
    for p in rows {
        let tallies = db::tally(&state.pool, p.id).await?;
        out.push(proposal_list_json(&p, tallies));
    }
    Ok(Json(out))
}

async fn create_draft(
    State(state): State<AppState>,
    Json(req): Json<CreateDraftRequest>,
) -> VotingResult<(StatusCode, Json<serde_json::Value>)> {
    if req.title.len() > 200 {
        return Err(VotingError::BadRequest("proposal too large".into()));
    }
    if req.body_html.as_ref().is_some_and(|html| !html.is_empty()) {
        return Err(VotingError::BadRequest(
            "new drafts must use body_sections; body_html is read-only".into(),
        ));
    }
    deny_blacklisted(&state.cfg, &req.signed.address)?;
    let sections = req
        .body_sections
        .as_ref()
        .ok_or_else(|| VotingError::BadRequest("body_sections is required".into()))?;
    let prepared = prepare_sections(sections)?;
    let payload = parse_and_validate(
        &req.signed.payload,
        "draft",
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
        .ok_or_else(|| VotingError::Forbidden("register before drafting".into()))?;
    let (terra_h, bsc_b) = db::tip_heights(&state.pool).await?;
    let tip = if payload.chain == "terra" {
        terra_h
    } else {
        bsc_b
    };
    let height = resolve_balance_height(None, tip, Some(reg.registered_at_height));
    let bal = db::balance_at(&state.pool, &payload.chain, &req.signed.address, height).await?;
    if bal < state.cfg.min_proposal_raw {
        return Err(VotingError::Forbidden(
            "need at least 1000 CL8Y on this address's chain to start a draft".into(),
        ));
    }

    insert_purpose_signature(&state.pool, &payload.chain, &req.signed, "draft").await?;
    let sections = serde_json::Value::Object(prepared.sanitized);
    let id = db::insert_draft(
        &state.pool,
        &payload.chain,
        &req.signed.address,
        &req.title,
        &prepared.body_html,
        &prepared.canonical_json,
        &sections,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": id,
            "status": "draft",
            "terra_height": serde_json::Value::Null,
            "bsc_block": serde_json::Value::Null,
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
    let comments = db::list_comments(&state.pool, id).await?;
    let analysis = db::list_analysis(&state.pool, id).await?;
    Ok(Json(serde_json::json!({
        "id": p.id,
        "chain": p.chain,
        "proposer": p.proposer,
        "title": p.title,
        "body_html": p.body_html,
        "body_sections": p.body_sections.as_ref().map(|s| &s.0),
        "summary": proposal_summary(&p),
        "terra_height": p.terra_height,
        "bsc_block": p.bsc_block,
        "created_at": p.created_at,
        "opened_at": p.opened_at,
        "status": p.status,
        "tally": tallies,
        "comments": comments,
        "analysis": analysis.iter().map(|a| serde_json::json!({
            "id": a.id,
            "chain": a.chain,
            "wallet_address": a.wallet_address,
            "body_html": a.body_html,
            "source": a.source,
            "created_at": a.created_at,
            "sections": a.sections.0,
            "machine_generated": a.source == "ai",
        })).collect::<Vec<_>>(),
        "advisory": true,
    })))
}

async fn amend_sections(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<AmendRequest>,
) -> VotingResult<Json<serde_json::Value>> {
    if req.title.len() > 200 {
        return Err(VotingError::BadRequest("proposal too large".into()));
    }
    deny_blacklisted(&state.cfg, &req.signed.address)?;
    let proposal = db::get_proposal(&state.pool, id)
        .await?
        .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    if proposal.status != "draft" {
        return Err(VotingError::Conflict(
            "cannot amend after votes are open".into(),
        ));
    }
    if normalize_address(&req.signed.address) != proposal.proposer {
        return Err(VotingError::Forbidden(
            "only the proposer may amend sections".into(),
        ));
    }
    let current_hash = match &proposal.body_sections {
        Some(s) => prepare_sections(&s.0)?.body_hash,
        None => body_hash(&proposal.body_html),
    };
    let prepared = prepare_sections(&req.body_sections)?;
    let new_hash = prepared.body_hash.clone();
    let payload = parse_and_validate(
        &req.signed.payload,
        "amend",
        &req.signed.address,
        &state.cfg.terra_chain_id,
        &state.cfg.evm_chain_id,
    )?;
    deny_cross_scheme(&req.signed.chain, &payload.chain)?;
    if payload.proposal_id.as_deref() != Some(&id.to_string()) {
        return Err(VotingError::Unauthorized(
            "amend is for a different proposal".into(),
        ));
    }
    if payload.title.as_deref() != Some(req.title.as_str()) {
        return Err(VotingError::Unauthorized("signed title mismatch".into()));
    }
    if payload.body_hash.as_deref() != Some(new_hash.as_str()) {
        return Err(VotingError::Unauthorized(
            "signed body hash mismatch".into(),
        ));
    }
    if payload.prev_body_hash.as_deref() != Some(current_hash.as_str()) {
        return Err(VotingError::Conflict(
            "stale prev_body_hash; reload and re-sign".into(),
        ));
    }
    verify_signed(&req.signed, &req.signed.payload)?;
    insert_purpose_signature(&state.pool, &payload.chain, &req.signed, "amend").await?;
    let sections = serde_json::Value::Object(prepared.sanitized);
    db::update_draft_sections(
        &state.pool,
        id,
        &req.title,
        &prepared.body_html,
        &prepared.canonical_json,
        &sections,
    )
    .await?;
    Ok(Json(
        serde_json::json!({ "ok": true, "body_hash": new_hash }),
    ))
}

async fn add_comment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CommentRequest>,
) -> VotingResult<(StatusCode, Json<serde_json::Value>)> {
    if req.body_html.len() > MAX_COMMENT_BYTES {
        return Err(VotingError::BadRequest("comment too large".into()));
    }
    deny_blacklisted(&state.cfg, &req.signed.address)?;
    let proposal = db::get_proposal(&state.pool, id)
        .await?
        .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    if proposal.status != "draft" && proposal.status != "open" {
        return Err(VotingError::Forbidden("comments are closed".into()));
    }
    let sanitized = sanitize_proposal_html(&req.body_html);
    if visible_len(&sanitized) == 0 {
        return Err(VotingError::BadRequest("comment is empty".into()));
    }
    let expected_hash = body_hash(&req.body_html);
    let payload = parse_and_validate(
        &req.signed.payload,
        "comment",
        &req.signed.address,
        &state.cfg.terra_chain_id,
        &state.cfg.evm_chain_id,
    )?;
    deny_cross_scheme(&req.signed.chain, &payload.chain)?;
    if payload.proposal_id.as_deref() != Some(&id.to_string()) {
        return Err(VotingError::Unauthorized(
            "comment is for a different proposal".into(),
        ));
    }
    if payload.body_hash.as_deref() != Some(expected_hash.as_str()) {
        return Err(VotingError::Unauthorized(
            "signed body hash mismatch".into(),
        ));
    }
    verify_signed(&req.signed, &req.signed.payload)?;
    db::ledger_registration(&state.pool, &payload.chain, &req.signed.address)
        .await?
        .ok_or_else(|| VotingError::Forbidden("register before commenting".into()))?;
    let sig_id =
        insert_purpose_signature(&state.pool, &payload.chain, &req.signed, "comment").await?;
    let comment_id = db::insert_comment(
        &state.pool,
        id,
        &payload.chain,
        &req.signed.address,
        &sanitized,
        sig_id,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": comment_id,
            "max_per_wallet": MAX_COMMENTS_PER_WALLET_PER_PROPOSAL,
        })),
    ))
}

async fn list_comments(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> VotingResult<Json<Vec<db::CommentRow>>> {
    db::get_proposal(&state.pool, id)
        .await?
        .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    Ok(Json(db::list_comments(&state.pool, id).await?))
}

async fn attach_analysis(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<AnalysisRequest>,
) -> VotingResult<(StatusCode, Json<serde_json::Value>)> {
    deny_blacklisted(&state.cfg, &req.signed.address)?;
    require_committee(&state.cfg, &req.signed.address)?;
    let proposal = db::get_proposal(&state.pool, id)
        .await?
        .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    if proposal.status != "draft" {
        return Err(VotingError::Conflict(
            "analysis is append-only before open".into(),
        ));
    }
    if normalize_address(&req.signed.address) == proposal.proposer {
        return Err(VotingError::Forbidden(
            "proposer cannot attach independent analysis".into(),
        ));
    }
    let (sanitized, _canonical, expected_hash) = prepare_analysis(&req.sections)?;
    let payload = parse_and_validate(
        &req.signed.payload,
        "analyze",
        &req.signed.address,
        &state.cfg.terra_chain_id,
        &state.cfg.evm_chain_id,
    )?;
    deny_cross_scheme(&req.signed.chain, &payload.chain)?;
    if payload.proposal_id.as_deref() != Some(&id.to_string()) {
        return Err(VotingError::Unauthorized(
            "analysis is for a different proposal".into(),
        ));
    }
    if payload.body_hash.as_deref() != Some(expected_hash.as_str()) {
        return Err(VotingError::Unauthorized(
            "signed body hash mismatch".into(),
        ));
    }
    verify_signed(&req.signed, &req.signed.payload)?;
    let sig_id =
        insert_purpose_signature(&state.pool, &payload.chain, &req.signed, "analyze").await?;
    let analysis_id = db::insert_analysis(
        &state.pool,
        id,
        &payload.chain,
        &req.signed.address,
        &render_analysis_html(&sanitized),
        &sanitized,
        sig_id,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": analysis_id, "source": "committee" })),
    ))
}

async fn list_analysis(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> VotingResult<Json<Vec<serde_json::Value>>> {
    db::get_proposal(&state.pool, id)
        .await?
        .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    let rows = db::list_analysis(&state.pool, id).await?;
    Ok(Json(
        rows.into_iter()
            .map(|a| {
                serde_json::json!({
                    "id": a.id,
                    "chain": a.chain,
                    "wallet_address": a.wallet_address,
                    "body_html": a.body_html,
                    "source": a.source,
                    "created_at": a.created_at,
                    "sections": a.sections.0,
                    "machine_generated": a.source == "ai",
                })
            })
            .collect(),
    ))
}

async fn open_vote(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<OpenRequest>,
) -> VotingResult<Json<serde_json::Value>> {
    deny_blacklisted(&state.cfg, &req.signed.address)?;
    require_committee(&state.cfg, &req.signed.address)?;
    let _ignored_client_heights = (req.terra_height, req.bsc_block);
    let proposal = db::get_proposal(&state.pool, id)
        .await?
        .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    if proposal.status != "draft" {
        return Err(VotingError::Conflict(
            "proposal is not a draft (already open?)".into(),
        ));
    }
    let sections = proposal
        .body_sections
        .as_ref()
        .map(|s| s.0.clone())
        .ok_or_else(|| {
            VotingError::BadRequest("legacy proposal has no template sections".into())
        })?;
    let current_hash = prepare_sections(&sections)?.body_hash;
    let payload = parse_and_validate(
        &req.signed.payload,
        "open_vote",
        &req.signed.address,
        &state.cfg.terra_chain_id,
        &state.cfg.evm_chain_id,
    )?;
    deny_cross_scheme(&req.signed.chain, &payload.chain)?;
    if payload.proposal_id.as_deref() != Some(&id.to_string()) {
        return Err(VotingError::Unauthorized(
            "open is for a different proposal".into(),
        ));
    }
    if payload.body_hash.as_deref() != Some(current_hash.as_str()) {
        return Err(VotingError::Unauthorized(
            "open_vote must sign the current section hash".into(),
        ));
    }
    verify_signed(&req.signed, &req.signed.payload)?;
    let (terra_h, bsc_b) = db::tip_heights(&state.pool).await?;
    insert_purpose_signature(&state.pool, &payload.chain, &req.signed, "open_vote").await?;
    db::open_proposal(&state.pool, id, terra_h, bsc_b).await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "status": "open",
        "terra_height": terra_h,
        "bsc_block": bsc_b,
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
    deny_blacklisted(&state.cfg, &req.signed.address)?;
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
    if proposal.status != "open" {
        return Err(VotingError::Forbidden(
            "votes are closed until the poll is opened".into(),
        ));
    }
    let _reg = db::ledger_registration(&state.pool, &payload.chain, &req.signed.address)
        .await?
        .ok_or_else(|| VotingError::Forbidden("register before voting".into()))?;
    let weight = db::snapshot_weight(&state.pool, id, &payload.chain, &req.signed.address).await?;
    if weight <= 0.into() {
        return Err(VotingError::Forbidden(
            "no snapshot weight at proposal freeze".into(),
        ));
    }
    let sig_id = insert_purpose_signature(&state.pool, &payload.chain, &req.signed, "vote").await?;
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
