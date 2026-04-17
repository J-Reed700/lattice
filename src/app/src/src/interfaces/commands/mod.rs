// Single-file command modules grouped under domains/ for filesystem organization.
#[path = "domains/api_boundary.rs"]
pub mod api_boundary;
#[path = "domains/config.rs"]
pub mod config;
#[path = "domains/consolidated.rs"]
pub mod consolidated;
#[path = "domains/document_list.rs"]
pub mod document_list;
// Vertical-slice migration (model_management): commands live in features/model_management/.
#[path = "../../features/model_management/commands.rs"]
pub mod model_management;
#[path = "../../features/model_management/commands_extra.rs"]
pub mod model_management_commands;
#[path = "domains/model_setup.rs"]
pub mod model_setup;

// Test modules
#[cfg(test)]
#[path = "domains/command_tests.rs"]
mod command_tests;

// Module aliases retained for migration/backward compatibility.
