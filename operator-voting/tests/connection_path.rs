//! Every API connection must read canonical public ledger tables despite hostile role/URL paths.
#[tokio::test]
async fn api_pool_pins_every_connection_against_legacy_shadow_tables() {
    let Some(url) = voting_ledger::test_lock::test_database_url() else {
        return;
    };
    let _lock = voting_ledger::test_lock::hold_integration_db(&url)
        .await
        .unwrap();
    let admin = voting_ledger::test_lock::test_pool(&url).await.unwrap();
    voting_ledger::db::migrate(&admin).await.unwrap();
    let actor = "terra1api_pool_fixture";
    sqlx::query("INSERT INTO public.voting_registrations(chain,wallet_address,registered_at_height,initial_balance,status) VALUES('terra',$1,7,1000,'active') ON CONFLICT(chain,wallet_address) DO UPDATE SET initial_balance=1000,registered_at_height=7").bind(actor).execute(&admin).await.unwrap();
    // This is an intentionally preserved old-shape user-schema shadow, not a canonical table.
    sqlx::raw_sql("CREATE TABLE voting.voting_registrations(chain text,wallet_address text,registered_at_height bigint,initial_balance numeric,status text); INSERT INTO voting.voting_registrations VALUES('terra','terra1api_pool_fixture',1,999,'active');").execute(&admin).await.unwrap();
    let separator = if url.contains('?') { "&" } else { "?" };
    let hostile = format!("{url}{separator}options=-c%20search_path%3Dvoting%2Cpg_temp");
    let api = operator_voting::db::connect(&hostile).await.unwrap();
    let mut held = Vec::new();
    for _ in 0..6 {
        let mut conn = api.acquire().await.unwrap();
        let path: String = sqlx::query_scalar("SHOW search_path")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(path, "pg_catalog, public, pg_temp");
        let registration = operator_voting::db::ledger_registration(&mut conn, "terra", actor)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(registration.initial_balance, "1000");
        assert_eq!(registration.registered_at_height, 7);
        held.push(conn);
    }
    drop(held);
    api.close().await;
    sqlx::query("DROP TABLE voting.voting_registrations")
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(
        "DELETE FROM public.voting_registrations WHERE chain='terra' AND wallet_address=$1",
    )
    .bind(actor)
    .execute(&admin)
    .await
    .unwrap();
}
