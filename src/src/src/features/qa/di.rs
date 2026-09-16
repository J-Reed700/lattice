//! QA feature dependency injection.
//!
//! Builds the `ConversationalQAService` (which orchestrates the shared
//! conversation service with a context manager + QA engine + metrics).
//! The embedding cache and LLM cache are parameters because the real
//! clients are swapped in at model-load time.

use std::sync::Arc;

use crate::features::conversation::ConversationServiceTrait;
use crate::features::qa::conversational_service::ConversationalQAService;
use crate::features::qa::{ConversationalQAServiceTrait, QAEngineTrait};
use crate::infrastructure::llm::noop_client::NoOpLLMClient;
use crate::infrastructure::observability::metrics::Metrics;
use crate::infrastructure::qa::engine::QAEngine;
use crate::infrastructure::services::context_manager::ContextManager;
use crate::infrastructure::services::traits::ContextManagerTrait;

#[derive(Clone)]
pub struct QaDi {
    pub conversational_qa_service: Arc<dyn ConversationalQAServiceTrait>,
}

pub fn build(conversation_service: Arc<dyn ConversationServiceTrait>) -> QaDi {
    // LLM is degraded until a real model loads. Once a real client replaces
    // the cache entry, ConversationalQAService picks it up on the next call.
    let llm_client =
        Arc::new(NoOpLLMClient::new("LLM not loaded yet".into())) as Arc<dyn crate::llm::LLMClient>;

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
