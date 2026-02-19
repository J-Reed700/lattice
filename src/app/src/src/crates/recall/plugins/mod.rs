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

pub mod config;
pub mod credentials;
pub mod file;
pub mod health;
pub mod model;
pub mod search;

pub mod backup_plugin;
pub mod batch_plugin;
pub mod cache_plugin;
pub mod conversation_plugin;
pub mod daily_notes_plugin;
pub mod download_plugin;
pub mod embeddings;
pub mod extraction;
pub mod favorites_plugin;
pub mod functions_plugin;
pub mod huggingface;
pub mod mention_plugin;
pub mod qa_plugin;
pub mod settings_plugin;
pub mod tags_plugin;
pub mod updates_plugin;
pub mod web;

use tauri::{plugin::TauriPlugin, Runtime};

/// Initialize all domain plugins
///
/// Returns a vector of Tauri plugins ready to be registered in the app builder.
///
/// # Usage
///
/// ```rust,no_run
/// use vault_desktop::plugins::init_plugins;
///
/// tauri::Builder::default()
///     .setup(|app| {
///         for plugin in init_plugins() {
///             app.handle().plugin(plugin)?;
///         }
///         Ok(())
///     })
///     // ...
/// ```
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
        // Batch 4: FINAL domains (conversations, batch, backup, updates, qa)
        conversation_plugin::init(),
        download_plugin::init(),
        batch_plugin::init(),
        backup_plugin::init(),
        updates_plugin::init(),
        qa_plugin::init(),
    ]
}
