//! Public DTOs. Generated TypeScript contracts consume these schemas; raw amounts remain strings.
use crate::proposal_sections::{AnalysisSections, ProposalSections};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VotingChain {
    Terra,
    Bsc,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Draft,
    Open,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VoteChoice {
    For,
    Against,
    Abstain,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotValidity {
    Valid,
    Invalid,
    LegacyUnverified,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Tally {
    pub choice: VoteChoice,
    pub weight: String,
    pub voters: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProposalListItem {
    pub id: String,
    pub chain: VotingChain,
    pub proposer: String,
    pub title: String,
    pub revision: i64,
    pub snapshot_validity: SnapshotValidity,
    pub snapshot_incident: Option<String>,
    pub summary: Option<String>,
    pub terra_height: Option<i64>,
    pub bsc_block: Option<i64>,
    pub created_at: String,
    pub opened_at: Option<String>,
    pub status: ProposalStatus,
    pub tally: Vec<Tally>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Comment {
    pub id: String,
    pub chain: VotingChain,
    pub wallet_address: String,
    pub body_html: String,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Analysis {
    pub id: String,
    pub chain: VotingChain,
    pub wallet_address: String,
    pub body_html: String,
    pub created_at: String,
    pub sections: AnalysisSections,
    pub source: String,
    pub revision: Option<i64>,
    pub machine_generated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProposalDetail {
    #[serde(flatten)]
    pub proposal: ProposalListItem,
    pub body_html: String,
    pub body_sections: Option<ProposalSections>,
    pub body_hash: Option<String>,
    pub comments: Vec<Comment>,
    pub analysis: Vec<Analysis>,
    pub advisory: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegistrationRow {
    pub chain: VotingChain,
    pub wallet_address: String,
    pub registered_at_height: i64,
    pub initial_balance: String,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisteredWallet {
    pub chain: VotingChain,
    pub address: String,
    pub registered_at_height: i64,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PendingChains {
    pub terra: bool,
    pub bsc: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegistrationLookup {
    pub terra: Option<RegistrationRow>,
    pub bsc: Option<RegistrationRow>,
    pub pending: PendingChains,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegistrationResponse {
    pub ok: bool,
    pub signature_id: String,
    pub receipt_id: String,
    pub pending: bool,
    pub registration: Option<RegisteredWallet>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DraftResponse {
    pub id: String,
    pub receipt_id: String,
    pub status: ProposalStatus,
    pub revision: i64,
    pub terra_height: Option<i64>,
    pub bsc_block: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AmendResponse {
    pub ok: bool,
    pub receipt_id: String,
    pub body_hash: String,
    pub revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CommentResponse {
    pub id: String,
    pub receipt_id: String,
    pub max_per_wallet: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AnalysisResponse {
    pub id: String,
    pub receipt_id: String,
    pub source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OpenResponse {
    pub ok: bool,
    pub receipt_id: String,
    pub status: ProposalStatus,
    pub terra_height: i64,
    pub bsc_block: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Snapshot {
    pub terra_height: Option<i64>,
    pub bsc_block: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VoteResponse {
    pub ok: bool,
    pub receipt_id: String,
    pub weight: String,
    pub snapshot: Snapshot,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Vote {
    pub choice: VoteChoice,
    pub weight: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BalanceResponse {
    pub chain: VotingChain,
    pub address: String,
    pub height: i64,
    pub as_of_height: i64,
    pub balance: String,
    pub registered: bool,
    pub snapshot_eligible: bool,
    pub pending: bool,
    pub initial_balance: Option<String>,
}
// Receipt content is a tagged domain export and contains variable historical data.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReceiptExport {
    pub signature: SignatureRecord,
    pub receipt: ActionReceipt,
    pub revision: Option<RevisionRecord>,
    pub snapshot: Option<Vec<SnapshotWeight>>,
    pub freeze: Option<FreezeRecord>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SignatureRecord {
    pub id: String,
    pub chain: VotingChain,
    pub wallet_address: String,
    pub pubkey: Option<String>,
    pub signature: String,
    pub payload_hash: String,
    pub purpose: String,
    pub created_at: String,
    pub signed_payload: Option<String>,
    pub nonce: Option<String>,
    pub proposal_id: Option<String>,
    pub revision: Option<i64>,
    pub domain: Option<String>,
    pub scheme: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionReceipt {
    pub signature_id: String,
    pub proposal_id: Option<String>,
    pub revision: Option<i64>,
    pub response: serde_json::Value,
    pub content: Option<serde_json::Value>,
    pub accepted_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RevisionRecord {
    pub proposal_id: String,
    pub revision: i64,
    pub title: String,
    pub body_canonical: String,
    pub body_sections: Option<ProposalSections>,
    pub signature_id: Option<String>,
    pub recorded_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SnapshotWeight {
    pub proposal_id: String,
    pub chain: VotingChain,
    pub wallet_address: String,
    pub weight: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FreezeRecord {
    pub terra_height: i64,
    pub bsc_block: i64,
    pub terra_block_hash: Option<String>,
    pub bsc_block_hash: Option<String>,
    pub terra_epoch: Option<i64>,
    pub bsc_epoch: Option<i64>,
    pub opened_at: String,
}

/// Immutable per-poll power. Future registration admission cannot change it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FrozenWeightResponse {
    pub proposal_id: String,
    pub chain: VotingChain,
    pub address: String,
    pub weight: String,
    pub eligible: bool,
}
