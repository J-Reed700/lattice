//! The one rule for every listing: no `limit` returns every matching row,
//! and a `limit` returns at most that many.

use crate::shared::error::AppError;

/// A caller's page of a listing. Nothing is defaulted or clamped.
#[derive(Debug, Clone, Copy, Default)]
pub struct Page {
    limit: Option<i64>,
    offset: i64,
}

impl Page {
    /// `limit` of at least 1 when given; `offset` of at least 0.
    pub fn new(limit: Option<i64>, offset: Option<i64>) -> Result<Self, AppError> {
        if limit.is_some_and(|limit| limit < 1) {
            return Err(AppError::InvalidInput(
                "limit must be at least 1".to_string(),
            ));
        }
        let offset = offset.unwrap_or(0);
        if offset < 0 {
            return Err(AppError::InvalidInput(
                "offset must not be negative".to_string(),
            ));
        }
        Ok(Self { limit, offset })
    }

    /// The first `limit` rows, for a caller whose count is required.
    pub fn first(limit: usize) -> Result<Self, AppError> {
        Self::new(Some(i64::try_from(limit).unwrap_or(i64::MAX)), None)
    }

    /// The most rows to return, or `None` for every row.
    pub fn limit(self) -> Option<i64> {
        self.limit
    }

    /// The value for SQLite's `LIMIT`, where -1 means no limit.
    pub fn sql_limit(self) -> i64 {
        self.limit.unwrap_or(-1)
    }

    pub fn offset(self) -> i64 {
        self.offset
    }
}
