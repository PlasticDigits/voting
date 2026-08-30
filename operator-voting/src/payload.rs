use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::blacklist::normalize_address;
use crate::config::{APP_NAME, SIGNATURE_TTL_SECS};
use crate::error::{VotingError, VotingResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedPayload {
    pub app: String,
    pub chain: String,
    pub chain_id: String,
    pub purpose: String,
    pub address: String,
    pub issued_at: i64,
    pub expires_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choice: Option<String>,
}

impl SignedPayload {
    pub fn canonical_json(&self) -> VotingResult<String> {
        serde_json::to_string(self)
            .map_err(|e| VotingError::BadRequest(format!("canonicalize: {e}")))
    }
}

/// SHA-256 hex of the canonical propose body.
/// For templated proposals (issue #10) this is the compact JSON of sanitized
/// `body_sections` (sorted keys). See `proposal_sections::prepare_sections`.
pub fn body_hash(canonical_body: &str) -> String {
    hex::encode(Sha256::digest(canonical_body.as_bytes()))
}

pub fn parse_and_validate(
    raw: &str,
    expected_purpose: &str,
    expected_address: &str,
    terra_chain_id: &str,
    evm_chain_id: &str,
) -> VotingResult<SignedPayload> {
    let payload: SignedPayload = serde_json::from_str(raw)
        .map_err(|e| VotingError::BadRequest(format!("invalid signed payload: {e}")))?;
    if payload.app != APP_NAME {
        return Err(VotingError::Unauthorized("wrong signing app domain".into()));
    }
    if payload.purpose != expected_purpose {
        return Err(VotingError::Unauthorized(format!(
            "purpose {} cannot be used as {expected_purpose}",
            payload.purpose
        )));
    }
    if normalize_address(&payload.address) != normalize_address(expected_address) {
        return Err(VotingError::Unauthorized(
            "payload address does not match claimed address".into(),
        ));
    }
    match payload.chain.as_str() {
        "terra" => {
            if payload.chain_id != terra_chain_id && payload.chain_id != "localterra" {
                return Err(VotingError::Unauthorized("wrong terra chain_id".into()));
            }
        }
        "bsc" => {
            if payload.chain_id != evm_chain_id && payload.chain_id != "31337" {
                return Err(VotingError::Unauthorized("wrong evm chain_id".into()));
            }
        }
        other => {
            return Err(VotingError::BadRequest(format!("unknown chain {other}")));
        }
    }
    let now = Utc::now().timestamp();
    if payload.expires_at < now {
        return Err(VotingError::Unauthorized("signature expired".into()));
    }
    if payload.issued_at > now + 60 {
        return Err(VotingError::Unauthorized(
            "signature issued in the future".into(),
        ));
    }
    if payload.expires_at - payload.issued_at > SIGNATURE_TTL_SECS + 30 {
        return Err(VotingError::Unauthorized("signature ttl too long".into()));
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> SignedPayload {
        let now = Utc::now().timestamp();
        SignedPayload {
            app: APP_NAME.into(),
            chain: "terra".into(),
            chain_id: "columbus-5".into(),
            purpose: "register".into(),
            address: "terra1abc".into(),
            issued_at: now,
            expires_at: now + 120,
            title: None,
            body_hash: None,
            proposal_id: None,
            choice: None,
        }
    }

    #[test]
    fn rejects_purpose_replay() {
        let raw = serde_json::to_string(&base()).unwrap();
        let err = parse_and_validate(&raw, "vote", "terra1abc", "columbus-5", "56").unwrap_err();
        assert!(matches!(err, VotingError::Unauthorized(_)));
    }

    #[test]
    fn rejects_address_mismatch() {
        let raw = serde_json::to_string(&base()).unwrap();
        let err =
            parse_and_validate(&raw, "register", "terra1other", "columbus-5", "56").unwrap_err();
        assert!(matches!(err, VotingError::Unauthorized(_)));
    }

    #[test]
    fn stable_canonical() {
        let a = base();
        let b = a.clone();
        assert_eq!(a.canonical_json().unwrap(), b.canonical_json().unwrap());
    }
}
