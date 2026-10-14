//! Versioned protocol and authoritative canonicalization; display sanitation is independent.
use crate::{
    config::{VotingConfig, MAX_COMMENT_BYTES},
    error::{VotingError, VotingResult},
    html::sanitize_proposal_html,
    payload::body_hash,
    proposal_sections::{prepare_analysis, prepare_sections, visible_len, AnalysisSections},
};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{OpenApi, ToSchema};
pub const PROTOCOL_VERSION: u32 = 2;
#[derive(Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CanonicalRequest {
    Proposal { sections: Value },
    Analysis { sections: AnalysisSections },
    Comment { body_html: String },
}
#[derive(Serialize, ToSchema)]
pub struct CanonicalResponse {
    pub protocol_version: u32,
    pub body_hash: String,
    pub canonical_json: Option<String>,
    pub sections: Option<Value>,
    pub body_html: String,
}
#[derive(Serialize, ToSchema)]
pub struct Metadata {
    pub protocol_version: u32,
    pub min_proposal_raw: String,
    pub decimals: u32,
    pub capabilities: Vec<String>,
}
pub fn router(cfg: VotingConfig) -> Router {
    Router::new()
        .route("/v1/canonicalize", post(canonicalize))
        .route("/v1/metadata", get(metadata))
        .with_state(cfg)
}
#[utoipa::path(get, path="/v1/metadata", responses((status=200, body=Metadata)))]
pub async fn metadata(State(cfg): State<VotingConfig>) -> Json<Metadata> {
    Json(Metadata {
        protocol_version: PROTOCOL_VERSION,
        min_proposal_raw: cfg.min_proposal_raw.to_string(),
        decimals: crate::config::CL8Y_DECIMALS,
        capabilities: vec![
            "drafts".into(),
            "revision_signatures".into(),
            "canonicalize".into(),
            "terra".into(),
            "bsc".into(),
        ],
    })
}
#[utoipa::path(post, path="/v1/canonicalize", request_body=CanonicalRequest, responses((status=200, body=CanonicalResponse), (status=400, body=crate::error::ErrorBody), (status=413, body=crate::error::ErrorBody), (status=429, body=crate::error::ErrorBody)))]
pub async fn canonicalize(
    Json(req): Json<CanonicalRequest>,
) -> VotingResult<Json<CanonicalResponse>> {
    let response = match req {
        CanonicalRequest::Proposal { sections } => {
            let p = prepare_sections(&sections)?;
            CanonicalResponse {
                protocol_version: PROTOCOL_VERSION,
                body_hash: p.body_hash,
                canonical_json: Some(p.canonical_json),
                sections: Some(Value::Object(p.sanitized)),
                body_html: p.body_html,
            }
        }
        CanonicalRequest::Analysis { sections } => {
            let (clean, canonical_json, hash) = prepare_analysis(&sections)?;
            let body_html = crate::proposal_sections::render_analysis_html(&clean);
            CanonicalResponse {
                protocol_version: PROTOCOL_VERSION,
                body_hash: hash,
                canonical_json: Some(canonical_json),
                sections: Some(serde_json::to_value(clean).unwrap()),
                body_html,
            }
        }
        CanonicalRequest::Comment { body_html } => {
            if body_html.len() > MAX_COMMENT_BYTES {
                return Err(VotingError::BadRequest("comment too large".into()));
            }
            let clean = sanitize_proposal_html(&body_html);
            if visible_len(&clean) == 0 {
                return Err(VotingError::BadRequest("comment empty".into()));
            }
            CanonicalResponse {
                protocol_version: PROTOCOL_VERSION,
                body_hash: body_hash(&clean),
                canonical_json: None,
                sections: None,
                body_html: clean,
            }
        }
    };
    Ok(Json(response))
}
#[derive(OpenApi)]
#[openapi(
    paths(metadata, canonicalize),
    components(schemas(Metadata, CanonicalRequest, CanonicalResponse, crate::error::ErrorBody))
)]
struct ProtocolDoc;
pub fn openapi() -> Value {
    let mut document = crate::api::ApiDoc::openapi();
    document.merge(ProtocolDoc::openapi());
    let mut value = serde_json::to_value(document).expect("OpenAPI serializes");
    // These responses are supplied by the application stack around every annotated handler.
    for item in value["paths"].as_object_mut().unwrap().values_mut() {
        for (method, operation) in item.as_object_mut().unwrap().iter_mut() {
            if !matches!(method.as_str(), "get" | "post" | "put" | "patch" | "delete") {
                continue;
            }
            let responses = operation["responses"].as_object_mut().unwrap();
            for (status, description) in [
                ("403", "Origin policy rejected request"),
                ("413", "Request body exceeds bounded limit"),
                ("415", "Unsupported request content type"),
                ("422", "Invalid typed request envelope"),
                ("500", "Service temporarily unavailable"),
            ] {
                responses.entry(status).or_insert_with(|| serde_json::json!({"description": description, "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ErrorBody"}}}}));
            }
        }
    }
    value
}
