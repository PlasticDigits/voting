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

#[derive(Serialize)]
struct ErrorBody {
    error: String,
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
        (status, Json(ErrorBody { error: self.to_string() })).into_response()
    }
}

pub type VotingResult<T> = Result<T, VotingError>;
