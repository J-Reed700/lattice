use sqlx::SqlitePool;
use crate::shared::error::{AppError, Result};

pub async fn migrate_to_document_level_fts(pool: &SqlitePool) -> Result<()> {
    sqlx::query("DROP TRIGGER IF EXISTS chunks_ai")
        .execute(pool)
        .await?;
    
    sqlx::query("DROP TRIGGER IF EXISTS chunks_au")
        .execute(pool)
        .await?;
    
    sqlx::query("DROP TRIGGER IF EXISTS chunks_ad")
        .execute(pool)
        .await?;
    
    sqlx::query("DROP TRIGGER IF EXISTS chunks_ai_update")
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM documents_fts")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO documents_fts(document_id, content)
        SELECT document_id, GROUP_CONCAT(content, ' ' ORDER BY chunk_index)
        FROM text_chunks
        GROUP BY document_id
        "#
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TRIGGER IF NOT EXISTS chunks_ai 
        AFTER INSERT ON chunks 
        WHEN (SELECT COUNT(*) FROM text_chunks WHERE document_id = new.document_id) = 1
        BEGIN
            INSERT INTO documents_fts(document_id, content)
            SELECT new.document_id, 
                   GROUP_CONCAT(content, ' ' ORDER BY chunk_index)
            FROM text_chunks 
            WHERE document_id = new.document_id;
        END
        "#
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TRIGGER IF NOT EXISTS chunks_ai_update 
        AFTER INSERT ON chunks 
        WHEN (SELECT COUNT(*) FROM text_chunks WHERE document_id = new.document_id) > 1
        BEGIN
            DELETE FROM documents_fts WHERE document_id = new.document_id;
            INSERT INTO documents_fts(document_id, content)
            SELECT new.document_id, 
                   GROUP_CONCAT(content, ' ' ORDER BY chunk_index)
            FROM text_chunks 
            WHERE document_id = new.document_id;
        END
        "#
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TRIGGER IF NOT EXISTS chunks_ad 
        AFTER DELETE ON chunks 
        BEGIN
            DELETE FROM documents_fts WHERE document_id = old.document_id;
            INSERT INTO documents_fts(document_id, content)
            SELECT old.document_id, 
                   GROUP_CONCAT(content, ' ' ORDER BY chunk_index)
            FROM text_chunks 
            WHERE document_id = old.document_id;
        END
        "#
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TRIGGER IF NOT EXISTS chunks_au 
        AFTER UPDATE ON chunks 
        BEGIN
            DELETE FROM documents_fts WHERE document_id = old.document_id;
            INSERT INTO documents_fts(document_id, content)
            SELECT new.document_id, 
                   GROUP_CONCAT(content, ' ' ORDER BY chunk_index)
            FROM text_chunks 
            WHERE document_id = new.document_id;
        END
        "#
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn verify_migration(pool: &SqlitePool) -> Result<()> {
    let result = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(DISTINCT document_id) FROM documents_fts"
    )
    .fetch_one(pool)
    .await?;
    
    let expected = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(DISTINCT document_id) FROM text_chunks"
    )
    .fetch_one(pool)
    .await?;
    
    if result != expected {
        return Err(AppError::Database(format!(
            "Migration verification failed: FTS5 has {} document entries but expected {}",
            result,
            expected
        )));
    }
    
    Ok(())
}
