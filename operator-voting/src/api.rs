use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::balance_query::{resolve_balance_height, resolve_chain};
use crate::blacklist::normalize_address;
use crate::config::{VotingConfig, MAX_COMMENTS_PER_WALLET_PER_PROPOSAL, MAX_COMMENT_BYTES};
use crate::crypto::{verify_eip191, verify_terra};
use crate::db;
use crate::error::{VotingError, VotingResult};
use crate::html::sanitize_proposal_html;
use crate::payload::{body_hash, SignedPayload};
use crate::proposal_sections::{
    prepare_analysis, prepare_sections, render_analysis_html, summary_html, visible_len,
    AnalysisSections,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub cfg: VotingConfig,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(readiness))
        .route("/openapi.json", get(openapi))
        .route("/v1/receipts/{id}", get(get_receipt))
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
        .route("/v1/proposals/{id}/weights/{addr}", get(get_frozen_weight))
        .route("/v1/balances/{addr}", get(get_balance))
        .with_state(state)
}

#[derive(Serialize, utoipa::ToSchema)]
struct Health {
    ok: bool,
    service: &'static str,
}

#[utoipa::path(get,path="/health",responses((status=200,description="Process liveness",body=Health)))]
async fn health() -> Json<Health> {
    Json(Health {
        ok: true,
        service: "operator-voting",
    })
}

#[utoipa::path(get,path="/openapi.json",responses((status=200,description="Generated OpenAPI document",body=Object)))]
async fn openapi() -> Json<serde_json::Value> {
    Json(crate::contracts::openapi())
}

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        openapi,
        health,
        readiness,
        register,
        registration,
        list_proposals,
        create_draft,
        get_proposal,
        amend_sections,
        add_comment,
        list_comments,
        attach_analysis,
        list_analysis,
        open_vote,
        cast_vote,
        get_vote,
        get_frozen_weight,
        get_balance,
        get_receipt
    ),
    components(schemas(
        SignedPayload,
        SignedRequest,
        CreateDraftRequest,
        AmendRequest,
        CommentRequest,
        AnalysisRequest,
        OpenRequest,
        CastVoteRequest
    ))
)]
pub struct ApiDoc;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SignedRequest {
    #[schema(value_type=crate::dto::VotingChain)]
    pub chain: String,
    pub address: String,
    pub payload: String,
    pub signature: String,
    pub pubkey: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateDraftRequest {
    #[serde(flatten)]
    pub signed: SignedRequest,
    pub title: String,
    #[serde(alias = "sections")]
    #[schema(value_type=Option<crate::proposal_sections::ProposalSections>)]
    pub body_sections: Option<serde_json::Value>,
    #[serde(default)]
    pub body_html: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AmendRequest {
    #[serde(flatten)]
    pub signed: SignedRequest,
    pub title: String,
    #[serde(alias = "sections")]
    #[schema(value_type=crate::proposal_sections::ProposalSections)]
    pub body_sections: serde_json::Value,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CommentRequest {
    #[serde(flatten)]
    pub signed: SignedRequest,
    pub body_html: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AnalysisRequest {
    #[serde(flatten)]
    pub signed: SignedRequest,
    pub sections: AnalysisSections,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct OpenRequest {
    #[serde(flatten)]
    pub signed: SignedRequest,
    /// Ignored. Freeze heights come from indexer_state only.
    pub terra_height: Option<i64>,
    pub bsc_block: Option<i64>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CastVoteRequest {
    #[serde(flatten)]
    pub signed: SignedRequest,
    #[schema(value_type=crate::dto::VoteChoice)]
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

/// A nonce lock serializes retries; proposal row locks serialize lifecycle changes.
/// Signature, mutation, revision history and receipt commit or roll back together.
async fn begin_action<'a>(
    state: &'a AppState,
    req: &SignedRequest,
    purpose: &str,
    proposal_id: Option<Uuid>,
) -> VotingResult<(
    sqlx::Transaction<'a, sqlx::Postgres>,
    Uuid,
    Option<serde_json::Value>,
    SignedPayload,
)> {
    let payload = crate::payload::parse_domain(
        &req.payload,
        purpose,
        &req.address,
        &state.cfg.terra_chain_id,
        &state.cfg.evm_chain_id,
    )?;
    if let Some(target) = proposal_id {
        if payload.proposal_id.as_deref() != Some(target.to_string().as_str()) {
            return Err(VotingError::Unauthorized(
                "signed action targets a different proposal".into(),
            ));
        }
    }
    deny_cross_scheme(&req.chain, &payload.chain)?;
    verify_signed(req, &req.payload)?;
    deny_blacklisted(&state.cfg, &req.address)?;
    if matches!(purpose, "analyze" | "open_vote") {
        require_committee(&state.cfg, &req.address)?;
    }
    if payload.version != 2 {
        return Err(VotingError::BadRequest(
            "payload version 2 required; reload and re-sign".into(),
        ));
    }
    let nonce = payload
        .nonce
        .as_deref()
        .filter(|n| !n.is_empty() && n.len() <= 128)
        .ok_or_else(|| VotingError::BadRequest("bounded unique nonce required".into()))?;
    let hash = body_hash(&req.payload);
    let mut tx = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await?;
    if matches!(purpose, "open_vote" | "vote") {
        sqlx::query("SELECT pg_advisory_xact_lock(8136,1),pg_advisory_xact_lock(8136,2)")
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "{}:{}:{}",
            req.chain,
            normalize_address(&req.address),
            nonce
        ))
        .execute(&mut *tx)
        .await?;
    let prior: Option<(Uuid,String,serde_json::Value)> = sqlx::query_as("SELECT s.id,s.payload_hash,r.response FROM voting.signatures s JOIN voting.action_receipts r ON r.signature_id=s.id WHERE s.chain=$1 AND s.wallet_address=$2 AND s.nonce=$3")
        .bind(&req.chain).bind(normalize_address(&req.address)).bind(nonce).fetch_optional(&mut *tx).await?;
    if let Some((id, old_hash, response)) = prior {
        if old_hash != hash {
            return Err(VotingError::Conflict(
                "nonce already consumed by different signed bytes".into(),
            ));
        }
        return Ok((tx, id, Some(response), payload));
    }
    if let Some(id) = proposal_id {
        db::get_proposal(&mut tx, id)
            .await?
            .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    }
    crate::payload::validate_time(&payload, chrono::Utc::now().timestamp())?;
    db::enforce_verified_action_quota(
        &mut tx,
        &payload.chain,
        &req.address,
        purpose,
        state.cfg.rate_limit_post_per_minute,
    )
    .await?;
    crate::payload::validate_time(&payload, chrono::Utc::now().timestamp())?;
    let sig_id = Uuid::new_v4();
    sqlx::query("INSERT INTO voting.signatures(id,chain,wallet_address,pubkey,signature,payload_hash,purpose,signed_payload,scheme,domain,nonce,proposal_id,revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
        .bind(sig_id).bind(&req.chain).bind(normalize_address(&req.address)).bind(&req.pubkey).bind(&req.signature).bind(hash).bind(purpose)
        .bind(&req.payload).bind(if req.chain == "terra" { "ADR-36" } else { "EIP-191" }).bind(format!("{}:{}",payload.app,payload.chain_id)).bind(nonce).bind(proposal_id).bind(payload.revision).execute(&mut *tx).await?;
    Ok((tx, sig_id, None, payload))
}

async fn finish_action(
    mut tx: sqlx::Transaction<'_, sqlx::Postgres>,
    signature_id: Uuid,
    proposal_id: Option<Uuid>,
    mut response: serde_json::Value,
) -> VotingResult<serde_json::Value> {
    let accepted_at = chrono::Utc::now();
    let raw: String =
        sqlx::query_scalar("SELECT signed_payload FROM voting.signatures WHERE id=$1")
            .bind(signature_id)
            .fetch_one(&mut *tx)
            .await?;
    let payload: SignedPayload = serde_json::from_str(&raw)
        .map_err(|_| VotingError::Unauthorized("stored action payload invalid".into()))?;
    crate::payload::validate_time(&payload, accepted_at.timestamp())?;
    let revision: Option<i64> = if let Some(id) = proposal_id {
        sqlx::query_scalar("SELECT revision FROM voting.proposals WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
    } else {
        None
    };
    let content = response
        .as_object_mut()
        .and_then(|o| o.remove("receipt_content"));
    response["receipt_id"] = serde_json::json!(signature_id);
    sqlx::query("INSERT INTO voting.action_receipts(signature_id,proposal_id,revision,response,content,accepted_at) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(signature_id).bind(proposal_id).bind(revision).bind(&response).bind(content).bind(accepted_at).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(response)
}

fn require_revision(payload: &SignedPayload, proposal: &db::ProposalRow) -> VotingResult<()> {
    if payload.revision != Some(proposal.revision) {
        return Err(VotingError::Conflict(
            "stale or missing revision; reload and re-sign".into(),
        ));
    }
    Ok(())
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
        "revision": p.revision,
        "snapshot_validity": p.snapshot_validity,
        "snapshot_incident": p.snapshot_incident,
        "summary": proposal_summary(p),
        "terra_height": p.terra_height,
        "bsc_block": p.bsc_block,
        "created_at": p.created_at,
        "opened_at": p.opened_at,
        "status": p.status,
        "tally": tallies,
    })
}

#[utoipa::path(post, path = "/v1/register", request_body = SignedRequest, responses((status = 200, description = "Accepted response, mutation responses include immutable receipt_id", body = crate::dto::RegistrationResponse),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn register(
    State(state): State<AppState>,
    Json(req): Json<SignedRequest>,
) -> VotingResult<Json<serde_json::Value>> {
    let (mut conn, sig_id, prior, payload) = begin_action(&state, &req, "register", None).await?;
    if let Some(prior) = prior {
        return Ok(Json(prior));
    }

    deny_cross_scheme(&req.chain, &payload.chain)?;
    deny_blacklisted(&state.cfg, &req.address)?;

    db::insert_registration_intent(&mut conn, &payload.chain, &req.address, sig_id).await?;
    let existing = db::ledger_registration(&mut conn, &payload.chain, &req.address).await?;
    Ok(Json(
        finish_action(
            conn,
            sig_id,
            None,
            serde_json::json!({
                "ok": true,
                "signature_id": sig_id,
                "pending": existing.is_none(),
                "registration": existing.map(|r| serde_json::json!({
                    "chain": r.chain,
                    "address": r.wallet_address,
                    "registered_at_height": r.registered_at_height,
                    "status": r.status,
                })),
            }),
        )
        .await?,
    ))
}

#[utoipa::path(get, path = "/v1/registration/{addr}", params(("addr" = String, Path, description = "Terra or EVM address")), responses((status = 200, description = "Accepted response, mutation responses include immutable receipt_id", body = crate::dto::RegistrationLookup),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn registration(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> VotingResult<Json<serde_json::Value>> {
    let mut conn = state.pool.acquire().await?;

    let terra = db::ledger_registration(&mut conn, "terra", &addr).await?;
    let bsc = db::ledger_registration(&mut conn, "bsc", &addr).await?;
    let pending_terra = db::pending_intent(&mut conn, "terra", &addr).await?;
    let pending_bsc = db::pending_intent(&mut conn, "bsc", &addr).await?;
    if terra.is_none() && bsc.is_none() && !pending_terra && !pending_bsc {
        return Err(VotingError::NotFound("not registered".into()));
    }
    Ok(Json(serde_json::json!({
        "terra": terra,
        "bsc": bsc,
        "pending": { "terra": pending_terra, "bsc": pending_bsc },
    })))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct ListQuery {
    status: Option<String>,
}

#[utoipa::path(get, path = "/v1/proposals", params(("status" = Option<String>, Query, description = "draft or open")), responses((status = 200, description = "Accepted response, mutation responses include immutable receipt_id", body = Vec<crate::dto::ProposalListItem>),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn list_proposals(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> VotingResult<Json<Vec<serde_json::Value>>> {
    let mut conn = state.pool.acquire().await?;

    if let Some(status) = q.status.as_deref() {
        if status != "draft" && status != "open" {
            return Err(VotingError::BadRequest(
                "status must be draft or open".into(),
            ));
        }
    }
    let rows = db::list_proposals(&mut conn, q.status.as_deref()).await?;
    let mut out = Vec::new();
    for p in rows {
        let tallies = db::tally(&mut conn, p.id).await?;
        out.push(proposal_list_json(&p, tallies));
    }
    Ok(Json(out))
}

#[utoipa::path(post, path = "/v1/proposals", request_body = CreateDraftRequest, responses((status = 201, description = "Accepted response, mutation responses include immutable receipt_id", body = crate::dto::DraftResponse),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn create_draft(
    State(state): State<AppState>,
    Json(req): Json<CreateDraftRequest>,
) -> VotingResult<(StatusCode, Json<serde_json::Value>)> {
    let (mut conn, sig_id, prior, payload) =
        begin_action(&state, &req.signed, "draft", None).await?;
    if let Some(prior) = prior {
        return Ok((StatusCode::CREATED, Json(prior)));
    }

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
    deny_cross_scheme(&req.signed.chain, &payload.chain)?;
    if payload.title.as_deref() != Some(req.title.as_str()) {
        return Err(VotingError::Unauthorized("signed title mismatch".into()));
    }
    if payload.body_hash.as_deref() != Some(prepared.body_hash.as_str()) {
        return Err(VotingError::Unauthorized(
            "signed body hash mismatch".into(),
        ));
    }

    let reg = db::ledger_registration(&mut conn, &payload.chain, &req.signed.address)
        .await?
        .ok_or_else(|| VotingError::Forbidden("register before drafting".into()))?;
    let (terra_h, bsc_b) = db::tip_heights(&mut conn).await?;
    let tip = if payload.chain == "terra" {
        terra_h
    } else {
        bsc_b
    };
    let height = resolve_balance_height(None, tip, Some(reg.registered_at_height));
    let bal = db::balance_at(&mut conn, &payload.chain, &req.signed.address, height).await?;
    if bal < state.cfg.min_proposal_raw {
        return Err(VotingError::Forbidden(
            "need at least 1000 CL8Y on this address's chain to start a draft".into(),
        ));
    }

    let sections = serde_json::Value::Object(prepared.sanitized);
    let id = db::insert_draft(
        &mut conn,
        &payload.chain,
        &req.signed.address,
        &req.title,
        &prepared.body_html,
        &prepared.canonical_json,
        &sections,
    )
    .await?;
    db::record_revision(&mut conn, id, sig_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(
            finish_action(
                conn,
                sig_id,
                Some(id),
                serde_json::json!({
                    "id": id,
                    "status": "draft",
                    "revision": 1,
                    "terra_height": serde_json::Value::Null,
                    "bsc_block": serde_json::Value::Null,
                }),
            )
            .await?,
        ),
    ))
}

#[utoipa::path(get, path = "/v1/proposals/{id}", params(("id" = String, Path, description = "UUID")), responses((status = 200, description = "Accepted response, mutation responses include immutable receipt_id", body = crate::dto::ProposalDetail),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn get_proposal(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> VotingResult<Json<serde_json::Value>> {
    let mut conn = state.pool.acquire().await?;

    let p = db::get_proposal(&mut conn, id)
        .await?
        .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    let tallies = db::tally(&mut conn, id).await?;
    let comments = db::list_comments(&mut conn, id).await?;
    let analysis = db::list_analysis(&mut conn, id).await?;
    Ok(Json(serde_json::json!({
        "id": p.id,
        "chain": p.chain,
        "proposer": p.proposer,
        "title": p.title,
        "revision": p.revision,
        "snapshot_validity": p.snapshot_validity,
        "snapshot_incident": p.snapshot_incident,
        "body_html": p.body_html,
        "body_sections": p.body_sections.as_ref().map(|s| &s.0),
        "body_hash": p.body_sections.as_ref().and_then(|s| prepare_sections(&s.0).ok()).map(|s| s.body_hash),
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
            "revision": a.revision,
            "created_at": a.created_at,
            "sections": a.sections.0,
            "machine_generated": a.source == "ai",
        })).collect::<Vec<_>>(),
        "advisory": true,
    })))
}

#[utoipa::path(put, path = "/v1/proposals/{id}/sections", request_body = AmendRequest, params(("id" = String, Path, description = "UUID")), responses((status = 200, description = "Accepted response, mutation responses include immutable receipt_id", body = crate::dto::AmendResponse),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn amend_sections(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<AmendRequest>,
) -> VotingResult<Json<serde_json::Value>> {
    let (mut conn, sig_id, prior, payload) =
        begin_action(&state, &req.signed, "amend", Some(id)).await?;
    if let Some(prior) = prior {
        return Ok(Json(prior));
    }

    if req.title.len() > 200 {
        return Err(VotingError::BadRequest("proposal too large".into()));
    }
    deny_blacklisted(&state.cfg, &req.signed.address)?;
    let proposal = db::get_proposal(&mut conn, id)
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
    require_revision(&payload, &proposal)?;

    let sections = serde_json::Value::Object(prepared.sanitized);
    db::update_draft_sections(
        &mut conn,
        id,
        &req.title,
        &prepared.body_html,
        &prepared.canonical_json,
        &sections,
    )
    .await?;
    db::record_revision(&mut conn, id, sig_id).await?;
    Ok(Json(
        finish_action(conn, sig_id, Some(id), serde_json::json!({ "ok": true, "body_hash": new_hash, "revision": proposal.revision + 1 })).await?,
    ))
}

#[utoipa::path(post, path = "/v1/proposals/{id}/comments", request_body = CommentRequest, params(("id" = String, Path, description = "UUID")), responses((status = 201, description = "Accepted response, mutation responses include immutable receipt_id", body = crate::dto::CommentResponse),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn add_comment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CommentRequest>,
) -> VotingResult<(StatusCode, Json<serde_json::Value>)> {
    let (mut conn, sig_id, prior, payload) =
        begin_action(&state, &req.signed, "comment", Some(id)).await?;
    if let Some(prior) = prior {
        return Ok((StatusCode::CREATED, Json(prior)));
    }

    if req.body_html.len() > MAX_COMMENT_BYTES {
        return Err(VotingError::BadRequest("comment too large".into()));
    }
    deny_blacklisted(&state.cfg, &req.signed.address)?;
    let proposal = db::get_proposal(&mut conn, id)
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
    db::ledger_registration(&mut conn, &payload.chain, &req.signed.address)
        .await?
        .ok_or_else(|| VotingError::Forbidden("register before commenting".into()))?;

    let comment_id = db::insert_comment(
        &mut conn,
        id,
        &payload.chain,
        &req.signed.address,
        &sanitized,
        sig_id,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(
            finish_action(
                conn,
                sig_id,
                Some(id),
                serde_json::json!({
                        "id": comment_id,
                        "max_per_wallet": MAX_COMMENTS_PER_WALLET_PER_PROPOSAL,
                "receipt_content": { "id":comment_id, "signed_body_html": req.body_html, "body_html": sanitized },
                    }),
            )
            .await?,
        ),
    ))
}

#[utoipa::path(get, path = "/v1/proposals/{id}/comments", params(("id" = String, Path, description = "UUID")), responses((status = 200, description = "Accepted response, mutation responses include immutable receipt_id", body = Vec<crate::dto::Comment>),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn list_comments(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> VotingResult<Json<Vec<db::CommentRow>>> {
    let mut conn = state.pool.acquire().await?;

    db::get_proposal(&mut conn, id)
        .await?
        .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    Ok(Json(db::list_comments(&mut conn, id).await?))
}

#[utoipa::path(post, path = "/v1/proposals/{id}/analysis", request_body = AnalysisRequest, params(("id" = String, Path, description = "UUID")), responses((status = 201, description = "Accepted response, mutation responses include immutable receipt_id", body = crate::dto::AnalysisResponse),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn attach_analysis(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<AnalysisRequest>,
) -> VotingResult<(StatusCode, Json<serde_json::Value>)> {
    let (mut conn, sig_id, prior, payload) =
        begin_action(&state, &req.signed, "analyze", Some(id)).await?;
    if let Some(prior) = prior {
        return Ok((StatusCode::CREATED, Json(prior)));
    }

    deny_blacklisted(&state.cfg, &req.signed.address)?;
    require_committee(&state.cfg, &req.signed.address)?;
    let proposal = db::get_proposal(&mut conn, id)
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
    require_revision(&payload, &proposal)?;

    let analysis_id = db::insert_analysis(
        &mut conn,
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
        Json(
            finish_action(
                conn,
                sig_id,
                Some(id),
                serde_json::json!({ "id": analysis_id, "source": "committee", "receipt_content": {"id":analysis_id,"sections": sanitized, "body_html": render_analysis_html(&sanitized)} }),
            )
            .await?,
        ),
    ))
}

#[utoipa::path(get, path = "/v1/proposals/{id}/analysis", params(("id" = String, Path, description = "UUID")), responses((status = 200, description = "Accepted response, mutation responses include immutable receipt_id", body = Vec<crate::dto::Analysis>),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn list_analysis(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> VotingResult<Json<Vec<serde_json::Value>>> {
    let mut conn = state.pool.acquire().await?;

    db::get_proposal(&mut conn, id)
        .await?
        .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    let rows = db::list_analysis(&mut conn, id).await?;
    Ok(Json(
        rows.into_iter()
            .map(|a| {
                serde_json::json!({
                        "id": a.id,
                        "chain": a.chain,
                        "wallet_address": a.wallet_address,
                        "body_html": a.body_html,
                        "source": a.source,
                "revision": a.revision,
                        "created_at": a.created_at,
                        "sections": a.sections.0,
                        "machine_generated": a.source == "ai",
                    })
            })
            .collect(),
    ))
}

#[utoipa::path(post, path = "/v1/proposals/{id}/open", request_body = OpenRequest, params(("id" = String, Path, description = "UUID")), responses((status = 200, description = "Accepted response, mutation responses include immutable receipt_id", body = crate::dto::OpenResponse),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn open_vote(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<OpenRequest>,
) -> VotingResult<Json<serde_json::Value>> {
    let (mut conn, sig_id, prior, payload) =
        begin_action(&state, &req.signed, "open_vote", Some(id)).await?;
    if let Some(prior) = prior {
        return Ok(Json(prior));
    }

    deny_blacklisted(&state.cfg, &req.signed.address)?;
    require_committee(&state.cfg, &req.signed.address)?;
    let _ignored_client_heights = (req.terra_height, req.bsc_block);
    let proposal = db::get_proposal(&mut conn, id)
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
    if payload.title.as_deref() != Some(proposal.title.as_str()) {
        return Err(VotingError::Unauthorized(
            "signed open title mismatch".into(),
        ));
    }
    require_revision(&payload, &proposal)?;
    let (terra_h, bsc_b) = db::open_proposal(&mut conn, id, sig_id).await?;
    Ok(Json(
        finish_action(
            conn,
            sig_id,
            Some(id),
            serde_json::json!({
                "ok": true,
                "status": "open",
                "terra_height": terra_h,
                "bsc_block": bsc_b,
            }),
        )
        .await?,
    ))
}

#[utoipa::path(post, path = "/v1/proposals/{id}/votes", request_body = CastVoteRequest, params(("id" = String, Path, description = "UUID")), responses((status = 200, description = "Accepted response, mutation responses include immutable receipt_id", body = crate::dto::VoteResponse),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn cast_vote(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CastVoteRequest>,
) -> VotingResult<Json<serde_json::Value>> {
    let (mut conn, sig_id, prior, payload) =
        begin_action(&state, &req.signed, "vote", Some(id)).await?;
    if let Some(prior) = prior {
        return Ok(Json(prior));
    }

    if !matches!(req.choice.as_str(), "for" | "against" | "abstain") {
        return Err(VotingError::BadRequest(
            "choice must be for, against, or abstain".into(),
        ));
    }
    deny_blacklisted(&state.cfg, &req.signed.address)?;
    deny_cross_scheme(&req.signed.chain, &payload.chain)?;
    if payload.proposal_id.as_deref() != Some(&id.to_string()) {
        return Err(VotingError::Unauthorized(
            "vote is for a different proposal".into(),
        ));
    }
    if payload.choice.as_deref() != Some(req.choice.as_str()) {
        return Err(VotingError::Unauthorized("signed choice mismatch".into()));
    }

    let proposal = db::get_proposal(&mut conn, id)
        .await?
        .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    if proposal.status != "open" {
        return Err(VotingError::Forbidden(
            "votes are closed until the poll is opened".into(),
        ));
    }
    if proposal.snapshot_validity != "valid" {
        return Err(VotingError::Conflict(
            "snapshot invalidated by chain incident".into(),
        ));
    }
    let _reg = db::ledger_registration(&mut conn, &payload.chain, &req.signed.address)
        .await?
        .ok_or_else(|| VotingError::Forbidden("register before voting".into()))?;
    let weight = db::snapshot_weight(&mut conn, id, &payload.chain, &req.signed.address).await?;
    if weight <= 0.into() {
        return Err(VotingError::Forbidden(
            "no snapshot weight at proposal freeze".into(),
        ));
    }

    db::insert_vote(
        &mut conn,
        id,
        &payload.chain,
        &req.signed.address,
        &req.choice,
        &weight,
        sig_id,
    )
    .await?;
    Ok(Json(finish_action(conn, sig_id, Some(id), serde_json::json!({
        "ok": true,
        "weight": weight.to_string(),
        "snapshot": { "terra_height": proposal.terra_height, "bsc_block": proposal.bsc_block },
    })).await?))
}

#[utoipa::path(get, path = "/v1/proposals/{id}/votes/{addr}", params(("id" = String, Path, description = "UUID"),("addr" = String, Path, description = "Terra or EVM address"),("chain" = Option<String>, Query, description = "terra or bsc")), responses((status = 200, description = "Accepted response, mutation responses include immutable receipt_id", body = crate::dto::Vote),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn get_vote(
    State(state): State<AppState>,
    Path((id, addr)): Path<(Uuid, String)>,
    Query(q): Query<ChainQuery>,
) -> VotingResult<Json<serde_json::Value>> {
    let mut conn = state.pool.acquire().await?;

    let chain = resolve_chain(&addr, q.chain.as_deref())?.to_string();
    match db::get_vote(&mut conn, id, &chain, &addr).await? {
        Some((choice, weight)) => Ok(Json(
            serde_json::json!({ "choice": choice, "weight": weight }),
        )),
        None => Err(VotingError::NotFound("vote".into())),
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct ChainQuery {
    chain: Option<String>,
    height: Option<i64>,
}

#[utoipa::path(get, path = "/v1/balances/{addr}", params(("addr" = String, Path, description = "Terra or EVM address"),("chain" = Option<String>, Query, description = "terra or bsc"),("height" = Option<i64>, Query, description = "Requested ledger height")), responses((status = 200, description = "Accepted response, mutation responses include immutable receipt_id", body = crate::dto::BalanceResponse),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn get_balance(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    Query(q): Query<ChainQuery>,
) -> VotingResult<Json<serde_json::Value>> {
    let mut conn = state.pool.acquire().await?;

    let chain = resolve_chain(&addr, q.chain.as_deref())?;
    let reg = db::ledger_registration(&mut conn, chain, &addr).await?;
    let snapshot_eligible = db::snapshot_eligible(&mut conn, chain, &addr).await?;
    let pending = if reg.is_some() {
        false
    } else {
        db::pending_intent(&mut conn, chain, &addr).await?
    };
    let (terra_h, bsc_b) = db::tip_heights(&mut conn).await?;
    let tip = if chain == "terra" { terra_h } else { bsc_b };
    let as_of_height =
        resolve_balance_height(q.height, tip, reg.as_ref().map(|r| r.registered_at_height));
    let amount = db::balance_at(&mut conn, chain, &addr, as_of_height).await?;
    // OV-B6: always emit registered/pending/as_of_height/initial_balance (issue #14).
    Ok(Json(serde_json::json!({
        "chain": chain,
        "address": normalize_address(&addr),
        "height": as_of_height,
        "as_of_height": as_of_height,
        "balance": amount.to_string(),
        "registered": reg.is_some(),
        "snapshot_eligible": snapshot_eligible,
        "pending": pending,
        "initial_balance": reg.as_ref().map(|r| r.initial_balance.clone()),
    })))
}

#[utoipa::path(get, path = "/v1/receipts/{id}", params(("id" = String, Path, description = "UUID")), responses((status = 200, description = "Accepted response, mutation responses include immutable receipt_id", body = crate::dto::ReceiptExport),(status = 400, description = "Invalid input", body = crate::error::ErrorBody),(status = 401, description = "Invalid signature/domain/time", body = crate::error::ErrorBody),(status = 403, description = "Actor, state, eligibility or quota forbidden", body = crate::error::ErrorBody),(status = 404, description = "Not found", body = crate::error::ErrorBody),(status = 409, description = "Revision, state or nonce conflict", body = crate::error::ErrorBody),(status = 429, description = "Rate limited", body = crate::error::ErrorBody)))]
async fn get_receipt(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> VotingResult<Json<serde_json::Value>> {
    let mut conn = state.pool.acquire().await?;
    Ok(Json(
        db::receipt(&mut conn, id)
            .await?
            .ok_or_else(|| VotingError::NotFound("receipt".into()))?,
    ))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct Readiness {
    pub ready: bool,
    pub service: &'static str,
    pub reason: &'static str,
}
#[utoipa::path(get,path="/ready",responses((status=200,description="Both canonical ledger chains ready",body=Readiness),(status=503,description="Ledger/database unavailable",body=Readiness)))]
async fn readiness(State(state): State<AppState>) -> (StatusCode, Json<Readiness>) {
    let ready = matches!(
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            db::ledger_ready(&state.pool)
        )
        .await,
        Ok(Ok(true))
    );
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(Readiness {
            ready,
            service: "operator-voting",
            reason: if ready {
                "ready"
            } else {
                "dual_chain_ledger_unavailable"
            },
        }),
    )
}

#[utoipa::path(get,path="/v1/proposals/{id}/weights/{addr}",params(("id"=String,Path,description="Proposal UUID"),("addr"=String,Path,description="Terra or EVM address"),("chain"=Option<String>,Query,description="terra or bsc")),responses((status=200,description="Stored immutable per-poll snapshot weight",body=crate::dto::FrozenWeightResponse),(status=400,body=crate::error::ErrorBody),(status=404,body=crate::error::ErrorBody),(status=500,body=crate::error::ErrorBody)))]
async fn get_frozen_weight(
    State(state): State<AppState>,
    Path((id, addr)): Path<(Uuid, String)>,
    Query(q): Query<ChainQuery>,
) -> VotingResult<Json<serde_json::Value>> {
    let chain = resolve_chain(&addr, q.chain.as_deref())?;
    let mut conn = state.pool.acquire().await?;
    let proposal = db::get_proposal(&mut conn, id)
        .await?
        .ok_or_else(|| VotingError::NotFound("proposal".into()))?;
    let weight = db::snapshot_weight(&mut conn, id, chain, &addr).await?;
    let eligible =
        proposal.status == "open" && proposal.snapshot_validity == "valid" && weight > 0.into();
    Ok(Json(
        serde_json::json!({"proposal_id":id,"chain":chain,"address":normalize_address(&addr),"weight":weight.to_string(),"eligible":eligible}),
    ))
}
