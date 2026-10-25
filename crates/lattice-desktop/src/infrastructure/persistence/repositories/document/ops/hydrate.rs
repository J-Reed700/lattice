//! Reconstruct document aggregates with two child queries per bounded batch.

use futures::TryStreamExt;
use std::collections::HashMap;

use crate::infrastructure::persistence::mappers::{ChunkMapper, ChunkModel};
use lattice_core::domain::entities::Document;
use lattice_core::shared::{error::Result, types::TagId};
use sqlx::SqliteConnection;

pub(super) async fn hydrate_many(
    conn: &mut SqliteConnection,
    documents: Vec<Document>,
) -> Result<Vec<Document>> {
    let mut hydrated = Vec::with_capacity(documents.len());
    // Bound both the parameters and the temporary child maps. The caller's
    // transaction keeps metadata, chunks and tags on the same snapshot.
    let mut documents = documents.into_iter().peekable();
    while documents.peek().is_some() {
        let batch: Vec<_> = documents.by_ref().take(500).collect();
        let ids: Vec<_> = batch.iter().map(|doc| doc.id().as_str()).collect();
        let scope = serde_json::to_string(&ids)?;
        let mut models = sqlx::query_as::<_, ChunkModel>(
            "SELECT id, document_id, content, chunk_index, contextualized_content,
                    context_prefix, start_char, end_char, language, token_count,
                    word_count, has_code, section, page_number, from_ocr
             FROM text_chunks WHERE document_id IN (SELECT value FROM json_each(?))
             ORDER BY document_id, chunk_index",
        )
        .bind(&scope)
        .fetch(&mut *conn);
        let mut chunks: HashMap<_, Vec<_>> = HashMap::new();
        while let Some(model) = models.try_next().await? {
            if let Ok(chunk) = ChunkMapper::to_entity(&model) {
                chunks
                    .entry(chunk.document_id().as_str().to_owned())
                    .or_default()
                    .push(chunk);
            }
        }
        drop(models);
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT document_id, tag_id FROM document_tags
             WHERE document_id IN (SELECT value FROM json_each(?))",
        )
        .bind(&scope)
        .fetch_all(&mut *conn)
        .await?;
        let mut tags: HashMap<_, Vec<_>> = HashMap::new();
        for (document_id, tag_id) in rows {
            if let Ok(tag) = TagId::from_string(tag_id) {
                tags.entry(document_id).or_default().push(tag);
            }
        }
        for document in batch {
            let id = document.id().as_str();
            let children = chunks.remove(id).unwrap_or_default();
            let tag_ids = tags.remove(id).unwrap_or_default();
            hydrated.push(document.from_parts(children, tag_ids)?);
        }
    }
    Ok(hydrated)
}
