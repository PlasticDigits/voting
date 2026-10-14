use num_bigint::BigInt;
use sqlx::PgPool;
use uuid::Uuid;

use crate::blacklist::normalize_address;
use crate::config::MAX_COMMENTS_PER_WALLET_PER_PROPOSAL;
use crate::error::{VotingError, VotingResult};
use crate::proposal_sections::AnalysisSections;

pub async fn migrate(pool: &PgPool) -> VotingResult<()> {
    // One sqlx migrator owns the database (ledger crate). Avoid checksum clashes.
    // Production: only the ledger writer runs this. operator-voting sets
    // APPLY_MIGRATIONS=false (the default when RUN_MODE=prod). See docs/OPS.md.
    voting_ledger::db::migrate(pool)
        .await
        .map_err(|e| VotingError::InvalidConfig(e.to_string()))
}

pub async fn connect(url: &str) -> VotingResult<PgPool> {
    Ok(PgPool::connect(url).await?)
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct RegistrationRow {
    pub chain: String,
    pub wallet_address: String,
    pub registered_at_height: i64,
    pub initial_balance: String,
    pub status: String,
}

pub async fn ledger_registration(
    conn: &mut sqlx::PgConnection,
    chain: &str,
    wallet: &str,
) -> VotingResult<Option<RegistrationRow>> {
    let wallet = normalize_address(wallet);
    let row = sqlx::query_as::<_, RegistrationRow>(
        r#"
        SELECT chain, wallet_address, registered_at_height, initial_balance::text, status
        FROM voting_registrations
        WHERE chain = $1 AND wallet_address = $2
        "#,
    )
    .bind(chain)
    .bind(&wallet)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

pub async fn balance_at(
    conn: &mut sqlx::PgConnection,
    chain: &str,
    wallet: &str,
    height: i64,
) -> VotingResult<BigInt> {
    let wallet = normalize_address(wallet);
    let sql = match chain {
        "terra" => "SELECT public.voting_cl8y_balance_at($1, $2)::text",
        "bsc" => "SELECT public.voting_bsc_cl8y_balance_at($1, $2)::text",
        _ => return Err(VotingError::BadRequest("unknown chain".into())),
    };
    let raw: String = sqlx::query_scalar(sql)
        .bind(&wallet)
        .bind(height)
        .fetch_one(&mut *conn)
        .await?;
    raw.parse()
        .map_err(|e| VotingError::BadRequest(format!("amount: {e}")))
}

pub async fn tip_heights(conn: &mut sqlx::PgConnection) -> VotingResult<(i64, i64)> {
    let terra: String =
        sqlx::query_scalar("SELECT value FROM indexer_state WHERE key = 'last_indexed_height'")
            .fetch_optional(&mut *conn)
            .await?
            .unwrap_or_else(|| "0".into());
    let bsc: String =
        sqlx::query_scalar("SELECT value FROM indexer_state WHERE key = 'last_indexed_bsc_block'")
            .fetch_optional(&mut *conn)
            .await?
            .unwrap_or_else(|| "0".into());
    Ok((terra.parse().unwrap_or(0), bsc.parse().unwrap_or(0)))
}

pub async fn insert_signature(
    conn: &mut sqlx::PgConnection,
    chain: &str,
    wallet: &str,
    pubkey: Option<&str>,
    signature: &str,
    payload_hash: &str,
    purpose: &str,
) -> VotingResult<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO voting.signatures
            (id, chain, wallet_address, pubkey, signature, payload_hash, purpose)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(id)
    .bind(chain)
    .bind(&normalize_address(wallet))
    .bind(pubkey)
    .bind(signature)
    .bind(payload_hash)
    .bind(purpose)
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

pub async fn pending_intent(
    conn: &mut sqlx::PgConnection,
    chain: &str,
    wallet: &str,
) -> VotingResult<bool> {
    let wallet = normalize_address(wallet);
    let found: Option<bool> = sqlx::query_scalar(
        r#"
        SELECT TRUE
        FROM voting.registration_intents
        WHERE chain = $1 AND wallet_address = $2 AND processed_at IS NULL
        "#,
    )
    .bind(chain)
    .bind(&wallet)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(found.is_some())
}

pub async fn insert_registration_intent(
    conn: &mut sqlx::PgConnection,
    chain: &str,
    wallet: &str,
    signature_id: Uuid,
) -> VotingResult<()> {
    sqlx::query(
        r#"
        INSERT INTO voting.registration_intents (chain, wallet_address, signature_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (chain, wallet_address) DO NOTHING
        "#,
    )
    .bind(chain)
    .bind(&normalize_address(wallet))
    .bind(signature_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProposalRow {
    pub id: Uuid,
    pub chain: String,
    pub proposer: String,
    pub title: String,
    pub body_html: String,
    pub terra_height: Option<i64>,
    pub bsc_block: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
    pub revision: i64,
    pub snapshot_validity: String,
    pub opened_at: Option<chrono::DateTime<chrono::Utc>>,
    pub body_sections: Option<sqlx::types::Json<serde_json::Value>>,
}

pub async fn insert_draft(
    conn: &mut sqlx::PgConnection,
    chain: &str,
    proposer: &str,
    title: &str,
    body_html: &str,
    body_canonical: &str,
    sections: &serde_json::Value,
) -> VotingResult<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO voting.proposals
            (id, chain, proposer, title, body_html, body_canonical, body_sections)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(id)
    .bind(chain)
    .bind(&normalize_address(proposer))
    .bind(title)
    .bind(body_html)
    .bind(body_canonical)
    .bind(sqlx::types::Json(sections))
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

pub async fn update_draft_sections(
    conn: &mut sqlx::PgConnection,
    id: Uuid,
    title: &str,
    body_html: &str,
    body_canonical: &str,
    sections: &serde_json::Value,
) -> VotingResult<()> {
    let result = sqlx::query(
        r#"
        UPDATE voting.proposals
        SET title = $2, body_html = $3, body_canonical = $4, body_sections = $5, revision = revision + 1
        WHERE id = $1 AND status = 'draft'
        "#,
    )
    .bind(id)
    .bind(title)
    .bind(body_html)
    .bind(body_canonical)
    .bind(sqlx::types::Json(sections))
    .execute(&mut *conn)
    .await?;
    if result.rows_affected() == 0 {
        return Err(VotingError::Conflict(
            "cannot amend after votes are open".into(),
        ));
    }
    Ok(())
}

pub async fn open_proposal(
    conn: &mut sqlx::PgConnection,
    id: Uuid,
    signature_id: Uuid,
) -> VotingResult<(i64, i64)> {
    Ok(
        sqlx::query_as("SELECT terra_height,bsc_block FROM voting.freeze_proposal($1,$2)")
            .bind(id)
            .bind(signature_id)
            .fetch_one(conn)
            .await?,
    )
}

pub async fn list_proposals(
    conn: &mut sqlx::PgConnection,
    status: Option<&str>,
) -> VotingResult<Vec<ProposalRow>> {
    Ok(sqlx::query_as::<_, ProposalRow>(
        r#"
        SELECT id, chain, proposer, title, body_html, terra_height, bsc_block, created_at, status, revision, snapshot_validity, opened_at, body_sections
        FROM voting.proposals
        WHERE ($1::text IS NULL OR status = $1)
        ORDER BY created_at DESC
        "#,
    )
    .bind(status)
    .fetch_all(&mut *conn)
    .await?)
}

pub async fn get_proposal(
    conn: &mut sqlx::PgConnection,
    id: Uuid,
) -> VotingResult<Option<ProposalRow>> {
    Ok(sqlx::query_as::<_, ProposalRow>(
        r#"
        SELECT id, chain, proposer, title, body_html, terra_height, bsc_block, created_at, status, revision, snapshot_validity, opened_at, body_sections
        FROM voting.proposals WHERE id = $1 FOR NO KEY UPDATE
        "#,
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?)
}

pub async fn snapshot_weight(
    conn: &mut sqlx::PgConnection,
    proposal_id: Uuid,
    chain: &str,
    wallet: &str,
) -> VotingResult<BigInt> {
    let raw: Option<String> = sqlx::query_scalar(
        r#"
        SELECT weight::text FROM voting.proposal_snapshots
        WHERE proposal_id = $1 AND chain = $2 AND wallet_address = $3
        "#,
    )
    .bind(proposal_id)
    .bind(chain)
    .bind(&normalize_address(wallet))
    .fetch_optional(&mut *conn)
    .await?;
    Ok(raw
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| BigInt::from(0)))
}

pub async fn insert_vote(
    conn: &mut sqlx::PgConnection,
    proposal_id: Uuid,
    chain: &str,
    wallet: &str,
    choice: &str,
    weight: &BigInt,
    signature_id: Uuid,
) -> VotingResult<()> {
    let result = sqlx::query(
        r#"
        INSERT INTO voting.votes
            (proposal_id, wallet_address, chain, choice, weight, signature_id)
        VALUES ($1, $2, $3, $4, $5::numeric, $6)
        ON CONFLICT (proposal_id, chain, wallet_address) DO NOTHING
        "#,
    )
    .bind(proposal_id)
    .bind(&normalize_address(wallet))
    .bind(chain)
    .bind(choice)
    .bind(weight.to_string())
    .bind(signature_id)
    .execute(&mut *conn)
    .await?;
    if result.rows_affected() == 0 {
        return Err(VotingError::Conflict("already voted".into()));
    }
    Ok(())
}

pub async fn get_vote(
    conn: &mut sqlx::PgConnection,
    proposal_id: Uuid,
    chain: &str,
    wallet: &str,
) -> VotingResult<Option<(String, String)>> {
    Ok(sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT choice, weight::text FROM voting.votes
        WHERE proposal_id = $1 AND chain = $2 AND wallet_address = $3
        "#,
    )
    .bind(proposal_id)
    .bind(chain)
    .bind(&normalize_address(wallet))
    .fetch_optional(&mut *conn)
    .await?)
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct TallyRow {
    pub choice: String,
    pub weight: String,
    pub voters: i64,
}

pub async fn tally(
    conn: &mut sqlx::PgConnection,
    proposal_id: Uuid,
) -> VotingResult<Vec<TallyRow>> {
    Ok(sqlx::query_as::<_, TallyRow>(
        r#"
        SELECT choice, COALESCE(SUM(weight), 0)::text AS weight, COUNT(*)::bigint AS voters
        FROM voting.votes
        WHERE proposal_id = $1
        GROUP BY choice
        "#,
    )
    .bind(proposal_id)
    .fetch_all(&mut *conn)
    .await?)
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct CommentRow {
    pub id: Uuid,
    pub chain: String,
    pub wallet_address: String,
    pub body_html: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn comment_count_for_wallet(
    conn: &mut sqlx::PgConnection,
    proposal_id: Uuid,
    chain: &str,
    wallet: &str,
) -> VotingResult<i64> {
    let n: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint FROM voting.proposal_comments
        WHERE proposal_id = $1 AND chain = $2 AND wallet_address = $3
        "#,
    )
    .bind(proposal_id)
    .bind(chain)
    .bind(&normalize_address(wallet))
    .fetch_one(&mut *conn)
    .await?;
    Ok(n)
}

pub async fn insert_comment(
    conn: &mut sqlx::PgConnection,
    proposal_id: Uuid,
    chain: &str,
    wallet: &str,
    body_html: &str,
    signature_id: Uuid,
) -> VotingResult<Uuid> {
    let n = comment_count_for_wallet(conn, proposal_id, chain, wallet).await?;
    if n >= MAX_COMMENTS_PER_WALLET_PER_PROPOSAL {
        return Err(VotingError::Forbidden(format!(
            "at most {MAX_COMMENTS_PER_WALLET_PER_PROPOSAL} comments per address per proposal"
        )));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO voting.proposal_comments
            (id, proposal_id, chain, wallet_address, body_html, signature_id)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(id)
    .bind(proposal_id)
    .bind(chain)
    .bind(&normalize_address(wallet))
    .bind(body_html)
    .bind(signature_id)
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

pub async fn list_comments(
    conn: &mut sqlx::PgConnection,
    proposal_id: Uuid,
) -> VotingResult<Vec<CommentRow>> {
    Ok(sqlx::query_as::<_, CommentRow>(
        r#"
        SELECT id, chain, wallet_address, body_html, created_at
        FROM voting.proposal_comments
        WHERE proposal_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(proposal_id)
    .fetch_all(&mut *conn)
    .await?)
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct AnalysisRow {
    pub id: Uuid,
    pub chain: String,
    pub wallet_address: String,
    pub body_html: String,
    pub source: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub sections: sqlx::types::Json<AnalysisSections>,
    pub revision: Option<i64>,
}

pub async fn insert_analysis(
    conn: &mut sqlx::PgConnection,
    proposal_id: Uuid,
    chain: &str,
    wallet: &str,
    body_html: &str,
    sections: &AnalysisSections,
    signature_id: Uuid,
) -> VotingResult<Uuid> {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM voting.proposal_analysis WHERE proposal_id=$1 AND chain=$2 AND wallet_address=$3")
        .bind(proposal_id).bind(chain).bind(normalize_address(wallet)).fetch_one(&mut *conn).await?;
    if count >= 5 {
        return Err(VotingError::Forbidden(
            "at most 5 analyses per address per proposal".into(),
        ));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO voting.proposal_analysis
            (id, proposal_id, chain, wallet_address, sections, body_html, source, signature_id,revision)
        VALUES ($1, $2, $3, $4, $5, $6, 'committee', $7,(SELECT revision FROM voting.proposals WHERE id=$2))
        "#,
    )
    .bind(id)
    .bind(proposal_id)
    .bind(chain)
    .bind(&normalize_address(wallet))
    .bind(sqlx::types::Json(sections.clone()))
    .bind(body_html)
    .bind(signature_id)
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

pub async fn list_analysis(
    conn: &mut sqlx::PgConnection,
    proposal_id: Uuid,
) -> VotingResult<Vec<AnalysisRow>> {
    Ok(sqlx::query_as::<_, AnalysisRow>(
        r#"
        SELECT id, chain, wallet_address, body_html, source, created_at, sections, revision
        FROM voting.proposal_analysis
        WHERE proposal_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(proposal_id)
    .fetch_all(&mut *conn)
    .await?)
}

/// Record a complete immutable content revision in the same action transaction.
pub async fn record_revision(
    conn: &mut sqlx::PgConnection,
    id: Uuid,
    signature_id: Uuid,
) -> VotingResult<()> {
    sqlx::query("INSERT INTO voting.proposal_revisions(proposal_id,revision,title,body_canonical,body_sections,signature_id) SELECT id,revision,title,body_canonical,body_sections,$2 FROM voting.proposals WHERE id=$1")
        .bind(id).bind(signature_id).execute(conn).await?;
    Ok(())
}

pub async fn receipt(
    conn: &mut sqlx::PgConnection,
    id: Uuid,
) -> VotingResult<Option<serde_json::Value>> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('signature',to_jsonb(s),'receipt',to_jsonb(r),'revision',to_jsonb(p),'snapshot',CASE WHEN s.purpose='open_vote' THEN (SELECT jsonb_agg(jsonb_build_object('proposal_id',x.proposal_id,'chain',x.chain,'wallet_address',x.wallet_address,'weight',x.weight::text)) FROM voting.proposal_snapshots x WHERE x.proposal_id=r.proposal_id) ELSE NULL END,'freeze',CASE WHEN s.purpose='open_vote' THEN (SELECT jsonb_build_object('terra_height',q.terra_height,'bsc_block',q.bsc_block,'terra_block_hash',q.terra_block_hash,'bsc_block_hash',q.bsc_block_hash,'terra_epoch',q.terra_epoch,'bsc_epoch',q.bsc_epoch,'opened_at',q.opened_at) FROM voting.proposals q WHERE q.id=r.proposal_id) ELSE NULL END) FROM voting.signatures s JOIN voting.action_receipts r ON r.signature_id=s.id LEFT JOIN voting.proposal_revisions p ON p.proposal_id=r.proposal_id AND p.revision=r.revision WHERE s.id=$1")
        .bind(id).fetch_optional(conn).await?)
}

/// Effective privileges, including inherited/PUBLIC grants, must be safe in prod.
pub async fn validate_production_privileges(pool: &PgPool) -> VotingResult<()> {
    let unsafe_role: bool = sqlx::query_scalar("SELECT rolsuper OR rolcreatedb OR rolcreaterole OR rolreplication OR rolbypassrls FROM pg_roles WHERE rolname=current_user")
        .fetch_one(pool).await?;
    if unsafe_role {
        return Err(VotingError::InvalidConfig(
            "API database role has administrative privileges".into(),
        ));
    }
    for table in [
        "public.voting_registrations",
        "public.cl8y_balances",
        "public.cl8y_bsc_balances",
        "public.cl8y_cw20_transfers",
        "public.cl8y_bep20_transfers",
        "public.indexer_state",
        "public.ledger_checkpoints",
        "public.ledger_canonical_blocks",
        "voting.signatures",
        "voting.votes",
        "voting.proposal_snapshots",
        "voting.action_receipts",
        "voting.proposal_revisions",
    ] {
        let unsafe_access: bool = sqlx::query_scalar(
            "SELECT has_table_privilege(current_user,$1,'UPDATE,DELETE,TRUNCATE')",
        )
        .bind(table)
        .fetch_one(pool)
        .await?;
        if unsafe_access {
            return Err(VotingError::InvalidConfig(format!(
                "API database role can mutate immutable/ledger table {table}"
            )));
        }
        if table.starts_with("public.") || table == "voting.proposal_snapshots" {
            let insert: bool =
                sqlx::query_scalar("SELECT has_table_privilege(current_user,$1,'INSERT')")
                    .bind(table)
                    .fetch_one(pool)
                    .await?;
            if insert {
                return Err(VotingError::InvalidConfig(format!(
                    "API database role can insert into {table}"
                )));
            }
        }
    }
    for table in [
        "public.voting_registrations",
        "public.indexer_state",
        "public.ledger_checkpoints",
        "public.ledger_canonical_blocks",
        "voting.signatures",
        "voting.registration_intents",
        "voting.proposals",
        "voting.proposal_snapshots",
        "voting.votes",
        "voting.proposal_comments",
        "voting.proposal_analysis",
        "voting.action_receipts",
        "voting.proposal_revisions",
    ] {
        let readable: bool =
            sqlx::query_scalar("SELECT has_table_privilege(current_user,$1,'SELECT')")
                .bind(table)
                .fetch_one(pool)
                .await?;
        if !readable {
            return Err(VotingError::InvalidConfig(format!(
                "API role lacks SELECT on {table}"
            )));
        }
    }
    for function in [
        "public.voting_cl8y_balance_at(text,bigint)",
        "public.voting_bsc_cl8y_balance_at(text,bigint)",
        "voting.freeze_proposal(uuid,uuid)",
    ] {
        let callable: bool =
            sqlx::query_scalar("SELECT has_function_privilege(current_user,$1,'EXECUTE')")
                .bind(function)
                .fetch_one(pool)
                .await?;
        if !callable {
            return Err(VotingError::InvalidConfig(format!(
                "API role lacks EXECUTE on {function}"
            )));
        }
    }
    for table in [
        "voting.signatures",
        "voting.registration_intents",
        "voting.votes",
        "voting.proposal_comments",
        "voting.proposal_analysis",
        "voting.action_receipts",
        "voting.proposal_revisions",
    ] {
        let writable: bool =
            sqlx::query_scalar("SELECT has_table_privilege(current_user,$1,'INSERT')")
                .bind(table)
                .fetch_one(pool)
                .await?;
        if !writable {
            return Err(VotingError::InvalidConfig(format!(
                "API role lacks INSERT on {table}"
            )));
        }
    }
    for column in [
        "id",
        "chain",
        "proposer",
        "title",
        "body_html",
        "body_canonical",
        "body_sections",
    ] {
        let writable: bool = sqlx::query_scalar(
            "SELECT has_column_privilege(current_user,'voting.proposals',$1,'INSERT')",
        )
        .bind(column)
        .fetch_one(pool)
        .await?;
        if !writable {
            return Err(VotingError::InvalidConfig(format!(
                "API role lacks draft INSERT on {column}"
            )));
        }
    }
    for column in [
        "status",
        "terra_height",
        "bsc_block",
        "opened_at",
        "terra_block_hash",
        "bsc_block_hash",
        "terra_epoch",
        "bsc_epoch",
        "snapshot_validity",
        "snapshot_incident",
        "created_at",
    ] {
        let unsafe_access: bool = sqlx::query_scalar(
            "SELECT has_column_privilege(current_user,'voting.proposals',$1,'INSERT,UPDATE')",
        )
        .bind(column)
        .fetch_one(pool)
        .await?;
        if unsafe_access {
            return Err(VotingError::InvalidConfig(format!(
                "API role can forge protected proposal column {column}"
            )));
        }
    }
    for column in [
        "title",
        "body_html",
        "body_canonical",
        "body_sections",
        "revision",
    ] {
        let writable: bool = sqlx::query_scalar(
            "SELECT has_column_privilege(current_user,'voting.proposals',$1,'UPDATE')",
        )
        .bind(column)
        .fetch_one(pool)
        .await?;
        if !writable {
            return Err(VotingError::InvalidConfig(format!(
                "API role lacks draft UPDATE on {column}"
            )));
        }
    }
    let creates: bool = sqlx::query_scalar("SELECT has_schema_privilege(current_user,'public','CREATE') OR has_schema_privilege(current_user,'voting','CREATE') OR has_database_privilege(current_user,current_database(),'CREATE,TEMP')")
        .fetch_one(pool).await?;
    if creates {
        return Err(VotingError::InvalidConfig(
            "API role can create/shadow database objects".into(),
        ));
    }
    Ok(())
}

/// The same positive/fresh/canonical dual-chain policy required by freeze.
pub async fn ledger_ready(pool: &PgPool) -> VotingResult<bool> {
    Ok(sqlx::query_scalar("SELECT count(*)=2 FROM public.ledger_checkpoints WHERE chain IN ('terra','bsc') AND height>0 AND network_tip>=height AND network_tip-height<=CASE chain WHEN 'terra' THEN 2 ELSE 17 END AND block_hash<>'' AND complete AND canonical AND degraded_reason IS NULL AND observed_at BETWEEN now()-interval '120 seconds' AND now()+interval '60 seconds'").fetch_one(pool).await?)
}
