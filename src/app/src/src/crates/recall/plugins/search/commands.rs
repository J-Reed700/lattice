//! Search Plugin Commands - Direct DTO Exposure (Single Source of Truth)
//!
//! This module has been migrated to use the new Native Result Boundary pattern.
//! The actual commands are now in interfaces/commands/search_commands.rs
//! This file remains as a re-export point for backward compatibility.

// Re-export the search commands from the main location
pub use crate::interfaces::commands::search_commands::{
    find_similar, hybrid_search, search_documents, search_fast, search_with_recency,
    semantic_search,
};

// Re-export cache commands for the search plugin
pub use crate::interfaces::commands::cache::clear_search_cache;

// Aliases for backward compatibility based on export_bindings.rs expectations
pub use find_similar as search_by_tags; // Temporary alias for compatibility
pub use search_fast as keyword_search; // Alias for compatibility
pub use search_with_recency as get_search_suggestions; // Temporary alias for compatibility
