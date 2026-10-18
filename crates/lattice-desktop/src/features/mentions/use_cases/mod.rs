//! Mentions feature — use cases.
//!
//! `update` was removed — no Tauri command or service invokes it.

pub mod create;
pub mod delete;
pub mod extract;
pub mod get_backlinks;
pub mod get_by_type;
pub mod get_for_document;
pub mod search;

pub use create::CreateMentionUseCase;
pub use delete::DeleteMentionUseCase;
pub use extract::ExtractMentionsUseCase;
pub use get_backlinks::GetBacklinksUseCase;
pub use get_by_type::GetMentionsByTypeUseCase;
pub use get_for_document::GetMentionsForDocumentUseCase;
pub use search::SearchMentionsUseCase;
