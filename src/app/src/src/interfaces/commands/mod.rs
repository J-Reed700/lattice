// Single-file command modules grouped under domains/ for filesystem organization.
#[path = "domains/api_boundary.rs"]
pub mod api_boundary;
// Vertical-slice migration (backup): commands live in features/backup/commands.rs.
#[path = "../../features/backup/commands.rs"]
pub mod backup;
// Vertical-slice migration (batch): commands live in features/batch/commands/.
#[path = "../../features/batch/commands/file_import.rs"]
pub mod batch_file_import;
#[path = "../../features/batch/commands/history.rs"]
pub mod batch_history;
#[path = "../../features/batch/commands/url_import.rs"]
pub mod batch_url_import;
// Vertical-slice migration (cache): commands live in features/cache/commands.rs.
#[path = "../../features/cache/commands.rs"]
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
// Vertical-slice migration (credentials): commands live in features/credentials/commands.rs.
#[path = "../../features/credentials/commands.rs"]
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
// Vertical-slice migration (favorites): commands live in features/favorites/commands.rs.
#[path = "../../features/favorites/commands.rs"]
pub mod favorites;
#[path = "domains/file.rs"]
pub mod file;
#[path = "domains/function_calling_commands.rs"]
pub mod function_calling_commands;
// Vertical-slice migration (health): commands live in features/health/commands.rs.
#[path = "../../features/health/commands.rs"]
pub mod health_commands;
#[path = "domains/hf_settings.rs"]
pub mod hf_settings;
#[path = "domains/indexing_commands.rs"]
pub mod indexing_commands;
// Vertical-slice migration (initialization): commands live in features/initialization/commands.rs.
#[path = "../../features/initialization/commands.rs"]
pub mod initialization;
#[path = "domains/llm.rs"]
pub mod llm;
// Vertical-slice migration (mentions): commands live in features/mentions/commands.rs.
#[path = "../../features/mentions/commands.rs"]
pub mod mentions;
// Vertical-slice migration (metrics): commands live in features/metrics/commands.rs.
#[path = "../../features/metrics/commands.rs"]
pub mod metrics_commands;
#[path = "domains/model_management.rs"]
pub mod model_management;
#[path = "domains/model_management_commands.rs"]
pub mod model_management_commands;
#[path = "domains/model_setup.rs"]
pub mod model_setup;
// Vertical-slice migration (qa): commands live in features/qa/commands.rs.
#[path = "../../features/qa/commands.rs"]
pub mod qa_commands;
// Vertical-slice migration (recent): commands live in features/recent/commands.rs.
#[path = "../../features/recent/commands.rs"]
pub mod recent_documents;
#[path = "domains/search_commands.rs"]
pub mod search_commands;
// Vertical-slice migration (tags): commands live in features/tags/commands.rs.
#[path = "../../features/tags/commands.rs"]
pub mod tag_commands_full;
// Vertical-slice migration (updates): command impls live in features/updates/commands.rs.
#[path = "../../features/updates/commands.rs"]
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
