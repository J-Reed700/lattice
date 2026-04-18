//! Conversation feature dependency injection.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::features::conversation::ConversationServiceTrait;
use crate::infrastructure::services::ConversationService;
use crate::features::conversation::use_cases::{
    CreateConversationUseCase, DeleteConversationUseCase, GetConversationMessagesUseCase,
    GetConversationUseCase, ListConversationsUseCase, RenameConversationUseCase,
};

#[derive(Clone)]
pub struct ConversationDi {
    pub conversation_service: Arc<dyn ConversationServiceTrait>,
    pub create_conversation_use_case: Arc<CreateConversationUseCase>,
    pub list_conversations_use_case: Arc<ListConversationsUseCase>,
    pub get_conversation_use_case: Arc<GetConversationUseCase>,
    pub get_conversation_messages_use_case: Arc<GetConversationMessagesUseCase>,
    pub rename_conversation_use_case: Arc<RenameConversationUseCase>,
    pub delete_conversation_use_case: Arc<DeleteConversationUseCase>,
}

pub fn build(db_pool: SqlitePool) -> ConversationDi {
    let conversation_service =
        Arc::new(ConversationService::new(db_pool)) as Arc<dyn ConversationServiceTrait>;

    ConversationDi {
        create_conversation_use_case: Arc::new(CreateConversationUseCase::new(
            conversation_service.clone(),
        )),
        list_conversations_use_case: Arc::new(ListConversationsUseCase::new(
            conversation_service.clone(),
        )),
        get_conversation_use_case: Arc::new(GetConversationUseCase::new(
            conversation_service.clone(),
        )),
        get_conversation_messages_use_case: Arc::new(GetConversationMessagesUseCase::new(
            conversation_service.clone(),
        )),
        rename_conversation_use_case: Arc::new(RenameConversationUseCase::new(
            conversation_service.clone(),
        )),
        delete_conversation_use_case: Arc::new(DeleteConversationUseCase::new(
            conversation_service.clone(),
        )),
        conversation_service,
    }
}
