//! Conversation feature dependency injection.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::features::conversation::ConversationServiceTrait;
use crate::features::conversation::use_cases::CreateConversationUseCase;
use crate::infrastructure::services::ConversationService;

#[derive(Clone)]
pub struct ConversationDi {
    pub conversation_service: Arc<dyn ConversationServiceTrait>,
    pub create_conversation_use_case: Arc<CreateConversationUseCase>,
}

pub fn build(db_pool: SqlitePool) -> ConversationDi {
    let conversation_service =
        Arc::new(ConversationService::new(db_pool)) as Arc<dyn ConversationServiceTrait>;

    ConversationDi {
        create_conversation_use_case: Arc::new(CreateConversationUseCase::new(
            conversation_service.clone(),
        )),
        conversation_service,
    }
}
