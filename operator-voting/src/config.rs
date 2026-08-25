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
        if run_mode == "prod" && cors_origins.is_empty() {
            return Err(VotingError::InvalidConfig("prod requires CORS_ORIGINS".into()));
        }
        Ok(Self {
            database_url,
            blacklist,
            min_proposal_raw,
            cors_origins,
            api_bind: std::env::var("API_BIND").unwrap_or_else(|_| "0.0.0.0:3002".into()),
            terra_chain_id: std::env::var("TERRA_CHAIN_ID").unwrap_or_else(|_| "columbus-5".into()),
            evm_chain_id: std::env::var("EVM_CHAIN_ID").unwrap_or_else(|_| "56".into()),
            run_mode,
        })
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
