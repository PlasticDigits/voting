//! Bounded HTTP decoding. Never include provider URLs, response bodies or reqwest errors.
use crate::error::{LedgerError, LedgerResult};
use serde::de::DeserializeOwned;
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub async fn decode<T: DeserializeOwned>(mut response: reqwest::Response) -> LedgerResult<T> {
    if !response.status().is_success() {
        return Err(LedgerError::Provider("http_status"));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
    {
        return Err(LedgerError::Provider("response_too_large"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| LedgerError::Provider("body_transport"))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(LedgerError::Provider("response_too_large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| LedgerError::Provider("invalid_json_schema"))
}
