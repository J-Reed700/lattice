//! The error of the persistence port: what a unit of work reports, without
//! naming the driver's error type.
use crate::shared::error::AppError;

/// A failure of the database: the driver's, or a unit that only reads having
/// changed rows.
#[derive(Debug)]
pub struct PersistenceError(Failure);

#[derive(Debug)]
enum Failure {
    Driver(sqlx::Error),
    ChangedWhileReading,
}

const CHANGED_WHILE_READING: &str =
    "a unit of work that only reads changed rows, so it was rolled back";

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
        Self(Failure::Driver(sqlx::Error::PoolClosed))
    }

    pub(super) fn changed_while_reading() -> Self {
        Self(Failure::ChangedWhileReading)
    }

    pub fn kind(&self) -> PersistenceErrorKind {
        let Failure::Driver(error) = &self.0 else {
            return PersistenceErrorKind::Other;
        };
        match error {
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
        match &self.0 {
            Failure::Driver(error) => error.fmt(f),
            Failure::ChangedWhileReading => f.write_str(CHANGED_WHILE_READING),
        }
    }
}

impl std::error::Error for PersistenceError {}

impl From<sqlx::Error> for PersistenceError {
    fn from(error: sqlx::Error) -> Self {
        Self(Failure::Driver(error))
    }
}

impl From<PersistenceError> for sqlx::Error {
    fn from(error: PersistenceError) -> Self {
        match error.0 {
            Failure::Driver(error) => error,
            Failure::ChangedWhileReading => sqlx::Error::Protocol(CHANGED_WHILE_READING.into()),
        }
    }
}

impl From<PersistenceError> for AppError {
    fn from(error: PersistenceError) -> Self {
        AppError::Database(error.to_string())
    }
}
