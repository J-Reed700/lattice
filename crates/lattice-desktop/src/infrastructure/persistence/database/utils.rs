use lattice_core::shared::error::{AppError, Result};
use sqlx::SqlitePool;

/// Issue #10: Fix SQL Injection in Dynamic Queries
/// Safe query builder that prevents SQL injection
pub struct SafeQueryBuilder {
    query: String,
    params: Vec<String>,
}

impl SafeQueryBuilder {
    pub fn new(base_query: &str) -> Self {
        Self {
            query: base_query.to_string(),
            params: Vec::new(),
        }
    }

    pub fn add_where_clause(&mut self, column: &str, values: &[String]) -> Result<()> {
        // Validate column name to prevent injection
        if !column.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(AppError::InvalidInput(format!(
                "Invalid column name: {}",
                column
            )));
        }

        if !values.is_empty() {
            let placeholders = vec!["?"; values.len()].join(", ");

            // SECURITY NOTE: This format! is SAFE despite using SQL string interpolation
            // because:
            // 1. Column name is validated above (alphanumeric + underscore only)
            // 2. Only placeholders ("?") are interpolated, NOT actual values
            // 3. Actual values are bound via .bind() in execute() method (line 201)
            // 4. This follows SQLx parameterized query pattern - SQL structure is built,
            //    but data is kept separate and properly escaped by the database driver
            // Example: If column="user_id" and values=["1", "2"], this creates:
            //   Query: "WHERE user_id IN (?, ?)"
            //   Params: ["1", "2"]  <- bound separately
            // NOT vulnerable to SQL injection because user data never enters SQL string.
            self.query
                .push_str(&format!(" WHERE {} IN ({})", column, placeholders));
            self.params.extend_from_slice(values);
        }

        Ok(())
    }

    pub async fn execute(&self, pool: &SqlitePool) -> Result<Vec<sqlx::sqlite::SqliteRow>> {
        let mut query = sqlx::query(&self.query);
        for param in &self.params {
            query = query.bind(param);
        }

        Ok(query.fetch_all(pool).await?)
    }
}
