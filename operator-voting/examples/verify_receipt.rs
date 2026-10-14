//! Offline verification: curl -fsS API/v1/receipts/UUID | cargo run -p operator-voting --example verify_receipt
use std::io::Read;
fn main() {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("read receipt JSON");
    let receipt: serde_json::Value = serde_json::from_str(&input).expect("parse receipt JSON");
    if operator_voting::payload::verify_exported_receipt(&receipt).is_err() {
        eprintln!("receipt verification failed");
        std::process::exit(1);
    }
    println!("verified wallet signature and signed action/content links");
}
