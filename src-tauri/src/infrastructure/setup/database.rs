use crate::infrastructure::persistence::database as db;
use crate::shared::error::{AppError, Result};
use std::path::PathBuf;

pub async fn setup_database(db_path: PathBuf) -> Result<db::DatabaseConnection> {
    let conn = db::DatabaseConnection::new(db_path.clone())
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to create database connection at {:?}.\n\n\
                              Possible causes:\n\
                              - Database file is locked by another process\n\
                              - Missing write permissions\n\
                              - Corrupted database file\n\n\
                              Suggested actions:\n\
                              - Close other instances of the application\n\
                              - Check folder permissions\n\
                              - Delete lattice.db if corrupted (WARNING: loses data)\n\n\
                              Error: {}",
                db_path, e
            ))
        })?;

    db::initialize_database(conn.pool()).await.map_err(|e| {
        AppError::Migration(format!(
            "Failed to initialize database schema.\n\n\
                              Possible causes:\n\
                              - Corrupted database file\n\
                              - Insufficient disk space (need ~100MB free)\n\
                              - Database version mismatch\n\n\
                              Suggested actions:\n\
                              - Free up disk space\n\
                              - Delete lattice.db to recreate (WARNING: loses data)\n\
                              - Reinstall the application\n\n\
                              Error: {}",
            e
        ))
    })?;

    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_error_messages() {
        let db_path = PathBuf::from("/test/lattice.db");
        let err = format!(
            "Failed to create database connection at {:?}.\n\n\
                          Possible causes:\n\
                          - Database file is locked by another process\n\
                          - Missing write permissions\n\
                          - Corrupted database file\n\n\
                          Suggested actions:\n\
                          - Close other instances of the application\n\
                          - Check folder permissions\n\
                          - Delete lattice.db if corrupted (WARNING: loses data)\n\n\
                          Error: test error",
            db_path
        );

        assert!(err.contains("Database file is locked"));
        assert!(err.contains("Close other instances"));
        assert!(err.contains("/test/lattice.db"));
    }
}
