//! Read-only native balance observation, suitable for bao-exec injected URL env.
use voting_ledger::{
    config::{split_urls, DEFAULT_CL8Y_CW20},
    lcd::LcdClient,
    LedgerError,
};
#[tokio::main]
async fn main() -> Result<(), LedgerError> {
    let urls = split_urls(
        &std::env::var("TERRA_RPC_URLS")
            .or_else(|_| std::env::var("TERRA_RPC_URL"))
            .map_err(|_| LedgerError::MissingEnv("TERRA_RPC_URLS"))?,
    );
    let lcd = LcdClient::new(vec![]).with_rpc_urls(urls);
    let snapshot = lcd
        .live_snapshot(DEFAULT_CL8Y_CW20, DEFAULT_CL8Y_CW20)
        .await?;
    println!(
        "{}",
        serde_json::json!({"ok":true,"chain":"terra","height":snapshot.height,"hash_bytes":snapshot.block_hash.len()/2,"unsigned_balance":snapshot.amount>=num_bigint::BigInt::from(0),"observation":"native_abci_response_height"})
    );
    Ok(())
}
