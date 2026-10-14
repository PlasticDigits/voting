use crate::amount::parse_hex_uint256;
use crate::config::normalize_evm_addr;
use crate::error::{LedgerError, LedgerResult};
use crate::parser::{Bep20Log, TRANSFER_TOPIC};
use crate::register::{LiveBalanceSnapshot, LiveBalanceSource, LiveBalancesSnapshot};
use async_trait::async_trait;
use num_bigint::BigInt;
use serde_json::{json, Value};
/// Explicit policy: index heads at least 15 blocks deep; freshness <=120s.
pub const BSC_FINALITY_DEPTH: i64 = 15;
/// Per-observation header fanout; the entire observation stays on one provider.
pub const BSC_HASH_CONCURRENCY: usize = 8;
/// Headers plus the independently validated end hash and log request.
pub const BSC_RANGE_RPC_CONCURRENCY: usize = BSC_HASH_CONCURRENCY + 2;
#[derive(Clone)]
pub struct BscClient {
    http: reqwest::Client,
    urls: Vec<String>,
}
impl BscClient {
    pub fn new(urls: Vec<String>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(8))
                .build()
                .expect("reqwest"),
            urls,
        }
    }
    pub fn enabled(&self) -> bool {
        !self.urls.is_empty()
    }
    async fn at(&self, url: &str, method: &str, params: Value) -> LedgerResult<Value> {
        let response = self
            .http
            .post(url)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .send()
            .await
            .map_err(|_| LedgerError::Provider("bsc_transport"))?;
        let envelope: Value = crate::transport::decode(response).await?;
        if envelope.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || envelope.get("id") != Some(&json!(1))
        {
            return Err(LedgerError::Provider("rpc_invalid_envelope"));
        }
        if let Some(error) = envelope.get("error") {
            // Fixed labels only: provider messages can contain URLs or credentials.
            let class = match error.get("code").and_then(Value::as_i64) {
                Some(-32005) => "rpc_limit",
                Some(-32601) => "rpc_method_unavailable",
                Some(-32602) => "rpc_parameters_or_range",
                Some(-32603) => "rpc_internal_error",
                _ => "rpc_error",
            };
            return Err(LedgerError::Provider(class));
        }
        envelope
            .get("result")
            .filter(|v| !v.is_null())
            .cloned()
            .ok_or(LedgerError::Provider("rpc_missing_result"))
    }
    async fn rpc(&self, method: &str, params: Value) -> LedgerResult<Value> {
        let mut last = LedgerError::Provider("no_bsc_provider");
        for url in &self.urls {
            match self.at(url, "eth_chainId", json!([])).await {
                Ok(v) if v.as_str() == Some("0x38") => {}
                Ok(_) => {
                    last = LedgerError::Provider("wrong_bsc_network");
                    continue;
                }
                Err(error) => {
                    last = error;
                    continue;
                }
            }
            match self.at(url, method, params.clone()).await {
                Ok(v) => match validate_rpc_result(method, &v, &params) {
                    Ok(()) => {
                        if method == "eth_getLogs" {
                            if let Err(e) = self.validate_log_hashes_at(url, &v).await {
                                last = e;
                                continue;
                            }
                        }
                        return Ok(v);
                    }
                    Err(e) => last = e,
                },
                Err(e) => last = e,
            }
        }
        Err(last)
    }
    pub async fn block_number(&self) -> LedgerResult<i64> {
        self.block_number_at_least(0).await
    }
    /// Fail over semantically when a fresh provider is behind committed finality.
    pub async fn block_number_at_least(&self, minimum: i64) -> LedgerResult<i64> {
        let mut last = LedgerError::Provider("no_bsc_provider");
        for url in &self.urls {
            let observation = async {
                self.check_network_at(url).await?;
                self.head_at(url, minimum).await
            }
            .await;
            match observation {
                Ok(v) => return Ok(v),
                Err(e) => last = e,
            }
        }
        Err(last)
    }
    async fn check_network_at(&self, url: &str) -> LedgerResult<()> {
        if self.at(url, "eth_chainId", json!([])).await?.as_str() != Some("0x38") {
            return Err(LedgerError::Provider("wrong_bsc_network"));
        }
        Ok(())
    }
    // Caller validates the network around the complete pinned observation.
    async fn head_at(&self, url: &str, minimum: i64) -> LedgerResult<i64> {
        let height = quantity(
            &self.at(url, "eth_blockNumber", json!([])).await?,
            "invalid_block_number",
        )?;
        let block = self
            .at(
                url,
                "eth_getBlockByNumber",
                json!([format!("0x{height:x}"), false]),
            )
            .await?;
        if quantity(
            block
                .get("number")
                .ok_or(LedgerError::Provider("missing_block_number"))?,
            "invalid_block_number",
        )? != height
        {
            return Err(LedgerError::Provider("wrong_head_block"));
        }
        hash(block.get("hash"), "invalid_block_hash")?;
        let timestamp = quantity(
            block
                .get("timestamp")
                .ok_or(LedgerError::Provider("missing_block_time"))?,
            "invalid_block_time",
        )?;
        crate::health::validate_head_time(timestamp)?;
        if height < minimum {
            return Err(LedgerError::Provider("provider_head_regressed"));
        }
        Ok(height)
    }
    async fn block_hash_at(&self, url: &str, height: i64) -> LedgerResult<String> {
        let block = self
            .at(
                url,
                "eth_getBlockByNumber",
                json!([format!("0x{height:x}"), false]),
            )
            .await?;
        if quantity(
            block
                .get("number")
                .ok_or(LedgerError::Provider("missing_block_number"))?,
            "invalid_block_number",
        )? != height
        {
            return Err(LedgerError::Provider("wrong_block_number"));
        }
        hash(block.get("hash"), "invalid_block_hash")
    }
    /// Final same-source check runs after accounting SQL, before publication.
    pub async fn finish_range(&self, height: i64, expected_hash: &str) -> LedgerResult<i64> {
        let url = self
            .urls
            .first()
            .filter(|_| self.urls.len() == 1)
            .ok_or(LedgerError::Provider("unpinned_bsc_observation"))?;
        self.check_network_at(url).await?;
        let (tip, end_hash) = tokio::try_join!(
            self.head_at(url, height.saturating_add(BSC_FINALITY_DEPTH)),
            self.block_hash_at(url, height)
        )?;
        if end_hash != expected_hash {
            return Err(LedgerError::Provider("bsc_changed_during_ingest"));
        }
        Ok(tip)
    }
    pub async fn block_hash(&self, height: i64) -> LedgerResult<String> {
        let v = self
            .rpc(
                "eth_getBlockByNumber",
                json!([format!("0x{height:x}"), false]),
            )
            .await?;
        if quantity(
            v.get("number")
                .ok_or(LedgerError::Provider("missing_block_number"))?,
            "invalid_block_number",
        )? != height
        {
            return Err(LedgerError::Provider("wrong_block_number"));
        }
        hash(v.get("hash"), "invalid_block_hash")
    }
    async fn block_hashes_at(
        &self,
        url: &str,
        heights: &[i64],
    ) -> LedgerResult<Vec<(i64, String)>> {
        let mut out = Vec::with_capacity(heights.len());
        for batch in heights.chunks(BSC_HASH_CONCURRENCY) {
            let mut requests = tokio::task::JoinSet::new();
            for &height in batch {
                let client = self.clone();
                let url = url.to_owned();
                requests.spawn(async move {
                    let block = client
                        .at(
                            &url,
                            "eth_getBlockByNumber",
                            json!([format!("0x{height:x}"), false]),
                        )
                        .await?;
                    if quantity(
                        block
                            .get("number")
                            .ok_or(LedgerError::Provider("missing_block_number"))?,
                        "invalid_block_number",
                    )? != height
                    {
                        return Err(LedgerError::Provider("wrong_block_number"));
                    }
                    Ok((height, hash(block.get("hash"), "invalid_block_hash")?))
                });
            }
            let mut first_error = None;
            while let Some(observation) = requests.join_next().await {
                match observation {
                    Ok(Ok(observation)) => out.push(observation),
                    Ok(Err(error)) => {
                        first_error.get_or_insert(error);
                    }
                    Err(_) => {
                        first_error.get_or_insert(LedgerError::Provider("hash_worker_exit"));
                    }
                }
            }
            // Drain this bounded batch before failover so abandoned HTTP requests
            // do not overlap the next observation on the same provider.
            if let Some(error) = first_error {
                return Err(error);
            }
        }
        // HTTP completion order must not alter canonical checkpoint order.
        out.sort_unstable_by_key(|(height, _)| *height);
        Ok(out)
    }
    pub async fn balance_of(&self, token: &str, wallet: &str) -> LedgerResult<BigInt> {
        self.balance_at(token, wallet, "latest").await
    }
    async fn balance_at(&self, token: &str, wallet: &str, tag: &str) -> LedgerResult<BigInt> {
        let token = normalize_evm_addr(token)?;
        let wallet = normalize_evm_addr(wallet)?;
        let data = format!("0x70a08231{:0>64}", wallet.trim_start_matches("0x"));
        let v = self
            .rpc("eth_call", json!([{"to":token,"data":data},tag]))
            .await?;
        let raw = v.as_str().ok_or(LedgerError::Provider("balance_schema"))?;
        if raw.len() != 66 {
            return Err(LedgerError::Provider("balance_uint256_width"));
        }
        parse_hex_uint256(raw)
    }
    pub async fn live_snapshot(
        &self,
        token: &str,
        wallet: &str,
    ) -> LedgerResult<LiveBalanceSnapshot> {
        for url in &self.urls {
            let pinned = Self {
                http: self.http.clone(),
                urls: vec![url.clone()],
            };
            let observation = async {
                let h = pinned.block_number().await?;
                let before = pinned.block_hash(h).await?;
                let amount = pinned
                    .balance_at(token, wallet, &format!("0x{h:x}"))
                    .await?;
                if pinned.block_hash(h).await? != before {
                    return Err(LedgerError::Provider("moving_live_snapshot"));
                }
                Ok(LiveBalanceSnapshot {
                    height: h,
                    amount,
                    block_hash: before,
                })
            }
            .await;
            if observation.is_ok() {
                return observation;
            }
        }
        Err(LedgerError::Provider("incoherent_live_snapshot"))
    }
    pub async fn live_balances(
        &self,
        token: &str,
        wallets: &[String],
    ) -> LedgerResult<LiveBalancesSnapshot> {
        for url in &self.urls {
            let pinned = Self {
                http: self.http.clone(),
                urls: vec![url.clone()],
            };
            let observation = async {
                let height = pinned.block_number().await?;
                let before = pinned.block_hash(height).await?;
                let tag = format!("0x{height:x}");
                let mut out = Vec::new();
                for batch in wallets.chunks(8) {
                    let pinned = &pinned;
                    let tag = &tag;
                    let read = |i: usize| async move {
                        match batch.get(i) {
                            Some(wallet) => pinned
                                .balance_at(token, wallet, &tag)
                                .await
                                .map(|b| Some((height, b))),
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
    async fn validate_log_hashes_at(&self, url: &str, logs: &Value) -> LedgerResult<()> {
        let mut hashes = std::collections::HashMap::new();
        for log in logs
            .as_array()
            .ok_or(LedgerError::Provider("logs_schema"))?
        {
            let height = quantity(
                log.get("blockNumber")
                    .ok_or(LedgerError::Provider("missing_log_block"))?,
                "invalid_log_block",
            )?;
            let actual = if let Some(hash) = hashes.get(&height) {
                hash
            } else {
                let block = self
                    .at(
                        url,
                        "eth_getBlockByNumber",
                        json!([format!("0x{height:x}"), false]),
                    )
                    .await?;
                if quantity(
                    block
                        .get("number")
                        .ok_or(LedgerError::Provider("missing_block_number"))?,
                    "invalid_block_number",
                )? != height
                {
                    return Err(LedgerError::Provider("wrong_block_number"));
                }
                let h = hash(block.get("hash"), "invalid_block_hash")?;
                hashes.insert(height, h);
                hashes.get(&height).unwrap()
            };
            if *actual != hash(log.get("blockHash"), "invalid_log_block_hash")? {
                return Err(LedgerError::Provider("noncanonical_log"));
            }
        }
        Ok(())
    }
    /// Logs, pending registration baselines, tip/finality and all cursor hashes
    /// come from one validated provider; semantic bad 200s fall through alternatives.
    pub async fn canonical_range(
        &self,
        token: &str,
        from: i64,
        to: i64,
        baselines: &[(i64, Option<String>)],
    ) -> LedgerResult<BscRangeSnapshot> {
        self.canonical_range_plan(token, from, to, baselines, false)
            .await
    }
    /// A bounded forward range chooses its end from the selected source's fresh head.
    pub async fn canonical_next_range(
        &self,
        token: &str,
        from: i64,
        maximum_to: i64,
        baselines: &[(i64, Option<String>)],
    ) -> LedgerResult<BscRangeSnapshot> {
        self.canonical_range_plan(token, from, maximum_to, baselines, true)
            .await
    }
    async fn canonical_range_plan(
        &self,
        token: &str,
        from: i64,
        maximum_to: i64,
        baselines: &[(i64, Option<String>)],
        plan: bool,
    ) -> LedgerResult<BscRangeSnapshot> {
        let mut last = LedgerError::Provider("no_bsc_provider");
        let mut orphaned = false;
        for url in &self.urls {
            let source = Self {
                http: self.http.clone(),
                urls: vec![url.clone()],
            };
            let observed = async {
                source.check_network_at(url).await?;
                let tip = source
                    .head_at(url, from.saturating_add(BSC_FINALITY_DEPTH))
                    .await?;
                let to = if plan {
                    maximum_to.min(tip.saturating_sub(BSC_FINALITY_DEPTH))
                } else {
                    maximum_to
                };
                if to > tip.saturating_sub(BSC_FINALITY_DEPTH) || from > to {
                    return Err(LedgerError::Provider("unfinalized_bsc_range"));
                }
                for (height, expected) in baselines.iter().filter(|(height, _)| *height <= to) {
                    if expected.as_ref() != Some(&source.block_hash_at(url, *height).await?) {
                        return Err(LedgerError::Provider("registration_baseline_orphaned"));
                    }
                }
                let heights = (from.max(to.saturating_sub(128))..=to).collect::<Vec<_>>();
                let (end_hash, raw_logs, hashes) = tokio::join!(
                    source.block_hash_at(url, to),
                    source.transfer_logs_inner(token, from, to, Some(url)),
                    source.block_hashes_at(url, &heights)
                );
                let (end_hash, raw_logs, hashes) = (end_hash?, raw_logs?, hashes?);
                let mut by_height = hashes
                    .iter()
                    .cloned()
                    .collect::<std::collections::HashMap<_, _>>();
                for (log, expected_hash) in &raw_logs {
                    // Standalone callers may request a range larger than retained
                    // history; validate each older log block once without retaining it.
                    if !by_height.contains_key(&log.block_number) {
                        by_height.insert(
                            log.block_number,
                            source.block_hash_at(url, log.block_number).await?,
                        );
                    }
                    if by_height.get(&log.block_number) != Some(expected_hash) {
                        return Err(LedgerError::Provider("wrong_log_block_hash"));
                    }
                }
                if by_height.get(&to) != Some(&end_hash) {
                    return Err(LedgerError::Provider("bsc_changed_during_ingest"));
                }
                source.check_network_at(url).await?;
                if source.block_hash_at(url, to).await? != end_hash {
                    return Err(LedgerError::Provider("bsc_changed_during_ingest"));
                }
                let logs = raw_logs.into_iter().map(|(log, _)| log).collect();
                Ok((to, tip, end_hash, logs, hashes))
            }
            .await;
            match observed {
                Ok((to, tip, end_hash, logs, hashes)) => {
                    return Ok(BscRangeSnapshot {
                        to,
                        tip,
                        end_hash,
                        logs,
                        hashes,
                        source,
                    })
                }
                Err(LedgerError::Provider("registration_baseline_orphaned")) => {
                    orphaned = true;
                    last = LedgerError::Provider("registration_baseline_orphaned");
                }
                Err(e) => last = e,
            }
        }
        if orphaned {
            return Err(LedgerError::Provider("registration_baseline_orphaned"));
        }
        Err(last)
    }
    pub async fn transfer_logs(
        &self,
        token: &str,
        from: i64,
        to: i64,
    ) -> LedgerResult<Vec<Bep20Log>> {
        Ok(self
            .transfer_logs_inner(token, from, to, None)
            .await?
            .into_iter()
            .map(|(log, _)| log)
            .collect())
    }
    async fn transfer_logs_inner(
        &self,
        token: &str,
        from: i64,
        to: i64,
        pinned_url: Option<&str>,
    ) -> LedgerResult<Vec<(Bep20Log, String)>> {
        let token = normalize_evm_addr(token)?;
        let mut ranges = vec![(from, to)];
        let mut out = Vec::new();
        let mut requests = 0;
        while let Some((lo, hi)) = ranges.pop() {
            requests += 1;
            if requests > 4096 || out.len() > 100_000 {
                return Err(LedgerError::Provider("log_work_limit"));
            }
            let params = json!([{"address":token,"fromBlock":format!("0x{lo:x}"),"toBlock":format!("0x{hi:x}"),"topics":[TRANSFER_TOPIC]}]);
            let response = match pinned_url {
                Some(url) => self
                    .at(url, "eth_getLogs", params.clone())
                    .await
                    .and_then(|v| validate_rpc_result("eth_getLogs", &v, &params).map(|_| v)),
                None => self.rpc("eth_getLogs", params).await,
            };
            let v = match response {
                Ok(v) => v,
                Err(_) if lo < hi => {
                    let mid = lo + (hi - lo) / 2;
                    ranges.push((mid + 1, hi));
                    ranges.push((lo, mid));
                    continue;
                }
                Err(e) => return Err(e),
            };
            let arr = v.as_array().ok_or(LedgerError::Provider("logs_schema"))?;
            for item in arr {
                let log = parse_rpc_log(item)?;
                if normalize_evm_addr(&log.address)? != token
                    || log.block_number < lo
                    || log.block_number > hi
                {
                    return Err(LedgerError::Provider("wrong_token_or_log_range"));
                }
                out.push((log, hash(item.get("blockHash"), "invalid_log_block_hash")?));
            }
        }
        out.sort_by_key(|(l, _)| (l.block_number, l.log_index));
        let mut identities = std::collections::HashSet::new();
        for (l, _) in &out {
            if !identities.insert((l.block_number, l.log_index)) {
                return Err(LedgerError::Provider("duplicate_log_identity"));
            }
        }
        Ok(out)
    }
}
fn quantity(v: &Value, class: &'static str) -> LedgerResult<i64> {
    let s = v
        .as_str()
        .and_then(|s| s.strip_prefix("0x"))
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or(LedgerError::Provider(class))?;
    i64::from_str_radix(s, 16).map_err(|_| LedgerError::Provider(class))
}
fn hash(v: Option<&Value>, class: &'static str) -> LedgerResult<String> {
    let s = v
        .and_then(Value::as_str)
        .filter(|s| {
            s.len() == 66 && s.starts_with("0x") && s[2..].bytes().all(|b| b.is_ascii_hexdigit())
        })
        .ok_or(LedgerError::Provider(class))?;
    Ok(s.to_ascii_lowercase())
}
fn parse_rpc_log(v: &Value) -> LedgerResult<Bep20Log> {
    if v.get("removed").and_then(Value::as_bool) != Some(false) {
        return Err(LedgerError::Provider("removed_or_missing_log_status"));
    }
    let topics = v
        .get("topics")
        .and_then(Value::as_array)
        .ok_or(LedgerError::Provider("missing_topics"))?
        .iter()
        .map(|t| hash(Some(t), "invalid_topic"))
        .collect::<LedgerResult<Vec<_>>>()?;
    if topics.len() != 3 || topics[0] != TRANSFER_TOPIC {
        return Err(LedgerError::Provider("invalid_transfer_topics"));
    }
    if topics[1..]
        .iter()
        .any(|t| !t[2..26].bytes().all(|b| b == b'0'))
    {
        return Err(LedgerError::Provider("invalid_address_topic"));
    }
    let data = hash(v.get("data"), "invalid_log_uint256")?;
    let address = v
        .get("address")
        .and_then(Value::as_str)
        .ok_or(LedgerError::Provider("missing_emitter"))?;
    let block_number = quantity(
        v.get("blockNumber")
            .ok_or(LedgerError::Provider("missing_log_block"))?,
        "invalid_log_block",
    )?;
    let index = quantity(
        v.get("logIndex")
            .ok_or(LedgerError::Provider("missing_log_index"))?,
        "invalid_log_index",
    )?;
    if block_number <= 0 || index > i32::MAX as i64 {
        return Err(LedgerError::Provider("invalid_log_identity"));
    }
    Ok(Bep20Log {
        address: normalize_evm_addr(address)?,
        topics,
        data,
        block_number,
        transaction_hash: hash(v.get("transactionHash"), "invalid_tx_hash")?,
        log_index: index as i32,
    })
}
pub struct BscBalanceSource {
    pub client: BscClient,
    pub token: String,
}
#[async_trait]
impl LiveBalanceSource for BscBalanceSource {
    async fn terra_live_balance(&self, _: &str) -> LedgerResult<LiveBalanceSnapshot> {
        Err(LedgerError::Provider("wrong_balance_chain"))
    }
    async fn bsc_live_balance(&self, address: &str) -> LedgerResult<LiveBalanceSnapshot> {
        self.client.live_snapshot(&self.token, address).await
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
    async fn bulk_live_balances(
        &self,
        chain: crate::db::Chain,
        wallets: &[String],
    ) -> LedgerResult<LiveBalancesSnapshot> {
        match chain {
            crate::db::Chain::Terra => {
                let src = self
                    .terra
                    .as_ref()
                    .ok_or(LedgerError::Provider("no_terra_provider"))?;
                src.lcd.live_balances(&src.token, wallets).await
            }
            crate::db::Chain::Bsc => {
                let src = self
                    .bsc
                    .as_ref()
                    .ok_or(LedgerError::Provider("no_bsc_provider"))?;
                src.client.live_balances(&src.token, wallets).await
            }
        }
    }

    async fn terra_live_balance(&self, address: &str) -> LedgerResult<LiveBalanceSnapshot> {
        let src = self
            .terra
            .as_ref()
            .ok_or(LedgerError::Provider("no_terra_provider"))?;
        src.lcd.live_snapshot(&src.token, address).await
    }
    async fn bsc_live_balance(&self, address: &str) -> LedgerResult<LiveBalanceSnapshot> {
        let src = self
            .bsc
            .as_ref()
            .ok_or(LedgerError::Provider("no_bsc_provider"))?;
        src.bsc_live_balance(address).await
    }
}

fn validate_rpc_result(method: &str, v: &Value, params: &Value) -> LedgerResult<()> {
    match method {
        "eth_blockNumber" => {
            if quantity(v, "invalid_block_number")? <= 0 {
                return Err(LedgerError::Provider("invalid_block_number"));
            }
        }
        "eth_getBlockByNumber" => {
            hash(v.get("hash"), "invalid_block_hash")?;
            quantity(
                v.get("number")
                    .ok_or(LedgerError::Provider("missing_block_number"))?,
                "invalid_block_number",
            )?;
        }
        "eth_call" => {
            hash(Some(v), "balance_uint256_width")?;
        }
        "eth_getLogs" => {
            for log in v.as_array().ok_or(LedgerError::Provider("logs_schema"))? {
                let parsed = parse_rpc_log(log)?;
                hash(log.get("blockHash"), "invalid_log_block_hash")?;
                let filter = params
                    .get(0)
                    .ok_or(LedgerError::Provider("missing_log_filter"))?;
                let emitter = filter
                    .get("address")
                    .and_then(Value::as_str)
                    .ok_or(LedgerError::Provider("missing_log_filter"))?;
                let lo = quantity(
                    filter
                        .get("fromBlock")
                        .ok_or(LedgerError::Provider("missing_log_filter"))?,
                    "invalid_log_filter",
                )?;
                let hi = quantity(
                    filter
                        .get("toBlock")
                        .ok_or(LedgerError::Provider("missing_log_filter"))?,
                    "invalid_log_filter",
                )?;
                if parsed.address != normalize_evm_addr(emitter)?
                    || parsed.block_number < lo
                    || parsed.block_number > hi
                {
                    return Err(LedgerError::Provider("wrong_token_or_log_range"));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Clone)]
pub struct BscRangeSnapshot {
    pub to: i64,
    pub tip: i64,
    pub end_hash: String,
    pub logs: Vec<Bep20Log>,
    pub hashes: Vec<(i64, String)>,
    pub source: BscClient,
}
