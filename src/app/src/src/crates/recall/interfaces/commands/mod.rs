// Single-file command modules grouped under domains/ for filesystem organization.
#[path = "domains/api_boundary.rs"]
pub mod api_boundary;
#[path = "domains/backup.rs"]
pub mod backup;
#[path = "domains/batch_file_import.rs"]
pub mod batch_file_import;
#[path = "domains/batch_history.rs"]
pub mod batch_history;
#[path = "domains/batch_url_import.rs"]
pub mod batch_url_import;
#[path = "domains/cache.rs"]
pub mod cache;
#[path = "domains/config.rs"]
pub mod config;
#[path = "domains/consolidated.rs"]
pub mod consolidated;
#[path = "domains/conversation.rs"]
pub mod conversation;
pub mod conversation_chat;
#[path = "domains/conversation_plugin_impl.rs"]
pub mod conversation_plugin_impl;
#[path = "domains/credentials.rs"]
pub mod credentials;
#[path = "domains/custom_model_commands.rs"]
pub mod custom_model_commands;
#[path = "domains/daily_notes_workspace.rs"]
pub mod daily_notes_workspace;
#[path = "domains/document_list.rs"]
pub mod document_list;
#[path = "domains/downloads.rs"]
pub mod downloads;
#[path = "domains/embeddings.rs"]
pub mod embeddings;
#[path = "domains/extraction.rs"]
pub mod extraction;
#[path = "domains/favorites.rs"]
pub mod favorites;
#[path = "domains/file.rs"]
pub mod file;
#[path = "domains/function_calling_commands.rs"]
pub mod function_calling_commands;
#[path = "domains/health_commands.rs"]
pub mod health_commands;
#[path = "domains/hf_settings.rs"]
pub mod hf_settings;
#[path = "domains/indexing_commands.rs"]
pub mod indexing_commands;
#[path = "domains/initialization.rs"]
pub mod initialization;
#[path = "domains/llm.rs"]
pub mod llm;
#[path = "domains/mentions.rs"]
pub mod mentions;
#[path = "domains/metrics_commands.rs"]
pub mod metrics_commands;
#[path = "domains/model_management.rs"]
pub mod model_management;
#[path = "domains/model_management_commands.rs"]
pub mod model_management_commands;
#[path = "domains/model_setup.rs"]
pub mod model_setup;
#[path = "domains/qa_commands.rs"]
pub mod qa_commands;
#[path = "domains/recent_documents.rs"]
pub mod recent_documents;
#[path = "domains/search_commands.rs"]
pub mod search_commands;
#[path = "domains/tag_commands_full.rs"]
pub mod tag_commands_full;
#[path = "domains/updates_commands.rs"]
pub mod updates_commands;
#[path = "domains/web_ingest.rs"]
pub mod web_ingest;

// Test modules
#[cfg(test)]
#[path = "domains/command_tests.rs"]
mod command_tests;

// Module aliases retained for migration/backward compatibility.
pub use backup as backup_commands;
pub use cache as cache_commands;
pub use conversation as conversation_commands;
pub use credentials as credentials_commands;
pub use favorites as favorites_commands;
pub use file as file_commands;
pub use health_commands as health;
pub use mentions as mentions_commands;
pub use recent_documents as recent_commands;
pub use tag_commands_full as tags;
