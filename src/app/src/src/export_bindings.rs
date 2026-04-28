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
            lattice::features::model_management::plugin::commands::download_model,
            lattice::features::model_management::plugin::commands::cancel_download,
            lattice::features::model_management::plugin::commands::delete_model,
            lattice::features::model_management::plugin::commands::list_downloaded_models,
            lattice::features::model_management::plugin::commands::get_download_status,
            lattice::features::model_management::plugin::commands::set_active_embedding_model,
            lattice::features::model_management::plugin::commands::set_active_inference_model,
            lattice::features::model_management::plugin::commands::get_active_models,
            lattice::features::model_management::plugin::commands::validate_model_compatibility,
            lattice::features::model_management::plugin::commands::get_model_info,
            lattice::features::model_management::plugin::commands::export_model,
            lattice::features::model_management::plugin::commands::import_model,
            lattice::features::model_management::plugin::commands::refresh_model_cache,
            // Catalog/Discovery (7 commands)
            lattice::features::model_management::commands::detect_system_capabilities,
            lattice::features::model_management::commands::get_compatible_models,
            lattice::features::model_management::commands::get_all_recommended_models,
            lattice::features::model_management::commands::search_model_catalog,
            lattice::features::model_management::commands::refresh_model_catalog,
            lattice::features::model_management::commands::clear_model_catalog_cache,
            lattice::features::model_management::commands::get_model_catalog_stats,
            // Search Plugin (3 commands - JSON-returning commands excluded)
            // Note: semantic_search, hybrid_search, search_fast return JSON and can't be used with specta
            lattice::features::search::commands::find_similar,
            lattice::features::search::commands::search_with_recency,
            lattice::features::search::commands::batch_search,
            // File Plugin (12 commands)
            lattice::features::file::plugin::commands::index_file,
            lattice::features::file::plugin::commands::index_directory,
            lattice::features::file::commands::get_file_metadata,
            lattice::features::file::plugin::commands::get_file_content,
            lattice::features::file::plugin::commands::update_file_metadata,
            lattice::features::file::plugin::commands::delete_file_index,
            lattice::features::file::plugin::commands::list_indexed_files,
            lattice::features::file::plugin::commands::get_indexing_status,
            lattice::features::file::plugin::commands::pause_indexing,
            lattice::features::file::plugin::commands::resume_indexing,
            lattice::features::file::plugin::commands::reindex_file,
            lattice::features::file::plugin::commands::validate_file_path,
            // Credentials Plugin (7 commands)
            lattice::features::credentials::plugin::commands::credentials_store,
            lattice::features::credentials::plugin::commands::credentials_get,
            lattice::features::credentials::plugin::commands::credentials_delete,
            lattice::features::credentials::plugin::commands::credentials_has,
            lattice::features::credentials::plugin::commands::credentials_clear_all,
            lattice::features::credentials::plugin::commands::credentials_set_endpoint,
            lattice::features::credentials::plugin::commands::credentials_get_endpoint,
            // Health Plugin (4 commands)
            lattice::features::health::plugin::commands::health_check,
            lattice::features::health::plugin::commands::get_system_stats,
            lattice::features::health::plugin::commands::get_version,
            lattice::features::health::plugin::commands::initialize_database,
            // Tags Plugin (5 commands)
            lattice::features::tags::plugin::get_all_tags_with_counts,
            lattice::features::tags::plugin::get_document_tags,
            lattice::features::tags::plugin::apply_tags,
            lattice::features::tags::plugin::remove_tag_from_document,
            lattice::features::tags::plugin::generate_tags_for_document,
            // Favorites Plugin (4 commands)
            lattice::features::favorites::plugin::add_favorite,
            lattice::features::favorites::plugin::remove_favorite,
            lattice::features::favorites::plugin::get_favorites,
            lattice::features::favorites::plugin::is_favorite,
            // Cache Plugin (5 commands)
            lattice::features::cache::commands::clear_cache,
            lattice::features::cache::commands::get_cache_stats,
            lattice::features::cache::commands::get_cache_metrics,
            lattice::features::cache::commands::clear_search_cache,
            lattice::features::cache::commands::cache_operation,
            // Embeddings Plugin (4 commands)
            lattice::features::embedding::plugin::embedding_operation,
            lattice::features::embedding::plugin::generate_embedding,
            lattice::features::embedding::plugin::generate_embeddings_batch,
            lattice::features::embedding::plugin::get_embedding_model_info,
            // HuggingFace Plugin (4 commands)
            lattice::features::huggingface::plugin::set_huggingface_token,
            lattice::features::huggingface::plugin::get_huggingface_token_status,
            lattice::features::huggingface::plugin::get_huggingface_token,
            lattice::features::huggingface::plugin::delete_huggingface_token,
            // Extraction Plugin (4 commands)
            lattice::features::extraction::plugin::parse_wikilinks,
            lattice::features::extraction::plugin::extract_document_title,
            lattice::features::extraction::plugin::resolve_wikilink,
            lattice::features::extraction::plugin::extract_and_resolve_links,
            // Conversation Plugin (6 commands)
            lattice::features::conversation::plugin::create_conversation,
            lattice::features::conversation::plugin::get_conversation,
            lattice::features::conversation::plugin::list_conversations,
            lattice::features::conversation::plugin::delete_conversation,
            lattice::features::conversation::plugin::get_conversation_messages,
            lattice::features::conversation::plugin::rename_conversation,
            // Batch Plugin (5 commands)
            lattice::features::batch::plugin::batch_import_files,
            lattice::features::batch::plugin::batch_import_urls,
            lattice::features::batch::plugin::get_batch_status,
            lattice::features::batch::plugin::cancel_batch,
            lattice::features::batch::plugin::get_batch_history,
            // Backup Plugin (7 commands)
            lattice::features::backup::plugin::plugin_create_backup,
            lattice::features::backup::plugin::plugin_restore_backup,
            lattice::features::backup::plugin::plugin_list_backups,
            lattice::features::backup::plugin::plugin_export_markdown,
            lattice::features::backup::plugin::plugin_export_json,
            lattice::features::backup::plugin::plugin_export_csv,
            lattice::features::backup::plugin::plugin_export_html,
            // Updates Plugin (2 commands)
            lattice::features::updates::plugin::check_for_updates,
            lattice::features::updates::plugin::get_version_info,
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
