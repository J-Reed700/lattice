//! SQLite's ceiling on bound parameters per statement, and the batch sizes
//! that follow from it.

/// `SQLITE_MAX_VARIABLE_NUMBER`: 32766 by default since SQLite 3.32.0, which
/// the bundled `libsqlite3-sys` is far past.
pub const SQLITE_MAX_BOUND_PARAMETERS: usize = 32_766;

/// How many rows of `params_per_row` bound values one statement takes beside
/// its `fixed_params` bound values.
pub const fn rows_per_statement(params_per_row: usize, fixed_params: usize) -> usize {
    (SQLITE_MAX_BOUND_PARAMETERS - fixed_params) / params_per_row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_statement_never_binds_more_than_sqlite_allows() {
        for (per_row, fixed) in [(1, 0), (1, 1), (2, 0), (3, 1), (6, 0), (15, 0)] {
            let rows = rows_per_statement(per_row, fixed);
            assert!(rows >= 1);
            assert!(rows * per_row + fixed <= SQLITE_MAX_BOUND_PARAMETERS);
            assert!((rows + 1) * per_row + fixed > SQLITE_MAX_BOUND_PARAMETERS);
        }
    }
}
