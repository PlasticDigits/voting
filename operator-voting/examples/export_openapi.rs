fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&operator_voting::contracts::openapi()).unwrap()
    );
}
