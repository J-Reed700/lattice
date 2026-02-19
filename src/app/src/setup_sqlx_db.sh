#!/bin/bash
set -e

echo "Creating fresh SQLx database..."
rm -f sqlx_prepare.db

# Create database
sqlite3 sqlx_prepare.db "SELECT 1;"

# Run migrations through Rust
cat > temp_migrate.rs << 'RUST'
use sqlx::SqlitePool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = SqlitePool::connect("sqlite://sqlx_prepare.db").await?;
    
    // Run each migration
    for file in std::fs::read_dir("migrations")? {
        let file = file?;
        let path = file.path();
        if path.extension().and_then(|s| s.to_str()) == Some("sql") {
            let sql = std::fs::read_to_string(&path)?;
            for statement in sql.split(';') {
                let stmt = statement.trim();
                if !stmt.is_empty() && !stmt.starts_with("--") {
                    sqlx::query(stmt).execute(&pool).await?;
                }
            }
            println!("Applied: {:?}", path.file_name());
        }
    }
    
    Ok(())
}
RUST

echo "✓ SQLx database created"
