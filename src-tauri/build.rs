use std::env;
use std::path::PathBuf;

#[path = "build_support/sidecar_guard.rs"]
mod sidecar_guard;

/// Embed the Windows application manifest into every executable this package
/// links: the app, `export_bindings`, and each test harness.
///
/// `rfd`, pulled in by `tauri-plugin-dialog`, statically imports
/// `TaskDialogIndirect`, which only comctl32 version 6 exports, and version 6
/// is reached by declaring a dependency on it in a manifest. Without one the
/// loader binds comctl32 to the 5.82 copy in System32, cannot resolve that
/// import, and kills the process before `main` with
/// STATUS_ENTRYPOINT_NOT_FOUND. The symptom was the whole Windows test run
/// dying in under a second while `cargo check` stayed green, which is why it
/// read as a broken runner rather than a missing manifest.
///
/// It has to be the un-suffixed `rustc-link-arg`. The `-tests` variant maps to
/// Cargo's `LinkArgTarget::Test`, which matches by target *kind*, so it reaches
/// `tests/*.rs` integration targets and never the lib's own unit-test harness
/// -- the exact binary that was failing (rust-lang/cargo#10937). That was the
/// first attempt at this fix and it was silently dropped.
///
/// Reaching every executable means also reaching the bins, which is why
/// `tauri_build` is constructed with `new_without_app_manifest()` below: two
/// `RT_MANIFEST` resources with id 1 make link.exe fail with CVT1100. Its
/// default manifest is content-identical to this file, so nothing is lost.
fn embed_windows_manifest() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return; // /MANIFEST is a link.exe flag
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app.manifest");
    println!("cargo::rerun-if-changed=windows-app.manifest");
    println!("cargo::rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo::rustc-link-arg=/MANIFESTINPUT:{}",
        manifest.display()
    );
}

fn main() {
    // An instrumented binary must never use the shipping identity or its data.
    // Keep the WebDriver endpoint opt-in and confined to test-only bundles.
    if env::var_os("CARGO_FEATURE_DESKTOP_E2E").is_some() {
        let target = env::var("TARGET").unwrap_or_default();
        let patch = env::var("TAURI_CONFIG").ok();
        let config = sidecar_guard::load_tauri_config(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            sidecar_guard::Platform::from_triple(&target),
            patch.as_deref(),
        );
        let isolated = config.ok().is_some_and(|(config, _)| {
            config
                .get("identifier")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|id| id.starts_with("tech.lattice.compatibility."))
        });
        if !isolated {
            println!("cargo::error=desktop-e2e requires a tech.lattice.compatibility.* test identifier; use the desktop test build script");
            return;
        }
    }
    embed_windows_manifest();

    if !guard_llama_sidecar() {
        // The guard logged `cargo::error`s; Cargo fails the build once this
        // script exits, so skip tauri-build and keep the output to the fix.
        return;
    }

    // Register custom plugin commands for ACL (Tauri v2 requirement)
    if let Err(error) = tauri_build::try_build(
        tauri_build::Attributes::new()
            // `embed_windows_manifest` above embeds the manifest for every
            // executable, this one included; leaving tauri-build's copy in the
            // resource file too would be a duplicate RT_MANIFEST id 1.
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
            .plugin(
                "model",
                tauri_build::InlinedPlugin::new().commands(&[
                    "download_model",
                    "check_first_run_status",
                    "download_default_embedding_model",
                    "delete_model",
                    "list_downloaded_models",
                    "is_model_already_downloaded",
                    "set_active_embedding_model",
                    "set_active_chat_model",
                    "set_active_utility_model",
                    "warm_up_active_chat_model",
                    "warm_up_active_utility_model",
                    "get_active_chat_model",
                    "get_active_embedding_model",
                    "get_active_models",
                    "clear_active_chat_model",
                    "clear_active_embedding_model",
                    "clear_active_utility_model",
                    "get_model_info",
                    "detect_system_capabilities",
                    "get_compatible_models",
                    "get_all_recommended_models",
                    "search_model_catalog",
                    "get_model_variants",
                    "refresh_model_catalog",
                    "clear_model_catalog_cache",
                    "get_model_catalog_stats",
                    "get_model_download_path",
                ]),
            )
            .plugin(
                "search",
                tauri_build::InlinedPlugin::new().commands(&[
                    "search_documents",
                    "search_fast",
                    "semantic_search",
                    "hybrid_search",
                    "find_similar",
                    "find_similar_documents",
                    "search_with_recency",
                    "batch_search",
                    "reranker_status",
                    "download_reranker",
                ]),
            )
            .plugin(
                "conversation",
                tauri_build::InlinedPlugin::new().commands(&[
                    "create_conversation",
                    "get_conversation",
                    "list_conversations",
                    "delete_conversation",
                    "get_conversation_messages",
                    "rename_conversation",
                    "chat_with_conversation",
                    "create_conversation_space",
                    "list_conversation_spaces",
                    "create_journal",
                    "list_journals",
                    "list_conversation_space_members",
                    "upsert_conversation_space_member",
                    "remove_conversation_space_member",
                    "update_conversation_space",
                    "archive_conversation_space",
                    "update_journal",
                    "archive_journal",
                    "delete_journal",
                    "move_conversation_to_space",
                    "add_conversation_to_journal",
                    "remove_conversation_from_journal",
                    "set_conversation_saved",
                    "set_conversation_bookmarked",
                    "set_conversation_pinned",
                    "set_conversation_archived",
                    "list_space_documents",
                    "list_conversation_linked_documents",
                    "remove_conversation_linked_document",
                    "add_conversation_web_source",
                    "list_conversation_web_sources",
                    "remove_conversation_web_source",
                    "list_document_space_memberships",
                    "set_document_space_membership",
                    "set_documents_space_membership",
                    "bookmark_conversation_message",
                    "unbookmark_conversation_message",
                    "delete_conversation_message",
                    "list_message_bookmarks",
                    "list_conversations_explorer",
                    "list_journal_conversations",
                    "set_journal_entry_pinned",
                    "list_journal_entry_pins",
                    "synthesize_journal_entries",
                    "truncate_conversation_after",
                    "fork_conversation",
                    "create_conversation_tangent",
                    "list_conversation_tangents",
                    "promote_conversation_tangent",
                    "continue_in_new_conversation",
                    "regenerate_response",
                    "compact_conversation",
                    "get_conversation_memory",
                    "manage_knowledge",
                    "add_documents_to_library",
                ]),
            )
            .plugin(
                "file",
                tauri_build::InlinedPlugin::new().commands(&[
                    "index_file",
                    "index_directory",
                    "get_file_metadata",
                    "read_file_content",
                    "read_file_bytes",
                    "get_file_content",
                    "remove_indexed_file",
                    "list_indexed_files",
                    "list_all_documents",
                    "get_indexing_status",
                    "get_indexing_stats",
                    "get_index_progress",
                    "cancel_indexing",
                    "pause_indexing",
                    "resume_indexing",
                    "clear_indexing_failure",
                    "reindex_file",
                    "delete_document",
                    "rename_document",
                    "open_file",
                    "open_file_by_id",
                    "get_file_path_by_id",
                    "show_in_folder",
                    "get_indexed_folders",
                    "get_indexing_activities",
                    "get_recent_documents",
                    "get_document",
                    "get_corpus_shape",
                    "list_conversations_citing_document",
                    "list_custom_collections",
                    "create_custom_collection",
                    "rename_custom_collection",
                    "move_custom_collection",
                    "delete_custom_collection",
                    "add_documents_to_custom_collection",
                    "remove_documents_from_custom_collection",
                ]),
            )
            .plugin(
                "settings",
                tauri_build::InlinedPlugin::new().commands(&[
                    "set_cloud_api_key",
                    "get_settings",
                    "get_settings_category",
                    "update_settings",
                    "reset_settings",
                    "export_settings",
                    "import_settings",
                    "get_system_theme",
                    "validate_folder_path",
                    "test_ollama_connection",
                    "test_llama_cpp_connection",
                    "test_custom_tool",
                    "add_watch_folder",
                    "remove_watch_folder",
                ]),
            )
            .plugin(
                "health",
                tauri_build::InlinedPlugin::new().commands(&[
                    "health_check",
                    "get_system_stats",
                    "get_version",
                    "initialize_database",
                ]),
            )
            .plugin(
                "cache",
                tauri_build::InlinedPlugin::new().commands(&[
                    "clear_cache",
                    "get_cache_stats",
                    "get_cache_metrics",
                    "clear_search_cache",
                ]),
            )
            .plugin(
                "tags",
                tauri_build::InlinedPlugin::new().commands(&[
                    "get_all_tags_with_counts",
                    "get_document_tags",
                    "apply_tags",
                    "remove_tag_from_document",
                    "generate_tags_for_document",
                ]),
            )
            .plugin(
                "favorites",
                tauri_build::InlinedPlugin::new().commands(&[
                    "add_favorite",
                    "remove_favorite",
                    "get_favorites",
                    "is_favorite",
                ]),
            )
            .plugin(
                "embeddings",
                tauri_build::InlinedPlugin::new().commands(&[
                    "generate_embedding",
                    "generate_embeddings_batch",
                    "get_embedding_model_info",
                ]),
            )
            .plugin(
                "huggingface",
                tauri_build::InlinedPlugin::new().commands(&[
                    "set_huggingface_token",
                    "get_huggingface_token_status",
                    "get_huggingface_token",
                    "delete_huggingface_token",
                ]),
            )
            .plugin(
                "extraction",
                tauri_build::InlinedPlugin::new().commands(&[
                    "parse_wikilinks",
                    "extract_document_title",
                    "resolve_wikilink",
                    "extract_and_resolve_links",
                ]),
            )
            .plugin(
                "mention",
                tauri_build::InlinedPlugin::new().commands(&[
                    "extract_mentions",
                    "search_mentions",
                    "get_mentions_for_document",
                    "get_backlinks_for_mention",
                    "get_mentions_by_type",
                    "create_mention",
                    "delete_mention",
                ]),
            )
            .plugin(
                "web",
                tauri_build::InlinedPlugin::new().commands(&[
                    "ingest_web_url",
                    "fetch_url_preview",
                    "extract_article",
                    "read_web_page",
                ]),
            )
            .plugin(
                "updates",
                tauri_build::InlinedPlugin::new()
                    .commands(&["check_for_updates", "get_version_info"]),
            )
            .plugin(
                "qa",
                tauri_build::InlinedPlugin::new()
                    .commands(&["check_llm_health", "generate_chat_starters"]),
            )
            .plugin(
                "jobs",
                tauri_build::InlinedPlugin::new().commands(&["list_jobs"]),
            )
            .plugin(
                "batch",
                tauri_build::InlinedPlugin::new().commands(&[
                    "batch_import_files",
                    "batch_import_urls",
                    "get_batch_status",
                    "cancel_batch",
                    "get_batch_history",
                    "delete_batch_job",
                    "retry_failed_items",
                ]),
            )
            .plugin(
                "download",
                tauri_build::InlinedPlugin::new().commands(&[
                    "start_model_download",
                    "pause_download",
                    "resume_download",
                    "cancel_download",
                    "retry_download",
                    "remove_download",
                    "clear_completed_downloads",
                    "get_download_status",
                    "list_downloads",
                ]),
            )
            .plugin(
                "functions",
                tauri_build::InlinedPlugin::new().commands(&[
                    "list_available_functions",
                    "execute_function",
                    "get_function_stats",
                ]),
            )
            .plugin(
                "dailynotes",
                tauri_build::InlinedPlugin::new().commands(&[
                    "get_today_note",
                    "quick_capture",
                    "get_daily_notes_range",
                    "get_previous_daily_note",
                    "get_next_daily_note",
                    "update_daily_note_content",
                    "list_workspace_notes",
                    "create_workspace_note",
                    "update_workspace_note",
                    "capture_reference",
                    "delete_workspace_note",
                ]),
            )
            .plugin(
                "backup",
                tauri_build::InlinedPlugin::new().commands(&[
                    "plugin_create_backup",
                    "plugin_restore_backup",
                    "plugin_list_backups",
                    "plugin_export_markdown",
                    "plugin_export_json",
                    "plugin_get_archive_status",
                    "plugin_begin_archive_setup",
                    "plugin_confirm_archive_setup",
                    "plugin_choose_archive_destination",
                    "plugin_set_archive_keep_count",
                    "plugin_set_archive_passphrase",
                    "plugin_rotate_recovery_code",
                    "plugin_disable_archive",
                    "plugin_create_archive_now",
                    "plugin_restore_archive",
                ]),
            )
            .plugin(
                "references",
                tauri_build::InlinedPlugin::new().commands(&[
                    "create_passage_reference",
                    "list_passage_references",
                    "update_passage_reference",
                    "delete_passage_reference",
                ]),
            )
            .plugin(
                "explorer",
                tauri_build::InlinedPlugin::new().commands(&[
                    "explorer_resolve_root",
                    "explorer_list_dir",
                    "explorer_read_file",
                    "explorer_locate_file",
                    "explorer_search",
                    "set_conversation_explorer_root",
                    "explorer_index_open",
                    "explorer_index_close",
                    "explorer_index_status",
                    "explorer_index_rebuild",
                    "explorer_folders_list",
                    "explorer_folder_rename",
                    "explorer_folder_set_pinned",
                    "explorer_folder_set_last_thread",
                    "explorer_folder_set_settings",
                    "explorer_folder_delete_index",
                    "explorer_folder_remove",
                ]),
            )
            .plugin(
                "learning",
                tauri_build::InlinedPlugin::new().commands(&[
                    "get_learning_plan",
                    "get_learning_portability_workspace",
                    "export_learning_pack",
                    "preview_learning_pack_import",
                    "apply_learning_pack_import",
                    "cancel_learning_pack_import_preview",
                    "preview_learning_curriculum_revision",
                    "accept_learning_curriculum_revision",
                    "discard_learning_curriculum_revision",
                    "start_learning_diagnostic",
                    "submit_learning_diagnostic",
                    "skip_learning_diagnostic",
                    "start_learning_generation_job",
                    "cancel_learning_generation_job",
                    "retry_learning_generation_job",
                    "get_learning_generation_job",
                    "list_learning_programs",
                    "get_learning_program",
                    "get_learning_lesson_evidence",
                    "get_learning_outline_evidence",
                    "generate_learning_program",
                    "cancel_learning_outline",
                    "repair_learning_outline",
                    "accept_learning_program",
                    "prepare_learning_lesson",
                    "complete_learning_lesson",
                    "submit_learning_attempt",
                    "delete_learning_program",
                    "get_learning_memory",
                    "ensure_learning_lesson_note",
                    "generate_learning_card_drafts",
                    "save_learning_card_draft",
                    "accept_learning_card_draft",
                    "discard_learning_card_draft",
                    "get_learning_canvas_workspace",
                    "create_learning_canvas",
                    "save_learning_canvas",
                    "create_learning_canvas_snapshot",
                    "restore_learning_canvas_snapshot",
                    "get_learning_source_workspace",
                    "get_learning_source_version",
                    "search_learning_sources",
                    "add_learning_web_source",
                    "add_learning_document_source",
                    "add_learning_text_source",
                    "refresh_learning_source",
                    "adopt_learning_source_version",
                    "update_learning_source_policy",
                    "delete_learning_source",
                    "reimport_learning_source",
                    "create_learning_source_selector",
                    "get_learning_source_selector",
                    "match_learning_source_selector",
                    "search_learning_sources_semantically",
                    "get_learning_recall_workspace",
                    "save_learning_recall_card",
                    "decide_learning_recall_duplicate",
                    "review_learning_recall_card",
                    "get_learning_practice_workspace",
                    "get_learning_practice_session",
                    "start_learning_practice_session",
                    "save_learning_practice_artifact",
                    "change_learning_practice_mode",
                    "open_learning_practice_source",
                    "request_learning_tutor_response",
                    "reveal_learning_practice_solution",
                    "submit_learning_practice_attempt",
                    "accept_learning_practice_proposal",
                    "reject_learning_practice_proposal",
                    "get_learning_assessment_workspace",
                    "create_learning_assessment_blueprint",
                    "start_learning_assessment_form",
                    "get_learning_assessment_form",
                    "save_learning_assessment_response",
                    "interrupt_learning_assessment_form",
                    "submit_learning_assessment_form",
                    "accept_learning_follow_up",
                    "dismiss_learning_follow_up",
                    "get_learning_practical_workspace",
                    "get_learning_practical_draft",
                    "save_learning_practical_draft",
                    "get_learning_runtime_catalog",
                    "prepare_learning_runtime_preset",
                    "save_learning_runtime_profile",
                    "generate_learning_practical_activity",
                    "start_learning_practical_run",
                    "cancel_learning_practical_run",
                    "start_learning_simulation",
                    "send_learning_simulation_turn",
                    "finish_learning_simulation",
                    "list_study_decks",
                    "get_study_deck",
                    "generate_study_deck",
                    "generate_conversation_study_deck",
                    "review_study_card",
                    "update_study_card",
                    "delete_study_deck",
                ]),
            )
            .plugin(
                "compare",
                tauri_build::InlinedPlugin::new().commands(&["compare_documents"]),
            )
            .plugin(
                "vault",
                tauri_build::InlinedPlugin::new().commands(&["rescan_vault"]),
            )
            .plugin(
                "transcription",
                tauri_build::InlinedPlugin::new()
                    .commands(&["transcribe_file", "get_transcription_status"]),
            )
            .plugin(
                "corpus-shape",
                tauri_build::InlinedPlugin::new().commands(&["cluster_vault_run", "list_clusters"]),
            ),
    ) {
        eprintln!("failed to run tauri-build: {}", error);
        std::process::exit(1);
    }
}

/// The tauri CLI exports this to the cargo it drives, for `dev` as well as
/// for `build` (including `build --debug`). Plain `cargo check`/`test`/
/// `clippy` — what CI and editors run, against empty placeholder sidecars —
/// never sets it.
const TAURI_CLI_ENV: &str = "TAURI_CLI_VERBOSITY";

/// Refuses any build whose bundled `llama-server` sidecars are unpinned or
/// different from `scripts/llama-server.lock`, or whose Tauri config drifted
/// from the lock. A *missing* sidecar is fatal whenever an app is actually
/// produced — a release build, or anything the tauri CLI drives — and only a
/// warning for a bare `cargo check`, which compiles source and ships nothing.
///
/// `LATTICE_ALLOW_UNPINNED_SIDECAR=1` lets a developer build against a
/// hand-built sidecar. It works in debug builds only and says so loudly; a
/// release build refuses it outright, so no combination of flags can wrap a
/// bundle around a binary the lock has not vouched for.
///
/// Returns `false` when the build must fail.
fn guard_llama_sidecar() -> bool {
    use sidecar_guard::ALLOW_UNPINNED_ENV;

    println!("cargo::rerun-if-env-changed={ALLOW_UNPINNED_ENV}");
    println!("cargo::rerun-if-env-changed={TAURI_CLI_ENV}");
    println!("cargo::rerun-if-env-changed=TAURI_CONFIG");
    let crate_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let target = env::var("TARGET").unwrap_or_default();
    let config_override = env::var("TAURI_CONFIG").ok();
    let evaluation = sidecar_guard::evaluate(&crate_dir, &target, config_override.as_deref());
    for path in &evaluation.watched {
        println!("cargo::rerun-if-changed={}", path.display());
    }

    let is_release = env::var("PROFILE").as_deref() == Ok("release");
    let allow_unpinned = env::var(ALLOW_UNPINNED_ENV).as_deref() == Ok("1");
    if is_release && allow_unpinned {
        println!(
            "cargo::error={ALLOW_UNPINNED_ENV}=1 is set, but it is a debug-only escape hatch: a \
             release build never ships a sidecar the lock has not vouched for. Unset it, then \
             run: bash src-tauri/scripts/fetch-llama-binaries.sh"
        );
        return false;
    }

    let mut fatal: Vec<&String> = evaluation.problems.iter().collect();
    if is_release || env::var_os(TAURI_CLI_ENV).is_some() {
        fatal.extend(&evaluation.placeholders);
    } else {
        for problem in &evaluation.placeholders {
            println!(
                "cargo::warning=no llama-server sidecar, so this build cannot run a local \
                 model: {problem}"
            );
        }
    }
    if fatal.is_empty() {
        return true;
    }

    if allow_unpinned {
        let banner = "!".repeat(72);
        println!("cargo::warning={banner}");
        println!(
            "cargo::warning=! {ALLOW_UNPINNED_ENV}=1: this build for {target} uses an UNVERIFIED"
        );
        println!(
            "cargo::warning=! llama-server sidecar. It is a debug build; DO NOT DISTRIBUTE IT."
        );
        for problem in fatal {
            println!("cargo::warning=!   - {problem}");
        }
        println!("cargo::warning={banner}");
        return true;
    }

    println!("cargo::error=llama-server sidecar guard: refusing to build for {target}:");
    for problem in fatal {
        println!("cargo::error=  - {problem}");
    }
    if !is_release {
        println!(
            "cargo::error=(local experiments only: {ALLOW_UNPINNED_ENV}=1 downgrades these \
             errors to warnings in a debug build; a release build refuses it)"
        );
    }
    false
}
