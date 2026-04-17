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
// Vertical-slice migration (credentials): plugin lives in features/credentials/plugin/.
#[path = "../features/credentials/plugin/mod.rs"]
pub mod credentials;
pub mod file;
// Vertical-slice migration (health): plugin lives in features/health/plugin/.
#[path = "../features/health/plugin/mod.rs"]
pub mod health;
pub mod model;
pub mod search;
pub mod web;

// Single-file plugin modules moved to domains/ for filesystem organization
// Vertical-slice migration (backup): plugin lives in features/backup/plugin.rs.
#[path = "../features/backup/plugin.rs"]
pub mod backup_plugin;
// Vertical-slice migration (batch): plugin lives in features/batch/plugin.rs.
#[path = "../features/batch/plugin.rs"]
pub mod batch_plugin;
// Vertical-slice migration (cache): plugin lives in features/cache/plugin.rs.
#[path = "../features/cache/plugin.rs"]
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
// Vertical-slice migration (favorites): plugin lives in features/favorites/plugin.rs.
#[path = "../features/favorites/plugin.rs"]
pub mod favorites_plugin;
#[path = "domains/functions_plugin.rs"]
pub mod functions_plugin;
#[path = "domains/huggingface.rs"]
pub mod huggingface;
// Vertical-slice migration (mentions): plugin lives in features/mentions/plugin.rs.
#[path = "../features/mentions/plugin.rs"]
pub mod mention_plugin;
// Vertical-slice migration (qa): plugin lives in features/qa/plugin.rs.
#[path = "../features/qa/plugin.rs"]
pub mod qa_plugin;
#[path = "domains/settings_plugin.rs"]
pub mod settings_plugin;
// Vertical-slice migration (tags): plugin lives in features/tags/plugin.rs.
#[path = "../features/tags/plugin.rs"]
pub mod tags_plugin;
// Vertical-slice migration (updates): plugin lives in features/updates/plugin.rs.
#[path = "../features/updates/plugin.rs"]
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
