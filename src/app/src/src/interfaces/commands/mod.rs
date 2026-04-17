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
// Vertical-slice migration (conversation): commands live in features/conversation/.
#[path = "../../features/conversation/commands.rs"]
pub mod conversation;
#[path = "../../features/conversation/chat.rs"]
pub mod conversation_chat;
#[path = "../../features/conversation/plugin_impl.rs"]
pub mod conversation_plugin_impl;
// Vertical-slice migration (credentials): commands live in features/credentials/commands.rs.
#[path = "../../features/credentials/commands.rs"]
pub mod credentials;
// Vertical-slice migration (custom_model): commands live in features/custom_model/commands.rs.
#[path = "../../features/custom_model/commands.rs"]
pub mod custom_model_commands;
#[path = "domains/document_list.rs"]
pub mod document_list;
// Vertical-slice migration (download): commands live in features/download/commands.rs.
#[path = "../../features/download/commands.rs"]
pub mod downloads;
// Vertical-slice migration (embedding): commands live in features/embedding/commands.rs.
#[path = "../../features/embedding/commands.rs"]
pub mod embeddings;
// Vertical-slice migration (extraction): commands live in features/extraction/commands.rs.
#[path = "../../features/extraction/commands.rs"]
pub mod extraction;
// Vertical-slice migration (favorites): commands live in features/favorites/commands.rs.
#[path = "../../features/favorites/commands.rs"]
pub mod favorites;
// Vertical-slice migration (file): commands live in features/file/commands.rs.
#[path = "../../features/file/commands.rs"]
pub mod file;
// Vertical-slice migration (function_calling): commands live in features/function_calling/commands.rs.
#[path = "../../features/function_calling/commands.rs"]
pub mod function_calling_commands;
// Vertical-slice migration (huggingface): HF token commands live in features/huggingface/commands.rs.
#[path = "../../features/huggingface/commands.rs"]
pub mod hf_settings;
// Vertical-slice migration (indexing): commands live in features/indexing/commands.rs.
#[path = "../../features/indexing/commands.rs"]
pub mod indexing_commands;
// Vertical-slice migration (initialization): commands live in features/initialization/commands.rs.
#[path = "../../features/initialization/commands.rs"]
pub mod initialization;
// Vertical-slice migration (llm): commands live in features/llm/commands.rs.
#[path = "../../features/llm/commands.rs"]
pub mod llm;
// Vertical-slice migration (mentions): commands live in features/mentions/commands.rs.
#[path = "../../features/mentions/commands.rs"]
pub mod mentions;
// Vertical-slice migration (model_management): commands live in features/model_management/.
#[path = "../../features/model_management/commands.rs"]
pub mod model_management;
#[path = "../../features/model_management/commands_extra.rs"]
pub mod model_management_commands;
#[path = "domains/model_setup.rs"]
pub mod model_setup;
// Vertical-slice migration (qa): commands live in features/qa/commands.rs.
#[path = "../../features/qa/commands.rs"]
pub mod qa_commands;
// Vertical-slice migration (recent): commands live in features/recent/commands.rs.
#[path = "../../features/recent/commands.rs"]
pub mod recent_documents;
// Vertical-slice migration (search): commands live in features/search/commands.rs.
#[path = "../../features/search/commands.rs"]
pub mod search_commands;
// Vertical-slice migration (tags): commands live in features/tags/commands.rs.
#[path = "../../features/tags/commands.rs"]
pub mod tag_commands_full;
// Vertical-slice migration (web): commands live in features/web/commands.rs.
#[path = "../../features/web/commands.rs"]
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
pub use mentions as mentions_commands;
pub use recent_documents as recent_commands;
pub use tag_commands_full as tags;
