//! Test Module
//!
//! Contains integration tests and test utilities for the Recall crate.

// Common test utilities module (used by all tests)
pub mod common;

// Hexagonal test infrastructure module (optional).
// TODO: Restore when test phase resumes and hexagonal module is present.

// Legacy test utilities (kept for backward compatibility)
mod legacy_common {
    pub mod test_utils {
        use sqlx::SqlitePool;
        use uuid::Uuid;

        /// Create a test database pool with UUID-based isolation
        ///
        /// Each test gets its own unique in-memory database to prevent interference.
        /// This is the gold standard for test isolation as approved by Oracle.
        pub async fn create_test_pool_with_uuid() -> SqlitePool {
            let db_name = format!(":memory:?uuid={}", Uuid::new_v4());
            let pool = SqlitePool::connect(&db_name)
                .await
                .expect("Failed to create test pool");

            // Run migrations to create schema
            sqlx::migrate!("./migrations")
                .run(&pool)
                .await
                .expect("Failed to run migrations");

            pool
        }
    }
}

// Plugin smoke tests module (Oracle Week 2 priority)
#[cfg(test)]
pub mod plugins;

// Integration tests module
#[cfg(test)]
pub mod integration;
