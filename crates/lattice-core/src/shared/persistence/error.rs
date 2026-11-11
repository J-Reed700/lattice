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

impl PersistenceError {
    pub fn kind(&self) -> PersistenceErrorKind {
        match &self.0 {
            sqlx::Error::RowNotFound => PersistenceErrorKind::NotFound,
            sqlx::Error::PoolTimedOut => PersistenceErrorKind::Busy,
            sqlx::Error::PoolClosed => PersistenceErrorKind::Closed,
            sqlx::Error::Configuration(_) => PersistenceErrorKind::Misconfigured,
            _ => PersistenceErrorKind::Other,
        }
    }
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

impl From<PersistenceError> for AppError {
    fn from(error: PersistenceError) -> Self {
        AppError::Database(error.to_string())
    }
}
