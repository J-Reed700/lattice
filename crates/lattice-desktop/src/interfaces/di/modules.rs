//! Domain Modules - Modular Replacement for ServiceContainer
//!
//! This module replaces the monolithic ServiceContainer (102 fields) with 7 focused
//! domain modules. Each module contains only the use cases and services relevant to
//! its bounded context.
//!
//! **Approved Architecture** (2026-01-21):
//! - Fixes stack overflow caused by God Object anti-pattern
//! - Aligns with DDD Bounded Contexts
//! - Follows Tauri 2.0 best practices (multiple State objects)
//! - Reduces command context from 102 pointers to ~5-15 per module
//!
//! ## Modules:
//! 1. **CoreModule** - Shared infrastructure (DB, security, runtime)
//! 2. **SearchModule** - All search functionality
//! 3. **IndexingModule** - Document ingestion and processing
//! 4. **AIModule** - LLM, conversations
//! 5. **LibraryModule** - Tags, favorites, mentions
//! 6. **FileOpsModule** - File system operations
//! 7. **SystemModule** - Settings, health, backup, cache

mod ai;
mod core;
pub(crate) mod embedding;
mod file_ops;
mod indexing;
mod library;
mod search;
mod system;

pub use self::core::CoreModule;
pub use ai::AIModule;
pub use file_ops::FileOpsModule;
pub use indexing::IndexingModule;
pub use library::LibraryModule;
pub use search::SearchModule;
pub use system::SystemModule;
