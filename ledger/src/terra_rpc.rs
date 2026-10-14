//! Native Comet RPC observations bind CW20 query bytes to response height.
//! Schemas: cosmos-sdk v0.53.6 ABCIQuery; wasmd v0.54.1 SmartContractState.
use crate::{
    amount::parse_raw_amount,
    error::{LedgerError, LedgerResult},
    register::{LiveBalanceSnapshot, LiveBalancesSnapshot},
};
use base64::Engine;
use num_bigint::BigInt;
use serde_json::Value;
#[derive(Clone)]
pub struct TerraRpcClient {
    http: reqwest::Client,
    urls: Vec<String>,
    chain_id: String,
}
impl TerraRpcClient {
    pub fn new(http: reqwest::Client, urls: Vec<String>, chain_id: String) -> Self {
        Self {
            http,
            urls,
            chain_id,
        }
    }
    async fn request(
        &self,
        url: &str,
        path: &str,
        params: &[(&str, String)],
    ) -> LedgerResult<Value> {
        let response = self
            .http
            .get(format!("{}{path}", url.trim_end_matches('/')))
            .header("Cache-Control", "no-cache")
            .query(params)
            .send()
            .await
            .map_err(|_| LedgerError::Provider("terra_rpc_transport"))?;
        let v: Value = crate::transport::decode(response).await?;
        if v.get("error").is_some() {
            return Err(LedgerError::Provider("terra_rpc_error"));
        }
        v.get("result")
            .cloned()
            .ok_or(LedgerError::Provider("terra_rpc_missing_result"))
    }
    async fn network(&self, url: &str) -> LedgerResult<()> {
        let v = self.request(url, "/status", &[]).await?;
        if v.pointer("/node_info/network").and_then(Value::as_str) != Some(self.chain_id.as_str())
            || v.pointer("/sync_info/catching_up").and_then(Value::as_bool) != Some(false)
        {
            return Err(LedgerError::Provider("wrong_or_syncing_terra_rpc"));
        }
        Ok(())
    }
    async fn block(&self, url: &str, height: Option<i64>) -> LedgerResult<(i64, String)> {
        let params = height
            .map(|h| vec![("height", h.to_string())])
            .unwrap_or_default();
        let v = self.request(url, "/block", &params).await?;
        let h = v
            .pointer("/block/header/height")
            .and_then(Value::as_str)
            .and_then(|h| h.parse::<i64>().ok())
            .filter(|h| *h > 0)
            .ok_or(LedgerError::Provider("invalid_terra_rpc_height"))?;
        if height.is_some_and(|expected| expected != h)
            || v.pointer("/block/header/chain_id").and_then(Value::as_str)
                != Some(self.chain_id.as_str())
        {
            return Err(LedgerError::Provider("wrong_terra_rpc_block"));
        }
        let time = v
            .pointer("/block/header/time")
            .and_then(Value::as_str)
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .ok_or(LedgerError::Provider("invalid_terra_rpc_time"))?;
        if height.is_none() {
            crate::health::validate_head_time(time.timestamp())?;
        }
        let hash = v
            .pointer("/block_id/hash")
            .and_then(Value::as_str)
            .ok_or(LedgerError::Provider("missing_terra_rpc_hash"))?;
        Ok((h, crate::lcd::normalize_block_hash(hash)?))
    }
    async fn balance(
        &self,
        url: &str,
        token: &str,
        wallet: &str,
        height: i64,
    ) -> LedgerResult<(i64, BigInt)> {
        let query = serde_json::json!({"balance":{"address":wallet}}).to_string();
        let data = smart_query_bytes(token, query.as_bytes());
        let result = self
            .request(
                url,
                "/abci_query",
                &[
                    (
                        "path",
                        "\"/cosmwasm.wasm.v1.Query/SmartContractState\"".into(),
                    ),
                    ("data", format!("0x{}", hex::encode(data))),
                    ("height", height.to_string()),
                    ("prove", "false".into()),
                ],
            )
            .await?;
        let response = result
            .get("response")
            .ok_or(LedgerError::Provider("missing_abci_response"))?;
        if response.get("code").and_then(Value::as_u64) != Some(0) {
            return Err(LedgerError::Provider("abci_query_error"));
        }
        let observed = response
            .get("height")
            .and_then(Value::as_str)
            .and_then(|h| h.parse::<i64>().ok())
            .filter(|h| *h > 0)
            .ok_or(LedgerError::Provider("invalid_abci_height"))?;
        if height > 0 && observed != height {
            return Err(LedgerError::Provider("unbound_abci_height"));
        }
        let encoded = response
            .get("value")
            .and_then(Value::as_str)
            .ok_or(LedgerError::Provider("missing_abci_value"))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| LedgerError::Provider("invalid_abci_base64"))?;
        let json = decode_smart_response(&bytes)?;
        let v: Value = serde_json::from_slice(json)
            .map_err(|_| LedgerError::Provider("invalid_abci_balance_json"))?;
        let raw = v
            .get("balance")
            .and_then(Value::as_str)
            .ok_or(LedgerError::Provider("invalid_abci_balance_schema"))?;
        Ok((observed, parse_raw_amount(raw)?))
    }
    pub async fn live_snapshot(
        &self,
        token: &str,
        wallet: &str,
    ) -> LedgerResult<LiveBalanceSnapshot> {
        for url in &self.urls {
            for _ in 0..3 {
                let observed = async {
                    self.network(url).await?;
                    let (height, hash) = self.block(url, None).await?;
                    let (observed, amount) = self.balance(url, token, wallet, height).await?;
                    let (after, _) = self.block(url, None).await?;
                    if observed != height
                        || height > after
                        || self.block(url, Some(height)).await?.1 != hash
                    {
                        return Err(LedgerError::Provider("incoherent_terra_rpc_snapshot"));
                    }
                    Ok(LiveBalanceSnapshot {
                        height,
                        amount,
                        block_hash: hash,
                    })
                }
                .await;
                if observed.is_ok() {
                    return observed;
                }
            }
        }
        Err(LedgerError::Provider("terra_rpc_snapshot_unavailable"))
    }
    pub async fn live_balances(
        &self,
        token: &str,
        wallets: &[String],
    ) -> LedgerResult<LiveBalancesSnapshot> {
        for url in &self.urls {
            let observed = async {
                self.network(url).await?;
                let (height, hash) = self.block(url, None).await?;
                let mut balances = Vec::new();
                for batch in wallets.chunks(8) {
                    let read = |i: usize| async move {
                        match batch.get(i) {
                            Some(wallet) => self
                                .balance(url, token, wallet, height)
                                .await
                                .map(|(_, b)| Some(b)),
                            None => Ok(None),
                        }
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
                    for result in [a, b, c, d, e, f, g, h] {
                        if let Some(balance) = result? {
                            balances.push(balance);
                        }
                    }
                }
                if self.block(url, Some(height)).await?.1 != hash {
                    return Err(LedgerError::Provider("terra_rpc_recovery_fork"));
                }
                Ok(LiveBalancesSnapshot {
                    height,
                    block_hash: hash,
                    balances,
                })
            }
            .await;
            if observed.is_ok() {
                return observed;
            }
        }
        Err(LedgerError::Provider("terra_rpc_recovery_unavailable"))
    }
}
fn encode_varint(mut n: usize, out: &mut Vec<u8>) {
    while n >= 128 {
        out.push((n as u8 & 127) | 128);
        n >>= 7;
    }
    out.push(n as u8);
}
pub(crate) fn smart_query_bytes(token: &str, query: &[u8]) -> Vec<u8> {
    let mut bytes = vec![10];
    encode_varint(token.len(), &mut bytes);
    bytes.extend_from_slice(token.as_bytes());
    bytes.push(18);
    encode_varint(query.len(), &mut bytes);
    bytes.extend_from_slice(query);
    bytes
}
fn decode_smart_response(bytes: &[u8]) -> LedgerResult<&[u8]> {
    if bytes.first() != Some(&10) {
        return Err(LedgerError::Provider("invalid_smart_response_tag"));
    }
    let mut size = 0_usize;
    let mut position = 1;
    for shift in (0..=63).step_by(7) {
        let b = *bytes
            .get(position)
            .ok_or(LedgerError::Provider("truncated_smart_response"))?;
        position += 1;
        if shift == 63 && b > 1 {
            return Err(LedgerError::Provider("oversized_smart_response"));
        }
        size |= ((b & 127) as usize)
            .checked_shl(shift)
            .ok_or(LedgerError::Provider("oversized_smart_response"))?;
        if b & 128 == 0 {
            if position.checked_add(size) != Some(bytes.len()) {
                return Err(LedgerError::Provider("invalid_smart_response_length"));
            }
            return Ok(&bytes[position..]);
        }
    }
    Err(LedgerError::Provider("invalid_smart_response_varint"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recorded_abci_value_decodes_exact_json() {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode("Cg97ImJhbGFuY2UiOiIwIn0=")
            .unwrap();
        assert_eq!(
            decode_smart_response(&bytes).unwrap(),
            br#"{"balance":"0"}"#
        );
        assert!(decode_smart_response(&[10, 128]).is_err());
        assert!(decode_smart_response(&[10, 1, b'x', b'y']).is_err());
    }
}
