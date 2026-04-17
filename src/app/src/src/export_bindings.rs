//! TypeScript Bindings Generator
//!
//! Standalone binary that generates TypeScript type definitions for all
//! Diamond Standard plugin commands.
//!
//! # Usage
//!
//! ```bash
//! cargo run --bin export_bindings
//! ```
//!
//! # Output
//!
//! Creates `src/app/websrc/lib/bindings.ts` with:
//! - TypeScript types for all 90 plugin commands (16 plugins)
//! - Auto-exported DTOs from command signatures
//! - Auto-generated header with timestamp

use specta_typescript::{BigIntExportBehavior, Typescript};
use std::path::PathBuf;

fn main() {
    // Determine output path relative to workspace root
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = match manifest_dir.parent() {
        Some(parent) => parent.to_path_buf(),
        None => {
            eprintln!(
                "✗ Error generating bindings: unable to determine workspace root from {}",
                manifest_dir.display()
            );
            std::process::exit(1);
        }
    };

    let output_path = workspace_root
        .join("websrc")
        .join("lib")
        .join("bindings.ts");

    // Build the tauri_specta builder with all plugin commands
    let builder =
        tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
            // Model Plugin (20 commands)
            // Download/Management (13 commands)
            vault::plugins::model::commands::download_model,
            vault::plugins::model::commands::cancel_download,
            vault::plugins::model::commands::delete_model,
            vault::plugins::model::commands::list_downloaded_models,
            vault::plugins::model::commands::get_download_status,
            vault::plugins::model::commands::set_active_embedding_model,
            vault::plugins::model::commands::set_active_inference_model,
            vault::plugins::model::commands::get_active_models,
            vault::plugins::model::commands::validate_model_compatibility,
            vault::plugins::model::commands::get_model_info,
            vault::plugins::model::commands::export_model,
            vault::plugins::model::commands::import_model,
            vault::plugins::model::commands::refresh_model_cache,
            // Catalog/Discovery (7 commands)
            vault::interfaces::commands::model_management::detect_system_capabilities,
            vault::interfaces::commands::model_management::get_compatible_models,
            vault::interfaces::commands::model_management::get_all_recommended_models,
            vault::interfaces::commands::model_management::search_model_catalog,
            vault::interfaces::commands::model_management::refresh_model_catalog,
            vault::interfaces::commands::model_management::clear_model_catalog_cache,
            vault::interfaces::commands::model_management::get_model_catalog_stats,
            // Search Plugin (3 commands - JSON-returning commands excluded)
            // Note: semantic_search, hybrid_search, search_fast return JSON and can't be used with specta
            vault::interfaces::commands::search_commands::find_similar,
            vault::interfaces::commands::search_commands::search_with_recency,
            vault::interfaces::commands::search_commands::batch_search,
            // File Plugin (12 commands)
            vault::plugins::file::commands::index_file,
            vault::plugins::file::commands::index_directory,
            vault::interfaces::commands::file::get_file_metadata,
            vault::plugins::file::commands::get_file_content,
            vault::plugins::file::commands::update_file_metadata,
            vault::plugins::file::commands::delete_file_index,
            vault::plugins::file::commands::list_indexed_files,
            vault::plugins::file::commands::get_indexing_status,
            vault::plugins::file::commands::pause_indexing,
            vault::plugins::file::commands::resume_indexing,
            vault::plugins::file::commands::reindex_file,
            vault::plugins::file::commands::validate_file_path,
            // Config Plugin (5 commands)
            vault::features::config::plugin::commands::get_config,
            vault::features::config::plugin::commands::save_config,
            vault::features::config::plugin::commands::get_watch_folders,
            vault::features::config::plugin::commands::add_watch_folder,
            vault::features::config::plugin::commands::remove_watch_folder,
            // Credentials Plugin (7 commands)
            vault::features::credentials::plugin::commands::credentials_store,
            vault::features::credentials::plugin::commands::credentials_get,
            vault::features::credentials::plugin::commands::credentials_delete,
            vault::features::credentials::plugin::commands::credentials_has,
            vault::features::credentials::plugin::commands::credentials_clear_all,
            vault::features::credentials::plugin::commands::credentials_set_endpoint,
            vault::features::credentials::plugin::commands::credentials_get_endpoint,
            // Health Plugin (4 commands)
            vault::features::health::plugin::commands::health_check,
            vault::features::health::plugin::commands::get_system_stats,
            vault::features::health::plugin::commands::get_version,
            vault::features::health::plugin::commands::initialize_database,
            // Tags Plugin (5 commands)
            vault::features::tags::plugin::get_all_tags_with_counts,
            vault::features::tags::plugin::get_document_tags,
            vault::features::tags::plugin::apply_tags,
            vault::features::tags::plugin::remove_tag_from_document,
            vault::features::tags::plugin::generate_tags_for_document,
            // Favorites Plugin (4 commands)
            vault::features::favorites::plugin::add_favorite,
            vault::features::favorites::plugin::remove_favorite,
            vault::features::favorites::plugin::get_favorites,
            vault::features::favorites::plugin::is_favorite,
            // Cache Plugin (5 commands)
            vault::features::cache::commands::clear_cache,
            vault::features::cache::commands::get_cache_stats,
            vault::features::cache::commands::get_cache_metrics,
            vault::features::cache::commands::clear_search_cache,
            vault::features::cache::commands::cache_operation,
            // Embeddings Plugin (4 commands)
            vault::features::embedding::plugin::embedding_operation,
            vault::features::embedding::plugin::generate_embedding,
            vault::features::embedding::plugin::generate_embeddings_batch,
            vault::features::embedding::plugin::get_embedding_model_info,
            // HuggingFace Plugin (4 commands)
            vault::features::huggingface::plugin::set_huggingface_token,
            vault::features::huggingface::plugin::get_huggingface_token_status,
            vault::features::huggingface::plugin::get_huggingface_token,
            vault::features::huggingface::plugin::delete_huggingface_token,
            // Extraction Plugin (4 commands)
            vault::plugins::extraction::parse_wikilinks,
            vault::plugins::extraction::extract_document_title,
            vault::plugins::extraction::resolve_wikilink,
            vault::plugins::extraction::extract_and_resolve_links,
            // Conversation Plugin (6 commands)
            vault::plugins::conversation_plugin::create_conversation,
            vault::plugins::conversation_plugin::get_conversation,
            vault::plugins::conversation_plugin::list_conversations,
            vault::plugins::conversation_plugin::delete_conversation,
            vault::plugins::conversation_plugin::get_conversation_messages,
            vault::plugins::conversation_plugin::rename_conversation,
            // Batch Plugin (5 commands)
            vault::features::batch::plugin::batch_import_files,
            vault::features::batch::plugin::batch_import_urls,
            vault::features::batch::plugin::get_batch_status,
            vault::features::batch::plugin::cancel_batch,
            vault::features::batch::plugin::get_batch_history,
            // Backup Plugin (7 commands)
            vault::features::backup::plugin::plugin_create_backup,
            vault::features::backup::plugin::plugin_restore_backup,
            vault::features::backup::plugin::plugin_list_backups,
            vault::features::backup::plugin::plugin_export_markdown,
            vault::features::backup::plugin::plugin_export_json,
            vault::features::backup::plugin::plugin_export_csv,
            vault::features::backup::plugin::plugin_export_html,
            // Updates Plugin (2 commands)
            vault::features::updates::plugin::check_for_updates,
            vault::features::updates::plugin::get_version_info,
        ]);

    // Export bindings to file
    let result = builder.export(
        Typescript::default()
            .header(
                "// Auto-generated TypeScript bindings for Diamond Standard Plugins\n\
                 // DO NOT EDIT - This file is auto-generated by export_bindings.rs\n\
                 // To regenerate: cargo run --bin export_bindings",
            )
            .bigint(BigIntExportBehavior::Number),
        output_path.clone(),
    );

    match result {
        Ok(_) => {
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("✗ Error generating bindings: {}", e);
            std::process::exit(1);
        }
    }
}
