//! # Conversation Use Cases
//!
//! Use cases for conversation management operations.
//!
//! Orchestrates conversation creation, listing, retrieval, renaming, and deletion
//! using the ConversationServiceTrait.

mod create;
mod delete;
mod get;
mod get_messages;
mod list;
mod rename;

pub use create::CreateConversationUseCase;
pub use delete::DeleteConversationUseCase;
pub use get::GetConversationUseCase;
pub use get_messages::GetConversationMessagesUseCase;
pub use list::ListConversationsUseCase;
pub use rename::RenameConversationUseCase;
