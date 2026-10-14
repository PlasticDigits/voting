fn main() {
    // sqlx::migrate! embeds the directory at compile time. Track newly added
    // forward migrations as well as edits during incremental Cargo builds.
    println!("cargo:rerun-if-changed=migrations");
}
