//! Search Plugin Commands - Direct DTO Exposure (Single Source of Truth)
//!
//! The actual command implementations live in `interfaces/commands/search_commands.rs`.
//! This module re-exports them for plugin registration and compatibility aliases.

pub use crate::interfaces::commands::search_commands::{
    batch_search, find_similar, hybrid_search, search_documents, search_fast, search_with_recency,
    semantic_search,
};

// Re-export cache command used alongside search operations.
pub use crate::interfaces::commands::cache::clear_search_cache;

// Compatibility aliases for older frontend call sites.
pub use find_similar as search_by_tags;
pub use search_fast as keyword_search;
pub use search_with_recency as get_search_suggestions;
