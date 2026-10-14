use thiserror::Error;

#[derive(Error)]
pub enum LedgerError {
    #[error("provider: {0}")]
    Provider(&'static str),
    #[error("missing required env {0}")]
    MissingEnv(&'static str),
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("migration error")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("lcd: {0}")]
    Lcd(String),
    #[error("bsc rpc: {0}")]
    BscRpc(String),
    #[error("amount: {0}")]
    Amount(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("reorg detected at height {height} (expected hash {expected}, got {actual})")]
    ReorgDetected {
        height: i64,
        expected: String,
        actual: String,
    },
    #[error("I/O error")]
    Io(#[from] std::io::Error),
}

pub type LedgerResult<T> = Result<T, LedgerError>;

impl std::fmt::Debug for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
