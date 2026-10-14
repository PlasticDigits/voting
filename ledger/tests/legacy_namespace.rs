//! Forward promotion of historical user-schema ledger rows. Only guarded ephemeral databases.
use sqlx::PgPool;
const PROMOTE: &str =
    include_str!("../migrations/20261014000105_promote_legacy_ledger_namespace.sql");

async fn fixture() -> Option<(
    PgPool,
    PgPool,
    String,
    voting_ledger::test_lock::IntegrationDbLock,
)> {
    let url = voting_ledger::test_lock::test_database_url()?;
    let lock = voting_ledger::test_lock::hold_integration_db(&url)
        .await
        .unwrap();
    let admin = voting_ledger::test_lock::test_pool(&url).await.unwrap();
    let name = format!("voting_test_promotion_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!(
        "COMMENT ON DATABASE {name} IS 'cl8y-voting-ephemeral-test'"
    ))
    .execute(&admin)
    .await
    .unwrap();
    let child_url = format!("{}/{}", url.rsplit_once('/').unwrap().0, name);
    let pool = voting_ledger::test_lock::test_pool(&child_url)
        .await
        .unwrap();
    voting_ledger::db::migrate(&pool).await.unwrap();
    sqlx::raw_sql("CREATE TABLE voting.voting_registrations(chain text,wallet_address text,registered_at_height bigint,registered_at_time timestamptz,initial_balance numeric(78,0),status text,signature_id uuid,PRIMARY KEY(chain,wallet_address)); CREATE TABLE voting.indexer_state(key text PRIMARY KEY,value text,updated_at timestamptz); CREATE TABLE voting.cl8y_bsc_balances(wallet_address text,bsc_block bigint,balance numeric(78,0),PRIMARY KEY(wallet_address,bsc_block)); CREATE TABLE voting.cl8y_balances(wallet_address text,height bigint,balance numeric(78,0),PRIMARY KEY(wallet_address,height)); CREATE TABLE voting.cl8y_bep20_transfers(id bigint PRIMARY KEY,bsc_block bigint,tx_hash text,log_index integer,from_address text,to_address text,amount numeric(78,0),UNIQUE(tx_hash,log_index)); CREATE TABLE voting.cl8y_cw20_transfers(id bigint PRIMARY KEY,height bigint,tx_hash text,msg_index integer,from_address text,to_address text,amount numeric(78,0),action text,UNIQUE(tx_hash,msg_index,from_address,to_address,amount)); CREATE TABLE voting._sqlx_migrations(version bigint PRIMARY KEY,success boolean,checksum bytea); INSERT INTO voting._sqlx_migrations VALUES(20260830000002,true,'old'::bytea); INSERT INTO voting.indexer_state VALUES('last_indexed_height','30830539',to_timestamp(1700000000)),('last_indexed_bsc_block','127547775',to_timestamp(1700000000)); UPDATE public.indexer_state SET value=CASE key WHEN 'last_indexed_height' THEN '30830571' WHEN 'last_indexed_bsc_block' THEN '127548256' ELSE value END;").execute(&pool).await.unwrap();
    for i in 0..15 {
        let wallet = format!("0x{i:040x}");
        sqlx::query("INSERT INTO voting.voting_registrations VALUES('bsc',$1,$2,to_timestamp(1700000000),$3::numeric,$4,$5)").bind(wallet).bind(127547000+i as i64).bind(format!("{}",1000+i)).bind(if i==14{"inactive"}else{"active"}).bind(uuid::Uuid::new_v4()).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO voting.cl8y_bep20_transfers VALUES(1,127547770,'legacy_tx_a',0,'a','b',7),(2,127547775,'legacy_tx_b',1,'b','c',8); INSERT INTO voting.cl8y_bsc_balances VALUES('b',127547770,7),('c',127547775,8);").execute(&pool).await.unwrap();
    Some((pool, admin, name, lock))
}
async fn cleanup(pool: PgPool, admin: PgPool, name: String) {
    pool.close().await;
    sqlx::query(&format!("DROP DATABASE {name}"))
        .execute(&admin)
        .await
        .unwrap();
}
async fn promote(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    match sqlx::raw_sql(PROMOTE).execute(&mut *tx).await {
        Ok(_) => tx.commit().await,
        Err(error) => {
            tx.rollback().await?;
            Err(error)
        }
    }
}

#[tokio::test]
async fn empty_canonical_preserves_electorate_history_cursors_and_frozen_polls() {
    let Some((pool, admin, name, _lock)) = fixture().await else {
        return;
    };
    let proposal = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO voting.proposals(id,chain,proposer,title,body_html,body_canonical,status,terra_height,bsc_block,opened_at) VALUES($1,'bsc','actor','Frozen','<p>history</p>','{}','open',30830500,127547700,clock_timestamp())").bind(proposal).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO voting.proposal_snapshots VALUES($1,'bsc','actor',999)")
        .bind(proposal)
        .execute(&pool)
        .await
        .unwrap();
    let before: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM voting.proposals p WHERE id=$1")
            .bind(proposal)
            .fetch_one(&pool)
            .await
            .unwrap();
    let tracker: i64 = sqlx::query_scalar("SELECT count(*) FROM public._sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    promote(&pool).await.unwrap();
    let differences:i64=sqlx::query_scalar("SELECT count(*) FROM ((SELECT chain,wallet_address,registered_at_height,registered_at_time,initial_balance,status,signature_id FROM voting.voting_registrations EXCEPT SELECT chain,wallet_address,registered_at_height,registered_at_time,initial_balance,status,signature_id FROM public.voting_registrations) UNION ALL (SELECT chain,wallet_address,registered_at_height,registered_at_time,initial_balance,status,signature_id FROM public.voting_registrations EXCEPT SELECT chain,wallet_address,registered_at_height,registered_at_time,initial_balance,status,signature_id FROM voting.voting_registrations)) d").fetch_one(&pool).await.unwrap();
    assert_eq!(differences, 0);
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM public.voting_registrations),(SELECT count(*) FROM public.cl8y_bep20_transfers),(SELECT count(*) FROM public.cl8y_bsc_balances)").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (15, 2, 2));
    let history_diff:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM (SELECT * FROM voting.cl8y_bep20_transfers EXCEPT SELECT * FROM public.cl8y_bep20_transfers) d)+(SELECT count(*) FROM (SELECT * FROM voting.cl8y_bsc_balances EXCEPT SELECT * FROM public.cl8y_bsc_balances) d)").fetch_one(&pool).await.unwrap();
    assert_eq!(history_diff, 0);
    let after: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM voting.proposals p WHERE id=$1")
            .bind(proposal)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    let weight: String = sqlx::query_scalar(
        "SELECT weight::text FROM voting.proposal_snapshots WHERE proposal_id=$1",
    )
    .bind(proposal)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(weight, "999");
    let cursor: String = sqlx::query_scalar(
        "SELECT value FROM public.indexer_state WHERE key='last_indexed_bsc_block'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cursor, "127547775");
    let certified: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.voting_registrations WHERE baseline_verified",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(certified, 0);
    let bad:i64=sqlx::query_scalar("SELECT count(*) FROM public.ledger_checkpoints WHERE complete OR canonical OR degraded_reason IS DISTINCT FROM 'legacy_electorate_requires_reconciliation'").fetch_one(&pool).await.unwrap();
    assert_eq!(bad, 0);
    let after_tracker: i64 = sqlx::query_scalar("SELECT count(*) FROM public._sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tracker, after_tracker);
    let archived: i64 =
        sqlx::query_scalar("SELECT count(*) FROM public.voting_legacy_ledger_archive")
            .fetch_one(&pool)
            .await
            .unwrap();
    // Repeating the completed promotion does not invalidate subsequent certified recovery.
    sqlx::query("UPDATE public.voting_registrations SET baseline_verified=true,baseline_block_hash='certified'").execute(&pool).await.unwrap();
    promote(&pool).await.unwrap();
    let certified: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.voting_registrations WHERE baseline_verified",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(certified, 15);
    let again: i64 = sqlx::query_scalar("SELECT count(*) FROM public.voting_legacy_ledger_archive")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(archived, again);
    let next:i64=sqlx::query_scalar("INSERT INTO public.cl8y_bep20_transfers(bsc_block,tx_hash,log_index,from_address,to_address,amount) VALUES(127547776,'next',0,'a','b',1) RETURNING id").fetch_one(&pool).await.unwrap();
    assert_eq!(next, 3);
    cleanup(pool, admin, name).await;
}

#[tokio::test]
async fn mixed_equal_rows_import_once_and_nonidentical_conflicts_roll_back() {
    let Some((pool, admin, name, _lock)) = fixture().await else {
        return;
    };
    sqlx::raw_sql("INSERT INTO public.voting_registrations(chain,wallet_address,registered_at_height,registered_at_time,initial_balance,status,signature_id) SELECT chain,wallet_address,registered_at_height,registered_at_time,initial_balance,status,signature_id FROM voting.voting_registrations LIMIT 1; INSERT INTO public.cl8y_bep20_transfers SELECT * FROM voting.cl8y_bep20_transfers WHERE id=1;").execute(&pool).await.unwrap();
    promote(&pool).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM public.voting_registrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 15);
    promote(&pool).await.unwrap();
    cleanup(pool, admin, name).await;
    drop(_lock);
    let Some((pool, admin, name, _lock)) = fixture().await else {
        return;
    };
    sqlx::query("INSERT INTO public.voting_registrations(chain,wallet_address,registered_at_height,registered_at_time,initial_balance,status,signature_id) SELECT chain,wallet_address,registered_at_height,registered_at_time,initial_balance+1,status,signature_id FROM voting.voting_registrations LIMIT 1").execute(&pool).await.unwrap();
    assert!(promote(&pool).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM public.voting_registrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let copied: i64 = sqlx::query_scalar("SELECT count(*) FROM public.cl8y_bep20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(copied, 0);
    let manifests: i64 =
        sqlx::query_scalar("SELECT count(*) FROM public.voting_legacy_storage_imports")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(manifests, 0);
    cleanup(pool, admin, name).await;
}

#[tokio::test]
async fn ambiguous_native_events_remain_fully_archived_and_legacy_changes_fail_closed() {
    let Some((pool, admin, name, _lock)) = fixture().await else {
        return;
    };
    sqlx::raw_sql("INSERT INTO voting.cl8y_cw20_transfers VALUES(1,30830530,'ambiguous',0,'a','b',7,'transfer'),(2,30830530,'ambiguous',0,'a','c',8,'transfer'),(3,30830531,'clear',0,'c','d',1,'transfer'),(4,30830532,'known_ambiguous',0,'a','d',2,'transfer'); INSERT INTO public.cl8y_cw20_legacy_identity_conflicts VALUES(77,30830532,'known_ambiguous',0,'a','b',3,'transfer'),(78,30830532,'known_ambiguous',0,'a','c',4,'transfer');").execute(&pool).await.unwrap();
    promote(&pool).await.unwrap();
    let copied: i64 = sqlx::query_scalar("SELECT count(*) FROM public.cl8y_cw20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(copied, 1);
    let ambiguous: i64 =
        sqlx::query_scalar("SELECT ambiguous_cw20_count FROM public.voting_legacy_storage_imports")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ambiguous, 3);
    let archived:i64=sqlx::query_scalar("SELECT count(*) FROM public.voting_legacy_ledger_archive WHERE source_relation='voting.cl8y_cw20_transfers'").fetch_one(&pool).await.unwrap();
    assert_eq!(archived, 4);
    let old: i64 = sqlx::query_scalar("SELECT count(*) FROM voting.cl8y_cw20_transfers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old, 4);
    sqlx::query("UPDATE voting.voting_registrations SET initial_balance=initial_balance+1 WHERE wallet_address=(SELECT wallet_address FROM voting.voting_registrations LIMIT 1)").execute(&pool).await.unwrap();
    assert!(promote(&pool).await.is_err());
    cleanup(pool, admin, name).await;
}
