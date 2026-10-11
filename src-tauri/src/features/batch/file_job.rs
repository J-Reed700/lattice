//! A file import's items: each file is prepared, then committed with its
//! chunks and embeddings in one transaction, so an import that stops keeps
//! every file it finished.
use super::dto::FileIndexingOptionsDto;
use super::items::BatchItem;
use super::worker::{ItemImporter, ItemResult};
use crate::application::ports::{
    document_scope::DocumentScopePort, EmbeddingPort, UnitOfWorkFactory,
};
use crate::features::indexing::dto::{ChunkingStrategyDto, IndexFileRequestDto};
use crate::features::indexing::use_cases::{
    index_file::PrepareForIndexingOutcome, IndexFileUseCase,
};
use crate::shared::{
    error::{AppError, Result},
    runtime::jobs::JobContext,
};
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::sync::Arc;

pub(super) type EmbeddingLoader =
    Arc<dyn Fn() -> BoxFuture<'static, Result<Arc<dyn EmbeddingPort>>> + Send + Sync>;

/// Where a file import files its documents, saved as the job's request.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FileImportRequest {
    #[serde(default)]
    pub space_id: Option<String>,
    /// Set when the files are a chat's attachments rather than library
    /// documents: they then belong to that conversation.
    #[serde(default)]
    pub owner_conversation_id: Option<String>,
    #[serde(default)]
    pub indexing: Option<FileIndexingOptionsDto>,
}

pub(crate) struct FileImporter {
    pub index_file: Arc<IndexFileUseCase>,
    pub uow_factory: Arc<dyn UnitOfWorkFactory>,
    pub document_scope: Arc<dyn DocumentScopePort>,
    /// Loads the active embedding model; imports wait for it.
    pub load_embedding: EmbeddingLoader,
}

impl FileImporter {
    fn index_request(
        &self,
        request: &FileImportRequest,
        item: &BatchItem,
    ) -> Result<IndexFileRequestDto> {
        let mut metadata = std::collections::HashMap::new();
        if let Some(group) = request
            .indexing
            .as_ref()
            .and_then(|indexing| indexing.source_group.as_ref())
        {
            let context = crate::domain::value_objects::source_context::SourceContext {
                group: group.clone(),
                position: item.position,
            };
            metadata.insert("source_context".into(), serde_json::to_string(&context)?);
        }
        Ok(IndexFileRequestDto {
            path: item.target.clone(),
            chunking_strategy: ChunkingStrategyDto::Semantic { max_tokens: 800 },
            tags: None,
            metadata: Some(metadata),
            space_id: request.space_id.clone(),
        })
    }

    /// Files the committed document where the import asked. `Err` carries the
    /// committed document, so the item's retry finishes the step.
    async fn file_document(
        &self,
        request: &FileImportRequest,
        document_id: String,
        stamp_owner: bool,
    ) -> std::result::Result<String, (AppError, Option<String>)> {
        // A chat's attachment is owned by that chat: out of the library, out
        // of every other conversation's retrieval, and deleted with it.
        if let Some(conversation) = request
            .owner_conversation_id
            .as_deref()
            .filter(|_| stamp_owner)
        {
            if let Err(error) = self
                .document_scope
                .set_conversation_owner(std::slice::from_ref(&document_id), Some(conversation))
                .await
            {
                return Err((error, Some(document_id)));
            }
        }
        if let Some(space) = request.space_id.as_deref() {
            self.document_scope
                .assign_documents(std::slice::from_ref(&document_id), space)
                .await
                .map_err(|error| (error, None))?;
        }
        Ok(document_id)
    }
}

#[async_trait]
impl ItemImporter for FileImporter {
    async fn prepare(&self, _context: &JobContext) -> Result<()> {
        let embedding = (self.load_embedding)().await.map_err(|error| {
            AppError::ServiceNotAvailable(format!(
                "Could not load the active embedding model: {error}"
            ))
        })?;
        if !embedding.is_ready().await? {
            return Err(AppError::ServiceNotAvailable(
                "The embedding model is not ready yet.".into(),
            ));
        }
        Ok(())
    }

    async fn import(&self, context: &JobContext, item: &BatchItem) -> Result<ItemResult> {
        let request: FileImportRequest = serde_json::from_value(context.job().requested.clone())?;
        let outcome = self
            .index_file
            .prepare_for_indexing(self.index_request(&request, item)?)
            .await;
        // Preparation copies the file into the library before any document
        // row exists. A stop gives that copy back now rather than leaving an
        // orphan for the next startup sweep.
        if context.is_cancelled() {
            if let Ok(PrepareForIndexingOutcome::Prepared(prepared)) = outcome {
                self.index_file.discard_prepared(*prepared).await;
            }
            return Ok(ItemResult::Stopped);
        }
        // Whether this item may stamp the document as the conversation's
        // attachment. A file the user already filed in the library stays
        // filed: attaching it to a chat must not quietly pull it out of the
        // library. A document can have one owning chat, so a file already
        // attached to another chat moves to this one — the newest chat to
        // attach it is the one that can see it — and a document this very
        // item committed on an earlier attempt, before stamping it failed,
        // is still this item's to stamp.
        let mut stamp_owner = false;
        let committed = match outcome {
            Ok(PrepareForIndexingOutcome::Duplicate { document_id }) => {
                match request.owner_conversation_id.as_deref() {
                    Some(conversation) => self
                        .document_scope
                        .conversation_owner(&document_id)
                        .await
                        .map(|owner| {
                            stamp_owner = match owner {
                                Some(owner) => owner != conversation,
                                None => item.document_id.as_deref() == Some(document_id.as_str()),
                            };
                            document_id
                        }),
                    None => Ok(document_id),
                }
            }
            Ok(PrepareForIndexingOutcome::Prepared(prepared)) => {
                stamp_owner = true;
                self.index_file
                    .commit_prepared(*prepared, self.uow_factory.as_ref())
                    .await
            }
            Err(error) => Err(error),
        };
        let filed = match committed {
            Ok(document_id) => self.file_document(&request, document_id, stamp_owner).await,
            Err(error) => Err((error, None)),
        };
        Ok(match filed {
            Ok(document_id) => ItemResult::Imported { document_id },
            Err((error, document_id)) => ItemResult::Failed {
                error: error.to_string(),
                document_id,
            },
        })
    }

    fn noun(&self) -> &'static str {
        "file"
    }
}
