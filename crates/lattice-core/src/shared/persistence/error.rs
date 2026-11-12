//! The error of the persistence port: what a unit of work reports, without
//! naming the driver's error type.
use crate::shared::error::AppError;

/// A failure of the database. Its message is the driver's.
#[derive(Debug)]
pub struct PersistenceError(pub(super) sqlx::Error);

/// What kind of failure it was, for callers that answer differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersistenceErrorKind {
    NotFound,
    Busy,
    Closed,
    Misconfigured,
    Other,
}

/// SQLite's primary result codes for a lock another connection holds. An
/// extended result code carries its primary code in its low byte.
const SQLITE_BUSY: i32 = 5;
const SQLITE_LOCKED: i32 = 6;

impl PersistenceError {
    pub(super) fn closed() -> Self {
        Self(sqlx::Error::PoolClosed)
    }

    pub fn kind(&self) -> PersistenceErrorKind {
        match &self.0 {
            sqlx::Error::RowNotFound => PersistenceErrorKind::NotFound,
            sqlx::Error::PoolTimedOut => PersistenceErrorKind::Busy,
            sqlx::Error::Database(error) if held_by_another_connection(error.code().as_deref()) => {
                PersistenceErrorKind::Busy
            }
            sqlx::Error::PoolClosed => PersistenceErrorKind::Closed,
            sqlx::Error::Configuration(_) => PersistenceErrorKind::Misconfigured,
            _ => PersistenceErrorKind::Other,
        }
    }
}

fn held_by_another_connection(code: Option<&str>) -> bool {
    code.and_then(|code| code.parse::<i32>().ok())
        .is_some_and(|code| matches!(code & 0xff, SQLITE_BUSY | SQLITE_LOCKED))
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for PersistenceError {}

impl From<sqlx::Error> for PersistenceError {
    fn from(error: sqlx::Error) -> Self {
        Self(error)
    }
}

impl From<PersistenceError> for sqlx::Error {
    fn from(error: PersistenceError) -> Self {
        error.0
    }
}

impl From<PersistenceError> for AppError {
    fn from(error: PersistenceError) -> Self {
        AppError::Database(error.to_string())
    }
}
