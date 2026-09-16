//! Database stats port.
//!
//! Provides database-level statistics such as file size.

use crate::shared::error::Result;
use async_trait::async_trait;

#[async_trait]
pub trait DatabaseStatsPort: Send + Sync {
    /// Get the SQLite database file size in bytes.
    async fn get_database_size_bytes(&self) -> Result<i64>;
}
