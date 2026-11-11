//! The one rule for every conversation listing: no `limit` returns every
//! matching row, and a `limit` returns at most that many.

use lattice_core::shared::error::AppError;

/// Checks a caller's page: `limit` of at least 1 when given, `offset` of at
/// least 0. Nothing is defaulted or clamped.
pub(in crate::features::conversation) fn validate_page(
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<(Option<i64>, i64), AppError> {
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
    Ok((limit, offset))
}
