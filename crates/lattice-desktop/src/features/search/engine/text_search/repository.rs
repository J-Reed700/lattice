use crate::features::search::engine::fts_query::{FtsIndex, FtsQuery};
use lattice_core::application::contracts::search::SearchResultRecord;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;
use sqlx::SqliteConnection;
use std::collections::HashSet;

pub(super) async fn scoped_hits(
    db: &Database,
    header_weight: f64,
    query: &FtsQuery,
    top_k: usize,
    space_id: Option<&str>,
    allowed_document_ids: Option<&HashSet<String>>,
) -> Result<Vec<SearchResultRecord>> {
    let mut read = db.begin_read().await?;
    let rows = scoped_rows(
        &mut read,
        header_weight,
        query,
        top_k,
        space_id,
        allowed_document_ids,
    )
    .await?;
    read.commit().await?;
    Ok(rows_to_results(rows))
}

async fn scoped_rows(
    connection: &mut SqliteConnection,
    header_weight: f64,
    query: &FtsQuery,
    top_k: usize,
    space_id: Option<&str>,
    allowed_document_ids: Option<&HashSet<String>>,
) -> std::result::Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
    // Apply ALL scope restrictions before ranking/LIMIT. Filtering the top
    // global hits afterwards can hide every hit from a selected chapter.
    //
    // The table name is chosen from a closed set, never from user input.
    // The weights follow the column order: `chunk_id` (unindexed),
    // `content`, `header`.
    let mut sql = match query.index {
        FtsIndex::Words => sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT c.id as chunk_id, c.document_id, c.content, \
             bm25(chunks_fts, 0.0, 1.0, ",
        ),
        FtsIndex::Trigram => sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT c.id as chunk_id, c.document_id, c.content, \
             bm25(chunks_trigram, 0.0, 1.0, ",
        ),
    };
    sql.push_bind(header_weight);
    sql.push(match query.index {
        FtsIndex::Words => {
            ") as score FROM chunks_fts JOIN text_chunks c ON chunks_fts.chunk_id = c.id \
             WHERE chunks_fts MATCH "
        }
        FtsIndex::Trigram => {
            ") as score FROM chunks_trigram \
             JOIN text_chunks c ON chunks_trigram.chunk_id = c.id \
             WHERE chunks_trigram MATCH "
        }
    });
    let order = " ORDER BY score, c.id LIMIT ";
    sql.push_bind(query.match_expression.clone());
    if space_id.is_none() && allowed_document_ids.is_none() {
        // An unscoped search is a vault-wide one, and a file attached to a
        // chat is not part of the vault. Left out here, before `LIMIT`,
        // rather than after, where it would cost the caller a result. A
        // scoped search keeps attachments: chat's allow-list names them.
        sql.push(
            " AND NOT EXISTS (SELECT 1 FROM documents d \
             WHERE d.id = c.document_id AND d.owner_conversation_id IS NOT NULL)",
        );
    }
    if let Some(space) = space_id {
        sql.push(" AND EXISTS (SELECT 1 FROM document_space_memberships m WHERE m.document_id = c.document_id AND m.space_id = ");
        sql.push_bind(space).push(")");
    }
    if let Some(ids) = allowed_document_ids {
        sql.push(" AND c.document_id IN (SELECT value FROM json_each(");
        // One bind for the whole list. A bind per document runs into
        // SQLite's 32,766-variable limit once a space is large enough, and
        // every search in that space then fails.
        let mut sorted: Vec<_> = ids.iter().collect();
        sorted.sort();
        sql.push_bind(serde_json::json!(sorted).to_string());
        sql.push("))");
    }
    sql.push(order).push_bind(top_k as i64);
    sql.build().fetch_all(connection).await
}

fn rows_to_results(rows: Vec<sqlx::sqlite::SqliteRow>) -> Vec<SearchResultRecord> {
    use sqlx::Row;

    let mut raw_results = Vec::with_capacity(rows.len());
    let mut max_score = 0.0_f32;
    for row in rows {
        let chunk_id: String = row.get("chunk_id");
        let document_id: String = row.get("document_id");
        let content: String = row.get("content");
        let score: f64 = row.get("score");
        let raw_score = (-score as f32).max(0.0); // BM25 returns negative scores, invert
        max_score = max_score.max(raw_score);
        raw_results.push((chunk_id, document_id, content, raw_score));
    }

    let normalize = |score: f32| -> f32 {
        if max_score > 0.0 {
            (score / max_score).clamp(0.0, 1.0)
        } else {
            0.0
        }
    };

    raw_results
        .into_iter()
        .map(
            |(chunk_id, document_id, content, raw_score)| SearchResultRecord {
                doc_id: document_id,
                chunk_id,
                content,
                score: normalize(raw_score),
            },
        )
        .collect()
}

pub(super) async fn clear_chunk_indexes(db: &Database) -> Result<()> {
    let mut tx = db.begin_write().await?;
    sqlx::query("DELETE FROM chunks_fts")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM chunks_trigram")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn chunk_index_count(db: &Database) -> Result<usize> {
    use sqlx::Row;
    let mut read = db.begin_read().await?;
    let row = sqlx::query("SELECT COUNT(*) as count FROM chunks_fts")
        .fetch_one(&mut *read)
        .await?;
    read.commit().await?;
    Ok(row.get::<i64, _>("count") as usize)
}
