use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VotingError {
    #[error("missing required env {0}")]
    MissingEnv(&'static str),
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migrate: {0}")]
    Migrate(String),
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorBody {
    pub error: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl IntoResponse for VotingError {
    fn into_response(self) -> Response {
        let status = match &self {
            VotingError::BadRequest(_) | VotingError::InvalidConfig(_) => StatusCode::BAD_REQUEST,
            VotingError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            VotingError::Forbidden(_) => StatusCode::FORBIDDEN,
            VotingError::NotFound(_) => StatusCode::NOT_FOUND,
            VotingError::Conflict(_) => StatusCode::CONFLICT,
            VotingError::MissingEnv(_) | VotingError::Database(_) | VotingError::Migrate(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        let (code, message) = match self {
            VotingError::BadRequest(_) => ("bad_request", "invalid request"),
            VotingError::InvalidConfig(_) | VotingError::MissingEnv(_) => {
                ("configuration_error", "service configuration error")
            }
            VotingError::Unauthorized(_) => ("unauthorized", "signature or authentication invalid"),
            VotingError::Forbidden(_) => ("forbidden", "action not permitted"),
            VotingError::NotFound(_) => ("not_found", "resource not found"),
            VotingError::Conflict(_) => ("conflict", "state changed; reload before signing again"),
            VotingError::Database(_) | VotingError::Migrate(_) => {
                ("internal_error", "service temporarily unavailable")
            }
        };
        // Never format database/provider/request internals into responses or logs.
        (
            status,
            Json(ErrorBody {
                error: message.into(),
                code: code.into(),
                request_id: None,
            }),
        )
            .into_response()
    }
}

pub type VotingResult<T> = Result<T, VotingError>;
