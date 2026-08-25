use crate::error::{LedgerError, LedgerResult};

pub const DEFAULT_CL8Y_CW20: &str =
    "terra16wtml2q66g82fdkx66tap0qjkahqwp4lwq3ngtygacg5q0kzycgqvhpax3";
pub const DEFAULT_CL8Y_BEP20: &str = "0x8F452a1fdd388A45e1080992eFF051b4dd9048d2";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunMode {
    Dev,
    Prod,
}

#[derive(Debug, Clone)]
pub struct LedgerConfig {
    pub database_url: String,
    pub terra_lcd_urls: Vec<String>,
    pub cl8y_token_address: String,
    pub bsc_cl8y_token_address: String,
    pub bsc_rpc_urls: Vec<String>,
    pub run_mode: RunMode,
    pub api_bind: String,
    pub poll_interval_ms: u64,
}

impl LedgerConfig {
    pub fn from_env() -> LedgerResult<Self> {
        dotenvy::dotenv().ok();
        let run_mode = match std::env::var("RUN_MODE")
            .unwrap_or_else(|_| "dev".into())
            .to_ascii_lowercase()
            .as_str()
        {
            "prod" | "production" => RunMode::Prod,
            _ => RunMode::Dev,
        };

        let database_url = require_env("DATABASE_URL")?;
        let cl8y_token_address = normalize_terra_addr(
            std::env::var("CL8Y_TOKEN_ADDRESS")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_CL8Y_CW20.to_string()),
        );
        if cl8y_token_address.is_empty() || !cl8y_token_address.starts_with("terra1") {
            return Err(LedgerError::InvalidConfig(
                "CL8Y_TOKEN_ADDRESS must be a terra1 CW20 address".into(),
            ));
        }

        let bsc_raw = std::env::var("BSC_CL8Y_TOKEN_ADDRESS")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_CL8Y_BEP20.to_string());
        let bsc_cl8y_token_address = normalize_evm_addr(&bsc_raw)?;

        let terra_lcd_raw = std::env::var("TERRA_LCD_URL")
            .or_else(|_| std::env::var("LCD_URLS"))
            .unwrap_or_default();
        let terra_lcd_urls = split_urls(&terra_lcd_raw);
        let bsc_rpc_urls = split_urls(&std::env::var("BSC_RPC_URLS").unwrap_or_default());

        if run_mode == RunMode::Prod {
            if terra_lcd_urls.is_empty() {
                return Err(LedgerError::InvalidConfig(
                    "prod requires TERRA_LCD_URL / LCD_URLS".into(),
                ));
            }
            if bsc_rpc_urls.is_empty() {
                return Err(LedgerError::InvalidConfig(
                    "prod voting requires BSC_RPC_URLS (BSC is core electorate)".into(),
                ));
            }
        }

        let api_bind = std::env::var("API_BIND").unwrap_or_else(|_| "0.0.0.0:3001".into());
        let poll_interval_ms = std::env::var("POLL_INTERVAL_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(4_000);

        Ok(Self {
            database_url,
            terra_lcd_urls,
            cl8y_token_address,
            bsc_cl8y_token_address,
            bsc_rpc_urls,
            run_mode,
            api_bind,
            poll_interval_ms,
        })
    }

    pub fn bsc_enabled(&self) -> bool {
        !self.bsc_rpc_urls.is_empty()
    }
}

pub fn split_urls(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn normalize_evm_addr(raw: &str) -> LedgerResult<String> {
    let trimmed = raw.trim();
    let hex = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    if hex.len() != 40 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(LedgerError::InvalidConfig(format!(
            "invalid EVM address: {raw}"
        )));
    }
    Ok(format!("0x{}", hex.to_ascii_lowercase()))
}

pub fn normalize_terra_addr(raw: String) -> String {
    raw.trim().to_ascii_lowercase()
}

fn require_env(key: &'static str) -> LedgerResult<String> {
    std::env::var(key)
        .map(|s| s.trim().to_string())
        .map_err(|_| LedgerError::MissingEnv(key))
        .and_then(|s| {
            if s.is_empty() {
                Err(LedgerError::MissingEnv(key))
            } else {
                Ok(s)
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evm_checksum_normalizes() {
        let a = normalize_evm_addr("0x8F452a1fdd388A45e1080992eFF051b4dd9048d2").unwrap();
        let b = normalize_evm_addr("0x8f452a1fdd388a45e1080992eff051b4dd9048d2").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn rejects_short_evm() {
        assert!(normalize_evm_addr("0xabc").is_err());
    }

    #[test]
    fn empty_bsc_rpc_skips() {
        assert!(split_urls("").is_empty());
        assert_eq!(split_urls(" https://a , ,https://b ").len(), 2);
    }
}
