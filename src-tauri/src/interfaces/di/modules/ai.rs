//! The AI module: conversations, models and their downloads, and tagging.

use super::CoreModule;
use sqlx::SqlitePool;
use std::sync::Arc;

// Application Use Cases - Conversation
use crate::features::conversation::use_cases::CreateConversationUseCase;

// Application Use Cases - LLM
use crate::features::llm::use_cases::{
    CheckModelDownloadedUseCase, DeleteModelUseCase, DownloadModelUseCase,
    GetAvailableModelsUseCase, GetBestModelUseCase, GetModelPathUseCase,
    GetRecommendedModelsUseCase, GetSystemCapabilitiesUseCase, ListDownloadedModelsUseCase,
};

// Application Use Cases - Tags
use crate::features::tags::use_cases::{AutoTagAllDocumentsUseCase, GenerateTagsUseCase};

// Application Ports
use crate::application::ports::{CredentialsPort, ModelCatalogPort};

// Service Traits
use crate::application::ports::LoadedChatModelPort;
use crate::features::conversation::ConversationServiceTrait;

/// AI module for LLM operations, Q&A, and conversations
///
/// **Fields**: ~25
/// - 17 use cases (conversation, LLM, AI-powered tags)
/// - 4 services
/// - LLM cache and inference engine
/// - Downloaded model repository
/// - Model catalog and credentials
#[derive(Clone)]
pub struct AIModule {
    conversation: crate::features::conversation::di::ConversationDi,
    llm: crate::features::llm::di::LlmDi,
    ai_tags: crate::features::tags::di::AiTagsDi,

    // Credentials handle echoed from CoreModule for legacy Container API.
    credentials: Arc<dyn CredentialsPort>,
}

impl AIModule {
    /// Build AIModule by composing conversation + llm + ai-tags features.
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
        model_provider: Arc<dyn LoadedChatModelPort>,
        _llm_endpoint: &str,
        _llm_model: &str,
        backend_devices: Arc<crate::features::llm::engine::system::BackendDevices>,
    ) -> crate::shared::error::Result<Self> {
        let conversation = crate::features::conversation::di::build(db_pool.clone());
        let llm = crate::features::llm::di::build(
            db_pool.clone(),
            core.db_conn().clone(),
            core.data_dir().clone(),
            core.credentials().clone(),
            backend_devices,
        )
        .await?;
        let ai_tags = crate::features::tags::di::build_ai(db_pool, model_provider);

        Ok(Self {
            conversation,
            llm,
            ai_tags,
            credentials: core.credentials().clone(),
        })
    }

    // Conversation use case getters
    pub fn create_conversation_use_case(&self) -> &Arc<CreateConversationUseCase> {
        &self.conversation.create_conversation_use_case
    }

    // LLM/model use case getters
    pub fn get_system_capabilities_use_case(&self) -> &Arc<GetSystemCapabilitiesUseCase> {
        &self.llm.get_system_capabilities_use_case
    }

    pub fn get_available_models_use_case(&self) -> &Arc<GetAvailableModelsUseCase> {
        &self.llm.get_available_models_use_case
    }

    pub fn get_recommended_models_use_case(&self) -> &Arc<GetRecommendedModelsUseCase> {
        &self.llm.get_recommended_models_use_case
    }

    pub fn get_best_model_use_case(&self) -> &Arc<GetBestModelUseCase> {
        &self.llm.get_best_model_use_case
    }

    pub fn check_model_downloaded_use_case(&self) -> &Arc<CheckModelDownloadedUseCase> {
        &self.llm.check_model_downloaded_use_case
    }

    pub fn get_model_path_use_case(&self) -> &Arc<GetModelPathUseCase> {
        &self.llm.get_model_path_use_case
    }

    pub fn download_model_use_case(&self) -> &Arc<DownloadModelUseCase> {
        &self.llm.download_model_use_case
    }

    pub fn delete_model_use_case(&self) -> &Arc<DeleteModelUseCase> {
        &self.llm.delete_model_use_case
    }

    pub fn list_models_use_case(&self) -> &Arc<ListDownloadedModelsUseCase> {
        &self.llm.list_models_use_case
    }

    // AI-powered tag use case getters
    pub fn generate_tags_use_case(&self) -> &Arc<GenerateTagsUseCase> {
        &self.ai_tags.generate_tags_use_case
    }

    pub fn auto_tag_all_documents_use_case(&self) -> &Arc<AutoTagAllDocumentsUseCase> {
        &self.ai_tags.auto_tag_all_documents_use_case
    }

    // Service getters
    pub fn conversation_service(&self) -> &Arc<dyn ConversationServiceTrait> {
        &self.conversation.conversation_service
    }

    pub fn document_scope(
        &self,
    ) -> &Arc<dyn crate::application::ports::document_scope::DocumentScopePort> {
        &self.conversation.document_scope
    }

    pub fn conversation_history(
        &self,
    ) -> &Arc<dyn crate::application::ports::ConversationHistoryPort> {
        &self.conversation.conversation_history
    }

    pub fn conversation_context(
        &self,
    ) -> &Arc<dyn crate::application::ports::conversation_context::ConversationContextPort> {
        &self.conversation.conversation_context
    }

    pub fn compaction_slots(&self) -> &Arc<crate::features::conversation::memory::CompactionSlots> {
        &self.conversation.compaction_slots
    }

    pub(crate) fn handoffs_in_flight(
        &self,
    ) -> &Arc<crate::features::conversation::handoff::HandoffsInFlight> {
        &self.conversation.handoffs_in_flight
    }

    pub fn conversation_memory(
        &self,
    ) -> &Arc<dyn crate::application::ports::conversation_memory::ConversationMemoryPort> {
        &self.conversation.conversation_memory
    }

    pub fn downloaded_model_repo(
        &self,
    ) -> &Arc<crate::features::download::downloaded_model_repository::DownloadedModelRepository>
    {
        &self.llm.downloaded_model_repo
    }

    pub fn model_catalog(&self) -> &Arc<dyn ModelCatalogPort> {
        &self.llm.model_catalog
    }

    pub fn model_catalog_cache(
        &self,
    ) -> &Arc<crate::features::model_management::cache_adapter::ModelCacheAdapter> {
        &self.llm.model_catalog_cache
    }

    pub fn download_manager(
        &self,
    ) -> &Arc<dyn crate::features::download::manager::DownloadManager> {
        &self.llm.download_manager
    }

    #[allow(dead_code)]
    pub fn credentials(&self) -> &Arc<dyn CredentialsPort> {
        &self.credentials
    }
}
