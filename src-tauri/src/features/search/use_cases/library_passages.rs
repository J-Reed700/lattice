//! The library's side of [`LibraryPassagesPort`]: a document's chunks as the
//! library stored them, and [`HybridSearchUseCase`] confined to a caller's
//! documents.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use super::HybridSearchUseCase;
use crate::application::ports::{
    ChunkRepositoryPort, DocumentRepository, LibraryChunk, LibraryDocumentText, LibraryPassageHit,
    LibraryPassagesPort,
};
use crate::domain::DocumentStatus;
use crate::features::search::dto::SearchRequestDto;
use crate::shared::error::{AppError, Result};

pub struct LibraryPassages {
    search: Arc<HybridSearchUseCase>,
    documents: Arc<dyn DocumentRepository>,
    chunks: Arc<dyn ChunkRepositoryPort>,
}

impl LibraryPassages {
    pub fn new(
        search: Arc<HybridSearchUseCase>,
        documents: Arc<dyn DocumentRepository>,
        chunks: Arc<dyn ChunkRepositoryPort>,
    ) -> Self {
        Self {
            search,
            documents,
            chunks,
        }
    }
}

#[async_trait]
impl LibraryPassagesPort for LibraryPassages {
    fn model_identity(&self) -> String {
        self.search.model_identity()
    }

    async fn document_text(&self, document_id: &str) -> Result<Option<LibraryDocumentText>> {
        let Some(document) = self.documents.find_by_id(document_id).await? else {
            return Ok(None);
        };
        if document.status() != DocumentStatus::Indexed {
            return Err(AppError::InvalidInput(
                "The document is not fully indexed. Let its import finish before adding it as a reference.".into(),
            ));
        }
        // One read returns one consistent set of chunks, so a re-import
        // racing this call yields the old document or the new, never a mix.
        let chunks = self.chunks.find_by_document(document_id).await?;
        if chunks.is_empty() {
            return Ok(None);
        }
        Ok(Some(LibraryDocumentText {
            title: document.file_name().to_owned(),
            file_path: document.file_path().to_string_lossy().into_owned(),
            chunks: chunks
                .iter()
                .map(|chunk| LibraryChunk {
                    id: chunk.id().as_str().to_owned(),
                    text: chunk.content().to_owned(),
                })
                .collect(),
        }))
    }

    async fn search(
        &self,
        query: &str,
        document_ids: &HashSet<String>,
        limit: usize,
    ) -> Result<Vec<LibraryPassageHit>> {
        if document_ids.is_empty() || query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let response = self
            .search
            .execute_scoped(
                SearchRequestDto::hybrid(query, limit),
                None,
                Some(document_ids),
            )
            .await?;
        Ok(response
            .results
            .into_iter()
            .filter_map(|result| {
                Some(LibraryPassageHit {
                    document_id: result.document_id?,
                    chunk_id: result.id,
                    score: result.score,
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::persistence::database::{initialize_database, DatabaseConnection};
    use crate::infrastructure::persistence::repositories::{
        ChunkRepository, DocumentRepositoryImpl,
    };

    const STAMP: &str = "2026-10-02T00:00:00Z";

    #[tokio::test]
    async fn document_text_returns_every_chunk_in_order_and_refuses_an_unfinished_import(
    ) -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = DatabaseConnection::new(dir.path().join("library.db")).await?;
        initialize_database(db.pool()).await?;
        let pool = db.pool().clone();
        let document_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,indexed_at,created_at,updated_at,checksum,status) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(&document_id).bind(format!("/tmp/{document_id}.txt")).bind("Field notes.txt")
            .bind(22_i64)
            .bind(STAMP).bind(STAMP).bind(STAMP).bind(STAMP)
            .bind("a".repeat(64)).bind("indexed")
            .execute(&pool).await?;
        for (index, content) in [
            (1_i64, "second".to_string()),
            (0, "first".to_string()),
            (2, "x".repeat(2_000_001)),
        ] {
            sqlx::query(
                "INSERT INTO text_chunks(id,document_id,content,chunk_index) VALUES(?,?,?,?)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&document_id)
            .bind(content)
            .bind(index)
            .execute(&pool)
            .await?;
        }
        let library = LibraryPassages::new(
            crate::features::search::mocks::empty_hybrid_search(),
            Arc::new(DocumentRepositoryImpl::new(pool.clone())),
            Arc::new(ChunkRepository::new(pool.clone())),
        );
        let document = library
            .document_text(&document_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("indexed document missing"))?;
        assert_eq!(document.title, "Field notes.txt");
        let texts: Vec<_> = document.chunks.iter().map(|c| c.text.len()).collect();
        assert_eq!(texts, vec![5, 6, 2_000_001]);
        assert_eq!(document.chunks[0].text, "first");
        assert!(library
            .document_text(&uuid::Uuid::new_v4().to_string())
            .await?
            .is_none());
        sqlx::query("UPDATE documents SET status='processing' WHERE id=?")
            .bind(&document_id)
            .execute(&pool)
            .await?;
        assert!(library.document_text(&document_id).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn an_empty_scope_searches_nothing() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = DatabaseConnection::new(dir.path().join("library.db")).await?;
        initialize_database(db.pool()).await?;
        let library = LibraryPassages::new(
            crate::features::search::mocks::empty_hybrid_search(),
            Arc::new(DocumentRepositoryImpl::new(db.pool().clone())),
            Arc::new(ChunkRepository::new(db.pool().clone())),
        );
        assert!(library
            .search("anything", &HashSet::new(), 10)
            .await?
            .is_empty());
        Ok(())
    }
}
