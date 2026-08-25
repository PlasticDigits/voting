use crate::blacklist::{parse_blacklist, Blacklist};
use crate::error::{VotingError, VotingResult};
use num_bigint::BigInt;

pub const DEFAULT_MIN_PROPOSAL_CL8Y: u64 = 1000;
pub const CL8Y_DECIMALS: u32 = 18;
pub const APP_NAME: &str = "cl8y-voting";
pub const MAX_BODY_BYTES: usize = 64 * 1024;
pub const SIGNATURE_TTL_SECS: i64 = 10 * 60;

#[derive(Debug, Clone)]
pub struct VotingConfig {
    pub database_url: String,
    pub blacklist: Blacklist,
    pub min_proposal_raw: BigInt,
    pub cors_origins: Vec<String>,
    pub api_bind: String,
    pub terra_chain_id: String,
    pub evm_chain_id: String,
    pub run_mode: String,
    /// Ledger writer owns schema. Prod must leave this false (restricted role).
    pub apply_migrations: bool,
    /// 0 disables the limiter (forbidden when `run_mode == prod`).
    pub rate_limit_post_per_minute: u32,
    pub rate_limit_post_burst: u32,
    /// Trust `X-Forwarded-For` / `X-Real-IP`. Coolify / prod default true.
    pub rate_limit_trust_forwarded: bool,
}

impl VotingConfig {
    pub fn from_env() -> VotingResult<Self> {
        dotenvy::dotenv().ok();
        let database_url = require("DATABASE_URL")?;
        let blacklist = parse_blacklist(&std::env::var("VOTING_BLACKLIST_ADDRESSES").unwrap_or_default())?;
        let min_human: u64 = std::env::var("MIN_PROPOSAL_CL8Y")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MIN_PROPOSAL_CL8Y);
        let min_proposal_raw = BigInt::from(min_human) * BigInt::from(10u64).pow(CL8Y_DECIMALS);
        let cors_origins: Vec<String> = std::env::var("CORS_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let run_mode = std::env::var("RUN_MODE").unwrap_or_else(|_| "dev".into());
        let apply_migrations = match std::env::var("APPLY_MIGRATIONS") {
            Ok(v) => parse_bool(&v),
            Err(_) => run_mode != "prod",
        };
        let rate_limit_post_per_minute: u32 = std::env::var("RATE_LIMIT_POST_PER_MINUTE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(default_post_per_minute(&run_mode));
        let rate_limit_post_burst: u32 = std::env::var("RATE_LIMIT_POST_BURST")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(20);
        let rate_limit_trust_forwarded = match std::env::var("RATE_LIMIT_TRUST_FORWARDED") {
            Ok(v) => parse_bool(&v),
            Err(_) => run_mode == "prod",
        };
        validate_prod_guards(
            &run_mode,
            &cors_origins,
            rate_limit_post_per_minute,
            apply_migrations,
        )?;
        Ok(Self {
            database_url,
            blacklist,
            min_proposal_raw,
            cors_origins,
            api_bind: std::env::var("API_BIND").unwrap_or_else(|_| "0.0.0.0:3002".into()),
            terra_chain_id: std::env::var("TERRA_CHAIN_ID").unwrap_or_else(|_| "columbus-5".into()),
            evm_chain_id: std::env::var("EVM_CHAIN_ID").unwrap_or_else(|_| "56".into()),
            run_mode,
            apply_migrations,
            rate_limit_post_per_minute,
            rate_limit_post_burst,
            rate_limit_trust_forwarded,
        })
    }

    pub fn for_tests(database_url: impl Into<String>, blacklist: &str) -> Self {
        Self {
            database_url: database_url.into(),
            blacklist: parse_blacklist(blacklist).expect("test blacklist"),
            min_proposal_raw: BigInt::from(1000u64) * BigInt::from(10u64).pow(CL8Y_DECIMALS),
            cors_origins: vec!["http://127.0.0.1:5173".into()],
            api_bind: "127.0.0.1:0".into(),
            terra_chain_id: "columbus-5".into(),
            evm_chain_id: "56".into(),
            run_mode: "dev".into(),
            apply_migrations: true,
            rate_limit_post_per_minute: 600,
            rate_limit_post_burst: 50,
            rate_limit_trust_forwarded: false,
        }
    }
}

pub fn default_post_per_minute(run_mode: &str) -> u32 {
    if run_mode == "prod" {
        60
    } else {
        600
    }
}

pub fn parse_bool(raw: &str) -> bool {
    matches!(raw.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

/// Prod must set CORS, a non-zero POST quota, and must not own schema (#7 / O2 / O6).
pub fn validate_prod_guards(
    run_mode: &str,
    cors_origins: &[String],
    rate_limit_post_per_minute: u32,
    apply_migrations: bool,
) -> VotingResult<()> {
    if run_mode != "prod" {
        return Ok(());
    }
    if cors_origins.is_empty() {
        return Err(VotingError::InvalidConfig("prod requires CORS_ORIGINS".into()));
    }
    if rate_limit_post_per_minute == 0 {
        return Err(VotingError::InvalidConfig(
            "prod requires RATE_LIMIT_POST_PER_MINUTE > 0".into(),
        ));
    }
    if apply_migrations {
        return Err(VotingError::InvalidConfig(
            "prod must leave APPLY_MIGRATIONS unset/false (ledger writer owns schema)".into(),
        ));
    }
    Ok(())
}

fn require(key: &'static str) -> VotingResult<String> {
    let v = std::env::var(key).map_err(|_| VotingError::MissingEnv(key))?;
    if v.trim().is_empty() {
        Err(VotingError::MissingEnv(key))
    } else {
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prod_requires_cors_and_post_quota() {
        assert!(validate_prod_guards("dev", &[], 0, true).is_ok());
        assert!(validate_prod_guards("prod", &[], 60, false).is_err());
        assert!(validate_prod_guards("prod", &["https://vote.cl8y.com".into()], 0, false).is_err());
        assert!(validate_prod_guards("prod", &["https://vote.cl8y.com".into()], 60, true).is_err());
        assert!(validate_prod_guards("prod", &["https://vote.cl8y.com".into()], 60, false).is_ok());
    }

    #[test]
    fn parse_bool_accepts_common_truthy() {
        assert!(parse_bool("true"));
        assert!(parse_bool("1"));
        assert!(!parse_bool("false"));
        assert!(!parse_bool(""));
    }
}
