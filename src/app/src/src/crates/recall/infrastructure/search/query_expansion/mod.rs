//! Query expansion module for enriching search queries.
//!
//! This module provides query expansion capabilities using:
//! - Domain-specific synonym dictionaries (ML, programming, search terms)
//! - User-defined synonym mappings
//! - Statistical low-signal term filtering
//!
//! # Architecture
//!
//! The module is organized into focused sub-modules:
//! - `config` - Configuration and data models
//! - `expander` - Core expansion logic
//! - `dictionaries` - Synonym sources and term-signal helpers
//!
//! # Example
//!
//! ```rust
//! use crate::infrastructure::search::query_expansion::{QueryExpander, QueryExpansionConfig};
//!
//! let config = QueryExpansionConfig::default();
//! let expander = QueryExpander::new(config)?;
//!
//! let expansion = expander.expand("ml algorithm");
//! // Result includes: "ml", "algorithm", "machine learning", "ai", "method", etc.
//! ```
//!
//! # Design Principles
//!
//! - **Modular**: Each component in its own focused module (<250 lines)
//! - **Configurable**: All behavior controlled via QueryExpansionConfig
//! - **User-First**: User synonyms always take priority over domain dictionary
//! - **Testable**: Comprehensive unit tests for each module

#[path = "modules/config.rs"]
pub mod config;
pub mod dictionaries;
#[path = "modules/expander.rs"]
pub mod expander;

// Re-export public API
pub use config::{QueryExpansion, QueryExpansionConfig};
pub use expander::QueryExpander;
