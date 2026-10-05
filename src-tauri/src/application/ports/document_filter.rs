use super::Filter;
use crate::shared::error::{AppError, Result};

/// Filter for querying documents by various criteria.
#[derive(Debug, Clone)]
pub struct DocumentFilter {
    /// Filter by file path pattern (SQL LIKE)
    pub path_pattern: Option<String>,
    /// Filter by status
    pub status: Option<String>,
    /// Limit number of results
    pub limit: Option<usize>,
}

impl Filter for DocumentFilter {
    fn validate(&self) -> Result<()> {
        if let Some(limit) = self.limit {
            if limit == 0 || limit > 10000 {
                return Err(AppError::InvalidInput(
                    "Limit must be between 1 and 10000".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
