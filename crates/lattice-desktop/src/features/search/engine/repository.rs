use crate::features::search::engine::fts_query::{self, FtsIndex, FtsQuery};
use lattice_core::application::contracts::search::engine::BM25Result;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use sqlx::Row;

fn query_failed(error: sqlx::Error) -> AppError {
    AppError::Database(format!("Database query failed: {}", error))
}

/// One statement per index. Written out rather than built by string
/// concatenation so `scripts/check-sql-contracts.py` can prepare both against
/// the migration. The first bind is the header column's `bm25()` weight
/// ([`fts_query::HEADER_BM25_WEIGHT`]), so this path and chat's keyword branch
/// rank alike.
///
/// Both leave out chunks of files attached to a chat. Every caller is a
/// vault-wide search, and dropping an attachment only after `LIMIT` would
/// spend one of the requested slots on a hit that then vanishes.
const WORDS_SQL: &str = "SELECT
        tc.id as chunk_id,
        tc.document_id as document_id,
        bm25(chunks_fts, 0.0, 1.0, ?) as score,
        d.file_name as filename,
        d.mime_type,
        d.size_bytes,
        d.created_at,
        tc.content
     FROM chunks_fts
     JOIN text_chunks tc ON chunks_fts.chunk_id = tc.id
     LEFT JOIN documents d ON tc.document_id = d.id
     WHERE chunks_fts MATCH ?
       AND d.owner_conversation_id IS NULL
     ORDER BY score
     LIMIT ?";

const TRIGRAM_SQL: &str = "SELECT
        tc.id as chunk_id,
        tc.document_id as document_id,
        bm25(chunks_trigram, 0.0, 1.0, ?) as score,
        d.file_name as filename,
        d.mime_type,
        d.size_bytes,
        d.created_at,
        tc.content
     FROM chunks_trigram
     JOIN text_chunks tc ON chunks_trigram.chunk_id = tc.id
     LEFT JOIN documents d ON tc.document_id = d.id
     WHERE chunks_trigram MATCH ?
       AND d.owner_conversation_id IS NULL
     ORDER BY score
     LIMIT ?";

pub(super) async fn bm25_results(
    db: &Database,
    query: &FtsQuery,
    top_k: usize,
) -> Result<Vec<BM25Result>> {
    let sql = match query.index {
        FtsIndex::Words => WORDS_SQL,
        FtsIndex::Trigram => TRIGRAM_SQL,
    };
    let match_expression = query.match_expression.clone();

    let mut read = db.begin_read().await?;
    let rows = sqlx::query(sql)
        .bind(fts_query::HEADER_BM25_WEIGHT)
        .bind(match_expression)
        .bind(top_k as i64)
        .fetch_all(&mut *read)
        .await
        .map_err(query_failed)?;
    read.commit().await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let chunk_id: String = row.get("chunk_id");
            let document_id: String = row.get("document_id");
            let score: f32 = row.get("score");
            let filename: Option<String> = row.try_get("filename").ok();
            let mime_type: Option<String> = row.try_get("mime_type").ok();
            let size_bytes: Option<i64> = row.try_get("size_bytes").ok();
            let created_at: Option<String> = row.try_get("created_at").ok();
            let content: Option<String> = row.try_get("content").ok();

            BM25Result {
                chunk_id,
                document_id,
                score: -score,
                filename,
                mime_type,
                size_bytes,
                created_at,
                content,
            }
        })
        .collect())
}

pub(super) async fn optimize_chunk_indexes(db: &Database) -> Result<()> {
    let mut tx = db.begin_write().await?;
    sqlx::query("INSERT INTO chunks_fts(chunks_fts) VALUES('optimize')")
        .execute(&mut *tx)
        .await
        .map_err(query_failed)?;
    sqlx::query("INSERT INTO chunks_trigram(chunks_trigram) VALUES('optimize')")
        .execute(&mut *tx)
        .await
        .map_err(query_failed)?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn rebuild_chunk_indexes(db: &Database) -> Result<()> {
    let mut tx = db.begin_write().await?;
    sqlx::query("INSERT INTO chunks_fts(chunks_fts) VALUES('rebuild')")
        .execute(&mut *tx)
        .await
        .map_err(query_failed)?;
    sqlx::query("INSERT INTO chunks_trigram(chunks_trigram) VALUES('rebuild')")
        .execute(&mut *tx)
        .await
        .map_err(query_failed)?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn document_updated_at(
    db: &Database,
    document_ids: &[String],
) -> Result<Vec<(String, String)>> {
    let placeholders = document_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");

    let query = format!(
        r#"
        SELECT id, updated_at
        FROM documents
        WHERE id IN ({})
        "#,
        placeholders
    );

    let mut query_builder = sqlx::query(&query);
    for id in document_ids {
        query_builder = query_builder.bind(id);
    }

    let mut read = db.begin_read().await?;
    let rows = query_builder
        .fetch_all(&mut *read)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch timestamps: {}", e)))?;
    read.commit().await?;

    rows.into_iter()
        .map(|row| {
            let id: String = row
                .try_get("id")
                .map_err(|e| AppError::Database(format!("Failed to get document id: {}", e)))?;
            let updated_at: String = row
                .try_get("updated_at")
                .map_err(|e| AppError::Database(format!("Failed to get updated_at: {}", e)))?;
            Ok((id, updated_at))
        })
        .collect()
}
