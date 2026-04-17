// Single-file command modules grouped under domains/ for filesystem organization.
#[path = "domains/api_boundary.rs"]
pub mod api_boundary;
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
#[path = "domains/document_list.rs"]
pub mod document_list;
// Vertical-slice migration (indexing): commands live in features/indexing/commands.rs.
#[path = "../../features/indexing/commands.rs"]
pub mod indexing_commands;
// Vertical-slice migration (llm): commands live in features/llm/commands.rs.
#[path = "../../features/llm/commands.rs"]
pub mod llm;
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
// Vertical-slice migration (search): commands live in features/search/commands.rs.
#[path = "../../features/search/commands.rs"]
pub mod search_commands;
// Vertical-slice migration (web): commands live in features/web/commands.rs.
#[path = "../../features/web/commands.rs"]
pub mod web_ingest;

// Test modules
#[cfg(test)]
#[path = "domains/command_tests.rs"]
mod command_tests;

// Module aliases retained for migration/backward compatibility.
pub use conversation as conversation_commands;
