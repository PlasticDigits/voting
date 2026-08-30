use num_bigint::BigInt;
use sqlx::PgPool;
use uuid::Uuid;

use crate::blacklist::normalize_address;
use crate::config::MAX_COMMENTS_PER_WALLET_PER_PROPOSAL;
use crate::error::{VotingError, VotingResult};
use crate::sections::{AnalysisSections, ProposalSections};

pub async fn migrate(pool: &PgPool) -> VotingResult<()> {
    // One sqlx migrator owns the database (ledger crate). Avoid checksum clashes.
    // Production: only the ledger writer runs this. operator-voting sets
    // APPLY_MIGRATIONS=false (the default when RUN_MODE=prod). See docs/OPS.md.
    voting_ledger::db::migrate(pool).await.map_err(|e| VotingError::InvalidConfig(e.to_string()))
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
    pool: &PgPool,
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
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn balance_at(pool: &PgPool, chain: &str, wallet: &str, height: i64) -> VotingResult<BigInt> {
    let wallet = normalize_address(wallet);
    let sql = match chain {
        "terra" => "SELECT public.voting_cl8y_balance_at($1, $2)::text",
        "bsc" => "SELECT public.voting_bsc_cl8y_balance_at($1, $2)::text",
        _ => return Err(VotingError::BadRequest("unknown chain".into())),
    };
    let raw: String = sqlx::query_scalar(sql)
        .bind(&wallet)
        .bind(height)
        .fetch_one(pool)
        .await?;
    raw.parse()
        .map_err(|e| VotingError::BadRequest(format!("amount: {e}")))
}

pub async fn tip_heights(pool: &PgPool) -> VotingResult<(i64, i64)> {
    let terra: String = sqlx::query_scalar(
        "SELECT value FROM indexer_state WHERE key = 'last_indexed_height'",
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or_else(|| "0".into());
    let bsc: String = sqlx::query_scalar(
        "SELECT value FROM indexer_state WHERE key = 'last_indexed_bsc_block'",
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or_else(|| "0".into());
    Ok((terra.parse().unwrap_or(0), bsc.parse().unwrap_or(0)))
}

pub async fn insert_signature(
    pool: &PgPool,
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
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn pending_intent(pool: &PgPool, chain: &str, wallet: &str) -> VotingResult<bool> {
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
    .fetch_optional(pool)
    .await?;
    Ok(found.is_some())
}

pub async fn insert_registration_intent(
    pool: &PgPool,
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
    .execute(pool)
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
    pub opened_at: Option<chrono::DateTime<chrono::Utc>>,
    pub body_sections: Option<sqlx::types::Json<ProposalSections>>,
}

pub async fn insert_draft(
    pool: &PgPool,
    chain: &str,
    proposer: &str,
    title: &str,
    body_html: &str,
    body_canonical: &str,
    sections: &ProposalSections,
) -> VotingResult<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO voting.proposals
            (id, chain, proposer, title, body_html, body_canonical, terra_height, bsc_block, status, body_sections)
        VALUES ($1, $2, $3, $4, $5, $6, NULL, NULL, 'draft', $7)
        "#,
    )
    .bind(id)
    .bind(chain)
    .bind(&normalize_address(proposer))
    .bind(title)
    .bind(body_html)
    .bind(body_canonical)
    .bind(sqlx::types::Json(sections.clone()))
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn update_draft_sections(
    pool: &PgPool,
    id: Uuid,
    title: &str,
    body_html: &str,
    body_canonical: &str,
    sections: &ProposalSections,
) -> VotingResult<()> {
    let result = sqlx::query(
        r#"
        UPDATE voting.proposals
        SET title = $2, body_html = $3, body_canonical = $4, body_sections = $5
        WHERE id = $1 AND status = 'draft'
        "#,
    )
    .bind(id)
    .bind(title)
    .bind(body_html)
    .bind(body_canonical)
    .bind(sqlx::types::Json(sections.clone()))
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(VotingError::Conflict("cannot amend after votes are open".into()));
    }
    Ok(())
}

pub async fn open_proposal(
    pool: &PgPool,
    id: Uuid,
    terra_height: i64,
    bsc_block: i64,
) -> VotingResult<()> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        r#"
        UPDATE voting.proposals
        SET status = 'open', terra_height = $2, bsc_block = $3, opened_at = NOW()
        WHERE id = $1 AND status = 'draft'
        "#,
    )
    .bind(id)
    .bind(terra_height)
    .bind(bsc_block)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Err(VotingError::Conflict("proposal is not a draft (already open?)".into()));
    }
    sqlx::query(
        r#"
        INSERT INTO voting.proposal_snapshots (proposal_id, chain, wallet_address, weight)
        SELECT $1, chain, wallet_address,
               CASE chain
                 WHEN 'terra' THEN public.voting_cl8y_balance_at(wallet_address, $2)
                 WHEN 'bsc' THEN public.voting_bsc_cl8y_balance_at(wallet_address, $3)
               END
        FROM voting_registrations
        WHERE status = 'active'
        "#,
    )
    .bind(id)
    .bind(terra_height)
    .bind(bsc_block)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn list_proposals(pool: &PgPool, status: Option<&str>) -> VotingResult<Vec<ProposalRow>> {
    Ok(sqlx::query_as::<_, ProposalRow>(
        r#"
        SELECT id, chain, proposer, title, body_html, terra_height, bsc_block, created_at, status, opened_at, body_sections
        FROM voting.proposals
        WHERE ($1::text IS NULL OR status = $1)
        ORDER BY created_at DESC
        "#,
    )
    .bind(status)
    .fetch_all(pool)
    .await?)
}

pub async fn get_proposal(pool: &PgPool, id: Uuid) -> VotingResult<Option<ProposalRow>> {
    Ok(sqlx::query_as::<_, ProposalRow>(
        r#"
        SELECT id, chain, proposer, title, body_html, terra_height, bsc_block, created_at, status, opened_at, body_sections
        FROM voting.proposals WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn snapshot_weight(
    pool: &PgPool,
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
    .fetch_optional(pool)
    .await?;
    Ok(raw
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| BigInt::from(0)))
}

pub async fn insert_vote(
    pool: &PgPool,
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
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(VotingError::Conflict("already voted".into()));
    }
    Ok(())
}

pub async fn get_vote(
    pool: &PgPool,
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
    .fetch_optional(pool)
    .await?)
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct TallyRow {
    pub choice: String,
    pub weight: String,
    pub voters: i64,
}

pub async fn tally(pool: &PgPool, proposal_id: Uuid) -> VotingResult<Vec<TallyRow>> {
    Ok(sqlx::query_as::<_, TallyRow>(
        r#"
        SELECT choice, COALESCE(SUM(weight), 0)::text AS weight, COUNT(*)::bigint AS voters
        FROM voting.votes
        WHERE proposal_id = $1
        GROUP BY choice
        "#,
    )
    .bind(proposal_id)
    .fetch_all(pool)
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
    pool: &PgPool,
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
    .fetch_one(pool)
    .await?;
    Ok(n)
}

pub async fn insert_comment(
    pool: &PgPool,
    proposal_id: Uuid,
    chain: &str,
    wallet: &str,
    body_html: &str,
    signature_id: Uuid,
) -> VotingResult<Uuid> {
    let n = comment_count_for_wallet(pool, proposal_id, chain, wallet).await?;
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
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn list_comments(pool: &PgPool, proposal_id: Uuid) -> VotingResult<Vec<CommentRow>> {
    Ok(sqlx::query_as::<_, CommentRow>(
        r#"
        SELECT id, chain, wallet_address, body_html, created_at
        FROM voting.proposal_comments
        WHERE proposal_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(proposal_id)
    .fetch_all(pool)
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
}

pub async fn insert_analysis(
    pool: &PgPool,
    proposal_id: Uuid,
    chain: &str,
    wallet: &str,
    body_html: &str,
    sections: &AnalysisSections,
    signature_id: Uuid,
) -> VotingResult<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO voting.proposal_analysis
            (id, proposal_id, chain, wallet_address, sections, body_html, source, signature_id)
        VALUES ($1, $2, $3, $4, $5, $6, 'committee', $7)
        "#,
    )
    .bind(id)
    .bind(proposal_id)
    .bind(chain)
    .bind(&normalize_address(wallet))
    .bind(sqlx::types::Json(sections.clone()))
    .bind(body_html)
    .bind(signature_id)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn list_analysis(pool: &PgPool, proposal_id: Uuid) -> VotingResult<Vec<AnalysisRow>> {
    Ok(sqlx::query_as::<_, AnalysisRow>(
        r#"
        SELECT id, chain, wallet_address, body_html, source, created_at, sections
        FROM voting.proposal_analysis
        WHERE proposal_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(proposal_id)
    .fetch_all(pool)
    .await?)
}
