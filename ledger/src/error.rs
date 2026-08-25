use thiserror::Error;

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("missing required env {0}")]
    MissingEnv(&'static str),
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migrate: {0}")]
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
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type LedgerResult<T> = Result<T, LedgerError>;
