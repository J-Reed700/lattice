//! Database migration utilities for upgrading existing databases
//!
//! This module provides a simple wrapper around sqlx's migration functionality.
//! Migrations are automatically discovered from the migrations/ directory.

use crate::shared::error::{AppError, Result};
use sqlx::migrate::Migrator;
use sqlx::{Row, SqlitePool};

/// Run all pending migrations from the migrations/ directory
///
/// This function uses sqlx::migrate!() to automatically discover and run
/// migration files from the migrations/ directory at compile time.
///
/// # Arguments
///
/// * `pool` - SQLite connection pool to run migrations against
///
/// # Returns
///
/// `Ok(())` if all migrations complete successfully, or an error if any fail.
///
/// # Example
///
/// ```rust
/// use sqlx::SqlitePool;
/// use crate::infrastructure::persistence::database::migrate::run_migrations;
///
/// let pool = SqlitePool::connect("sqlite::memory:").await?;
/// run_migrations(&pool).await?;
/// ```
pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    tracing::info!("Starting database migrations...");

    let migrator = sqlx::migrate!("./migrations");
    if let Err(error) = migrator.run(pool).await {
        let error_text = error.to_string();
        if let Some(version) = parse_modified_migration_version(&error_text) {
            tracing::warn!(
                version = version,
                "Detected modified-applied migration; attempting non-destructive checksum reconciliation"
            );
            reconcile_modified_migration_checksum(pool, &migrator, version).await?;
            ensure_conversation_state_schema(pool).await?;

            migrator.run(pool).await.map_err(|e| {
                tracing::error!("Migration failed after reconciliation: {}", e);
                AppError::Database(format!("Migration failed after reconciliation: {}", e))
            })?;
        } else {
            tracing::error!("Migration failed: {}", error_text);
            return Err(AppError::Database(format!(
                "Migration failed: {}",
                error_text
            )));
        }
    }

    tracing::info!("All migrations completed successfully");
    Ok(())
}

fn parse_modified_migration_version(error: &str) -> Option<i64> {
    let marker = "migration ";
    let mismatch = " was previously applied but has been modified";
    let start = error.find(marker)? + marker.len();
    let end = error[start..].find(mismatch)? + start;
    error[start..end].trim().parse::<i64>().ok()
}

async fn reconcile_modified_migration_checksum(
    pool: &SqlitePool,
    migrator: &Migrator,
    version: i64,
) -> Result<()> {
    let migration = migrator
        .iter()
        .find(|m| m.version == version)
        .ok_or_else(|| {
            AppError::Database(format!(
                "Failed to reconcile migration {}: migration not found in source",
                version
            ))
        })?;

    let result = sqlx::query("UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = ?2")
        .bind(migration.checksum.as_ref())
        .bind(version)
        .execute(pool)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to update migration checksum for {}: {}",
                version, e
            ))
        })?;

    if result.rows_affected() == 0 {
        return Err(AppError::Database(format!(
            "Failed to reconcile migration {}: no applied migration row found",
            version
        )));
    }

    Ok(())
}

async fn ensure_conversation_state_schema(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS conversation_spaces (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            description TEXT,
            icon TEXT,
            accent_color TEXT,
            space_prompt TEXT,
            default_model_name TEXT,
            tool_preferences_json TEXT,
            is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        AppError::Database(format!("Failed to ensure conversation_spaces table: {}", e))
    })?;

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO conversation_spaces (id, name, description, sort_order)
        VALUES ('space_general', 'General', 'Default space for all conversations', 0)
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to ensure default conversation space row exists: {}",
            e
        ))
    })?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS collaborator_profiles (
            id TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            email TEXT,
            avatar_url TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to ensure collaborator_profiles table: {}",
            e
        ))
    })?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS conversation_space_members (
            space_id TEXT NOT NULL,
            member_id TEXT NOT NULL,
            role TEXT NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (space_id, member_id),
            FOREIGN KEY (space_id) REFERENCES conversation_spaces(id) ON DELETE CASCADE,
            FOREIGN KEY (member_id) REFERENCES collaborator_profiles(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to ensure conversation_space_members table: {}",
            e
        ))
    })?;

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO collaborator_profiles (
            id, display_name, email, avatar_url, created_at, updated_at
        ) VALUES (
            'member_local_owner', 'Local Owner', NULL, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to ensure default collaborator profile row exists: {}",
            e
        ))
    })?;

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO conversation_space_members (space_id, member_id, role)
        SELECT id, 'member_local_owner', 'owner'
        FROM conversation_spaces
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to ensure default space owner memberships: {}",
            e
        ))
    })?;

    ensure_column_exists(
        pool,
        "conversations",
        "space_id",
        "ALTER TABLE conversations ADD COLUMN space_id TEXT NOT NULL DEFAULT 'space_general'",
    )
    .await?;
    ensure_column_exists(
        pool,
        "conversations",
        "is_saved",
        "ALTER TABLE conversations ADD COLUMN is_saved INTEGER NOT NULL DEFAULT 0 CHECK (is_saved IN (0, 1))",
    )
    .await?;
    ensure_column_exists(
        pool,
        "conversations",
        "is_bookmarked",
        "ALTER TABLE conversations ADD COLUMN is_bookmarked INTEGER NOT NULL DEFAULT 0 CHECK (is_bookmarked IN (0, 1))",
    )
    .await?;
    ensure_column_exists(
        pool,
        "conversations",
        "is_pinned",
        "ALTER TABLE conversations ADD COLUMN is_pinned INTEGER NOT NULL DEFAULT 0 CHECK (is_pinned IN (0, 1))",
    )
    .await?;
    ensure_column_exists(
        pool,
        "conversations",
        "is_archived",
        "ALTER TABLE conversations ADD COLUMN is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1))",
    )
    .await?;
    ensure_column_exists(
        pool,
        "conversations",
        "saved_at",
        "ALTER TABLE conversations ADD COLUMN saved_at TEXT",
    )
    .await?;
    ensure_column_exists(
        pool,
        "conversations",
        "bookmarked_at",
        "ALTER TABLE conversations ADD COLUMN bookmarked_at TEXT",
    )
    .await?;
    ensure_column_exists(
        pool,
        "conversations",
        "pinned_at",
        "ALTER TABLE conversations ADD COLUMN pinned_at TEXT",
    )
    .await?;
    ensure_column_exists(
        pool,
        "conversations",
        "archived_at",
        "ALTER TABLE conversations ADD COLUMN archived_at TEXT",
    )
    .await?;

    sqlx::query(
        "UPDATE conversations SET space_id = 'space_general' WHERE space_id IS NULL OR TRIM(space_id) = ''",
    )
    .execute(pool)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to normalize conversation space_id defaults: {}",
            e
        ))
    })?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS conversation_message_bookmarks (
            id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            message_id TEXT NOT NULL,
            title TEXT,
            note TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
            FOREIGN KEY (message_id) REFERENCES conversation_messages(id) ON DELETE CASCADE,
            UNIQUE (conversation_id, message_id)
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to ensure conversation_message_bookmarks table: {}",
            e
        ))
    })?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS document_space_memberships (
            document_id TEXT NOT NULL,
            space_id TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (document_id, space_id),
            FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE,
            FOREIGN KEY (space_id) REFERENCES conversation_spaces(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to ensure document_space_memberships table: {}",
            e
        ))
    })?;

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO document_space_memberships (document_id, space_id)
        SELECT DISTINCT
            cd.document_id,
            c.space_id
        FROM conversation_documents cd
        INNER JOIN conversations c ON c.id = cd.conversation_id
        WHERE cd.document_id IS NOT NULL
          AND TRIM(cd.document_id) <> ''
          AND c.space_id IS NOT NULL
          AND TRIM(c.space_id) <> ''
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to backfill document_space_memberships table: {}",
            e
        ))
    })?;

    for index_sql in [
        "CREATE INDEX IF NOT EXISTS idx_conversation_spaces_sort ON conversation_spaces(sort_order, updated_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_conversation_spaces_archived ON conversation_spaces(is_archived, updated_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_conversations_space_updated ON conversations(space_id, updated_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_conversations_saved ON conversations(space_id, is_saved, updated_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_conversations_bookmarked ON conversations(space_id, is_bookmarked, updated_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_conversations_pinned ON conversations(space_id, is_pinned, pinned_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_conversations_archived ON conversations(space_id, is_archived, updated_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_message_bookmarks_conversation ON conversation_message_bookmarks(conversation_id, created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_message_bookmarks_message ON conversation_message_bookmarks(message_id)",
        "CREATE INDEX IF NOT EXISTS idx_document_space_memberships_space_doc ON document_space_memberships(space_id, document_id)",
        "CREATE INDEX IF NOT EXISTS idx_document_space_memberships_document_space ON document_space_memberships(document_id, space_id)",
        "CREATE INDEX IF NOT EXISTS idx_collaborator_profiles_display_name ON collaborator_profiles(display_name COLLATE NOCASE)",
        "CREATE INDEX IF NOT EXISTS idx_conversation_space_members_space_role ON conversation_space_members(space_id, role)",
        "CREATE INDEX IF NOT EXISTS idx_conversation_space_members_member_space ON conversation_space_members(member_id, space_id)",
    ] {
        sqlx::query(index_sql).execute(pool).await.map_err(|e| {
            AppError::Database(format!("Failed to ensure conversation index exists: {}", e))
        })?;
    }

    Ok(())
}

async fn ensure_column_exists(
    pool: &SqlitePool,
    table_name: &str,
    column_name: &str,
    alter_sql: &str,
) -> Result<()> {
    let pragma_sql = format!("PRAGMA table_info({})", table_name);
    let rows = sqlx::query(&pragma_sql)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to inspect table_info for {}: {}",
                table_name, e
            ))
        })?;

    let has_column = rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|value| value == column_name)
            .unwrap_or(false)
    });
    if has_column {
        return Ok(());
    }

    sqlx::query(alter_sql).execute(pool).await.map_err(|e| {
        AppError::Database(format!(
            "Failed to add missing column {}.{}: {}",
            table_name, column_name, e
        ))
    })?;

    Ok(())
}
