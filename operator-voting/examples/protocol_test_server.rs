//! Isolated browser middleware fixture, never a production entrypoint.
use operator_voting::{api::AppState, application::application, db, VotingConfig};
#[tokio::main]
async fn main() {
    let raw = std::env::var("LEDGER_TEST_DATABASE_URL").expect("explicit isolated test DB URL");
    let url = url::Url::parse(&raw).expect("URL");
    assert!(std::env::var("LEDGER_TEST_ALLOW_DESTRUCTIVE").as_deref() == Ok("1"));
    assert!(
        matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
            && url.path().starts_with("/voting_test_"),
        "requires a local voting_test_ database"
    );
    let pool = db::connect(&raw).await.unwrap();
    let marker: Option<String> = sqlx::query_scalar("SELECT shobj_description(oid,'pg_database') FROM pg_database WHERE datname=current_database()")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        marker.as_deref(),
        Some("cl8y-voting-ephemeral-test"),
        "database must have explicit ephemeral test marker"
    );
    db::migrate(&pool).await.unwrap();
    sqlx::query("INSERT INTO public.voting_registrations(chain,wallet_address,registered_at_height,initial_balance,status) VALUES('bsc','0x19e7e376e7c213b7e7e7e46cc70a5dd086daff2a',50,1000000000000000000000,'active') ON CONFLICT DO NOTHING").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO public.indexer_state(key,value) VALUES('last_indexed_height','100'),('last_indexed_bsc_block','100') ON CONFLICT(key) DO UPDATE SET value=EXCLUDED.value").execute(&pool).await.unwrap();
    let mut cfg = VotingConfig::for_tests(raw, "");
    cfg.cors_origins = vec!["http://127.0.0.1:5178".into()];
    cfg.rate_limit_post_burst = 100;
    let app = application(AppState { pool, cfg }).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3991")
        .await
        .unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .unwrap();
}
