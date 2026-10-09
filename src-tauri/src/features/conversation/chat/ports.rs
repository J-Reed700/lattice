//! Dependencies the chat workflows are allowed to use. Composition lives in the feature DI adapter.
use crate::application::ports::llm_port::OptionalLlmLoader;
use crate::application::ports::{EmbeddingPort, LLMPort};
use crate::shared::error::Result;
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait ChatModels: Send + Sync {
    async fn get_or_load_llm(&self) -> Result<Arc<dyn LLMPort>>;
    async fn get_or_load_router_llm(&self) -> Result<Arc<dyn LLMPort>>;
    async fn get_or_load_utility_llm(&self) -> Result<Option<Arc<dyn LLMPort>>>;
    fn utility_llm_loader(&self) -> OptionalLlmLoader;
    async fn get_or_load_embedding(&self) -> Result<Arc<dyn EmbeddingPort>>;
}

#[async_trait]
pub trait ChatStorage: Send + Sync {
    fn db_pool(&self) -> &sqlx::SqlitePool;
    fn conversation_service(
        &self,
    ) -> Arc<dyn crate::features::conversation::ConversationServiceTrait>;
    fn conversation_history(&self) -> Arc<dyn crate::application::ports::ConversationHistoryPort>;
    fn conversation_context(
        &self,
    ) -> Arc<dyn crate::application::ports::conversation_context::ConversationContextPort>;
    fn document_scope(
        &self,
    ) -> Arc<dyn crate::application::ports::document_scope::DocumentScopePort>;
    fn document_repository(&self) -> Arc<dyn crate::application::ports::DocumentRepository>;
    fn chunk_repository(&self) -> Arc<dyn crate::application::ports::ChunkRepositoryPort>;
}

#[async_trait]
pub trait ChatRetrieval: Send + Sync {
    fn function_registry(
        &self,
    ) -> &Arc<dyn crate::features::function_calling::FunctionRegistryTrait>;
    fn function_executor(
        &self,
    ) -> &Arc<dyn crate::features::function_calling::FunctionExecutorTrait>;
    fn hybrid_search_use_case(
        &self,
    ) -> Arc<crate::features::search::use_cases::HybridSearchUseCase>;
    fn web_service(&self) -> Arc<crate::features::web::services::web::WebService>;
    async fn summary_search(
        &self,
    ) -> Option<Arc<crate::features::summaries::search::SummarySearch>>;
}

#[async_trait]
pub trait ChatPolicy: Send + Sync {
    async fn settings(&self) -> Result<crate::features::settings::dto::SettingsDto>;
    async fn validate_message(&self, message: &str) -> Result<String>;
    async fn create_conversation(
        &self,
        request: crate::features::conversation::dto::CreateConversationRequestDto,
    ) -> Result<crate::features::conversation::dto::CreateConversationResponseDto>;
    /// `cancel` is the turn's stop button: the user is waiting on this
    /// compaction, so stopping the turn stops it too.
    async fn compact_for_turn(
        &self,
        id: &str,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<()>;
    fn consolidate_after_turn(&self, id: String);
}

pub trait ChatRuntime: ChatModels + ChatStorage + ChatRetrieval + ChatPolicy {
    fn share(&self) -> Arc<dyn ChatRuntime>;
}
