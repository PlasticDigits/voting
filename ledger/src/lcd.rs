use crate::amount::parse_raw_amount;
use crate::error::{LedgerError, LedgerResult};
use crate::parser::{Cw20Attribute, Cw20Event};
use crate::register::{LiveBalanceSnapshot, LiveBalanceSource, LiveBalancesSnapshot};
use async_trait::async_trait;
use base64::Engine;
use num_bigint::BigInt;
use serde::Deserialize;

#[derive(Clone)]
pub struct LcdClient {
    http: reqwest::Client,
    urls: Vec<String>,
    expected_chain_id: String,
    rpc: Option<crate::terra_rpc::TerraRpcClient>,
}
impl LcdClient {
    pub fn new(urls: Vec<String>) -> Self {
        Self::new_for_chain(urls, "columbus-5")
    }
    pub fn new_for_chain(urls: Vec<String>, expected_chain_id: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(8))
                .build()
                .expect("reqwest"),
            urls,
            expected_chain_id: expected_chain_id.into(),
            rpc: None,
        }
    }
    pub fn with_rpc_urls(mut self, urls: Vec<String>) -> Self {
        if !urls.is_empty() {
            self.rpc = Some(crate::terra_rpc::TerraRpcClient::new(
                self.http.clone(),
                urls,
                self.expected_chain_id.clone(),
            ));
        }
        self
    }
    async fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> LedgerResult<T> {
        let mut last = LedgerError::Provider("no_lcd_provider");
        for base in &self.urls {
            let response = self
                .http
                .get(format!("{}{path}", base.trim_end_matches('/')))
                .send()
                .await;
            let decoded = async {
                let response = response.map_err(|_| LedgerError::Provider("lcd_transport"))?;
                let value: serde_json::Value = crate::transport::decode(response).await?;
                if path.contains("/blocks/") {
                    let block: BlockResponse = serde_json::from_value(value.clone())
                        .map_err(|_| LedgerError::Provider("block_schema"))?;
                    let expected = path.rsplit('/').next().and_then(|s| s.parse().ok());
                    validate_header(
                        &block,
                        expected,
                        path.ends_with("/latest"),
                        &self.expected_chain_id,
                    )?;
                }
                serde_json::from_value(value)
                    .map_err(|_| LedgerError::Provider("invalid_json_schema"))
            }
            .await;
            match decoded {
                Ok(v) => return Ok(v),
                Err(e) => last = e,
            }
        }
        Err(last)
    }
    pub async fn latest_height(&self) -> LedgerResult<i64> {
        let resp: BlockResponse = self
            .get_json("/cosmos/base/tendermint/v1beta1/blocks/latest")
            .await?;
        validate_header(&resp, None, true, &self.expected_chain_id)
    }
    pub async fn cw20_balance(&self, token: &str, address: &str) -> LedgerResult<BigInt> {
        #[derive(Deserialize)]
        struct Smart {
            data: serde_json::Value,
        }
        let query = serde_json::json!({ "balance": { "address": address } });
        let b64 = base64::engine::general_purpose::STANDARD.encode(query.to_string());
        let resp: Smart = self
            .get_json(&format!("/cosmwasm/wasm/v1/contract/{token}/smart/{b64}"))
            .await?;
        parse_raw_amount(
            resp.data
                .get("balance")
                .and_then(|v| v.as_str())
                .ok_or(LedgerError::Provider("cw20_balance_schema"))?,
        )
    }
    /// One provider's live balance and actual response height, with canonical
    /// block identity. Registration never pairs a cached tip with another state.
    pub async fn live_snapshot(
        &self,
        token: &str,
        address: &str,
    ) -> LedgerResult<LiveBalanceSnapshot> {
        if let Some(rpc) = &self.rpc {
            return rpc.live_snapshot(token, address).await;
        }
        for url in &self.urls {
            let pinned = Self {
                http: self.http.clone(),
                urls: vec![url.clone()],
                expected_chain_id: self.expected_chain_id.clone(),
                rpc: self.rpc.clone(),
            };
            for _ in 0..3 {
                let observation = async {
                    let before = pinned.latest_height().await?;
                    let before_hash = pinned.block_hash(before).await?;
                    let query = serde_json::json!({"balance":{"address":address}});
                    let b64 = base64::engine::general_purpose::STANDARD.encode(query.to_string());
                    // Live request: no historical-state header for registration.
                    let response = pinned
                        .http
                        .get(format!(
                            "{}/cosmwasm/wasm/v1/contract/{token}/smart/{b64}",
                            url.trim_end_matches('/')
                        ))
                        .send()
                        .await
                        .map_err(|_| LedgerError::Provider("lcd_transport"))?;
                    let observed = response
                        .headers()
                        .get("x-cosmos-block-height")
                        .and_then(|h| h.to_str().ok())
                        .and_then(|h| h.parse::<i64>().ok())
                        .ok_or(LedgerError::Provider("unbound_live_balance_height"))?;
                    let v: serde_json::Value = crate::transport::decode(response).await?;
                    let raw = v
                        .get("data")
                        .and_then(|d| d.get("balance"))
                        .and_then(|b| b.as_str())
                        .ok_or(LedgerError::Provider("cw20_balance_schema"))?;
                    let amount = parse_raw_amount(raw)?;
                    let hash = before_hash;
                    let after = pinned.latest_height().await?;
                    if observed != before
                        || observed > after
                        || pinned.block_hash(observed).await? != hash
                    {
                        return Err(LedgerError::Provider("moving_live_snapshot"));
                    }
                    Ok(LiveBalanceSnapshot {
                        height: observed,
                        amount,
                        block_hash: hash,
                    })
                }
                .await;
                if observation.is_ok() {
                    return observation;
                }
            }
        }
        Err(LedgerError::Provider("incoherent_live_snapshot"))
    }
    /// Bulk recovery pins ordinary recent LCD state, not an archive service.
    pub async fn live_balances(
        &self,
        token: &str,
        wallets: &[String],
    ) -> LedgerResult<LiveBalancesSnapshot> {
        if let Some(rpc) = &self.rpc {
            return rpc.live_balances(token, wallets).await;
        }
        for url in &self.urls {
            let pinned = Self {
                http: self.http.clone(),
                urls: vec![url.clone()],
                expected_chain_id: self.expected_chain_id.clone(),
                rpc: self.rpc.clone(),
            };
            let observation = async {
                let height = pinned.latest_height().await?;
                let before = pinned.block_hash(height).await?;
                let mut out = Vec::new();
                for batch in wallets.chunks(8) {
                    let pinned = &pinned;
                    let read = |i: usize| async move {
                        let Some(wallet) = batch.get(i) else {
                            return Ok::<_, LedgerError>(None);
                        };
                        let query = serde_json::json!({"balance":{"address":wallet}});
                        let b64 =
                            base64::engine::general_purpose::STANDARD.encode(query.to_string());
                        let response = pinned
                            .http
                            .get(format!(
                                "{}/cosmwasm/wasm/v1/contract/{token}/smart/{b64}",
                                url.trim_end_matches('/')
                            ))
                            .header("x-cosmos-block-height", height.to_string())
                            .send()
                            .await
                            .map_err(|_| LedgerError::Provider("lcd_transport"))?;
                        let observed = response
                            .headers()
                            .get("x-cosmos-block-height")
                            .and_then(|h| h.to_str().ok())
                            .and_then(|h| h.parse::<i64>().ok());
                        if observed != Some(height) {
                            return Err(LedgerError::Provider("unbound_lcd_recovery_height"));
                        }
                        let v: serde_json::Value = crate::transport::decode(response).await?;
                        let raw = v
                            .get("data")
                            .and_then(|d| d.get("balance"))
                            .and_then(|b| b.as_str())
                            .ok_or(LedgerError::Provider("cw20_balance_schema"))?;
                        Ok(Some((height, parse_raw_amount(raw)?)))
                    };
                    let (a, b, c, d, e, f, g, h) = tokio::join!(
                        read(0),
                        read(1),
                        read(2),
                        read(3),
                        read(4),
                        read(5),
                        read(6),
                        read(7)
                    );
                    for observation in [a, b, c, d, e, f, g, h] {
                        if let Some(v) = observation? {
                            out.push(v);
                        }
                    }
                }
                if pinned.block_hash(height).await? != before {
                    return Err(LedgerError::Provider("recovery_source_fork"));
                }
                Ok(LiveBalancesSnapshot {
                    height,
                    block_hash: before,
                    balances: out.into_iter().map(|(_, b)| b).collect(),
                })
            }
            .await;
            if observation.is_ok() {
                return observation;
            }
        }
        Err(LedgerError::Provider("incoherent_bulk_live_snapshot"))
    }
    /// Validate every execution page before returning anything to persistence.
    /// Cosmos SDK 0.50 GetTxsEvent: execution lives in tx_responses, not txs.
    pub async fn block_txs(&self, height: i64) -> LedgerResult<Vec<LcdTx>> {
        self.block_hash(height).await?;
        let mut out = Vec::new();
        let mut keys = std::collections::HashSet::new();
        let mut seen = std::collections::HashSet::new();
        let mut key = String::new();
        let mut total = None;
        let mut retained = 0_usize;
        for page_number in 1..=1000 {
            let path=format!("/cosmos/tx/v1beta1/txs?query=tx.height%3D{height}&order_by=ORDER_BY_ASC&pagination.limit=100&pagination.count_total=true&page={page_number}&limit=100{}", if key.is_empty() { String::new() } else { format!("&pagination.key={}", percent_encode(&key)) });
            let page: Search = self.get_json(&path).await?;
            let total_raw = page
                .total
                .as_deref()
                .or_else(|| page.pagination.as_ref().map(|p| p.total.as_str()))
                .ok_or(LedgerError::Provider("missing_tx_total"))?;
            let n = total_raw
                .parse::<usize>()
                .map_err(|_| LedgerError::Provider("invalid_tx_total"))?;
            let legacy = page.pagination.is_some();
            if n > 100_000 || total.is_some_and(|t| t != n) || page.tx_responses.len() > 100 {
                return Err(LedgerError::Provider("incomplete_tx_pagination"));
            }
            total = Some(n);
            for tx in page.tx_responses {
                tx.validate(height)?;
                retained = retained.saturating_add(tx.retained_bytes());
                if retained > MAX_BLOCK_RETAINED_BYTES {
                    return Err(LedgerError::Provider("block_memory_budget"));
                }
                if !seen.insert(tx.tx_hash()) {
                    return Err(LedgerError::Provider("duplicate_tx_identity"));
                }
                out.push(tx);
            }
            key = page.pagination.and_then(|p| p.next_key).unwrap_or_default();
            if !legacy && out.len() < n {
                if out.len() != page_number * 100 {
                    return Err(LedgerError::Provider("incomplete_numbered_tx_page"));
                }
                continue;
            }
            if key.is_empty() {
                if out.len() != n {
                    return Err(LedgerError::Provider("incomplete_tx_total"));
                }
                return Ok(out);
            }
            retained = retained.saturating_add(key.len() + 64);
            if retained > MAX_BLOCK_RETAINED_BYTES {
                return Err(LedgerError::Provider("block_memory_budget"));
            }
            if !keys.insert(key.clone()) || out.len() >= n {
                return Err(LedgerError::Provider("repeated_tx_page"));
            }
        }
        Err(LedgerError::Provider("tx_page_limit"))
    }
    async fn manifest(&self, height: i64) -> LedgerResult<(String, Vec<String>)> {
        use sha2::{Digest, Sha256};
        let response: BlockResponse = self
            .get_json(&format!("/cosmos/base/tendermint/v1beta1/blocks/{height}"))
            .await?;
        validate_header(&response, Some(height), false, &self.expected_chain_id)?;
        let block = response
            .sdk_block
            .as_ref()
            .or(response.block.as_ref())
            .ok_or(LedgerError::Provider("missing_block_header"))?;
        let data = block
            .data
            .as_ref()
            .ok_or(LedgerError::Provider("missing_block_transaction_manifest"))?;
        let rows = if data.txs.is_null() {
            &[][..]
        } else {
            data.txs
                .as_array()
                .ok_or(LedgerError::Provider("invalid_block_transaction_manifest"))?
                .as_slice()
        };
        if rows.len() > 100_000 {
            return Err(LedgerError::Provider("block_transaction_limit"));
        }
        let mut hashes = Vec::with_capacity(rows.len());
        for row in rows {
            let raw = row
                .as_str()
                .ok_or(LedgerError::Provider("invalid_block_transaction_bytes"))?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(raw)
                .map_err(|_| LedgerError::Provider("invalid_block_transaction_base64"))?;
            hashes.push(hex::encode_upper(Sha256::digest(bytes)));
        }
        Ok((normalize_block_hash(&response.block_id.hash)?, hashes))
    }
    /// A bound block's physical transaction bytes prove search completeness/order.
    /// Every method/page comes from one provider; failover retries the whole block.
    pub async fn canonical_block(&self, height: i64) -> LedgerResult<TerraBlockSnapshot> {
        let mut last = LedgerError::Provider("no_lcd_provider");
        for url in &self.urls {
            let source = Self {
                http: self.http.clone(),
                urls: vec![url.clone()],
                expected_chain_id: self.expected_chain_id.clone(),
                rpc: self.rpc.clone(),
            };
            let observed = async {
                let (hash, expected) = source.manifest(height).await?;
                let executions = source.block_txs(height).await?;
                let actual = executions.iter().map(LcdTx::tx_hash).collect::<Vec<_>>();
                if actual != expected {
                    return Err(LedgerError::Provider(
                        "incomplete_or_reordered_execution_set",
                    ));
                }
                if source.block_hash(height).await? != hash {
                    return Err(LedgerError::Provider("terra_changed_during_ingest"));
                }
                let tip = source.latest_height().await?;
                if height > tip {
                    return Err(LedgerError::Provider("future_terra_height"));
                }
                Ok((hash, executions, tip))
            }
            .await;
            match observed {
                Ok((hash, executions, tip)) => {
                    return Ok(TerraBlockSnapshot {
                        hash,
                        executions,
                        tip,
                        source,
                    })
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }
    pub async fn block_hash(&self, height: i64) -> LedgerResult<String> {
        let resp: BlockResponse = self
            .get_json(&format!("/cosmos/base/tendermint/v1beta1/blocks/{height}"))
            .await?;
        validate_header(&resp, Some(height), false, &self.expected_chain_id)?;
        normalize_block_hash(&resp.block_id.hash)
    }
}
fn percent_encode(raw: &str) -> String {
    raw.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
#[derive(Deserialize)]
struct BlockResponse {
    block_id: BlockId,
    block: Option<BlockInner>,
    sdk_block: Option<BlockInner>,
}
#[derive(Deserialize)]
struct BlockId {
    hash: String,
}
#[derive(Deserialize)]
struct BlockInner {
    header: Header,
    data: Option<BlockData>,
}
#[derive(Deserialize)]
struct BlockData {
    txs: serde_json::Value,
}
#[derive(Deserialize)]
struct Header {
    height: String,
    chain_id: String,
    time: String,
}
fn validate_header(
    resp: &BlockResponse,
    expected: Option<i64>,
    require_fresh: bool,
    expected_chain: &str,
) -> LedgerResult<i64> {
    let block = resp
        .sdk_block
        .as_ref()
        .or(resp.block.as_ref())
        .ok_or(LedgerError::Provider("missing_block_header"))?;
    if let (Some(legacy), Some(sdk)) = (&resp.block, &resp.sdk_block) {
        if legacy.header.height != sdk.header.height
            || legacy.header.chain_id != sdk.header.chain_id
        {
            return Err(LedgerError::Provider("ambiguous_block_headers"));
        }
    }
    let h = block
        .header
        .height
        .parse::<i64>()
        .map_err(|_| LedgerError::Provider("invalid_block_height"))?;
    if h <= 0 || expected.is_some_and(|e| e != h) || block.header.chain_id != expected_chain {
        return Err(LedgerError::Provider("wrong_terra_network_or_height"));
    }
    let timestamp = chrono::DateTime::parse_from_rfc3339(&block.header.time)
        .map_err(|_| LedgerError::Provider("invalid_block_time"))?
        .timestamp();
    if require_fresh {
        crate::health::validate_head_time(timestamp)?;
    }
    Ok(h)
}
#[derive(Deserialize)]
struct Search {
    tx_responses: Vec<LcdTx>,
    pagination: Option<Pagination>,
    total: Option<String>,
}
#[derive(Deserialize)]
struct Pagination {
    next_key: Option<String>,
    total: String,
}
#[derive(Debug, Deserialize, Clone)]
pub struct LcdTx {
    pub txhash: Option<String>,
    pub height: String,
    pub code: u32,
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
    fn validate(&self, height: i64) -> LedgerResult<()> {
        if self.height.parse::<i64>().ok() != Some(height)
            || self
                .txhash
                .as_ref()
                .is_none_or(|s| s.is_empty() || s.len() > 128)
            || (self.code == 0 && self.logs.is_none() && self.events.is_none())
        {
            return Err(LedgerError::Provider(
                "invalid_execution_identity_or_events",
            ));
        }
        self.wasm_events()?;
        Ok(())
    }
    pub fn tx_hash(&self) -> String {
        self.txhash
            .as_ref()
            .map(|hash| {
                if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                    hash.to_ascii_uppercase()
                } else {
                    hash.clone()
                }
            })
            .unwrap_or_default()
    }
    pub fn wasm_events(&self) -> LedgerResult<Vec<Cw20Event>> {
        if self.code != 0 {
            return Ok(Vec::new());
        }
        let normalize = |events: Vec<&LcdEvent>| {
            events
                .into_iter()
                .filter(|e| matches!(e.event_type.as_str(), "wasm" | "wasm-wasm"))
                .map(to_cw20)
                .collect::<Vec<_>>()
        };
        let logs = normalize(
            self.logs
                .iter()
                .flatten()
                .flat_map(|l| l.events.iter())
                .collect(),
        );
        let raw = normalize(self.events.iter().flatten().collect());
        if !logs.is_empty() && !raw.is_empty() && semantic_stream(&logs) != semantic_stream(&raw) {
            return Err(LedgerError::Provider("ambiguous_execution_event_stream"));
        }
        Ok(if logs.is_empty() { raw } else { logs })
    }
    fn retained_bytes(&self) -> usize {
        fn event(e: &LcdEvent) -> usize {
            std::mem::size_of::<LcdEvent>()
                + e.event_type.capacity()
                + 32
                + e.attributes.capacity() * std::mem::size_of::<LcdAttr>()
                + e.attributes
                    .iter()
                    .map(|a| a.key.capacity() + a.value.capacity() + 64)
                    .sum::<usize>()
        }
        std::mem::size_of::<Self>()
            + self.height.capacity()
            + self.txhash.as_ref().map_or(0, String::capacity)
            + 128
            + self.logs.as_ref().map_or(0, |logs| {
                logs.capacity() * std::mem::size_of::<LcdLog>()
                    + logs
                        .iter()
                        .flat_map(|l| &l.events)
                        .map(event)
                        .sum::<usize>()
            })
            + self
                .events
                .as_ref()
                .map_or(0, |events| events.iter().map(event).sum::<usize>())
    }
}
fn semantic_stream(events: &[Cw20Event]) -> Vec<Vec<Option<String>>> {
    events
        .iter()
        .map(|e| {
            let last = |key: &str| {
                e.attributes
                    .iter()
                    .rev()
                    .find(|a| a.key == key)
                    .map(|a| a.value.clone())
            };
            let action = last("action");
            let from = if action.as_deref() == Some("mint") {
                Some(String::new())
            } else {
                last("from")
                    .or_else(|| last("sender"))
                    .map(|v| v.to_ascii_lowercase())
            };
            let to = if action.as_deref() == Some("burn") {
                Some(String::new())
            } else {
                last("to")
                    .or_else(|| last("recipient"))
                    .map(|v| v.to_ascii_lowercase())
            };
            if matches!(
                action.as_deref(),
                Some("transfer" | "send" | "mint" | "burn")
            ) {
                vec![
                    last("_contract_address").map(|v| v.to_ascii_lowercase()),
                    action,
                    from,
                    to,
                    last("amount"),
                ]
            } else {
                vec![
                    last("_contract_address").map(|v| v.to_ascii_lowercase()),
                    action,
                ]
            }
        })
        .collect()
}
fn decode_attr(s: &str) -> String {
    // Tendermint's legacy base64 attributes encode keys and values together.
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .filter(|s| s.bytes().all(|b| b.is_ascii_graphic() || b == b' '))
        .unwrap_or_else(|| s.to_string())
}
fn to_cw20(e: &LcdEvent) -> Cw20Event {
    let encoded = e
        .attributes
        .iter()
        .any(|a| decode_attr(&a.key) == "_contract_address" && a.key != "_contract_address");
    Cw20Event {
        event_type: e.event_type.clone(),
        attributes: e
            .attributes
            .iter()
            .map(|a| Cw20Attribute {
                key: if encoded {
                    decode_attr(&a.key)
                } else {
                    a.key.clone()
                },
                value: if encoded {
                    decode_attr(&a.value)
                } else {
                    a.value.clone()
                },
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
    async fn terra_live_balance(&self, address: &str) -> LedgerResult<LiveBalanceSnapshot> {
        self.lcd.live_snapshot(&self.cl8y_token, address).await
    }
    async fn bsc_live_balance(&self, _: &str) -> LedgerResult<LiveBalanceSnapshot> {
        Err(LedgerError::Provider("wrong_balance_chain"))
    }
}
#[cfg(test)]
mod execution_tests {
    use super::*;
    #[test]
    fn failed_execution_and_base64_empty_logs_fallback() {
        let attrs=[("_contract_address",crate::config::DEFAULT_CL8Y_CW20),("action","transfer"),("from","terra1alice"),("to","terra1bob"),("amount","5")].map(|(k,v)|serde_json::json!({"key":base64::engine::general_purpose::STANDARD.encode(k),"value":base64::engine::general_purpose::STANDARD.encode(v)}));
        let mut tx:LcdTx=serde_json::from_value(serde_json::json!({"txhash":"REAL_HASH","height":"100","code":0,"logs":[],"events":[{"type":"wasm","attributes":attrs}]})).unwrap();
        tx.validate(100).unwrap();
        let parsed = crate::parser::parse_cw20_wasm_events(
            &tx.wasm_events().unwrap(),
            crate::config::DEFAULT_CL8Y_CW20,
            &["terra1alice"],
        )
        .unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].amount, BigInt::from(5));
        tx.code = 5;
        assert!(tx.wasm_events().unwrap().is_empty());
        tx.validate(100).unwrap();
        tx.txhash = None;
        assert!(tx.validate(100).is_err());
    }
    #[test]
    fn missing_execution_identity_or_event_schema_is_never_empty_success() {
        let tx: LcdTx = serde_json::from_value(
            serde_json::json!({"txhash":"REAL_HASH","height":"100","code":0}),
        )
        .unwrap();
        assert!(tx.validate(100).is_err());
        assert!(tx.validate(101).is_err());
    }
}

/// SDK JSON byte fields use base64; Comet RPC uses hex. Store one byte identity.
pub fn normalize_block_hash(raw: &str) -> LedgerResult<String> {
    if raw.len() == 64 && raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(raw.to_ascii_uppercase());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|_| LedgerError::Provider("invalid_block_hash"))?;
    if bytes.len() != 32 {
        return Err(LedgerError::Provider("invalid_block_hash"));
    }
    Ok(hex::encode_upper(bytes))
}

pub const MAX_BLOCK_RETAINED_BYTES: usize = 64 * 1024 * 1024;

pub struct TerraBlockSnapshot {
    pub hash: String,
    pub executions: Vec<LcdTx>,
    pub tip: i64,
    pub source: LcdClient,
}
