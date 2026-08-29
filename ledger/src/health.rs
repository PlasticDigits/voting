//! Ledger `/health` diagnostics for indexer lag vs live registration snapshots.
//!
//! Issue [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9): a process that is
//! up (`ok`) is not the same as a cursor that can serve a just-registered wallet.
//! HTTP 200 stays liveness so Coolify does not bounce the poller on boot.

/// True when at least one active registration's live snapshot height is still
/// ahead of `last_indexed_*`. GET `/v1/balances` clamps independently (OV-B1).
pub fn indexer_behind_registration(cursor: i64, max_registered_at: i64) -> bool {
    max_registered_at > 0 && cursor < max_registered_at
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_registrations_is_not_behind() {
        assert!(!indexer_behind_registration(0, 0));
        assert!(!indexer_behind_registration(10, 0));
    }

    #[test]
    fn cursor_zero_with_a_snapshot_is_behind() {
        assert!(indexer_behind_registration(0, 100));
    }

    #[test]
    fn cursor_below_register_is_behind() {
        assert!(indexer_behind_registration(50, 100));
        assert!(!indexer_behind_registration(100, 100));
        assert!(!indexer_behind_registration(120, 100));
    }
}
