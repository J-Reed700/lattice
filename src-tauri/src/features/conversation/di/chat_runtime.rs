//! Adapts desktop composition to the explicit chat workflow contracts.
use crate::application::ports::llm_port::OptionalLlmLoader;
use crate::application::ports::{EmbeddingPort, LLMPort};
use crate::features::conversation::chat::ports::*;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::function_calling::{FunctionExecutorTrait, FunctionRegistryTrait};
use crate::interfaces::di::Container;
use crate::shared::error::Result;
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
impl ChatModels for Container {
    async fn get_or_load_llm(&self) -> Result<Arc<dyn LLMPort>> {
        Container::get_or_load_llm(self).await
    }
    async fn get_or_load_router_llm(&self) -> Result<Arc<dyn LLMPort>> {
        Container::get_or_load_router_llm(self).await
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
    fn conversation_history(&self) -> Arc<dyn crate::application::ports::ConversationHistoryPort> {
        Container::conversation_history(self)
    }
    fn conversation_context(
        &self,
    ) -> Arc<dyn crate::application::ports::conversation_context::ConversationContextPort> {
        Container::conversation_context(self)
    }
    fn document_scope(
        &self,
    ) -> Arc<dyn crate::application::ports::document_scope::DocumentScopePort> {
        Container::document_scope(self)
    }
    fn document_repository(&self) -> Arc<dyn crate::application::ports::DocumentRepository> {
        Container::document_repository(self)
    }
    fn chunk_repository(&self) -> Arc<dyn crate::application::ports::ChunkRepositoryPort> {
        Container::chunk_repository(self)
    }
    fn chat_records(&self) -> Arc<dyn ChatRecords> {
        Arc::new(ConversationRepository::new(self.db_pool().clone()))
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
                crate::shared::error::AppError::RateLimitExceeded(error.to_string())
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
    fn consolidate_after_turn(&self, id: String) {
        crate::features::conversation::compaction::consolidate_after_turn(self.clone(), id);
    }
}

impl ChatRuntime for Container {
    fn share(&self) -> Arc<dyn ChatRuntime> {
        Arc::new(self.clone())
    }
}
