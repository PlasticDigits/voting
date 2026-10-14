//! Apply `deploy/grants.sql` (O1 / L10) and assert the restricted role
//! can read balance functions + write `voting.*` but cannot write ledger ingest.

fn test_db_url() -> Option<String> {
    voting_ledger::test_lock::test_database_url()
}

const GRANTS_SQL: &str = include_str!("../../deploy/grants.sql");

#[test]
fn grants_sql_is_least_privilege() {
    assert!(
        GRANTS_SQL.contains("REVOKE INSERT, UPDATE, DELETE, TRUNCATE"),
        "Coolify grants must revoke ledger writes"
    );
    for table in [
        "voting_registrations",
        "cl8y_balances",
        "cl8y_bsc_balances",
        "cl8y_cw20_transfers",
        "cl8y_bep20_transfers",
        "indexer_state",
    ] {
        assert!(
            GRANTS_SQL.contains(table),
            "grants.sql must mention {table}"
        );
    }
    assert!(
        !GRANTS_SQL.contains("GRANT ALL ON SCHEMA voting"),
        "CREATE on schema voting is not least privilege (USAGE only)"
    );
    assert!(
        GRANTS_SQL.contains("current_database()"),
        "CONNECT grant must follow the database the file is applied to"
    );
    assert!(
        GRANTS_SQL.contains("public.voting_cl8y_balance_at"),
        "EXECUTE must target public.* (role voting + schema voting shadows search_path)"
    );
}

#[tokio::test]
async fn restricted_role_from_grants_sql_cannot_write_ledger() {
    let Some(url) = test_db_url() else {
        eprintln!("skip: set LEDGER_TEST_DATABASE_URL");
        return;
    };
    let _lock = voting_ledger::test_lock::hold_integration_db(&url)
        .await
        .expect("advisory lock");
    let admin = voting_ledger::test_lock::test_pool(&url)
        .await
        .expect("admin pool");
    voting_ledger::db::migrate(&admin).await.unwrap();

    // Coolify applies this file with psql (DO blocks + multiple GRANTs).
    // sqlx pool execute / raw_sql is not that path and can stop after the first statement.
    apply_grants_sql(&url);

    // Reuse the guarded ephemeral admin URL; SET ROLE avoids any test login password.
    let restricted = sqlx::postgres::PgPoolOptions::new()
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE operator_voting")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .expect("isolated restricted role pool");
    operator_voting::db::validate_production_privileges(&restricted)
        .await
        .expect("effective permission matrix");
    assert!(operator_voting::db::validate_production_privileges(&admin)
        .await
        .is_err());

    for grant in [
        "GRANT UPDATE(balance) ON public.cl8y_balances TO operator_voting",
        "GRANT INSERT(wallet_address,height,balance) ON public.cl8y_balances TO PUBLIC",
        "GRANT TRUNCATE ON voting.proposal_comments TO operator_voting",
        "GRANT TRUNCATE ON voting.proposal_analysis TO operator_voting",
        "GRANT UPDATE(processed_at) ON voting.registration_intents TO operator_voting",
    ] {
        sqlx::query(grant).execute(&admin).await.unwrap();
        assert!(
            operator_voting::db::validate_production_privileges(&restricted)
                .await
                .is_err(),
            "unsafe effective privilege accepted: {grant}"
        );
        apply_grants_sql(&url);
        operator_voting::db::validate_production_privileges(&restricted)
            .await
            .unwrap();
    }
    let leaky = format!("voting_test_leaky_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE ROLE {leaky} NOLOGIN"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!(
        "GRANT UPDATE(balance) ON public.cl8y_balances TO {leaky}"
    ))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT {leaky} TO operator_voting"))
        .execute(&admin)
        .await
        .unwrap();
    assert!(
        operator_voting::db::validate_production_privileges(&restricted)
            .await
            .is_err(),
        "inherited column write must fail startup"
    );
    sqlx::query(&format!("REVOKE {leaky} FROM operator_voting"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!(
        "REVOKE UPDATE(balance) ON public.cl8y_balances FROM {leaky}"
    ))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(&format!("DROP ROLE {leaky}"))
        .execute(&admin)
        .await
        .unwrap();
    operator_voting::db::validate_production_privileges(&restricted)
        .await
        .unwrap();
    let read = sqlx::query_scalar::<_, String>(
        "SELECT public.voting_cl8y_balance_at('terra1missing', 1)::text",
    )
    .fetch_one(&restricted)
    .await;
    assert!(
        read.is_ok(),
        "restricted role must EXECUTE balance functions: {read:?}"
    );

    let write_bal = sqlx::query(
        "INSERT INTO cl8y_balances (wallet_address, height, balance) VALUES ('x', 1, 1)",
    )
    .execute(&restricted)
    .await;
    assert!(
        write_bal.is_err(),
        "restricted role must not write cl8y_balances"
    );

    let write_xfer = sqlx::query(
        "INSERT INTO cl8y_cw20_transfers (height, tx_hash, from_address, to_address, amount, action) VALUES (1, 'h', 'a', 'b', 1, 'transfer')",
    )
    .execute(&restricted)
    .await;
    assert!(
        write_xfer.is_err(),
        "restricted role must not write cl8y_cw20_transfers"
    );

    let write_state =
        sqlx::query("UPDATE indexer_state SET value = '9' WHERE key = 'last_indexed_height'")
            .execute(&restricted)
            .await;
    assert!(
        write_state.is_err(),
        "restricted role must not write indexer_state"
    );

    let write_reg = sqlx::query(
        "INSERT INTO voting_registrations (chain, wallet_address, registered_at_height, initial_balance) VALUES ('terra', 'terra1x', 1, 1)",
    )
    .execute(&restricted)
    .await;
    assert!(
        write_reg.is_err(),
        "restricted role must not insert voting_registrations"
    );

    let sig_ok = sqlx::query(
        r#"
        INSERT INTO voting.signatures (id, chain, wallet_address, signature, payload_hash, purpose)
        VALUES (gen_random_uuid(), 'terra', 'terra1test', 'sig', 'hash', 'register')
        "#,
    )
    .execute(&restricted)
    .await;
    assert!(
        sig_ok.is_ok(),
        "restricted role must write voting.signatures: {sig_ok:?}"
    );

    let draft_ok = sqlx::query(
        r#"
        INSERT INTO voting.proposals (id, chain, proposer, title, body_html, body_canonical)
        VALUES (gen_random_uuid(), 'terra', 'terra1test', 't', '<p>b</p>', '{}')
        "#,
    )
    .execute(&restricted)
    .await;
    assert!(
        draft_ok.is_ok(),
        "restricted role must write voting.proposals: {draft_ok:?}"
    );

    let comment_ok = sqlx::query(
        r#"
        WITH s AS (
            INSERT INTO voting.signatures (id, chain, wallet_address, signature, payload_hash, purpose)
            VALUES (gen_random_uuid(), 'terra', 'terra1test', 'sig2', 'hash2', 'comment')
            RETURNING id
        ), p AS (
            SELECT id FROM voting.proposals WHERE proposer = 'terra1test' LIMIT 1
        )
        INSERT INTO voting.proposal_comments (id, proposal_id, chain, wallet_address, body_html, signature_id)
        SELECT gen_random_uuid(), p.id, 'terra', 'terra1test', '<p>ok</p>', s.id FROM s, p
        "#,
    )
    .execute(&restricted)
    .await;
    assert!(
        comment_ok.is_ok(),
        "restricted role must write voting.proposal_comments: {comment_ok:?}"
    );

    let analysis_ok = sqlx::query(
        r#"
        WITH s AS (
            INSERT INTO voting.signatures (id, chain, wallet_address, signature, payload_hash, purpose)
            VALUES (gen_random_uuid(), 'terra', 'terra1committee', 'sig3', 'hash3', 'analyze')
            RETURNING id
        ), p AS (
            SELECT id FROM voting.proposals WHERE proposer = 'terra1test' LIMIT 1
        )
        INSERT INTO voting.proposal_analysis
            (id, proposal_id, chain, wallet_address, sections, body_html, source, signature_id,revision)
        SELECT gen_random_uuid(), p.id, 'terra', 'terra1committee',
               '{"what":"ok","benefits":"ok","risks":"ok","short_term":"ok","long_term":"ok"}'::jsonb,
               '<p>ok</p>', 'committee', s.id,1
        FROM s, p
        "#,
    )
    .execute(&restricted)
    .await;
    assert!(
        analysis_ok.is_ok(),
        "restricted role must write voting.proposal_analysis: {analysis_ok:?}"
    );

    for table in [
        "voting.signatures",
        "voting.votes",
        "voting.proposal_snapshots",
        "voting.action_receipts",
        "voting.proposal_revisions",
        "voting.proposal_comments",
        "voting.proposal_analysis",
    ] {
        for privilege in ["UPDATE", "DELETE", "TRUNCATE"] {
            let allowed: bool =
                sqlx::query_scalar("SELECT has_table_privilege(current_user,$1,$2)")
                    .bind(table)
                    .bind(privilege)
                    .fetch_one(&restricted)
                    .await
                    .unwrap();
            assert!(!allowed, "unexpected {table} {privilege}");
        }
    }
    assert!(sqlx::query(
        "INSERT INTO voting.proposal_snapshots VALUES(gen_random_uuid(),'terra','forged',999)"
    )
    .execute(&restricted)
    .await
    .is_err());
    // Original 999 attack, from a connection with pre-existing hostile temp objects.
    let mut hostile = admin.acquire().await.unwrap();
    sqlx::query("CREATE TEMP TABLE voting_registrations AS SELECT * FROM public.voting_registrations WITH NO DATA").execute(&mut *hostile).await.unwrap();
    sqlx::query("INSERT INTO pg_temp.voting_registrations(chain,wallet_address,registered_at_height,initial_balance) VALUES('terra','terra1shadow',1,999)").execute(&mut *hostile).await.unwrap();
    sqlx::query(
        "CREATE TEMP TABLE cl8y_balances AS SELECT * FROM public.cl8y_balances WITH NO DATA",
    )
    .execute(&mut *hostile)
    .await
    .unwrap();
    sqlx::query("SET ROLE operator_voting")
        .execute(&mut *hostile)
        .await
        .unwrap();
    sqlx::query("SET search_path=pg_temp,voting,public")
        .execute(&mut *hostile)
        .await
        .unwrap();
    let actual: String =
        sqlx::query_scalar("SELECT public.voting_cl8y_balance_at('terra1shadow',1)::text")
            .fetch_one(&mut *hostile)
            .await
            .unwrap();
    assert_eq!(actual, "0", "hostile temp tables returned forged 999");
    sqlx::query("RESET ROLE")
        .execute(&mut *hostile)
        .await
        .unwrap();
    sqlx::query("RESET search_path")
        .execute(&mut *hostile)
        .await
        .unwrap();
    sqlx::query("DROP TABLE pg_temp.voting_registrations,pg_temp.cl8y_balances")
        .execute(&mut *hostile)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE voting.future_table(id int)")
        .execute(&admin)
        .await
        .unwrap();
    let future: bool = sqlx::query_scalar("SELECT has_table_privilege(current_user,'voting.future_table','SELECT,INSERT,UPDATE,DELETE,TRUNCATE')").fetch_one(&restricted).await.unwrap();
    assert!(!future, "future-table blanket privilege creep");
    sqlx::query("DROP TABLE voting.future_table")
        .execute(&admin)
        .await
        .unwrap();
    let password: Option<String> =
        sqlx::query_scalar("SELECT rolpassword FROM pg_authid WHERE rolname='operator_voting'")
            .fetch_one(&admin)
            .await
            .unwrap();
    assert!(
        password.is_none(),
        "fresh role must have no default usable password"
    );

    assert!(sqlx::query("INSERT INTO voting.proposals(id,chain,proposer,title,body_html,body_canonical,status,terra_height,bsc_block,opened_at) VALUES(gen_random_uuid(),'terra','forged','t','b','{}','open',1,1,now())").execute(&restricted).await.is_err(),"direct forged opening must be denied");
    assert!(
        sqlx::query("SELECT * FROM voting.freeze_proposal(gen_random_uuid(),gen_random_uuid())")
            .execute(&restricted)
            .await
            .is_err(),
        "freeze without signed complete revision must be denied"
    );
    let create = sqlx::query("CREATE TABLE voting.should_not_exist (id int)")
        .execute(&restricted)
        .await;
    assert!(
        create.is_err(),
        "restricted role must not CREATE in schema voting"
    );
}

fn apply_grants_sql(url: &str) {
    let output = std::process::Command::new("psql")
        .env("PGCONNECT_TIMEOUT", "8")
        .args(["-v", "ON_ERROR_STOP=1", "--no-psqlrc", "-X", "-d", url])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .expect("psql stdin")
                .write_all(GRANTS_SQL.as_bytes())?;
            child.wait_with_output()
        })
        .expect("psql must be on PATH to apply deploy/grants.sql (postgresql-client)");
    assert!(
        output.status.success(),
        "psql deploy/grants.sql failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn populated_upgrade_preserves_legacy_and_revokes_blanket_grants() {
    let Some(url) = test_db_url() else {
        return;
    };
    let _lock = voting_ledger::test_lock::hold_integration_db(&url)
        .await
        .unwrap();
    let admin = voting_ledger::test_lock::test_pool(&url).await.unwrap();
    let database = format!("voting_test_upgrade_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {database}"))
        .execute(&admin)
        .await
        .unwrap();
    let upgrade_url = format!("{}/{}", url.rsplit_once('/').unwrap().0, database);
    assert!(
        voting_ledger::test_lock::test_pool(&upgrade_url)
            .await
            .is_err(),
        "unmarked database must refuse before migrations"
    );
    sqlx::query(&format!(
        "COMMENT ON DATABASE {database} IS 'cl8y-voting-ephemeral-test'"
    ))
    .execute(&admin)
    .await
    .unwrap();
    let upgrade = voting_ledger::test_lock::test_pool(&upgrade_url)
        .await
        .unwrap();
    let mut conn = upgrade.acquire().await.unwrap();
    sqlx::query("SET search_path TO public,pg_temp")
        .execute(&mut *conn)
        .await
        .unwrap();
    for sql in [
        include_str!("../../ledger/migrations/20260825000001_ledger.sql"),
        include_str!("../../ledger/migrations/20260825000002_voting.sql"),
        include_str!("../../ledger/migrations/20260825000003_qualify_balance_fns.sql"),
        include_str!("../../ledger/migrations/20260830000001_proposal_sections.sql"),
        include_str!("../../ledger/migrations/20260830000002_draft_lifecycle.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&mut *conn).await.unwrap();
    }
    let id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO voting.proposals(id,chain,proposer,title,body_html,body_canonical,status,terra_height,bsc_block,opened_at) VALUES($1,'terra','legacy','old title','<p>legacy</p>','legacy','open',20,20,now())").bind(id).execute(&mut *conn).await.unwrap();
    sqlx::query("INSERT INTO voting.proposal_snapshots VALUES($1,'terra','legacy',99)")
        .bind(id)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::raw_sql("DO $$ BEGIN IF NOT EXISTS(SELECT FROM pg_roles WHERE rolname='operator_voting') THEN CREATE ROLE operator_voting LOGIN; END IF; END $$; GRANT ALL ON ALL TABLES IN SCHEMA voting TO operator_voting; ALTER DEFAULT PRIVILEGES IN SCHEMA voting GRANT ALL ON TABLES TO operator_voting;").execute(&mut *conn).await.unwrap();
    for sql in [
        include_str!("../../ledger/migrations/20261014000001_ledger_hardening.sql"),
        include_str!("../../ledger/migrations/20261014000100_transactional_actions.sql"),
        include_str!("../../ledger/migrations/20261014000101_restrict_existing_api_role.sql"),
        include_str!("../../ledger/migrations/20261014000102_verified_action_guards.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&mut *conn).await.unwrap();
    }
    let (validity,title,weight):(String,String,String)=sqlx::query_as("SELECT p.snapshot_validity,p.title,s.weight::text FROM voting.proposals p JOIN voting.proposal_snapshots s ON s.proposal_id=p.id WHERE p.id=$1").bind(id).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(
        (validity.as_str(), title.as_str(), weight.as_str()),
        ("legacy_unverified", "old title", "99")
    );
    let revisions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM voting.proposal_revisions WHERE proposal_id=$1")
            .bind(id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!(revisions, 1);
    let restricted = sqlx::postgres::PgPoolOptions::new()
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE operator_voting")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&upgrade_url)
        .await
        .unwrap();
    operator_voting::db::validate_production_privileges(&restricted)
        .await
        .unwrap();
    apply_grants_sql(&upgrade_url);
    apply_grants_sql(&upgrade_url);
    operator_voting::db::validate_production_privileges(&restricted)
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE voting.proposal_snapshots SET weight=999")
            .execute(&restricted)
            .await
            .is_err()
    );
    assert!(sqlx::query("DELETE FROM voting.proposal_revisions")
        .execute(&restricted)
        .await
        .is_err());
    restricted.close().await;
    drop(conn);
    upgrade.close().await;
    sqlx::query(&format!("DROP DATABASE {database}"))
        .execute(&admin)
        .await
        .unwrap();
}
