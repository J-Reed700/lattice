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
// config plugin lives in features/config/plugin/. Use `crate::features::config::plugin`.
// credentials plugin lives in features/credentials/plugin/. Use `crate::features::credentials::plugin`.
// file plugin lives in features/file/plugin/. Use `crate::features::file::plugin`.
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
// backup plugin lives in features/backup/plugin.rs. Use `crate::features::backup::plugin`.
// batch plugin lives in features/batch/plugin.rs. Use `crate::features::batch::plugin`.
// cache plugin lives in features/cache/plugin.rs. Use `crate::features::cache::plugin`.
// Vertical-slice migration (conversation): plugin lives in features/conversation/plugin.rs.
#[path = "../features/conversation/plugin.rs"]
pub mod conversation_plugin;
// daily_notes plugin lives in features/daily_notes/plugin.rs. Use `crate::features::daily_notes::plugin`.
// download plugin lives in features/download/plugin.rs. Use `crate::features::download::plugin`.
// embeddings plugin lives in features/embedding/plugin.rs. Use `crate::features::embedding::plugin`.
// extraction plugin lives in features/extraction/plugin.rs. Use `crate::features::extraction::plugin`.
// favorites plugin lives in features/favorites/plugin.rs. Use `crate::features::favorites::plugin`.
// function_calling plugin lives in features/function_calling/plugin.rs. Use `crate::features::function_calling::plugin`.
// huggingface plugin lives in features/huggingface/plugin.rs. Use `crate::features::huggingface::plugin`.
// mentions plugin lives in features/mentions/plugin.rs. Use `crate::features::mentions::plugin`.
// qa plugin lives in features/qa/plugin.rs. Use `crate::features::qa::plugin`.
// Vertical-slice migration (settings): plugin lives in features/settings/plugin.rs.
#[path = "../features/settings/plugin.rs"]
pub mod settings_plugin;
// tags plugin lives in features/tags/plugin.rs. Use `crate::features::tags::plugin`.
// updates plugin lives in features/updates/plugin.rs. Use `crate::features::updates::plugin`.

use tauri::plugin::TauriPlugin;

/// Initialize all domain plugins.
pub fn init_plugins() -> Vec<TauriPlugin<tauri::Wry>> {
    vec![
        // Batch 1: Core infrastructure (model, search, file, config, credentials, health)
        model::init(),
        search::init(),
        crate::features::file::plugin::init(),
        crate::features::config::plugin::init(),
        crate::features::credentials::plugin::init(),
        crate::features::health::plugin::init(),
        settings_plugin::init(),
        // Batch 2: Metadata & caching (tags, favorites, cache, mentions)
        crate::features::tags::plugin::init(),
        crate::features::favorites::plugin::init(),
        crate::features::cache::plugin::init(),
        crate::features::mentions::plugin::init(),
        crate::features::function_calling::plugin::init(),
        crate::features::daily_notes::plugin::init(),
        // Batch 3: AI services (embeddings, huggingface, extraction, web)
        crate::features::embedding::plugin::init(),
        crate::features::huggingface::plugin::init(),
        crate::features::extraction::plugin::init(),
        web::init(),
        // Batch 4: Final domains (conversations, batch, backup, updates, QA)
        conversation_plugin::init(),
        crate::features::download::plugin::init(),
        crate::features::batch::plugin::init(),
        crate::features::backup::plugin::init(),
        crate::features::updates::plugin::init(),
        crate::features::qa::plugin::init(),
    ]
}
