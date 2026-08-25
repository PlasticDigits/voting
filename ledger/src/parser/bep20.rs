use crate::amount::{parse_hex_uint256, Amount};
use crate::config::normalize_evm_addr;
use crate::error::{LedgerError, LedgerResult};

/// ERC-20 / BEP-20 Transfer(address,address,uint256)
pub const TRANSFER_TOPIC: &str =
    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

pub const ZERO_EVM_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bep20Log {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: i64,
    pub transaction_hash: String,
    pub log_index: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bep20Transfer {
    pub token: String,
    pub from: String,
    pub to: String,
    pub amount: Amount,
    pub bsc_block: i64,
    pub tx_hash: String,
    pub log_index: i32,
}

fn topic_address(topic: &str) -> LedgerResult<String> {
    let hex = topic.trim().trim_start_matches("0x");
    if hex.len() < 40 {
        return Err(LedgerError::Amount(format!("short topic: {topic}")));
    }
    let addr = &hex[hex.len() - 40..];
    normalize_evm_addr(&format!("0x{addr}"))
}

/// Parse a single log. Returns `Ok(None)` when the log is not a Transfer on the pinned token
/// or does not touch the registered set.
pub fn parse_bep20_transfer_log(
    log: &Bep20Log,
    pinned_token: &str,
    registered: &[&str],
) -> LedgerResult<Option<Bep20Transfer>> {
    let token = normalize_evm_addr(&log.address)?;
    let pinned = normalize_evm_addr(pinned_token)?;
    if token != pinned {
        return Ok(None);
    }
    if log.topics.is_empty() {
        return Ok(None);
    }
    let topic0 = log.topics[0].trim().to_ascii_lowercase();
    if topic0 != TRANSFER_TOPIC {
        return Ok(None);
    }
    if log.topics.len() < 3 {
        return Ok(None);
    }
    let from = topic_address(&log.topics[1])?;
    let to = topic_address(&log.topics[2])?;
    let amount = parse_hex_uint256(&log.data)?;
    let registered_l: Vec<String> = registered
        .iter()
        .map(|a| normalize_evm_addr(a))
        .collect::<Result<_, _>>()?;
    let touches = registered_l.iter().any(|w| w == &from || w == &to);
    if !touches {
        return Ok(None);
    }
    Ok(Some(Bep20Transfer {
        token,
        from,
        to,
        amount,
        bsc_block: log.block_number,
        tx_hash: log.transaction_hash.to_ascii_lowercase(),
        log_index: log.log_index,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amount::human_to_raw;

    const TOKEN: &str = "0x8F452a1fdd388A45e1080992eFF051b4dd9048d2";
    const ALICE: &str = "0x1111111111111111111111111111111111111111";
    const BOB: &str = "0x2222222222222222222222222222222222222222";

    fn topic_addr(addr: &str) -> String {
        format!("0x{:0>64}", addr.trim_start_matches("0x").to_ascii_lowercase())
    }

    fn transfer_log(token: &str, from: &str, to: &str) -> Bep20Log {
        Bep20Log {
            address: token.into(),
            topics: vec![
                TRANSFER_TOPIC.into(),
                topic_addr(from),
                topic_addr(to),
            ],
            data: format!("0x{:0>64x}", 10u64.pow(18)),
            block_number: 100,
            transaction_hash: "0xabc".into(),
            log_index: 0,
        }
    }

    #[test]
    fn registered_leg_kept() {
        let log = transfer_log(TOKEN, ALICE, BOB);
        let parsed = parse_bep20_transfer_log(&log, TOKEN, &[ALICE]).unwrap().unwrap();
        assert_eq!(parsed.from, ALICE);
        assert_eq!(parsed.to, BOB);
        assert_eq!(parsed.amount, human_to_raw(1));
    }

    #[test]
    fn lookalike_token_ignored() {
        let fake = "0x8f452a1fdd388a45e1080992eff051b4dd9048d3";
        let log = transfer_log(fake, ALICE, BOB);
        assert!(parse_bep20_transfer_log(&log, TOKEN, &[ALICE])
            .unwrap()
            .is_none());
    }

    #[test]
    fn unregistered_flood_ignored() {
        let log = transfer_log(TOKEN, BOB, "0x3333333333333333333333333333333333333333");
        assert!(parse_bep20_transfer_log(&log, TOKEN, &[ALICE])
            .unwrap()
            .is_none());
    }

    #[test]
    fn mint_from_zero() {
        let log = transfer_log(TOKEN, ZERO_EVM_ADDRESS, ALICE);
        let parsed = parse_bep20_transfer_log(&log, TOKEN, &[ALICE]).unwrap().unwrap();
        assert_eq!(parsed.from, ZERO_EVM_ADDRESS);
    }
}
