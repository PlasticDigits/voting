//! Real TCP authentication must require an explicitly provisioned ephemeral secret.
//! A uniquely named role/database prevents changing any concurrent test role's credentials.
//! The disposable server must enforce password authentication for TCP test roles;
//! localhost pg_hba trust would deliberately fail the refusal assertions.
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    Connection, PgConnection,
};
use std::str::FromStr;

async fn rejected(options: PgConnectOptions) {
    let error = match PgConnection::connect_with(&options).await {
        Ok(connection) => {
            connection.close().await.unwrap();
            panic!("unprovisioned/default credential authenticated")
        }
        Err(error) => error,
    };
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("28P01"),
        "authentication must fail at PostgreSQL, not networking or permissions"
    );
}

#[tokio::test]
async fn null_and_historical_default_passwords_fail_real_tcp_then_ephemeral_secret_authenticates() {
    let Some(url) = voting_ledger::test_lock::test_database_url() else {
        return;
    };
    let _lock = voting_ledger::test_lock::hold_integration_db(&url)
        .await
        .unwrap();
    let admin = voting_ledger::test_lock::test_pool(&url).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let database = format!("voting_test_auth_{suffix}");
    let role = format!("voting_test_actor_{suffix}");
    sqlx::query(&format!("CREATE DATABASE {database}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!(
        "COMMENT ON DATABASE {database} IS 'cl8y-voting-ephemeral-test'"
    ))
    .execute(&admin)
    .await
    .unwrap();
    let child_url = format!("{}/{}", url.rsplit_once('/').unwrap().0, database);
    let child = voting_ledger::test_lock::test_pool(&child_url)
        .await
        .unwrap();
    voting_ledger::db::migrate(&child).await.unwrap();
    // Exercise the exact provisioning file with only the role identifier isolated.
    let grants = include_str!("../../deploy/grants.sql").replace("operator_voting", &role);
    sqlx::raw_sql(&grants).execute(&child).await.unwrap();
    let null_password: bool =
        sqlx::query_scalar("SELECT rolpassword IS NULL FROM pg_authid WHERE rolname=$1")
            .bind(&role)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert!(
        null_password,
        "fresh grants must not create a usable password"
    );
    let options = PgConnectOptions::from_str(&child_url)
        .unwrap()
        .username(&role);
    rejected(options.clone().password("")).await;
    // Historical public development default is deliberately rejected too.
    rejected(options.clone().password("change-me-in-prod")).await;

    // Random synthetic secret remains in memory/bound values; no shell arguments or output.
    let secret = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let mut provision = child.acquire().await.unwrap();
    sqlx::query("SELECT set_config('voting_test.credential',$1,false),set_config('voting_test.actor',$2,false),set_config('password_encryption','scram-sha-256',false)").bind(&secret).bind(&role).execute(&mut *provision).await.unwrap();
    sqlx::raw_sql("DO $$ BEGIN EXECUTE format('ALTER ROLE %I PASSWORD %L',current_setting('voting_test.actor'),current_setting('voting_test.credential')); END $$;").execute(&mut *provision).await.unwrap();
    sqlx::query("RESET voting_test.credential")
        .execute(&mut *provision)
        .await
        .unwrap();
    drop(provision);
    let restricted = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options.clone().password(&secret))
        .await
        .expect("explicit secret authenticates restricted role");
    let identity: (String, String) = sqlx::query_as("SELECT current_user::text,session_user::text")
        .fetch_one(&restricted)
        .await
        .unwrap();
    assert_eq!(identity, (role.clone(), role.clone()));
    operator_voting::db::validate_production_privileges(&restricted)
        .await
        .unwrap();
    let balance: String = sqlx::query_scalar("SELECT public.voting_bsc_cl8y_balance_at('0x0000000000000000000000000000000000000000',1)::text").fetch_one(&restricted).await.unwrap();
    assert_eq!(balance, "0");
    assert!(
        sqlx::query("INSERT INTO public.indexer_state(key,value) VALUES('forged','1')")
            .execute(&restricted)
            .await
            .is_err()
    );
    rejected(options.password("change-me-in-prod")).await;
    restricted.close().await;
    child.close().await;
    sqlx::query(&format!("DROP DATABASE {database}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
}

#[tokio::test]
async fn permanent_table_and_function_shadows_cannot_change_canonical_balance() {
    let Some(url) = voting_ledger::test_lock::test_database_url() else {
        return;
    };
    let _lock = voting_ledger::test_lock::hold_integration_db(&url)
        .await
        .unwrap();
    let admin = voting_ledger::test_lock::test_pool(&url).await.unwrap();
    let database = format!("voting_test_shadow_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {database}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!(
        "COMMENT ON DATABASE {database} IS 'cl8y-voting-ephemeral-test'"
    ))
    .execute(&admin)
    .await
    .unwrap();
    let child_url = format!("{}/{}", url.rsplit_once('/').unwrap().0, database);
    let child = voting_ledger::test_lock::test_pool(&child_url)
        .await
        .unwrap();
    voting_ledger::db::migrate(&child).await.unwrap();
    let address = "0x0000000000000000000000000000000000000042";
    sqlx::query("INSERT INTO public.voting_registrations(chain,wallet_address,registered_at_height,initial_balance,status) VALUES('bsc',$1,100,123,'active')").bind(address).execute(&child).await.unwrap();
    sqlx::raw_sql("CREATE TABLE voting.voting_registrations AS SELECT * FROM public.voting_registrations; UPDATE voting.voting_registrations SET initial_balance=999; CREATE TABLE voting.cl8y_bsc_balances AS SELECT * FROM public.cl8y_bsc_balances; CREATE FUNCTION voting.voting_bsc_cl8y_balance_at(text,bigint) RETURNS numeric LANGUAGE sql AS 'SELECT 999::numeric';").execute(&child).await.unwrap();
    let hostile_url = format!("{child_url}?options=-c%20search_path%3Dvoting%2Cpublic%2Cpg_temp");
    let api = operator_voting::db::connect(&hostile_url).await.unwrap();
    let mut held = Vec::new();
    for _ in 0..6 {
        let mut connection = api.acquire().await.unwrap();
        // Both implicit invocation on the pinned pool and explicit security-definer
        // invocation must use public data, despite permanent historical namespace objects.
        let implicit: String =
            sqlx::query_scalar("SELECT voting_bsc_cl8y_balance_at($1,100)::text")
                .bind(address)
                .fetch_one(&mut *connection)
                .await
                .unwrap();
        assert_eq!(implicit, "123");
        sqlx::query("SET search_path=voting,public,pg_temp")
            .execute(&mut *connection)
            .await
            .unwrap();
        let explicit: String =
            sqlx::query_scalar("SELECT public.voting_bsc_cl8y_balance_at($1,100)::text")
                .bind(address)
                .fetch_one(&mut *connection)
                .await
                .unwrap();
        assert_eq!(explicit, "123");
        let hostile: String =
            sqlx::query_scalar("SELECT voting.voting_bsc_cl8y_balance_at($1,100)::text")
                .bind(address)
                .fetch_one(&mut *connection)
                .await
                .unwrap();
        assert_eq!(hostile, "999", "attack fixture must actually differ");
        held.push(connection);
    }
    drop(held);
    api.close().await;
    child.close().await;
    sqlx::query(&format!("DROP DATABASE {database}"))
        .execute(&admin)
        .await
        .unwrap();
}
