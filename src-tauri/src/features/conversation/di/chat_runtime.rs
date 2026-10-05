//! Adapts desktop composition to the explicit chat workflow contracts.
use crate::application::ports::llm_port::OptionalLlmLoader;
use crate::application::ports::{EmbeddingPort, LLMPort};
use crate::features::conversation::chat::ports::*;
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
    fn db_pool(&self) -> &sqlx::SqlitePool {
        Container::db_pool(self)
    }
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
}

#[async_trait]
impl ChatRetrieval for Container {
    fn function_registry(
        &self,
    ) -> &Arc<dyn crate::features::function_calling::FunctionRegistryTrait> {
        Container::function_registry(self)
    }
    fn function_executor(
        &self,
    ) -> &Arc<dyn crate::features::function_calling::FunctionExecutorTrait> {
        Container::function_executor(self)
    }
    fn semantic_search_use_case(
        &self,
    ) -> Arc<crate::features::search::use_cases::SemanticSearchUseCase> {
        Container::semantic_search_use_case(self)
    }
    fn hybrid_search_use_case(
        &self,
    ) -> Arc<crate::features::search::use_cases::HybridSearchUseCase> {
        Container::hybrid_search_use_case(self)
    }
    fn reranker(&self) -> Arc<dyn crate::features::search::engine::reranker::Reranker> {
        Container::reranker(self)
    }
    fn web_service(&self) -> Arc<crate::features::web::services::web::WebService> {
        Container::web_service(self)
    }
    async fn summary_search(
        &self,
    ) -> Option<Arc<crate::features::summaries::search::SummarySearch>> {
        Container::summary_search(self).await
    }
}

#[async_trait]
impl ChatPolicy for Container {
    async fn settings(&self) -> Result<crate::features::settings::dto::SettingsDto> {
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
    async fn compact_for_turn(&self, id: &str) -> Result<()> {
        crate::features::conversation::compaction::compact_for_turn(self, id).await
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
