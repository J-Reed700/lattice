//! Tauri Plugin Infrastructure (Phase 1: Diamond Standard)
//!
//! This module provides domain-sharded Tauri plugins using tauri-specta v2.
//! Each plugin encapsulates a specific domain (model, search, file) with full type safety.
//!
//! # Architecture
//!
//! - **Model Plugin**: 13 commands for model management (download, delete, set active, etc.)
//! - **Search Plugin**: 6 commands for search operations (semantic, hybrid, keyword, etc.)
//! - **File Plugin**: 12 commands for file operations (index, metadata, content, etc.)
//! - **Config Plugin**: 5 commands for configuration management (get/save config, watch folders)
//! - **Credentials Plugin**: 7 commands for secure credential storage (API keys, endpoints)
//! - **Health Plugin**: 4 commands for system health and diagnostics
//!
//! # Coexistence with Gateway
//!
//! Plugins coexist peacefully with the existing gateway pattern:
//! - Gateway: Single `gateway_command` routing to 16 domain adapters
//! - Plugins: Domain-specific commands with tauri-specta type generation
//!
//! Frontend can use either:
//! - Gateway API: `vaultApi.invoke(domain, command, args)`
//! - Plugin APIs: `modelPlugin.downloadModel(args)` (type-safe)
//!
//! # Oracle Mandate
//!
//! "Plugins must be thin wrappers. ALL business logic stays in domain implementation.
//! Plugins delegate to `interfaces/commands/*_impl` functions without duplication." - Gemini 3 Pro

// Directory-backed plugin modules
pub mod config;
pub mod credentials;
pub mod file;
pub mod health;
pub mod model;
pub mod search;
pub mod web;

// Single-file plugin modules moved to domains/ for filesystem organization
#[path = "domains/backup_plugin.rs"]
pub mod backup_plugin;
#[path = "domains/batch_plugin.rs"]
pub mod batch_plugin;
#[path = "domains/cache_plugin.rs"]
pub mod cache_plugin;
#[path = "domains/conversation_plugin.rs"]
pub mod conversation_plugin;
#[path = "domains/daily_notes_plugin.rs"]
pub mod daily_notes_plugin;
#[path = "domains/download_plugin.rs"]
pub mod download_plugin;
#[path = "domains/embeddings.rs"]
pub mod embeddings;
#[path = "domains/extraction.rs"]
pub mod extraction;
#[path = "domains/favorites_plugin.rs"]
pub mod favorites_plugin;
#[path = "domains/functions_plugin.rs"]
pub mod functions_plugin;
#[path = "domains/huggingface.rs"]
pub mod huggingface;
#[path = "domains/mention_plugin.rs"]
pub mod mention_plugin;
#[path = "domains/qa_plugin.rs"]
pub mod qa_plugin;
#[path = "domains/settings_plugin.rs"]
pub mod settings_plugin;
#[path = "domains/tags_plugin.rs"]
pub mod tags_plugin;
#[path = "domains/updates_plugin.rs"]
pub mod updates_plugin;

use tauri::plugin::TauriPlugin;

/// Initialize all domain plugins.
pub fn init_plugins() -> Vec<TauriPlugin<tauri::Wry>> {
    vec![
        // Batch 1: Core infrastructure (model, search, file, config, credentials, health)
        model::init(),
        search::init(),
        file::init(),
        config::init(),
        credentials::init(),
        health::init(),
        settings_plugin::init(),
        // Batch 2: Metadata & caching (tags, favorites, cache, mentions)
        tags_plugin::init(),
        favorites_plugin::init(),
        cache_plugin::init(),
        mention_plugin::init(),
        functions_plugin::init(),
        daily_notes_plugin::init(),
        // Batch 3: AI services (embeddings, huggingface, extraction, web)
        embeddings::init(),
        huggingface::init(),
        extraction::init(),
        web::init(),
        // Batch 4: Final domains (conversations, batch, backup, updates, QA)
        conversation_plugin::init(),
        download_plugin::init(),
        batch_plugin::init(),
        backup_plugin::init(),
        updates_plugin::init(),
        qa_plugin::init(),
    ]
}
