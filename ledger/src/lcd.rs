use async_trait::async_trait;
use base64::Engine;
use num_bigint::BigInt;
use serde::Deserialize;

use crate::amount::parse_raw_amount;
use crate::error::{LedgerError, LedgerResult};
use crate::parser::{Cw20Attribute, Cw20Event};
use crate::register::LiveBalanceSource;

#[derive(Clone)]
pub struct LcdClient {
    http: reqwest::Client,
    urls: Vec<String>,
}

impl LcdClient {
    pub fn new(urls: Vec<String>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .expect("reqwest"),
            urls,
        }
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> LedgerResult<T> {
        if self.urls.is_empty() {
            return Err(LedgerError::Lcd("no TERRA_LCD_URL configured".into()));
        }
        let mut last = LedgerError::Lcd("all LCD endpoints failed".into());
        for base in &self.urls {
            let url = format!("{}{path}", base.trim_end_matches('/'));
            match self.http.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    return resp
                        .json::<T>()
                        .await
                        .map_err(|e| LedgerError::Lcd(e.to_string()));
                }
                Ok(resp) => {
                    last = LedgerError::Lcd(format!("LCD {} {}", url, resp.status()));
                }
                Err(e) => last = LedgerError::Lcd(e.to_string()),
            }
        }
        Err(last)
    }

    pub async fn latest_height(&self) -> LedgerResult<i64> {
        #[derive(Deserialize)]
        struct BlockResponse {
            block: BlockInner,
        }
        #[derive(Deserialize)]
        struct BlockInner {
            header: Header,
        }
        #[derive(Deserialize)]
        struct Header {
            height: String,
        }
        let resp: BlockResponse = self
            .get_json("/cosmos/base/tendermint/v1beta1/blocks/latest")
            .await?;
        resp.block
            .header
            .height
            .parse()
            .map_err(|e| LedgerError::Lcd(format!("invalid height: {e}")))
    }

    pub async fn cw20_balance(&self, token: &str, address: &str) -> LedgerResult<BigInt> {
        #[derive(Deserialize)]
        struct Smart {
            data: serde_json::Value,
        }
        let query = serde_json::json!({ "balance": { "address": address } });
        let b64 = base64::engine::general_purpose::STANDARD.encode(query.to_string());
        let path = format!("/cosmwasm/wasm/v1/contract/{token}/smart/{b64}");
        let resp: Smart = self.get_json(&path).await?;
        let raw = resp
            .data
            .get("balance")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LedgerError::Lcd("CW20 balance missing".into()))?;
        parse_raw_amount(raw)
    }

    pub async fn block_txs(&self, height: i64) -> LedgerResult<Vec<LcdTx>> {
        #[derive(Deserialize)]
        struct Search {
            txs: Option<Vec<LcdTx>>,
        }
        let path = format!(
            "/cosmos/tx/v1beta1/txs?query={}&pagination.limit=100",
            urlencoding_height(height)
        );
        let resp: Search = self.get_json(&path).await?;
        Ok(resp.txs.unwrap_or_default())
    }

    pub async fn block_hash(&self, height: i64) -> LedgerResult<String> {
        #[derive(Deserialize)]
        struct BlockResponse {
            block_id: BlockId,
        }
        #[derive(Deserialize)]
        struct BlockId {
            hash: String,
        }
        let resp: BlockResponse = self
            .get_json(&format!(
                "/cosmos/base/tendermint/v1beta1/blocks/{height}"
            ))
            .await?;
        Ok(resp.block_id.hash)
    }
}

fn urlencoding_height(height: i64) -> String {
    // SDK 0.50+ query= format. Keep simple; no extra crates.
    format!("tx.height%3D{height}")
}

#[derive(Debug, Deserialize, Clone)]
pub struct LcdTx {
    pub txhash: Option<String>,
    pub logs: Option<Vec<LcdLog>>,
    pub events: Option<Vec<LcdEvent>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LcdLog {
    pub events: Vec<LcdEvent>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LcdEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub attributes: Vec<LcdAttr>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LcdAttr {
    pub key: String,
    pub value: String,
}

impl LcdTx {
    pub fn tx_hash(&self) -> String {
        self.txhash.clone().unwrap_or_default()
    }

    pub fn wasm_events(&self) -> Vec<Cw20Event> {
        let mut events = Vec::new();
        if let Some(logs) = &self.logs {
            for log in logs {
                for e in &log.events {
                    events.push(to_cw20(e));
                }
            }
        } else if let Some(raw) = &self.events {
            for e in raw {
                events.push(to_cw20(e));
            }
        }
        events
    }
}

fn to_cw20(e: &LcdEvent) -> Cw20Event {
    Cw20Event {
        event_type: e.event_type.clone(),
        attributes: e
            .attributes
            .iter()
            .map(|a| Cw20Attribute {
                key: a.key.clone(),
                value: a.value.clone(),
            })
            .collect(),
    }
}

pub struct LcdBalanceSource {
    pub lcd: LcdClient,
    pub cl8y_token: String,
}

#[async_trait]
impl LiveBalanceSource for LcdBalanceSource {
    async fn terra_live_balance(&self, address: &str) -> LedgerResult<(i64, BigInt)> {
        let height = self.lcd.latest_height().await?;
        let amount = self.lcd.cw20_balance(&self.cl8y_token, address).await?;
        Ok((height, amount))
    }

    async fn bsc_live_balance(&self, _address: &str) -> LedgerResult<(i64, BigInt)> {
        Err(LedgerError::BscRpc(
            "LcdBalanceSource does not serve BSC; use CompositeBalanceSource".into(),
        ))
    }
}
