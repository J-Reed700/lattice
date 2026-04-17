//! # Conversation Use Cases
//!
//! Use cases for conversation management operations.
//!
//! Orchestrates conversation creation, listing, retrieval, renaming, and deletion
//! using the ConversationServiceTrait.

mod create_conversation;
mod delete_conversation;
mod get_conversation;
mod get_conversation_messages;
mod list_conversations;
mod rename_conversation;

pub use create_conversation::CreateConversationUseCase;
pub use delete_conversation::DeleteConversationUseCase;
pub use get_conversation::GetConversationUseCase;
pub use get_conversation_messages::GetConversationMessagesUseCase;
pub use list_conversations::ListConversationsUseCase;
pub use rename_conversation::RenameConversationUseCase;
