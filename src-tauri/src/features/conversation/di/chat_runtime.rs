//! Adapts desktop composition to the explicit chat workflow contracts.
use crate::features::conversation::chat::ports::*;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::function_calling::{FunctionExecutorTrait, FunctionRegistryTrait};
use crate::interfaces::di::Container;
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::OptionalLlmLoader;
use lattice_core::application::ports::{EmbeddingPort, LLMPort};
use lattice_core::shared::error::Result;
use std::sync::Arc;
use tracing::warn;

#[async_trait]
impl ChatModels for Container {
    async fn get_or_load_llm(&self) -> Result<Arc<dyn LLMPort>> {
        Container::get_or_load_llm(self).await
    }
    async fn get_or_load_router_llm(&self) -> Result<Arc<dyn LLMPort>> {
        Container::get_or_load_router_llm(self).await
    }
    async fn get_or_load_utility_llm(&self) -> Result<Option<Arc<dyn LLMPort>>> {
        Container::get_or_load_utility_llm(self).await
    }
    fn utility_llm_loader(&self) -> OptionalLlmLoader {
        Container::utility_llm_loader(self)
    }
    async fn get_or_load_embedding(&self) -> Result<Arc<dyn EmbeddingPort>> {
        Container::get_or_load_embedding(self).await
    }
}

#[async_trait]
impl ChatStorage for Container {
    fn conversation_service(
        &self,
    ) -> Arc<dyn crate::features::conversation::ConversationServiceTrait> {
        Container::conversation_service(self)
    }
    fn conversation_history(
        &self,
    ) -> Arc<dyn lattice_core::application::ports::ConversationHistoryPort> {
        Container::conversation_history(self)
    }
    fn conversation_context(
        &self,
    ) -> Arc<dyn lattice_core::application::ports::conversation_context::ConversationContextPort>
    {
        Container::conversation_context(self)
    }
    fn document_scope(
        &self,
    ) -> Arc<dyn lattice_core::application::ports::document_scope::DocumentScopePort> {
        Container::document_scope(self)
    }
    fn document_repository(&self) -> Arc<dyn lattice_core::application::ports::DocumentRepository> {
        Container::document_repository(self)
    }
    fn chunk_repository(&self) -> Arc<dyn lattice_core::application::ports::ChunkRepositoryPort> {
        Container::chunk_repository(self)
    }
    fn chat_records(&self) -> Arc<dyn ChatRecords> {
        Arc::new(ConversationRepository::new(self.db_pool().clone()))
    }

    async fn ensure_page_snapshot(&self, conversation_id: &str, url: &str) {
        if let Err(error) = self
            .web_page_snapshots()
            .ensure_queued(conversation_id, url, false)
            .await
        {
            warn!(%error, url, "Could not queue a frozen copy of a cited page");
        }
    }
    fn conversation_memory(
        &self,
    ) -> Arc<dyn lattice_core::application::ports::conversation_memory::ConversationMemoryPort>
    {
        Container::conversation_memory(self)
    }
    fn compaction_slots(&self) -> Arc<crate::features::conversation::memory::CompactionSlots> {
        Container::compaction_slots(self)
    }
    fn handoffs_in_flight(&self) -> Arc<crate::features::conversation::handoff::HandoffsInFlight> {
        Container::handoffs_in_flight(self)
    }
    fn conversation_branches(&self) -> Arc<dyn ConversationBranches> {
        Arc::new(ConversationRepository::new(self.db_pool().clone()))
    }
    fn chat_starter_cache(&self) -> Arc<dyn ChatStarterCache> {
        Arc::new(
            crate::features::conversation::starters::ChatStarterCacheRepository::new(
                self.db_pool().clone(),
            ),
        )
    }
    async fn explorer_turn(
        &self,
        conversation_id: &str,
        focus: Option<&ExplorerFocusDto>,
    ) -> Option<ExplorerTurn> {
        ExplorerTurn::resolve(
            self.db_pool(),
            self.folder_index().map(Arc::as_ref),
            conversation_id,
            focus,
        )
        .await
    }
}

/// The function registry and executor as one tool surface.
struct RegisteredTools {
    registry: Arc<dyn FunctionRegistryTrait>,
    executor: Arc<dyn FunctionExecutorTrait>,
}

#[async_trait]
impl ChatTools for RegisteredTools {
    fn list_tools(&self) -> Vec<ToolDefinition> {
        self.registry.list_tools()
    }
    async fn execute(&self, call: FunctionCall) -> Result<FunctionResult> {
        self.executor.execute(call).await
    }
}

#[async_trait]
impl PageReader for crate::features::web::services::web::WebService {
    async fn read_page(&self, url: &str) -> Result<FetchUrlContentOutput> {
        crate::features::web::services::web::WebService::read_page(self, url)
            .await
            .map(|read| read.output)
    }
}

#[async_trait]
impl ChatRetrieval for Container {
    fn tools(&self) -> Arc<dyn ChatTools> {
        Arc::new(RegisteredTools {
            registry: Arc::clone(Container::function_registry(self)),
            executor: Arc::clone(Container::function_executor(self)),
        })
    }
    fn library_search(&self) -> Arc<dyn crate::features::search::trait_def::LibrarySearchTrait> {
        Container::hybrid_search_use_case(self)
    }
    fn page_reader(&self) -> Arc<dyn PageReader> {
        Container::web_service(self)
    }
    async fn summary_search(
        &self,
    ) -> Option<Arc<dyn crate::features::summaries::search::SummarySearchPort>> {
        Container::summary_search(self)
            .await
            .map(|search| search as Arc<dyn crate::features::summaries::search::SummarySearchPort>)
    }
    fn planner_health(&self) -> Arc<crate::features::conversation::chat::retrieval::PlannerHealth> {
        Arc::clone(self.ai.planner_health())
    }
    fn intent_pace(&self) -> Arc<lattice_core::application::services::prompt_pace::PromptPace> {
        Arc::clone(self.ai.intent_pace())
    }
    fn judge_pace(&self) -> Arc<lattice_core::application::services::prompt_pace::PromptPace> {
        Arc::clone(self.ai.judge_pace())
    }
}

#[async_trait]
impl ChatPolicy for Container {
    async fn settings(&self) -> Result<SettingsDto> {
        self.get_settings_use_case().execute().await
    }
    async fn validate_message(&self, message: &str) -> Result<String> {
        self.security_context()
            .rate_limiters()
            .qa
            .check_rate_limit(message)
            .await
            .map_err(|error| {
                lattice_core::shared::error::AppError::RateLimitExceeded(error.to_string())
            })?;
        self.security_context()
            .input_validator()
            .validate_chat_message(message)
    }
    async fn create_conversation(
        &self,
        request: crate::features::conversation::dto::CreateConversationRequestDto,
    ) -> Result<crate::features::conversation::dto::CreateConversationResponseDto> {
        self.create_conversation_use_case().execute(request).await
    }
    async fn compact_for_turn(
        &self,
        id: &str,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<()> {
        crate::features::conversation::compaction::compact_for_turn(self, id, cancel).await
    }
    fn consolidate_after_turn(
        &self,
        id: String,
        summary_refold: Option<lattice_core::domain::conversation::memory::SummaryRefold>,
    ) {
        crate::features::conversation::compaction::consolidate_after_turn(
            self.share(),
            id,
            summary_refold,
        );
    }
    async fn check_read_rate_limit(&self, name: &str) -> Result<()> {
        self.security_context()
            .rate_limiters()
            .search
            .check_rate_limit(name)
            .await
            .map_err(|e| {
                lattice_core::shared::error::AppError::Other(format!("Rate limit exceeded: {}", e))
            })
    }
    async fn check_delete_rate_limit(&self, name: &str) -> Result<()> {
        self.security_context()
            .rate_limiters()
            .llm_question
            .check_rate_limit(name)
            .await
            .map_err(|e| {
                lattice_core::shared::error::AppError::Other(format!("Rate limit exceeded: {}", e))
            })
    }
    async fn delete_conversation_unmetered(&self, conversation_id: &str) -> Result<()> {
        // A turn already generating for this conversation should not continue
        // producing work after the user deletes it. Vector indexing is a
        // separate detached task; its final write checks the source rows
        // atomically.
        self.turn_cancellation()
            .request_cancel(conversation_id, None);

        // A file attached to a chat exists only for that chat: deleting through
        // the document use case is what also drops its vectors and releases
        // its library blob. A failure is logged and does not block the delete
        // the user asked for: the startup sweep collects whatever is left.
        delete_conversation_attachments(self, conversation_id).await;

        self.conversation_service()
            .delete_conversation(conversation_id)
            .await?;
        // Its snapshot rows went with it (cascade); their files go now.
        self.web_page_snapshots()
            .sweep_in_background("conversation deleted");

        Ok(())
    }
}

/// Delete every document this conversation owns.
///
/// Never fails the caller: a document that resists deletion is worth a log
/// line, not a conversation the user cannot remove.
async fn delete_conversation_attachments(container: &Container, conversation_id: &str) {
    let scope = container.document_scope();
    let attachments = match scope.documents_owned_by_conversation(conversation_id).await {
        Ok(ids) => ids,
        Err(error) => {
            warn!(%error, conversation_id, "Could not list this conversation's attachments");
            return;
        }
    };
    if attachments.is_empty() {
        return;
    }

    let delete_document = container.delete_document_use_case();
    let mut deleted = 0usize;
    for document_id in &attachments {
        match delete_document.execute(document_id.clone()).await {
            Ok(_) => deleted += 1,
            Err(error) => {
                warn!(%error, conversation_id, document_id, "Could not delete an attachment")
            }
        }
    }
    tracing::info!(
        conversation_id,
        owned = attachments.len(),
        deleted,
        "Deleted the conversation's attachments"
    );
}

impl ChatJobs for Container {
    fn jobs(&self) -> Arc<lattice_core::shared::runtime::jobs::JobRuntime> {
        Arc::clone(Container::jobs(self))
    }
    fn research_waiters(&self) -> Arc<crate::features::conversation::chat::ResearchWaiters> {
        Arc::clone(&self.research_waiters)
    }
    fn background(&self) -> Arc<lattice_core::shared::runtime::background::BackgroundTasks> {
        Arc::clone(self.core.background())
    }
}

impl ChatCancellation for Container {
    fn turn_cancellation(
        &self,
    ) -> Arc<crate::features::conversation::chat::cancellation::TurnCancellationRegistry> {
        Arc::clone(&self.turn_cancellation)
    }
}

impl ChatRuntime for Container {
    fn share(&self) -> Arc<dyn ChatRuntime> {
        Arc::new(self.clone())
    }
}
