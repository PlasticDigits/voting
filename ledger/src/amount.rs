use num_bigint::BigInt;
use num_traits::Zero;

use crate::error::{LedgerError, LedgerResult};

/// Raw 18-decimal integer amount. Never use floats.
pub type Amount = BigInt;

pub const CL8Y_DECIMALS: u32 = 18;

/// Parse a decimal-integer string (no exponent, no fraction).
pub fn parse_raw_amount(raw: &str) -> LedgerResult<Amount> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(LedgerError::Amount("empty amount".into()));
    }
    if !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return Err(LedgerError::Amount(format!(
            "amount must be an unsigned integer string: {trimmed}"
        )));
    }
    trimmed
        .parse::<BigInt>()
        .map_err(|e| LedgerError::Amount(e.to_string()))
}

pub fn parse_hex_uint256(data: &str) -> LedgerResult<Amount> {
    let hex = data.trim().trim_start_matches("0x");
    if hex.is_empty() {
        return Ok(BigInt::zero());
    }
    BigInt::parse_bytes(hex.as_bytes(), 16)
        .ok_or_else(|| LedgerError::Amount(format!("invalid hex amount: {data}")))
}

pub fn human_to_raw(human: u64) -> Amount {
    BigInt::from(human) * BigInt::from(10u64).pow(CL8Y_DECIMALS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_float_and_sign() {
        assert!(parse_raw_amount("1.0").is_err());
        assert!(parse_raw_amount("-1").is_err());
        assert!(parse_raw_amount("1e18").is_err());
    }

    #[test]
    fn parses_18_dec_string() {
        let a = parse_raw_amount("1000000000000000000").unwrap();
        assert_eq!(a, human_to_raw(1));
    }

    #[test]
    fn hex_uint256_roundtrip() {
        let a = parse_hex_uint256("0x0000000000000000000000000000000000000000000000000de0b6b3a7640000")
            .unwrap();
        assert_eq!(a, human_to_raw(1));
    }
}
