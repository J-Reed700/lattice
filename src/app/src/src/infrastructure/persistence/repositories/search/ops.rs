use crate::domain::repositories::search_repository::SearchResult;
use crate::error::AppError;
use crate::infrastructure::persistence::database::connection::{
    query_with_heavy_timeout, query_with_timeout,
};
use sqlx::{Row, SqliteConnection};
use tracing::{debug, error};

pub async fn search_bm25(
    conn: &mut SqliteConnection,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, AppError> {
    let query_string = query.to_string();
    let query_clone = query_string.clone();

    let results: Vec<sqlx::sqlite::SqliteRow> = query_with_timeout(|| async {
        sqlx::query(
            "SELECT
                fts.chunk_id,
                fts.document_id,
                bm25(chunks_fts) as score,
                d.file_name as filename,
                d.mime_type,
                d.size_bytes,
                d.created_at,
                fts.content
             FROM text_chunks_fts fts
             LEFT JOIN documents d ON fts.document_id = d.id
             WHERE chunks_fts MATCH ?
             ORDER BY score
             LIMIT ?",
        )
        .bind(&query_string)
        .bind(limit as i64)
        .fetch_all(conn)
        .await
    })
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to execute BM25 search");
        AppError::Database(format!("Failed to execute BM25 search: {}", e))
    })?;

    let search_results: Vec<SearchResult> = results
        .into_iter()
        .map(|row| {
            let document_id: String = row.get("document_id");
            let score: f32 = row.get("score");
            let filename: Option<String> = row.try_get("filename").ok();
            let mime_type: Option<String> = row.try_get("mime_type").ok();
            let size_bytes: Option<i64> = row.try_get("size_bytes").ok();
            let created_at: Option<String> = row.try_get("created_at").ok();
            let content: Option<String> = row.try_get("content").ok();

            SearchResult {
                document_id,
                score: -score,
                filename,
                mime_type,
                size_bytes,
                created_at,
                content,
            }
        })
        .collect();

    debug!(query = %query_clone, count = search_results.len(), "BM25 search completed");
    Ok(search_results)
}

pub async fn count_searchable_chunks(conn: &mut SqliteConnection) -> Result<i64, AppError> {
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM text_chunks"#)
        .fetch_one(conn)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to count chunks");
            AppError::Database(format!("Failed to count chunks: {}", e))
        })?;

    debug!(count = count, "Counted searchable chunks");
    Ok(count)
}

pub async fn optimize_index(conn: &mut SqliteConnection) -> Result<(), AppError> {
    query_with_heavy_timeout(|| async {
        sqlx::query("INSERT INTO documents_fts(documents_fts) VALUES('optimize')")
            .execute(conn)
            .await
    })
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to optimize index");
        AppError::Database(format!("Failed to optimize index: {}", e))
    })?;

    debug!("Index optimization completed");
    Ok(())
}

pub async fn rebuild_index(conn: &mut SqliteConnection) -> Result<(), AppError> {
    query_with_heavy_timeout(|| async {
        sqlx::query("INSERT INTO documents_fts(documents_fts) VALUES('rebuild')")
            .execute(conn)
            .await
    })
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to rebuild index");
        AppError::Database(format!("Failed to rebuild index: {}", e))
    })?;

    debug!("Index rebuild completed");
    Ok(())
}
