mod bep20;
mod cw20;

pub use bep20::{
    parse_bep20_transfer_log, Bep20Log, Bep20Transfer, TRANSFER_TOPIC, ZERO_EVM_ADDRESS,
};
pub use cw20::{parse_cw20_wasm_events, Cw20Attribute, Cw20Event, Cw20Transfer};

use num_bigint::BigInt;
use num_traits::Zero;

/// Apply a single transfer to a registered wallet's running balance.
/// Self-transfer (from == to) is a no-op. Mint credits `to`; burn debits `from`.
pub fn apply_transfer(
    previous: &BigInt,
    wallet: &str,
    from: &str,
    to: &str,
    amount: &BigInt,
) -> BigInt {
    let wallet_n = wallet.to_ascii_lowercase();
    let from_n = from.to_ascii_lowercase();
    let to_n = to.to_ascii_lowercase();
    if from_n == to_n {
        return previous.clone();
    }
    let mut next = previous.clone();
    if from_n == wallet_n {
        next -= amount;
    }
    if to_n == wallet_n {
        next += amount;
    }
    if next < BigInt::zero() {
        BigInt::zero()
    } else {
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amount::human_to_raw;

    #[test]
    fn sequential_transfers() {
        let wallet = "terra1alice";
        let mut bal = human_to_raw(100);
        bal = apply_transfer(&bal, wallet, wallet, "terra1bob", &human_to_raw(10));
        assert_eq!(bal, human_to_raw(90));
        bal = apply_transfer(&bal, wallet, "terra1carol", wallet, &human_to_raw(5));
        assert_eq!(bal, human_to_raw(95));
    }

    #[test]
    fn self_transfer_noop() {
        let wallet = "0xaaaabbbbccccddddeeeeffff0000111122223333";
        let bal = human_to_raw(50);
        let next = apply_transfer(&bal, wallet, wallet, wallet, &human_to_raw(50));
        assert_eq!(next, bal);
    }

    #[test]
    fn case_insensitive_evm() {
        let wallet = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let bal = human_to_raw(10);
        let next = apply_transfer(
            &bal,
            wallet,
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            &human_to_raw(1),
        );
        assert_eq!(next, human_to_raw(9));
    }
}
