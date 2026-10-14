use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::blacklist::normalize_address;
use crate::config::{APP_NAME, SIGNATURE_TTL_SECS};
use crate::error::{VotingError, VotingResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SignedPayload {
    #[serde(default = "legacy_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
    pub app: String,
    #[schema(value_type=crate::dto::VotingChain)]
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
    #[schema(value_type=Option<crate::dto::VoteChoice>)]
    pub choice: Option<String>,
    /// Current canonical hash the signer last saw (amend optimistic lock).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_body_hash: Option<String>,
}

fn legacy_version() -> u32 {
    1
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
    let payload = parse_domain(
        raw,
        expected_purpose,
        expected_address,
        terra_chain_id,
        evm_chain_id,
    )?;
    validate_time(&payload, Utc::now().timestamp())?;
    Ok(payload)
}

pub fn parse_domain(
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
            if payload.chain_id != terra_chain_id {
                return Err(VotingError::Unauthorized("wrong terra chain_id".into()));
            }
        }
        "bsc" => {
            if payload.chain_id != evm_chain_id {
                return Err(VotingError::Unauthorized("wrong evm chain_id".into()));
            }
        }
        other => {
            return Err(VotingError::BadRequest(format!("unknown chain {other}")));
        }
    }
    Ok(payload)
}

/// Injectable wall clock, with checked arithmetic in debug and release builds.
pub fn validate_time(payload: &SignedPayload, now: i64) -> VotingResult<()> {
    if payload.expires_at <= now {
        return Err(VotingError::Unauthorized("signature expired".into()));
    }
    let duration = payload
        .expires_at
        .checked_sub(payload.issued_at)
        .filter(|d| *d > 0 && *d <= SIGNATURE_TTL_SECS + 30)
        .ok_or_else(|| VotingError::Unauthorized("invalid signature ttl".into()))?;
    let _ = duration;
    let skew = payload
        .issued_at
        .checked_sub(now)
        .ok_or_else(|| VotingError::Unauthorized("invalid signature issue time".into()))?;
    if !(-SIGNATURE_TTL_SECS - 30..=60).contains(&skew) {
        return Err(VotingError::Unauthorized(
            "signature issue time out of bounds".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> SignedPayload {
        let now = Utc::now().timestamp();
        SignedPayload {
            version: 2,
            nonce: Some(uuid::Uuid::new_v4().to_string()),
            revision: None,
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
            prev_body_hash: None,
        }
    }

    #[test]
    fn rejects_timestamp_extrema_and_reversed() {
        let mut p = base();
        for (issued, expires) in [
            (i64::MIN, i64::MAX),
            (i64::MAX, i64::MIN),
            (100, 100),
            (101, 100),
        ] {
            p.issued_at = issued;
            p.expires_at = expires;
            assert!(validate_time(&p, 99).is_err());
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

/// Verify an exported receipt without database access or a current clock.
/// This proves the wallet signed the action and its disclosed content. Ledger
/// power remains the operator's offchain checkpoint evidence, not wallet-signed.
/// Hash-only legacy rows deliberately fail this verifier.
pub fn verify_exported_receipt(export: &serde_json::Value) -> VotingResult<()> {
    use crate::crypto::{verify_adr36, verify_eip191};
    let fail = || VotingError::Unauthorized("receipt content/linkage mismatch".into());
    let sig = &export["signature"];
    let raw = sig["signed_payload"].as_str().ok_or_else(fail)?;
    let payload: SignedPayload = serde_json::from_str(raw).map_err(|_| fail())?;
    let actor = sig["wallet_address"].as_str().ok_or_else(fail)?;
    let signature = sig["signature"].as_str().ok_or_else(fail)?;
    if sig["payload_hash"] != body_hash(raw)
        || sig["chain"] != payload.chain
        || sig["purpose"] != payload.purpose
        || sig["nonce"] != serde_json::json!(payload.nonce)
        || normalize_address(&payload.address) != actor
        || payload.app != APP_NAME
        || sig["domain"] != format!("{}:{}", payload.app, payload.chain_id)
        || payload.version != 2
        || sig["id"] != export["receipt"]["signature_id"]
    {
        return Err(fail());
    }
    if payload
        .nonce
        .as_deref()
        .filter(|n| !n.is_empty() && n.len() <= 128)
        .is_none()
        || sig["revision"] != serde_json::json!(payload.revision)
    {
        return Err(fail());
    }
    let accepted_at = chrono::DateTime::parse_from_rfc3339(
        export["receipt"]["accepted_at"].as_str().ok_or_else(fail)?,
    )
    .map_err(|_| fail())?;
    validate_time(&payload, accepted_at.timestamp())?;
    if export["receipt"]["response"]["receipt_id"] != sig["id"] {
        return Err(fail());
    }
    match payload.chain.as_str() {
        "terra" if sig["scheme"] == "ADR-36" => verify_adr36(
            actor,
            raw,
            signature,
            sig["pubkey"].as_str().ok_or_else(fail)?,
        )?,
        "bsc" if sig["scheme"] == "EIP-191" => verify_eip191(actor, raw, signature)?,
        _ => return Err(fail()),
    }
    if let Some(id) = payload.proposal_id.as_deref() {
        if export["receipt"]["proposal_id"] != id || sig["proposal_id"] != id {
            return Err(fail());
        }
    }
    if let Some(expected) = payload.revision {
        let accepted = export["receipt"]["revision"].as_i64().ok_or_else(fail)?;
        let expected = if payload.purpose == "amend" {
            expected.checked_add(1).ok_or_else(fail)?
        } else {
            expected
        };
        if accepted != expected {
            return Err(fail());
        }
    }
    match payload.purpose.as_str() {
        "draft" | "amend" | "open_vote" => {
            let revision = &export["revision"];
            let canonical = revision["body_canonical"].as_str().ok_or_else(fail)?;
            let prepared = crate::proposal_sections::prepare_sections(&revision["body_sections"])?;
            if prepared.canonical_json != canonical
                || serde_json::Value::Object(prepared.sanitized.clone())
                    != revision["body_sections"]
            {
                return Err(fail());
            }
            if payload.purpose == "draft"
                && export["receipt"]["response"]["id"] != revision["proposal_id"]
            {
                return Err(fail());
            }
            if payload.purpose == "amend"
                && (export["receipt"]["response"]["revision"] != revision["revision"]
                    || export["receipt"]["response"]["body_hash"] != prepared.body_hash)
            {
                return Err(fail());
            }

            if payload.title.as_deref() != revision["title"].as_str()
                || payload.body_hash.as_deref() != Some(body_hash(canonical).as_str())
                || revision["proposal_id"] != export["receipt"]["proposal_id"]
                || revision["revision"] != export["receipt"]["revision"]
            {
                return Err(fail());
            }
        }
        "comment" => {
            let content = &export["receipt"]["content"];
            let raw = content["signed_body_html"].as_str().ok_or_else(fail)?;
            if payload.body_hash.as_deref() != Some(body_hash(raw).as_str())
                || content["body_html"] != crate::html::sanitize_proposal_html(raw)
                || content["id"] != export["receipt"]["response"]["id"]
            {
                return Err(fail());
            }
        }
        "analyze" => {
            let sections: crate::proposal_sections::AnalysisSections =
                serde_json::from_value(export["receipt"]["content"]["sections"].clone())
                    .map_err(|_| fail())?;
            let (clean, _, hash) = crate::proposal_sections::prepare_analysis(&sections)?;
            if serde_json::to_value(&clean).map_err(|_| fail())?
                != export["receipt"]["content"]["sections"]
                || payload.body_hash.as_deref() != Some(hash.as_str())
                || export["receipt"]["content"]["body_html"]
                    != crate::proposal_sections::render_analysis_html(&clean)
                || export["receipt"]["content"]["id"] != export["receipt"]["response"]["id"]
            {
                return Err(fail());
            }
        }
        "vote" | "register" => (),
        _ => return Err(fail()),
    }
    Ok(())
}
