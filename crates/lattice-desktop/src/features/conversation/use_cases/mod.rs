//! # Conversation Use Cases
//!
//! Currently only `CreateConversationUseCase` is exposed — used by `chat.rs`
//! when a chat turn needs to lazily create a conversation. The other CRUD
//! use cases were removed because the Tauri plugin commands route directly
//! through `features/conversation/commands.rs` and never invoked them.

mod create;

pub use create::CreateConversationUseCase;
