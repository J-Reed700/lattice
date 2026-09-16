//! QA feature dependency injection.
//!
//! Builds the `ConversationalQAService` (which orchestrates the shared
//! conversation service with a context manager + QA engine + metrics).
//! The embedding cache and LLM cache are parameters because the real
//! clients are swapped in at model-load time.

use std::sync::Arc;

use crate::application::ports::EmbeddingPort;
use crate::features::conversation::ConversationServiceTrait;
use crate::features::llm::engine::noop_client::NoOpLLMClient;
use crate::features::qa::conversational_service::ConversationalQAService;
use crate::features::qa::engine::QAEngine;
use crate::features::qa::use_cases::AskQuestionUseCase;
use crate::features::qa::{ConversationalQAServiceTrait, QAEngineTrait};
use crate::infrastructure::observability::metrics::Metrics;
use crate::infrastructure::services::context_manager::ContextManager;
use crate::infrastructure::services::traits::ContextManagerTrait;
use crate::interfaces::di::Container;
use crate::shared::error::Result;

#[derive(Clone)]
pub struct QaDi {
    pub conversational_qa_service: Arc<dyn ConversationalQAServiceTrait>,
}

pub fn build(conversation_service: Arc<dyn ConversationServiceTrait>) -> QaDi {
    // LLM is degraded until a real model loads. Once a real client replaces
    // the cache entry, ConversationalQAService picks it up on the next call.
    let llm_client = Arc::new(NoOpLLMClient::new("LLM not loaded yet".into()))
        as Arc<dyn crate::features::llm::engine::LLMClient>;

    let context_manager = Arc::new(ContextManager::new(8192)) as Arc<dyn ContextManagerTrait>;
    let qa_engine = Arc::new(QAEngine::new(llm_client)) as Arc<dyn QAEngineTrait>;

    let conversational_qa_service = Arc::new(ConversationalQAService::new(
        conversation_service,
        context_manager,
        qa_engine,
        Arc::new(Metrics::new()),
    )) as Arc<dyn ConversationalQAServiceTrait>;

    QaDi {
        conversational_qa_service,
    }
}

/// QA's registrar surface on `Container`.
impl Container {
    // Q&A (uses vector_search and document_repo from SearchModule)
    pub async fn ask_question_use_case(&self) -> Result<Arc<AskQuestionUseCase>> {
        let llm = self.get_or_load_llm().await?;

        let hyde_service = Arc::new(crate::features::qa::hyde::HyDEService::new(Arc::clone(
            &llm,
        )));

        let qa_embedding = self.get_or_load_embedding().await.unwrap_or_else(|_| {
            use crate::application::ports::MockEmbeddingPort;
            Arc::new(MockEmbeddingPort::new_degraded()) as Arc<dyn EmbeddingPort>
        });

        Ok(Arc::new(AskQuestionUseCase::new(
            hyde_service,
            qa_embedding,
            Arc::clone(self.search.vector_search()),
            llm,
            Arc::clone(self.search.document_repo())
                as Arc<dyn crate::application::ports::DocumentRepositoryPort>,
            Arc::clone(self.indexing.chunk_repository()),
        )))
    }

    /// Get conversational Q&A service (for conversational Q&A commands, from AIModule)
    pub fn conversational_qa_service(&self) -> Arc<dyn ConversationalQAServiceTrait> {
        Arc::clone(self.ai.conversational_qa_service())
    }
}
