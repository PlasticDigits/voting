use crate::error::{VotingError, VotingResult};
use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct Blacklist {
    addrs: HashSet<String>,
}

impl Blacklist {
    pub fn contains(&self, address: &str) -> bool {
        self.addrs.contains(&normalize_address(address))
    }

    pub fn is_empty(&self) -> bool {
        self.addrs.is_empty()
    }
}

/// Terra bech32 is case-insensitive; EVM compares lowercase 0x.
pub fn normalize_address(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        format!("0x{}", trimmed[2..].to_ascii_lowercase())
    } else {
        trimmed.to_ascii_lowercase()
    }
}

pub fn parse_blacklist(raw: &str) -> VotingResult<Blacklist> {
    let mut addrs = HashSet::new();
    for part in raw.split(',') {
        let item = part.trim();
        if item.is_empty() {
            continue;
        }
        if item.starts_with("0x") || item.starts_with("0X") {
            let hex = &item[2..];
            if hex.len() != 40 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(VotingError::InvalidConfig(format!(
                    "invalid EVM blacklist address: {item}"
                )));
            }
            addrs.insert(normalize_address(item));
        } else if item.contains('1') {
            addrs.insert(normalize_address(item));
        } else {
            return Err(VotingError::InvalidConfig(format!(
                "invalid blacklist address: {item}"
            )));
        }
    }
    Ok(Blacklist { addrs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evm_case_and_trim() {
        let bl = parse_blacklist(" 0xAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAa ").unwrap();
        assert!(bl.contains("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(bl.contains("0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"));
    }

    #[test]
    fn terra_case() {
        let bl = parse_blacklist("terra1ABC").unwrap();
        assert!(bl.contains("terra1abc"));
    }

    #[test]
    fn invalid_fails_startup() {
        assert!(parse_blacklist("0xabc").is_err());
        assert!(parse_blacklist("not-an-address").is_err());
    }
}
