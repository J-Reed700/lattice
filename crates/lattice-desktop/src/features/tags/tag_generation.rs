//! LLM tag suggestions for one document, and auto-tagging the untagged library.
//! Suggesting reads only; applying goes through the shared tag service under its
//! per-document lock, like every other tag and file-metadata write.

use crate::features::tags::generator::{
    DocumentMetadata, TagGenerator, TAG_GENERATION_SYSTEM_PROMPT,
};
use crate::features::tags::repository::TagRepository;
use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
use lattice_core::application::ports::llm_port::InferencePriority;
use lattice_core::application::ports::{LoadedChatModelPort, RepositoryPort, TagServiceTrait};
use lattice_core::application::services::completion_input::{complete_text, TextCall};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use std::sync::Arc;
use tracing::warn;

/// Generates tags with the loaded chat model and applies them through the tag service.
#[derive(Clone)]
pub struct TagGenerationService {
    database: Database,
    tag_service: Arc<dyn TagServiceTrait>,
    model_provider: Arc<dyn LoadedChatModelPort>,
}

impl TagGenerationService {
    pub fn new(
        database: Database,
        tag_service: Arc<dyn TagServiceTrait>,
        model_provider: Arc<dyn LoadedChatModelPort>,
    ) -> Self {
        Self {
            database,
            tag_service,
            model_provider,
        }
    }

    /// Suggest up to `max_tags` tags for one document.
    pub async fn generate_tags(&self, document_id: &str, max_tags: usize) -> Result<Vec<String>> {
        if max_tags == 0 {
            return Err(AppError::InvalidInput("Max tags must be at least 1".into()));
        }

        let document_repo = DocumentRepositoryImpl::new(self.database.clone());
        let document = document_repo
            .find_by_id(document_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Document not found".into()))?;

        let content = document.reading_text();
        if content.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Document has no content available for tag generation".into(),
            ));
        }

        let metadata = DocumentMetadata {
            title: Some(document.file_name().to_string()),
            file_type: document.file_type().map(|t| t.to_string()),
            author: None,
        };

        let user_message = TagGenerator::build_user_message(&content, &metadata, max_tags);
        let llm = self.model_provider.current_model().ok_or_else(|| {
            AppError::AiModelsNotInstalled("Default chat model not installed".to_string())
        })?;
        let response = suggest_tags(llm.as_ref(), &user_message).await?;

        if response.trim().is_empty() {
            return Ok(Vec::new());
        }

        let tags = TagGenerator::parse_tags(&response, max_tags)
            .into_iter()
            .filter(|tag| !tag.is_empty())
            .collect();

        Ok(tags)
    }

    /// Tag up to `max_documents` untagged documents; returns how many were tagged.
    pub async fn auto_tag_all_documents(&self, max_documents: usize) -> Result<usize> {
        const DEFAULT_MAX_TAGS: usize = 5;

        if max_documents == 0 {
            return Err(AppError::InvalidInput(
                "Max documents must be greater than 0".to_string(),
            ));
        }

        let tag_repo = TagRepository::new(self.database.clone());
        let untagged_ids = tag_repo.untagged_document_ids(max_documents).await?;

        let mut tagged_count = 0;
        for document_id in untagged_ids {
            let tags = match self.generate_tags(&document_id, DEFAULT_MAX_TAGS).await {
                Ok(tags) => tags,
                Err(error) => {
                    warn!(
                        "Auto-tagging failed for document {}: {}",
                        document_id, error
                    );
                    continue;
                }
            };

            if tags.is_empty() {
                continue;
            }

            if let Err(error) = self.apply_locked(&document_id, tags).await {
                warn!(
                    "Auto-tagging failed to apply tags for document {}: {}",
                    document_id, error
                );
                continue;
            }

            tagged_count += 1;
        }

        Ok(tagged_count)
    }

    async fn apply_locked(&self, document_id: &str, tags: Vec<String>) -> Result<()> {
        let _lock = self
            .tag_service
            .acquire_lock_with_timeout(document_id)
            .await?;
        self.tag_service.apply_tags(document_id, tags).await?;
        Ok(())
    }
}

/// The model's tag suggestions for one document. Labels are upkeep: they
/// wait behind anyone chatting.
async fn suggest_tags(
    llm: &dyn lattice_core::application::ports::LLMPort,
    user_message: &str,
) -> Result<String> {
    complete_text(
        llm,
        user_message,
        &[format!("System: {TAG_GENERATION_SYSTEM_PROMPT}")],
        TextCall::at(InferencePriority::Maintenance),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lattice_core::application::ports::llm_port::{
        CompletionInput, CompletionRequest, CompletionResponse,
    };
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<CompletionRequest>>);

    #[async_trait]
    impl lattice_core::application::ports::LLMPort for Recorder {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            self.0.lock().await.push(request.clone());
            Ok(CompletionResponse::from_text("rust, ownership"))
        }
        fn model_name(&self) -> &str {
            "recorder"
        }
        fn max_context_tokens(&self) -> usize {
            4096
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::indexing_slicing)]
    async fn tag_suggestions_queue_as_upkeep_with_their_instructions_as_system() {
        let llm = Recorder::default();
        assert_eq!(
            suggest_tags(&llm, "Document: Ownership").await.unwrap(),
            "rust, ownership"
        );
        let sent = llm.0.lock().await;
        assert_eq!(sent[0].priority, InferencePriority::Maintenance);
        assert!(matches!(
            sent[0].input.as_slice(),
            [
                CompletionInput::Message { role: system, content: instructions },
                CompletionInput::Message { role: user, content: message },
            ] if system == "system"
                && instructions == TAG_GENERATION_SYSTEM_PROMPT.trim()
                && user == "user"
                && message == "Document: Ownership"
        ));
    }

    struct NoModel;

    impl LoadedChatModelPort for NoModel {
        fn current_model(&self) -> Option<Arc<dyn lattice_core::application::ports::LLMPort>> {
            None
        }
    }

    async fn service() -> TagGenerationService {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        TagGenerationService::new(
            Database::new(pool),
            Arc::new(crate::features::tags::mocks::MockTagService::new()),
            Arc::new(NoModel),
        )
    }

    #[tokio::test]
    async fn asking_for_no_tags_is_refused() {
        let refused = service().await.generate_tags("doc-1", 0).await;

        assert!(matches!(refused, Err(AppError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn auto_tagging_no_documents_is_refused() {
        let refused = service().await.auto_tag_all_documents(0).await;

        assert!(matches!(refused, Err(AppError::InvalidInput(_))));
    }
}
