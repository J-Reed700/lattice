//! # Initialization DTOs
//!
//! Data Transfer Objects for system initialization use cases.
//!
//! These DTOs handle first-run setup operations including:
//! - Database schema creation
//!
//! ## Design Philosophy (Bricks and Studs)
//!
//! - **Minimal**: Only essential fields
//! - **Clear**: Descriptive names and comprehensive documentation
//! - **Stable**: Versioned schemas for breaking changes
//! - **Validated**: Strong typing with serde

use serde::{Deserialize, Serialize};

/// Response from initialize_database use case.
///
/// Contains the outcome of database schema creation and migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeDatabaseResponseDto {
    /// Whether initialization was successful
    pub success: bool,

    /// Path to the database file
    pub database_path: String,

    /// Schema version after initialization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<i64>,

    /// Whether this was a first-time initialization (true) or existing database (false)
    #[serde(default)]
    pub is_new_database: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_database_response_serialization() {
        let response = InitializeDatabaseResponseDto {
            success: true,
            database_path: "/path/to/lattice.db".to_string(),
            schema_version: Some(1),
            is_new_database: true,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: InitializeDatabaseResponseDto = serde_json::from_str(&json).unwrap();

        assert!(deserialized.success);
        assert_eq!(deserialized.database_path, "/path/to/lattice.db");
        assert_eq!(deserialized.schema_version, Some(1));
        assert!(deserialized.is_new_database);
    }
}
