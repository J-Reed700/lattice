// Single-file command modules grouped under domains/ for filesystem organization.
pub mod cloud_models;
pub mod consolidated;
pub mod document_list;
pub mod indexing;
pub mod model_setup;

#[cfg(test)]
mod command_tests;

// Module aliases retained for migration/backward compatibility.
