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
// Vertical-slice migration (config): plugin lives in features/config/plugin/.
#[path = "../features/config/plugin/mod.rs"]
pub mod config;
// Vertical-slice migration (credentials): plugin lives in features/credentials/plugin/.
#[path = "../features/credentials/plugin/mod.rs"]
pub mod credentials;
// Vertical-slice migration (file): plugin lives in features/file/plugin/.
#[path = "../features/file/plugin/mod.rs"]
pub mod file;
// health plugin lives in features/health/plugin/. Use `crate::features::health::plugin`.
// Vertical-slice migration (model_management): plugin lives in features/model_management/plugin/.
#[path = "../features/model_management/plugin/mod.rs"]
pub mod model;
// Vertical-slice migration (search): plugin lives in features/search/plugin/.
#[path = "../features/search/plugin/mod.rs"]
pub mod search;
// Vertical-slice migration (web): plugin lives in features/web/plugin.rs.
#[path = "../features/web/plugin.rs"]
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
// Vertical-slice migration (conversation): plugin lives in features/conversation/plugin.rs.
#[path = "../features/conversation/plugin.rs"]
pub mod conversation_plugin;
// daily_notes plugin lives in features/daily_notes/plugin.rs. Use `crate::features::daily_notes::plugin`.
// Vertical-slice migration (download): plugin lives in features/download/plugin.rs.
#[path = "../features/download/plugin.rs"]
pub mod download_plugin;
// Vertical-slice migration (embedding): plugin lives in features/embedding/plugin.rs.
#[path = "../features/embedding/plugin.rs"]
pub mod embeddings;
// Vertical-slice migration (extraction): plugin lives in features/extraction/plugin.rs.
#[path = "../features/extraction/plugin.rs"]
pub mod extraction;
// Vertical-slice migration (favorites): plugin lives in features/favorites/plugin.rs.
#[path = "../features/favorites/plugin.rs"]
pub mod favorites_plugin;
// Vertical-slice migration (function_calling): plugin lives in features/function_calling/plugin.rs.
#[path = "../features/function_calling/plugin.rs"]
pub mod functions_plugin;
// Vertical-slice migration (huggingface): plugin lives in features/huggingface/plugin.rs.
#[path = "../features/huggingface/plugin.rs"]
pub mod huggingface;
// Vertical-slice migration (mentions): plugin lives in features/mentions/plugin.rs.
#[path = "../features/mentions/plugin.rs"]
pub mod mention_plugin;
// Vertical-slice migration (qa): plugin lives in features/qa/plugin.rs.
#[path = "../features/qa/plugin.rs"]
pub mod qa_plugin;
// Vertical-slice migration (settings): plugin lives in features/settings/plugin.rs.
#[path = "../features/settings/plugin.rs"]
pub mod settings_plugin;
// Vertical-slice migration (tags): plugin lives in features/tags/plugin.rs.
#[path = "../features/tags/plugin.rs"]
pub mod tags_plugin;
// updates plugin lives in features/updates/plugin.rs. Use `crate::features::updates::plugin`.

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
        crate::features::health::plugin::init(),
        settings_plugin::init(),
        // Batch 2: Metadata & caching (tags, favorites, cache, mentions)
        tags_plugin::init(),
        favorites_plugin::init(),
        cache_plugin::init(),
        mention_plugin::init(),
        functions_plugin::init(),
        crate::features::daily_notes::plugin::init(),
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
        crate::features::updates::plugin::init(),
        qa_plugin::init(),
    ]
}
