//! TypeScript Bindings Generator
//!
//! Standalone binary that generates TypeScript type definitions for registered
//! Specta-compatible plugin commands.
//!
//! # Usage
//!
//! ```bash
//! cargo run --bin export_bindings
//! cargo run --bin export_bindings -- --check
//! ```
//!
//! # Output
//!
//! Creates `src/lib/bindings.ts` with:
//! - TypeScript wrappers and DTOs for the registered plugin commands
//! - Auto-exported DTOs from command signatures
//! - A deterministic generated header

mod bindings_settings;

use specta_typescript::{BigIntExportBehavior, Typescript};
use std::path::{Path, PathBuf};

fn normalize_generated_bindings(path: &Path) -> std::io::Result<()> {
    let generated = std::fs::read_to_string(path)?;
    let generated =
        bindings_settings::normalize_settings_output(&generated).map_err(std::io::Error::other)?;
    let normalized = generated
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(path, format!("{normalized}\n"))
}

/// Where `generated` and `committed` first differ, as a 1-based line number and
/// the two lines; `None` when they are the same text.
///
/// Compared line by line, so the line-ending convention is not part of the
/// comparison. It used to be byte for byte, and on Windows git checks the
/// committed file out with CRLF endings while the generator writes LF — so the
/// check reported up-to-date bindings as stale there, and only there. A missing
/// line is reported against an empty one.
fn first_difference(generated: &str, committed: &str) -> Option<(usize, String, String)> {
    let mut generated_lines = generated.lines();
    let mut committed_lines = committed.lines();
    let mut line_number = 0usize;
    loop {
        line_number += 1;
        match (generated_lines.next(), committed_lines.next()) {
            (None, None) => return None,
            (generated, committed) if generated == committed => {}
            (generated, committed) => {
                return Some((
                    line_number,
                    generated.unwrap_or_default().to_string(),
                    committed.unwrap_or_default().to_string(),
                ));
            }
        }
    }
}

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

    let canonical_output_path = workspace_root.join("src").join("lib").join("bindings.ts");
    let check_only = std::env::args().skip(1).any(|arg| arg == "--check");
    let output_path = if check_only {
        std::env::temp_dir().join(format!("lattice-bindings-{}.ts", uuid::Uuid::new_v4()))
    } else {
        canonical_output_path.clone()
    };

    // Build the tauri_specta builder with all plugin commands.
    // rustfmt::skip: collect_commands! parses each entry as a `path`, and
    // rustfmt would split the turbofish on the generic command across lines.
    #[rustfmt::skip]
    let builder =
        tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
            lattice::features::study::plugin::list_study_decks,
            lattice::features::study::plugin::get_study_deck,
            lattice::features::study::plugin::generate_study_deck,
            lattice::features::study::plugin::generate_conversation_study_deck,
            lattice::features::study::plugin::review_study_card,
            lattice::features::study::plugin::update_study_card,
            lattice::features::study::plugin::delete_study_deck,
            // Model management plugin
            lattice::features::model_management::plugin::commands::download_model,
            lattice::features::model_management::plugin::commands::check_first_run_status,
            lattice::features::model_management::plugin::commands::download_default_embedding_model,
            lattice::features::model_management::plugin::commands::cancel_download,
            lattice::features::model_management::plugin::commands::delete_model,
            lattice::features::model_management::plugin::commands::get_model_download_path::<tauri::Wry>,
            lattice::features::model_management::plugin::commands::list_downloaded_models,
            lattice::features::model_management::plugin::commands::is_model_already_downloaded,
            lattice::features::model_management::plugin::commands::get_download_status,
            lattice::features::model_management::plugin::commands::set_active_embedding_model,
            lattice::features::model_management::plugin::commands::set_active_chat_model,
            lattice::features::model_management::plugin::commands::set_active_inference_model,
            lattice::features::model_management::plugin::commands::get_active_chat_model,
            lattice::features::model_management::plugin::commands::get_active_embedding_model,
            lattice::features::model_management::plugin::commands::get_active_models,
            lattice::features::model_management::plugin::commands::clear_active_chat_model,
            lattice::features::model_management::plugin::commands::clear_active_embedding_model,
            lattice::features::model_management::plugin::commands::set_active_utility_model,
            lattice::features::model_management::plugin::commands::clear_active_utility_model,
            lattice::features::model_management::plugin::commands::warm_up_active_chat_model,
            lattice::features::model_management::plugin::commands::warm_up_active_utility_model,
            lattice::features::model_management::plugin::commands::validate_model_compatibility,
            lattice::features::model_management::plugin::commands::get_model_info,
            lattice::features::model_management::plugin::commands::export_model,
            lattice::features::model_management::plugin::commands::import_model,
            lattice::features::model_management::plugin::commands::refresh_model_cache,
            // Catalog/discovery commands
            lattice::features::model_management::commands::detect_system_capabilities,
            lattice::features::model_management::commands::get_compatible_models,
            lattice::features::model_management::commands::get_all_recommended_models,
            lattice::features::model_management::commands::search_model_catalog,
            lattice::features::model_management::commands::refresh_model_catalog,
            lattice::features::model_management::commands::clear_model_catalog_cache,
            lattice::features::model_management::commands::get_model_catalog_stats,
            // Search plugin
            lattice::features::search::commands::search_documents,
            lattice::features::search::commands::search_fast,
            lattice::features::search::commands::semantic_search,
            lattice::features::search::commands::hybrid_search,
            lattice::features::search::commands::find_similar,
            lattice::features::search::commands::find_similar_documents,
            lattice::features::search::commands::search_with_recency,
            lattice::features::search::commands::batch_search,
            lattice::features::search::plugin::reranker_status,
            lattice::features::search::plugin::download_reranker,
            lattice::features::function_calling::plugin::list_available_functions,
            lattice::features::function_calling::plugin::execute_function,
            lattice::features::function_calling::plugin::get_function_stats,
            lattice::features::qa::plugin::ask_question_wrapper,
            lattice::features::qa::plugin::ask_question_stream_wrapper::<tauri::Wry>,
            lattice::features::qa::plugin::check_llm_health_wrapper,
            lattice::features::qa::plugin::generate_chat_starters_wrapper,
            lattice::features::conversation::plugin::chat_with_conversation,
            lattice::features::conversation::plugin::regenerate_response,
            lattice::features::conversation::plugin::truncate_conversation_after,
            lattice::features::conversation::plugin::fork_conversation,
            lattice::features::conversation::plugin::compact_conversation,
            lattice::features::conversation::plugin::get_conversation_memory,
            lattice::features::settings::plugin::get_system_theme::<tauri::Wry>,
            lattice::features::settings::plugin::set_cloud_api_key,
            // Additional public IPC contracts (use the same signatures as runtime).
            lattice::features::file::commands::open_file,
            lattice::features::file::commands::open_file_by_id,
            lattice::features::file::commands::read_file_content,
            lattice::features::file::commands::read_file_bytes,
            lattice::features::file::commands::show_in_folder,
            lattice::interfaces::commands::document_list::get_document,
            lattice::features::compare::plugin::compare_documents,
            lattice::features::references::plugin::create_passage_reference,
            lattice::features::references::plugin::list_passage_references,
            lattice::features::references::plugin::update_passage_reference,
            lattice::features::references::plugin::delete_passage_reference,
            // File plugin
            lattice::features::file::plugin::commands::index_file,
            lattice::features::file::plugin::commands::index_directory,
            lattice::features::file::commands::get_file_metadata,
            lattice::features::file::plugin::commands::get_file_content,
            lattice::features::file::plugin::commands::update_file_metadata,
            lattice::features::file::plugin::commands::delete_file_index,
            lattice::features::file::plugin::commands::remove_indexed_file,
            lattice::features::file::plugin::commands::list_indexed_files,
            lattice::features::file::plugin::commands::list_all_documents,
            lattice::features::file::plugin::commands::get_indexing_status,
            lattice::features::file::plugin::commands::get_indexing_stats,
            lattice::features::file::plugin::commands::get_index_progress,
            lattice::features::file::plugin::commands::cancel_indexing,
            lattice::features::file::plugin::commands::pause_indexing,
            lattice::features::file::plugin::commands::resume_indexing,
            lattice::features::file::plugin::commands::clear_indexing_failure,
            lattice::features::file::plugin::commands::reindex_file,
            lattice::features::file::plugin::commands::delete_document,
            lattice::features::file::plugin::commands::rename_document,
            lattice::features::file::plugin::commands::validate_file_path,
            lattice::features::file::plugin::commands::get_file_path_by_id,
            lattice::features::file::plugin::commands::get_indexed_folders,
            lattice::features::file::plugin::commands::get_indexing_activities,
            lattice::features::file::plugin::commands::get_recent_documents,
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
            lattice::features::embedding::plugin::initialize_models,
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
            // Conversation plugin
            lattice::features::conversation::plugin::create_conversation,
            lattice::features::conversation::plugin::get_conversation,
            lattice::features::conversation::plugin::list_conversations,
            lattice::features::conversation::plugin::delete_conversation,
            lattice::features::conversation::plugin::get_conversation_messages,
            lattice::features::conversation::plugin::rename_conversation,
            // Chat commands accept a Tauri Window for streaming and are not
            // representable by tauri-specta's standalone collector.
            lattice::features::conversation::plugin::create_conversation_space,
            lattice::features::conversation::plugin::list_conversation_spaces,
            lattice::features::conversation::plugin::create_journal,
            lattice::features::conversation::plugin::list_journals,
            lattice::features::conversation::plugin::update_journal,
            lattice::features::conversation::plugin::archive_journal,
            lattice::features::conversation::plugin::delete_journal,
            lattice::features::conversation::plugin::list_conversation_space_members,
            lattice::features::conversation::plugin::upsert_conversation_space_member,
            lattice::features::conversation::plugin::remove_conversation_space_member,
            lattice::features::conversation::plugin::update_conversation_space,
            lattice::features::conversation::plugin::archive_conversation_space,
            lattice::features::conversation::plugin::move_conversation_to_space,
            lattice::features::conversation::plugin::add_conversation_to_journal,
            lattice::features::conversation::plugin::remove_conversation_from_journal,
            lattice::features::conversation::plugin::list_space_documents,
            lattice::features::conversation::plugin::list_conversation_linked_documents,
            lattice::features::conversation::plugin::remove_conversation_linked_document,
            lattice::features::conversation::plugin::add_conversation_web_source,
            lattice::features::conversation::plugin::list_conversation_web_sources,
            lattice::features::conversation::plugin::remove_conversation_web_source,
            lattice::features::conversation::plugin::list_document_space_memberships,
            lattice::features::conversation::plugin::set_document_space_membership,
            lattice::features::conversation::plugin::set_documents_space_membership,
            lattice::features::conversation::plugin::set_conversation_saved,
            lattice::features::conversation::plugin::set_conversation_bookmarked,
            lattice::features::conversation::plugin::set_conversation_pinned,
            lattice::features::conversation::plugin::set_conversation_archived,
            lattice::features::conversation::plugin::bookmark_conversation_message,
            lattice::features::conversation::plugin::unbookmark_conversation_message,
            lattice::features::conversation::plugin::delete_conversation_message,
            lattice::features::conversation::plugin::list_message_bookmarks,
            lattice::features::conversation::plugin::list_conversations_explorer,
            lattice::features::conversation::plugin::list_journal_conversations,
            lattice::features::conversation::plugin::synthesize_journal_entries,
            // Batch plugin
            lattice::features::batch::plugin::batch_import_files,
            lattice::features::batch::plugin::batch_import_urls,
            lattice::features::batch::plugin::get_batch_status,
            lattice::features::batch::plugin::cancel_batch,
            lattice::features::batch::plugin::get_batch_history,
            lattice::features::batch::plugin::delete_batch_job,
            lattice::features::batch::plugin::retry_failed_items,
            // Backup Plugin (7 commands)
            lattice::features::backup::plugin::plugin_create_backup,
            lattice::features::backup::plugin::plugin_restore_backup,
            lattice::features::backup::plugin::plugin_list_backups,
            lattice::features::backup::plugin::plugin_export_markdown,
            lattice::features::backup::plugin::plugin_export_json,
            lattice::features::backup::plugin::plugin_export_csv,
            lattice::features::backup::plugin::plugin_export_html,
            lattice::features::backup::plugin::plugin_get_archive_status,
            lattice::features::backup::plugin::plugin_begin_archive_setup,
            lattice::features::backup::plugin::plugin_confirm_archive_setup,
            lattice::features::backup::plugin::plugin_choose_archive_destination::<tauri::Wry>,
            lattice::features::backup::plugin::plugin_set_archive_keep_count,
            lattice::features::backup::plugin::plugin_set_archive_passphrase,
            lattice::features::backup::plugin::plugin_rotate_recovery_code,
            lattice::features::backup::plugin::plugin_disable_archive,
            lattice::features::backup::plugin::plugin_create_archive_now,
            lattice::features::backup::plugin::plugin_restore_archive::<tauri::Wry>,
            // Daily notes plugin
            lattice::features::daily_notes::plugin::list_workspace_notes,
            lattice::features::daily_notes::plugin::create_workspace_note,
            lattice::features::daily_notes::plugin::update_workspace_note,
            lattice::features::daily_notes::plugin::delete_workspace_note,
            lattice::features::daily_notes::plugin::get_today_note,
            lattice::features::daily_notes::plugin::quick_capture,
            lattice::features::daily_notes::plugin::get_daily_notes_range,
            lattice::features::daily_notes::plugin::get_previous_daily_note,
            lattice::features::daily_notes::plugin::get_next_daily_note,
            lattice::features::daily_notes::plugin::update_daily_note_content,
            // Download lifecycle plugin
            lattice::features::download::plugin::start_model_download,
            lattice::features::download::plugin::pause_download,
            lattice::features::download::plugin::resume_download,
            lattice::features::download::plugin::download_cancel,
            lattice::features::download::plugin::retry_download,
            lattice::features::download::plugin::remove_download,
            lattice::features::download::plugin::clear_completed_downloads,
            lattice::features::download::plugin::download_get_status,
            lattice::features::download::plugin::list_downloads,
            // Mentions plugin
            lattice::features::mentions::plugin::extract_mentions,
            lattice::features::mentions::plugin::search_mentions,
            lattice::features::mentions::plugin::get_mentions_for_document,
            lattice::features::mentions::plugin::get_backlinks_for_mention,
            lattice::features::mentions::plugin::get_mentions_by_type,
            lattice::features::mentions::plugin::create_mention,
            lattice::features::mentions::plugin::delete_mention,
            // Settings plugin
            lattice::features::settings::plugin::test_ollama_connection,
            lattice::features::settings::plugin::test_llama_cpp_connection,
            lattice::features::settings::plugin::test_custom_tool,
            lattice::features::settings::plugin::get_settings,
            lattice::features::settings::plugin::get_settings_category,
            lattice::features::settings::plugin::update_settings,
            lattice::features::settings::plugin::reset_settings,
            lattice::features::settings::plugin::export_settings,
            lattice::features::settings::plugin::import_settings,
            lattice::features::settings::plugin::validate_folder_path,
            lattice::features::settings::plugin::add_watch_folder,
            lattice::features::settings::plugin::remove_watch_folder,
            // Web and vault plugins
            lattice::features::web::plugin::ingest_web_url,
            lattice::features::web::plugin::fetch_url_preview,
            lattice::features::web::plugin::extract_article,
            lattice::features::web::plugin::read_web_page,
            lattice::features::vault::plugin::rescan_vault,
            // Updates Plugin (2 commands)
            lattice::features::updates::plugin::check_for_updates,
            lattice::features::updates::plugin::get_version_info,
            // Corpus-shape plugin
            lattice::features::file::plugin::commands::get_corpus_shape,
            lattice::features::file::plugin::commands::list_conversations_citing_document,
            lattice::features::corpus_shape::commands::cluster_vault_debug,
            lattice::features::corpus_shape::commands::cluster_vault_run::<tauri::Wry>,
            lattice::features::corpus_shape::commands::list_clusters,
            // Transcription plugin (2 commands)
            lattice::features::transcription::plugin::transcribe_file,
            lattice::features::transcription::plugin::get_transcription_status,
        ])
        .typ::<lattice::features::conversation::chat::ChatResponse>()
        .typ::<lattice::features::conversation::chat::ChatStreamEventDto>()
        .typ::<lattice::features::conversation::chat::RetrievalTraceDto>()
        // Persisted in `metadata.turn` rather than returned by a command, so it
        // reaches no signature on its own and has to be named here.
        .typ::<lattice::features::conversation::chat::TurnRecordDto>()
        .typ::<lattice::features::conversation::chat::ToolPreferences>();

    // Export bindings to file
    let result = builder.export(
        Typescript::default()
            .header(
                "// Auto-generated TypeScript bindings for Lattice plugins\n\
                 // DO NOT EDIT - generated by export_bindings.rs\n\
                 // To regenerate: cargo run --bin export_bindings\n\
                 // @ts-nocheck\n\
                 // tauri-specta currently emits runtime helpers unused by this app; declarations\n\
                 // exported from this file remain type-checked at their use sites.",
            )
            .bigint(BigIntExportBehavior::Number),
        output_path.clone(),
    );

    if result.is_ok() {
        if let Err(error) = normalize_generated_bindings(&output_path) {
            eprintln!("Failed to normalize generated bindings: {error}");
            std::process::exit(1);
        }
    }

    match result {
        Ok(_) if check_only => {
            let generated = std::fs::read_to_string(&output_path);
            let committed = std::fs::read_to_string(&canonical_output_path);
            let _ = std::fs::remove_file(&output_path);
            match (generated, committed) {
                (Ok(generated), Ok(committed)) => match first_difference(&generated, &committed) {
                    None => println!("TypeScript bindings are up to date."),
                    Some((line, generated, committed)) => {
                        eprintln!(
                                "TypeScript bindings are stale. Run `cargo run --bin export_bindings`.\n\
                                 First difference at line {line}:\n  generated: {generated}\n  committed: {committed}"
                            );
                        std::process::exit(1);
                    }
                },
                (Err(error), _) => {
                    eprintln!("Failed to read generated bindings: {error}");
                    std::process::exit(1);
                }
                (_, Err(error)) => {
                    eprintln!("Failed to read committed bindings: {error}");
                    std::process::exit(1);
                }
            }
        }
        Ok(_) => {
            println!("Generated {}", canonical_output_path.display());
        }
        Err(e) => {
            eprintln!("✗ Error generating bindings: {}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::first_difference;

    /// The Windows CI failure: same bindings, checked out with CRLF endings.
    #[test]
    fn a_crlf_checkout_of_the_same_bindings_is_not_stale() {
        let generated = "export type A = string\nexport type B = number\n";
        let committed = "export type A = string\r\nexport type B = number\r\n";
        assert_eq!(first_difference(generated, committed), None);
    }

    #[test]
    fn a_real_difference_is_reported_with_its_line() {
        let generated = "export type A = string\nexport type B = number\n";
        let committed = "export type A = string\nexport type B = boolean\n";
        assert_eq!(
            first_difference(generated, committed),
            Some((
                2,
                "export type B = number".to_string(),
                "export type B = boolean".to_string()
            ))
        );
    }

    /// A command added in Rust but not yet regenerated shows up as extra lines.
    #[test]
    fn bindings_that_only_differ_in_length_are_stale() {
        let generated = "export type A = string\nexport type B = number\n";
        let committed = "export type A = string\n";
        assert_eq!(
            first_difference(generated, committed),
            Some((2, "export type B = number".to_string(), String::new()))
        );
    }
}
