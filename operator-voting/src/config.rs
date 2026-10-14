use crate::blacklist::{parse_blacklist, parse_committee, Blacklist};
use crate::error::{VotingError, VotingResult};
use num_bigint::BigInt;

pub const DEFAULT_MIN_PROPOSAL_CL8Y: u64 = 1000;
pub const CL8Y_DECIMALS: u32 = 18;
pub const APP_NAME: &str = "cl8y-voting";
pub const MAX_BODY_BYTES: usize = 64 * 1024;
pub const MAX_COMMENT_BYTES: usize = 8 * 1024;
/// Per-address cap on one proposal. Counts toward spam control; O-RL1 still applies.
pub const MAX_COMMENTS_PER_WALLET_PER_PROPOSAL: i64 = 20;
pub const SIGNATURE_TTL_SECS: i64 = 10 * 60;

#[derive(Debug, Clone)]
pub struct VotingConfig {
    pub database_url: String,
    pub blacklist: Blacklist,
    /// Allowlist for `open_vote` and independent analysis. Empty → nobody can open.
    pub committee: Blacklist,
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
    /// Trust bounded X-Forwarded-For chains only from explicit peer CIDRs. Default false.
    pub rate_limit_trust_forwarded: bool,
    pub trusted_proxy_cidrs: Vec<ipnet::IpNet>,
}

impl VotingConfig {
    pub fn from_env() -> VotingResult<Self> {
        dotenvy::dotenv().ok();
        let database_url = require("DATABASE_URL")?;
        let blacklist =
            parse_blacklist(&std::env::var("VOTING_BLACKLIST_ADDRESSES").unwrap_or_default())?;
        let committee =
            parse_committee(&std::env::var("VOTING_COMMITTEE_ADDRESSES").unwrap_or_default())?;
        let min_human: u64 = env_number("MIN_PROPOSAL_CL8Y", DEFAULT_MIN_PROPOSAL_CL8Y)?;
        let min_proposal_raw = BigInt::from(min_human) * BigInt::from(10u64).pow(CL8Y_DECIMALS);
        let cors_origins: Vec<String> = std::env::var("CORS_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let run_mode =
            normalize_run_mode(&std::env::var("RUN_MODE").unwrap_or_else(|_| "dev".into()))?;
        let apply_migrations = match std::env::var("APPLY_MIGRATIONS") {
            Ok(v) => parse_config_bool(&v)?,
            Err(_) => run_mode != "prod",
        };
        let rate_limit_post_per_minute: u32 = env_number(
            "RATE_LIMIT_POST_PER_MINUTE",
            default_post_per_minute(&run_mode),
        )?;
        let rate_limit_post_burst: u32 = env_number("RATE_LIMIT_POST_BURST", 20)?;
        if min_human == 0 || rate_limit_post_burst == 0 {
            return Err(VotingError::InvalidConfig(
                "proposal threshold and rate limit burst must be positive".into(),
            ));
        }
        let rate_limit_trust_forwarded = match std::env::var("RATE_LIMIT_TRUST_FORWARDED") {
            Ok(v) => parse_config_bool(&v)?,
            Err(_) => false,
        };
        let trusted_proxy_cidrs = std::env::var("RATE_LIMIT_TRUSTED_PROXY_CIDRS")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(|s| {
                s.trim()
                    .parse::<ipnet::IpNet>()
                    .map_err(|_| VotingError::InvalidConfig("invalid trusted proxy CIDR".into()))
            })
            .collect::<VotingResult<Vec<_>>>()?;
        if rate_limit_trust_forwarded && trusted_proxy_cidrs.is_empty() {
            return Err(VotingError::InvalidConfig(
                "forwarded headers require RATE_LIMIT_TRUSTED_PROXY_CIDRS".into(),
            ));
        }
        let terra_chain_id =
            std::env::var("TERRA_CHAIN_ID").unwrap_or_else(|_| "columbus-5".into());
        let evm_chain_id = std::env::var("EVM_CHAIN_ID").unwrap_or_else(|_| "56".into());
        validate_networks(&run_mode, &terra_chain_id, &evm_chain_id)?;
        if run_mode == "prod"
            && std::env::var("VOTING_API_REPLICAS").unwrap_or_else(|_| "1".into()) != "1"
        {
            return Err(VotingError::InvalidConfig("in-process rate limits require VOTING_API_REPLICAS=1; aggregate limits require a shared edge limiter".into()));
        }
        validate_prod_guards(
            &run_mode,
            &cors_origins,
            rate_limit_post_per_minute,
            apply_migrations,
        )?;
        Ok(Self {
            database_url,
            blacklist,
            committee,
            min_proposal_raw,
            cors_origins,
            api_bind: std::env::var("API_BIND").unwrap_or_else(|_| "0.0.0.0:3002".into()),
            terra_chain_id,
            evm_chain_id,
            run_mode,
            apply_migrations,
            rate_limit_post_per_minute,
            rate_limit_post_burst,
            rate_limit_trust_forwarded,
            trusted_proxy_cidrs,
        })
    }

    pub fn for_tests(database_url: impl Into<String>, blacklist: &str) -> Self {
        Self {
            database_url: database_url.into(),
            blacklist: parse_blacklist(blacklist).expect("test blacklist"),
            committee: parse_committee("").expect("empty committee"),
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
            trusted_proxy_cidrs: vec![],
        }
    }
}

pub fn default_post_per_minute(run_mode: &str) -> u32 {
    if matches!(run_mode, "prod" | "production") {
        60
    } else {
        600
    }
}

pub fn parse_bool(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Prod must set CORS, a non-zero POST quota, and must not own schema (#7 / O2 / O6).
pub fn validate_prod_guards(
    run_mode: &str,
    cors_origins: &[String],
    rate_limit_post_per_minute: u32,
    apply_migrations: bool,
) -> VotingResult<()> {
    validate_origins(cors_origins)?;
    if normalize_run_mode(run_mode)? != "prod" {
        return Ok(());
    }
    if cors_origins.is_empty() {
        return Err(VotingError::InvalidConfig(
            "prod requires CORS_ORIGINS".into(),
        ));
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

pub fn normalize_run_mode(raw: &str) -> VotingResult<String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "prod" | "production" => Ok("prod".into()),
        "dev" | "development" => Ok("dev".into()),
        "test" => Ok("test".into()),
        _ => Err(VotingError::InvalidConfig(
            "RUN_MODE must be prod/production, dev/development or test".into(),
        )),
    }
}

pub fn validate_networks(mode: &str, terra: &str, evm: &str) -> VotingResult<()> {
    if normalize_run_mode(mode)? == "prod" && (terra != "columbus-5" || evm != "56") {
        return Err(VotingError::InvalidConfig(
            "production requires Terra columbus-5 and BSC 56".into(),
        ));
    }
    if !matches!(terra, "columbus-5" | "localterra") || !matches!(evm, "56" | "31337") {
        return Err(VotingError::InvalidConfig(
            "unsupported voting chain configuration".into(),
        ));
    }
    Ok(())
}

pub fn validate_origins(origins: &[String]) -> VotingResult<()> {
    for origin in origins {
        let parsed = url::Url::parse(origin)
            .map_err(|_| VotingError::InvalidConfig("invalid CORS origin".into()))?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || parsed.origin().ascii_serialization() != *origin
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(VotingError::InvalidConfig("CORS_ORIGINS requires exact HTTP(S) origins without paths, credentials, query or fragment".into()));
        }
        origin
            .parse::<axum::http::HeaderValue>()
            .map_err(|_| VotingError::InvalidConfig("invalid CORS header".into()))?;
    }
    Ok(())
}

fn env_number<T: std::str::FromStr>(key: &'static str, default: T) -> VotingResult<T> {
    match std::env::var(key) {
        Ok(raw) => raw
            .parse()
            .map_err(|_| VotingError::InvalidConfig(format!("{key} requires an integer"))),
        Err(_) => Ok(default),
    }
}
fn parse_config_bool(raw: &str) -> VotingResult<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(VotingError::InvalidConfig(
            "boolean configuration must be true/false".into(),
        )),
    }
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
    fn modes_origins_and_networks_fail_closed() {
        for mode in ["prod", "production", " PROD "] {
            assert_eq!(normalize_run_mode(mode).unwrap(), "prod");
            assert!(validate_networks(mode, "localterra", "56").is_err());
            assert!(validate_networks(mode, "columbus-5", "31337").is_err());
        }
        for bad in ["staging", "preview", ""] {
            assert!(normalize_run_mode(bad).is_err());
        }
        for bad in [
            "null",
            "*",
            "https://example.com/",
            "https://example.com/path",
            "https://user:secret@example.com",
            "https://example.com/?secret=x",
        ] {
            assert!(validate_origins(&[bad.into()]).is_err());
        }
        assert!(
            validate_origins(&["https://example.com".into(), "http://[::1]:5173".into()]).is_ok()
        );
        assert!(validate_networks("development", "localterra", "31337").is_ok());
    }

    #[test]
    fn parse_bool_accepts_common_truthy() {
        assert!(parse_bool("true"));
        assert!(parse_bool("1"));
        assert!(!parse_bool("false"));
        assert!(!parse_bool(""));
    }
}
