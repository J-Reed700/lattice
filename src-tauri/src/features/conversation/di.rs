//! Conversation feature dependency injection.

use std::sync::Arc;

use crate::application::ports::ConversationHistoryPort;
use sqlx::SqlitePool;

use crate::application::services::conversation_memory::CompactionSlots;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::conversation::service::ConversationService;
use crate::features::conversation::use_cases::CreateConversationUseCase;
use crate::features::conversation::ConversationServiceTrait;
use crate::interfaces::di::Container;

#[derive(Clone)]
pub struct ConversationDi {
    pub document_scope: Arc<dyn crate::application::ports::document_scope::DocumentScopePort>,
    pub conversation_service: Arc<dyn ConversationServiceTrait>,
    pub conversation_history: Arc<dyn ConversationHistoryPort>,
    pub conversation_context:
        Arc<dyn crate::application::ports::conversation_context::ConversationContextPort>,
    pub create_conversation_use_case: Arc<CreateConversationUseCase>,
    /// Process-wide single-flight slots for compaction.
    ///
    /// Held here rather than inside a long-lived job because the job also needs
    /// the utility model and the continuation model's token budget, and both of
    /// those change when the user switches models. A job is built per request
    /// around these shared slots, so a model switch cannot leave compaction
    /// pinned to a stale budget while still guaranteeing that one conversation
    /// never compacts twice at once.
    pub compaction_slots: Arc<CompactionSlots>,
    /// The memory ledger port, shared so every caller sees one snapshot read.
    pub conversation_memory:
        Arc<dyn crate::application::ports::conversation_memory::ConversationMemoryPort>,
}

pub fn build(db_pool: SqlitePool) -> ConversationDi {
    let document_scope =
        Arc::new(crate::infrastructure::document_scope::SqliteDocumentScope::new(db_pool.clone()));
    let conversation_context = Arc::new(
        crate::infrastructure::conversation_context::SqliteConversationContext::new(
            db_pool.clone(),
        ),
    );
    let repository = Arc::new(ConversationRepository::new(db_pool));
    let conversation_memory = repository.clone()
        as Arc<dyn crate::application::ports::conversation_memory::ConversationMemoryPort>;
    let conversation_service = Arc::new(ConversationService::new(repository));

    ConversationDi {
        document_scope,
        conversation_context,
        compaction_slots: Arc::new(CompactionSlots::default()),
        conversation_memory,
        conversation_history: conversation_service.clone(),
        create_conversation_use_case: Arc::new(CreateConversationUseCase::new(
            conversation_service.clone(),
        )),
        conversation_service,
    }
}

/// Conversation's registrar surface on `Container`.
impl Container {
    // Conversations (from AIModule)
    pub fn create_conversation_use_case(&self) -> Arc<CreateConversationUseCase> {
        Arc::clone(self.ai.create_conversation_use_case())
    }

    /// Get conversation service (for conversation commands, from AIModule)
    pub fn conversation_service(&self) -> Arc<dyn ConversationServiceTrait> {
        Arc::clone(self.ai.conversation_service())
    }

    /// Read-only history backed by the same service used for conversation writes.
    pub fn conversation_history(&self) -> Arc<dyn ConversationHistoryPort> {
        Arc::clone(self.ai.conversation_history())
    }

    pub fn conversation_context(
        &self,
    ) -> Arc<dyn crate::application::ports::conversation_context::ConversationContextPort> {
        Arc::clone(self.ai.conversation_context())
    }

    pub fn document_scope(
        &self,
    ) -> Arc<dyn crate::application::ports::document_scope::DocumentScopePort> {
        Arc::clone(self.ai.document_scope())
    }

    /// Shared compaction slots. One per process, by construction.
    pub fn compaction_slots(&self) -> Arc<CompactionSlots> {
        Arc::clone(self.ai.compaction_slots())
    }

    /// The conversation-memory ledger port.
    pub fn conversation_memory(
        &self,
    ) -> Arc<dyn crate::application::ports::conversation_memory::ConversationMemoryPort> {
        Arc::clone(self.ai.conversation_memory())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn history_and_writes_share_the_same_service() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        let services = build(pool);
        assert_eq!(
            Arc::as_ptr(&services.conversation_service) as *const (),
            Arc::as_ptr(&services.conversation_history) as *const ()
        );
    }
}
