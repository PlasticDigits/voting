use async_trait::async_trait;
use num_bigint::BigInt;
use serde::Deserialize;
use serde_json::json;

use crate::amount::parse_hex_uint256;
use crate::config::normalize_evm_addr;
use crate::error::{LedgerError, LedgerResult};
use crate::parser::{Bep20Log, TRANSFER_TOPIC};
use crate::register::LiveBalanceSource;

/// ERC-20 balanceOf(address) selector.
const BALANCE_OF_SELECTOR: &str = "70a08231";

#[derive(Clone)]
pub struct BscClient {
    http: reqwest::Client,
    urls: Vec<String>,
}

impl BscClient {
    pub fn new(urls: Vec<String>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .expect("reqwest"),
            urls,
        }
    }

    pub fn enabled(&self) -> bool {
        !self.urls.is_empty()
    }

    async fn rpc(&self, method: &str, params: serde_json::Value) -> LedgerResult<serde_json::Value> {
        if self.urls.is_empty() {
            return Err(LedgerError::BscRpc(
                "BSC_RPC_URLS empty; cannot invent balances".into(),
            ));
        }
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let mut last = LedgerError::BscRpc("all BSC RPC URLs failed".into());
        for url in &self.urls {
            // Never log the URL (may contain keys).
            match self.http.post(url).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    #[derive(Deserialize)]
                    struct Rpc {
                        result: Option<serde_json::Value>,
                        error: Option<serde_json::Value>,
                    }
                    let parsed: Rpc = resp
                        .json()
                        .await
                        .map_err(|e| LedgerError::BscRpc(e.to_string()))?;
                    if let Some(err) = parsed.error {
                        last = LedgerError::BscRpc(err.to_string());
                        continue;
                    }
                    return parsed
                        .result
                        .ok_or_else(|| LedgerError::BscRpc("empty result".into()));
                }
                Ok(resp) => last = LedgerError::BscRpc(format!("status {}", resp.status())),
                Err(e) => last = LedgerError::BscRpc(e.to_string()),
            }
        }
        Err(last)
    }

    pub async fn block_number(&self) -> LedgerResult<i64> {
        let v = self.rpc("eth_blockNumber", json!([])).await?;
        let hex = v
            .as_str()
            .ok_or_else(|| LedgerError::BscRpc("blockNumber not hex".into()))?;
        i64::from_str_radix(hex.trim_start_matches("0x"), 16)
            .map_err(|e| LedgerError::BscRpc(e.to_string()))
    }

    pub async fn balance_of(&self, token: &str, wallet: &str) -> LedgerResult<BigInt> {
        let token = normalize_evm_addr(token)?;
        let wallet = normalize_evm_addr(wallet)?;
        let data = format!("0x{BALANCE_OF_SELECTOR}{:0>64}", wallet.trim_start_matches("0x"));
        let v = self
            .rpc("eth_call", json!([{ "to": token, "data": data }, "latest"]))
            .await?;
        let hex = v
            .as_str()
            .ok_or_else(|| LedgerError::BscRpc("balanceOf not hex".into()))?;
        parse_hex_uint256(hex)
    }

    pub async fn transfer_logs(
        &self,
        token: &str,
        from_block: i64,
        to_block: i64,
    ) -> LedgerResult<Vec<Bep20Log>> {
        let token = normalize_evm_addr(token)?;
        let v = self
            .rpc(
                "eth_getLogs",
                json!([{
                    "address": token,
                    "fromBlock": format!("0x{from_block:x}"),
                    "toBlock": format!("0x{to_block:x}"),
                    "topics": [TRANSFER_TOPIC]
                }]),
            )
            .await?;
        let arr = v
            .as_array()
            .ok_or_else(|| LedgerError::BscRpc("getLogs not array".into()))?;
        let mut out = Vec::new();
        for item in arr {
            out.push(parse_rpc_log(item)?);
        }
        Ok(out)
    }
}

fn parse_rpc_log(v: &serde_json::Value) -> LedgerResult<Bep20Log> {
    let topics = v
        .get("topics")
        .and_then(|t| t.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let block_hex = v
        .get("blockNumber")
        .and_then(|x| x.as_str())
        .unwrap_or("0x0");
    let log_hex = v.get("logIndex").and_then(|x| x.as_str()).unwrap_or("0x0");
    Ok(Bep20Log {
        address: v
            .get("address")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        topics,
        data: v
            .get("data")
            .and_then(|x| x.as_str())
            .unwrap_or("0x0")
            .to_string(),
        block_number: i64::from_str_radix(block_hex.trim_start_matches("0x"), 16)
            .map_err(|e| LedgerError::BscRpc(e.to_string()))?,
        transaction_hash: v
            .get("transactionHash")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        log_index: i32::from_str_radix(log_hex.trim_start_matches("0x"), 16)
            .map_err(|e| LedgerError::BscRpc(e.to_string()))?,
    })
}

pub struct BscBalanceSource {
    pub client: BscClient,
    pub token: String,
}

#[async_trait]
impl LiveBalanceSource for BscBalanceSource {
    async fn terra_live_balance(&self, _address: &str) -> LedgerResult<(i64, BigInt)> {
        Err(LedgerError::Lcd(
            "BscBalanceSource does not serve Terra".into(),
        ))
    }

    async fn bsc_live_balance(&self, address: &str) -> LedgerResult<(i64, BigInt)> {
        let block = self.client.block_number().await?;
        let amount = self.client.balance_of(&self.token, address).await?;
        Ok((block, amount))
    }
}

pub struct CompositeBalanceSource {
    pub terra: Option<LcdBalancePair>,
    pub bsc: Option<BscBalanceSource>,
}

pub struct LcdBalancePair {
    pub lcd: crate::lcd::LcdClient,
    pub token: String,
}

#[async_trait]
impl LiveBalanceSource for CompositeBalanceSource {
    async fn terra_live_balance(&self, address: &str) -> LedgerResult<(i64, BigInt)> {
        let src = self
            .terra
            .as_ref()
            .ok_or_else(|| LedgerError::Lcd("no Terra LCD configured".into()))?;
        let height = src.lcd.latest_height().await?;
        let amount = src.lcd.cw20_balance(&src.token, address).await?;
        Ok((height, amount))
    }

    async fn bsc_live_balance(&self, address: &str) -> LedgerResult<(i64, BigInt)> {
        let src = self
            .bsc
            .as_ref()
            .ok_or_else(|| LedgerError::BscRpc("no BSC RPC configured".into()))?;
        src.bsc_live_balance(address).await
    }
}

