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
        if let Self::Database(error) = self {
            if let sqlx::Error::Database(database) = error {
                if let Some(code) = database.code().filter(|code| safe_sqlstate(code)) {
                    return write!(f, "database error (sqlstate={code})");
                }
            }
            let class = match error {
                sqlx::Error::Configuration(_) => "configuration",
                sqlx::Error::InvalidArgument(_) => "invalid_argument",
                sqlx::Error::Database(_) => "database",
                sqlx::Error::Io(_) => "io",
                sqlx::Error::Tls(_) => "tls",
                sqlx::Error::Protocol(_) => "protocol",
                sqlx::Error::RowNotFound => "row_not_found",
                sqlx::Error::TypeNotFound { .. } => "type_not_found",
                sqlx::Error::ColumnIndexOutOfBounds { .. } => "column_index",
                sqlx::Error::ColumnNotFound(_) => "column_not_found",
                sqlx::Error::ColumnDecode { .. } => "column_decode",
                sqlx::Error::Encode(_) => "encode",
                sqlx::Error::Decode(_) => "decode",
                sqlx::Error::PoolTimedOut => "pool_timeout",
                sqlx::Error::PoolClosed => "pool_closed",
                sqlx::Error::WorkerCrashed => "worker_crashed",
                sqlx::Error::Migrate(_) => "migration",
                _ => "other",
            };
            write!(f, "database error (class={class})")
        } else {
            std::fmt::Display::fmt(self, f)
        }
    }
}

// Database diagnostics are identifiers only; never format the underlying SQLx error.
fn safe_sqlstate(code: &str) -> bool {
    code.len() == 5
        && code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_debug_reports_only_static_classes_and_validated_sqlstate() {
        let sentinel = "postgres://user:SENTINEL@private.example/database?query=secret";
        for (source, class) in [
            (sqlx::Error::Protocol(sentinel.into()), "protocol"),
            (
                sqlx::Error::ColumnNotFound(sentinel.into()),
                "column_not_found",
            ),
            (sqlx::Error::Io(std::io::Error::other(sentinel)), "io"),
            (sqlx::Error::PoolTimedOut, "pool_timeout"),
        ] {
            let error = LedgerError::Database(source);
            assert_eq!(error.to_string(), "database error");
            assert_eq!(
                format!("{error:?}"),
                format!("database error (class={class})")
            );
        }
        assert!(safe_sqlstate("23505"));
        assert!(safe_sqlstate("42P01"));
        for code in ["", "42p01", "23505\n", "ABCDESECRET", "SE:ET", sentinel] {
            assert!(!safe_sqlstate(code));
        }
    }
}
