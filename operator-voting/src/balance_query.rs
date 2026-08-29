//! Default balance-read height and chain inference for GET `/v1/balances`.
//!
//! Invariant **OV-B1** (issue [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9)):
//! omitted height for a registered wallet is `max(indexer tip, registered_at_height)`.
//! Explicit `?height=` is used as-is so L6 historical queries below register stay 0.
//! Never sum Terra and BSC.

use crate::error::{VotingError, VotingResult};

pub fn infer_chain(addr: &str) -> &'static str {
    let trimmed = addr.trim();
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        "bsc"
    } else {
        "terra"
    }
}

/// Reject `?chain=` that contradicts the address prefix. Identity v1 forbids a cross-chain sum.
pub fn resolve_chain(addr: &str, requested: Option<&str>) -> VotingResult<&'static str> {
    let inferred = infer_chain(addr);
    let Some(raw) = requested.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(inferred);
    };
    let requested = raw.to_ascii_lowercase();
    if requested != "terra" && requested != "bsc" {
        return Err(VotingError::BadRequest(format!(
            "unsupported chain {raw}; use terra or bsc"
        )));
    }
    if requested != inferred {
        return Err(VotingError::BadRequest(
            "chain query does not match address; refusing cross-chain sum".into(),
        ));
    }
    Ok(inferred)
}

/// Height passed to `public.voting_*_cl8y_balance_at`.
///
/// * `explicit` `Some` — caller asked for a historical height (L6 applies).
/// * `explicit` `None` — dApp default: `max(tip, registered_at)` so a live snapshot
///   is not read as 0 while `last_indexed_*` is still 0 or behind register.
pub fn resolve_balance_height(explicit: Option<i64>, tip: i64, registered_at: Option<i64>) -> i64 {
    match explicit {
        Some(h) => h,
        None => match registered_at {
            Some(reg) => tip.max(reg),
            None => tip,
        },
    }
}

/// Clamp the proposer's chain freeze to `registered_at_height`. Do not apply a Terra
/// height to the BSC freeze (or the reverse) — those numbers are different clocks.
pub fn freeze_heights(chain: &str, terra_tip: i64, bsc_tip: i64, registered_at: i64) -> (i64, i64) {
    match chain {
        "terra" => (terra_tip.max(registered_at), bsc_tip),
        "bsc" => (terra_tip, bsc_tip.max(registered_at)),
        _ => (terra_tip, bsc_tip),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_height_uses_register_when_tip_is_zero() {
        assert_eq!(resolve_balance_height(None, 0, Some(100)), 100);
    }

    #[test]
    fn default_height_uses_register_when_tip_lags() {
        assert_eq!(resolve_balance_height(None, 50, Some(100)), 100);
    }

    #[test]
    fn default_height_follows_tip_once_ahead() {
        assert_eq!(resolve_balance_height(None, 120, Some(100)), 120);
    }

    #[test]
    fn explicit_height_below_register_is_preserved_for_l6() {
        assert_eq!(resolve_balance_height(Some(50), 120, Some(100)), 50);
    }

    #[test]
    fn unregistered_default_is_tip() {
        assert_eq!(resolve_balance_height(None, 0, None), 0);
        assert_eq!(resolve_balance_height(None, 9, None), 9);
    }

    #[test]
    fn chain_mismatch_is_rejected() {
        assert!(resolve_chain("terra1abc", Some("bsc")).is_err());
        assert!(resolve_chain("0xabc", Some("terra")).is_err());
        assert_eq!(resolve_chain("terra1abc", Some("terra")).unwrap(), "terra");
        assert_eq!(resolve_chain("0xAbC", Some("bsc")).unwrap(), "bsc");
        assert_eq!(resolve_chain("terra1abc", None).unwrap(), "terra");
    }

    #[test]
    fn freeze_clamps_only_the_proposer_chain() {
        assert_eq!(freeze_heights("terra", 50, 9_000_000, 100), (100, 9_000_000));
        assert_eq!(freeze_heights("bsc", 15_000_000, 40, 80), (15_000_000, 80));
    }
}
