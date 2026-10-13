//! Tests for the settings repository.
use super::*;
use crate::features::settings::dto::{VectorIndexCompressionModeDto, MAX_VECTOR_COMPRESSION_DIMS};
use std::sync::Arc;
use tempfile::TempDir;

async fn create_test_repository() -> (SettingsRepository, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let repo = SettingsRepository::new(temp_dir.path().to_path_buf())
        .await
        .unwrap();
    (repo, temp_dir)
}

#[tokio::test]
async fn ollama_connection_registration_survives_provider_changes_and_reopening() {
    let (repo, dir) = create_test_repository().await;
    assert!(!repo.get_all().await.unwrap().llm.ollama_configured);

    let cloud = repo
        .update(
            Some(SettingsCategory::Llm),
            HashMap::from([
                ("provider".into(), serde_json::json!("openai")),
                ("model".into(), serde_json::json!("cloud-model")),
            ]),
        )
        .await
        .unwrap();
    assert!(!cloud.llm.ollama_configured);

    for provider in ["ollama", "llamacpp", "local", "auto"] {
        let settings = repo
            .update(
                Some(SettingsCategory::Llm),
                HashMap::from([("provider".into(), serde_json::json!(provider))]),
            )
            .await
            .unwrap();
        assert!(settings.llm.ollama_configured);
    }
    let reopened = SettingsRepository::new(dir.path().to_path_buf())
        .await
        .unwrap();
    assert!(reopened.get_all().await.unwrap().llm.ollama_configured);
}

#[tokio::test]
async fn saving_ollama_defaults_registers_a_connection_without_selecting_it() {
    let (repo, _dir) = create_test_repository().await;
    let settings = repo
        .update(
            Some(SettingsCategory::Llm),
            HashMap::from([(
                "ollamaUrl".into(),
                serde_json::json!("http://localhost:11434"),
            )]),
        )
        .await
        .unwrap();
    assert!(settings.llm.ollama_configured);
    assert_eq!(settings.llm.provider, LLMProvider::Auto);
}

/// Concurrent updates to *different* categories must all survive. Before
/// the write lock, each caller read the same base state and the last
/// writer reverted every earlier one, while the UI reported success for
/// all of them.
#[tokio::test]
async fn concurrent_updates_to_distinct_categories_do_not_lose_writes() {
    let temp_dir = TempDir::new().unwrap();
    let repo = Arc::new(
        SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .unwrap(),
    );

    let mut handles = Vec::new();

    {
        let repo = Arc::clone(&repo);
        handles.push(tokio::spawn(async move {
            let mut updates = HashMap::new();
            updates.insert("maxResults".to_string(), serde_json::json!(42));
            repo.update(Some(SettingsCategory::Search), updates).await
        }));
    }
    {
        let repo = Arc::clone(&repo);
        handles.push(tokio::spawn(async move {
            let mut updates = HashMap::new();
            updates.insert("chunkSize".to_string(), serde_json::json!(1234));
            repo.update(Some(SettingsCategory::Indexing), updates).await
        }));
    }
    {
        let repo = Arc::clone(&repo);
        handles.push(tokio::spawn(async move {
            let mut updates = HashMap::new();
            updates.insert("telemetryEnabled".to_string(), serde_json::json!(true));
            repo.update(Some(SettingsCategory::Privacy), updates).await
        }));
    }

    for handle in handles {
        handle.await.unwrap().expect("update should succeed");
    }

    let settings = repo.get_all().await.unwrap();
    assert_eq!(settings.search.max_results, 42, "search update was lost");
    assert_eq!(
        settings.indexing.chunk_size, 1234,
        "indexing update was lost"
    );
    assert!(
        settings.privacy.telemetry_enabled,
        "privacy update was lost"
    );
}

#[tokio::test]
async fn import_merges_and_restores_secrets_after_the_previous_writer_finishes() {
    let (repo, _dir) = create_test_repository().await;
    let mut imported = repo.get_all().await.unwrap();
    imported.llm.ollama_auth_header_name = "Authorization".into();
    imported.llm.ollama_auth_header_value.clear();
    imported.search.max_results = 42;

    let held = repo.write_lock.lock().await;
    let mut importing = Box::pin(repo.import_settings(imported, true));
    assert!(futures::poll!(&mut importing).is_pending());
    let mut current = repo.get_all().await.unwrap();
    current.vault.enabled = true;
    current.llm.ollama_auth_header_name = "Authorization".into();
    current.llm.ollama_auth_header_value = "Bearer newly-rotated-token".into();
    repo.persist(&current).await.unwrap();
    drop(held);

    let result = importing.await.unwrap();
    assert!(
        result.settings.vault.enabled,
        "merge must retain the preceding vault update"
    );
    assert!(
        !result.vault_just_enabled,
        "transition must use the state under the lock"
    );
    assert_eq!(
        result.settings.llm.ollama_auth_header_value,
        "Bearer newly-rotated-token"
    );
    let saved = repo.get_all().await.unwrap();
    assert_eq!(saved.search.max_results, 42);
    assert!(saved.vault.enabled);
    assert_eq!(
        saved.llm.ollama_auth_header_value,
        "Bearer newly-rotated-token"
    );
}

/// An unparseable settings file must be preserved, not overwritten. It is
/// the only copy of the user's vault path, indexed folders and model roles.
#[tokio::test]
async fn corrupt_settings_file_is_quarantined_not_destroyed() {
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join(SETTINGS_FILE_NAME);

    let garbage = "{ this is not valid json";
    fs::write(&settings_path, garbage).await.unwrap();

    let repo = SettingsRepository::new(temp_dir.path().to_path_buf())
        .await
        .unwrap();
    // Reading falls back to defaults...
    let settings = repo.get_all().await.unwrap();
    assert_eq!(settings.indexing.chunk_size, 800);

    // ...but the original bytes still exist somewhere.
    let mut preserved = Vec::new();
    let mut entries = tokio::fs::read_dir(temp_dir.path()).await.unwrap();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.contains(".corrupt-") {
            preserved.push(tokio::fs::read_to_string(entry.path()).await.unwrap());
        }
    }

    assert_eq!(
        preserved.len(),
        1,
        "expected exactly one quarantined settings file"
    );
    assert_eq!(
        preserved[0], garbage,
        "quarantined file must hold the original bytes verbatim"
    );
}

/// Every write must use its own temp file, so a concurrent writer can't
/// truncate the file another writer is about to rename into place.
#[tokio::test]
async fn concurrent_saves_never_leave_unparseable_settings() {
    let temp_dir = TempDir::new().unwrap();
    let repo = Arc::new(
        SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .unwrap(),
    );
    let settings_path = temp_dir.path().join(SETTINGS_FILE_NAME);

    let mut handles = Vec::new();
    for i in 0..16 {
        let repo = Arc::clone(&repo);
        handles.push(tokio::spawn(async move {
            let mut updates = HashMap::new();
            updates.insert("maxResults".to_string(), serde_json::json!(i + 1));
            repo.update(Some(SettingsCategory::Search), updates).await
        }));
    }
    for handle in handles {
        handle.await.unwrap().expect("update should succeed");
    }

    let contents = fs::read_to_string(&settings_path).await.unwrap();
    serde_json::from_str::<SettingsFile>(&contents)
        .expect("settings file must remain parseable after concurrent writes");

    // And no temp files should be left lying around.
    let mut leftovers = Vec::new();
    let mut entries = tokio::fs::read_dir(temp_dir.path()).await.unwrap();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(TEMP_SUFFIX) {
            leftovers.push(name);
        }
    }
    assert!(
        leftovers.is_empty(),
        "temp files left behind: {:?}",
        leftovers
    );
}

#[tokio::test]
async fn test_new_repository_creates_default_settings() {
    let (repo, _temp_dir) = create_test_repository().await;

    let settings = repo.get_all().await.unwrap();

    assert_eq!(settings.indexing.chunk_size, 800);
    assert_eq!(settings.search.max_results, 10);
    assert_eq!(settings.llm.model, "llama3.2:latest");
}

#[tokio::test]
async fn test_get_category() {
    let (repo, _temp_dir) = create_test_repository().await;

    let search = repo.get_category(SettingsCategory::Search).await.unwrap();

    assert!(search.is_object());
    assert_eq!(search["maxResults"], 10);
}

#[tokio::test]
async fn test_save_and_retrieve() {
    let (repo, _temp_dir) = create_test_repository().await;

    let mut settings = repo.get_all().await.unwrap();
    settings.search.max_results = 20;

    repo.save_all(&settings).await.unwrap();

    let retrieved = repo.get_all().await.unwrap();
    assert_eq!(retrieved.search.max_results, 20);
}

#[tokio::test]
async fn test_update_category() {
    let (repo, _temp_dir) = create_test_repository().await;

    let mut updates = HashMap::new();
    updates.insert("maxResults".to_string(), serde_json::json!(30));

    let updated = repo
        .update(Some(SettingsCategory::Search), updates)
        .await
        .unwrap();

    assert_eq!(updated.search.max_results, 30);

    let retrieved = repo.get_all().await.unwrap();
    assert_eq!(retrieved.search.max_results, 30);
}

#[tokio::test]
async fn test_reset_category() {
    let (repo, _temp_dir) = create_test_repository().await;

    // Modify settings
    let mut updates = HashMap::new();
    updates.insert("maxResults".to_string(), serde_json::json!(50));
    repo.update(Some(SettingsCategory::Search), updates)
        .await
        .unwrap();

    let reset = repo.reset(Some(SettingsCategory::Search)).await.unwrap();

    assert_eq!(reset.search.max_results, 10); // Back to default
}

#[tokio::test]
async fn test_reset_all() {
    let (repo, _temp_dir) = create_test_repository().await;

    // Modify multiple categories
    let mut settings = repo.get_all().await.unwrap();
    settings.search.max_results = 50;
    settings.indexing.chunk_size = 1024;
    repo.save_all(&settings).await.unwrap();

    let reset = repo.reset(None).await.unwrap();

    assert_eq!(reset.search.max_results, 10);
    assert_eq!(reset.indexing.chunk_size, 800);
}

#[tokio::test]
async fn test_export_and_import() {
    let (repo, temp_dir) = create_test_repository().await;

    // Modify settings
    let mut settings = repo.get_all().await.unwrap();
    settings.search.max_results = 25;
    repo.save_all(&settings).await.unwrap();

    let export_path = temp_dir.path().join("export.json");
    repo.export(export_path.to_str().unwrap()).await.unwrap();

    repo.reset(None).await.unwrap();

    let imported = repo
        .import(export_path.to_str().unwrap(), false)
        .await
        .unwrap()
        .settings;

    assert_eq!(imported.search.max_results, 25);
}

#[tokio::test]
async fn export_leaves_out_auth_tokens_and_import_restores_them() {
    let (repo, temp_dir) = create_test_repository().await;
    let mut settings = repo.get_all().await.unwrap();
    settings.llm.ollama_auth_header_name = "Authorization".to_string();
    settings.llm.ollama_auth_header_value = "Bearer ollama-secret".to_string();
    settings.llm.llama_cpp.auth_header_name = "X-Api-Key".to_string();
    settings.llm.llama_cpp.auth_header_value = "llama-secret".to_string();
    repo.save_all(&settings).await.unwrap();

    let export_path = temp_dir.path().join("export.json");
    repo.export(export_path.to_str().unwrap()).await.unwrap();
    let exported = fs::read_to_string(&export_path).await.unwrap();
    assert!(!exported.contains("ollama-secret"), "{exported}");
    assert!(!exported.contains("llama-secret"), "{exported}");
    assert!(exported.contains("X-Api-Key"));

    let imported = repo
        .import(export_path.to_str().unwrap(), false)
        .await
        .unwrap()
        .settings;
    assert_eq!(
        imported.llm.ollama_auth_header_value,
        "Bearer ollama-secret"
    );
    assert_eq!(imported.llm.llama_cpp.auth_header_value, "llama-secret");
}

#[test]
fn a_header_whose_token_is_not_here_is_dropped_on_import() {
    let mut imported = SettingsDto::default();
    imported.llm.ollama_auth_header_name = "Authorization".to_string();
    let existing = SettingsDto::default();

    restore_redacted_secrets(&mut imported, &existing);

    assert!(imported.llm.ollama_auth_header_name.is_empty());
    assert!(imported.llm.ollama_auth_header_value.is_empty());
}

#[tokio::test]
async fn test_import_merge() {
    let (repo, temp_dir) = create_test_repository().await;

    let mut export_settings = SettingsDto::default();
    export_settings.search.max_results = 15;

    let export_path = temp_dir.path().join("export.json");
    let json = serde_json::to_string_pretty(&export_settings).unwrap();
    fs::write(&export_path, json).await.unwrap();

    // Modify local indexing settings
    let mut local_settings = repo.get_all().await.unwrap();
    local_settings.indexing.chunk_size = 1024;
    repo.save_all(&local_settings).await.unwrap();

    let merged = repo
        .import(export_path.to_str().unwrap(), true)
        .await
        .unwrap()
        .settings;

    assert_eq!(merged.search.max_results, 15);
    // Note: Current implementation replaces categories, so chunk_size will be reset
    // This is expected behavior for category-level merging
}

#[tokio::test]
async fn test_validation_on_save() {
    let (repo, _temp_dir) = create_test_repository().await;

    let mut settings = repo.get_all().await.unwrap();
    settings.indexing.chunk_size = 0; // Invalid

    let result = repo.save_all(&settings).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_validation_errors() {
    let (repo, _temp_dir) = create_test_repository().await;

    let mut settings = SettingsDto::default();
    settings.indexing.chunk_size = 0;
    settings.search.similarity_threshold = 1.5;

    let validation = repo.validate(&settings);

    assert!(!validation.valid);
    assert!(validation.has_errors());
    assert!(validation.errors.contains_key("indexing"));
    assert!(validation.errors.contains_key("search"));
}

#[tokio::test]
async fn test_validation_warnings() {
    let (repo, _temp_dir) = create_test_repository().await;

    let mut settings = SettingsDto::default();
    settings.indexing.chunk_size = 15000; // Very large

    let validation = repo.validate(&settings);

    assert!(validation.valid); // Still valid, just has warnings
    assert!(validation.has_warnings());
    assert!(validation.warnings.contains_key("indexing"));
}

#[tokio::test]
async fn test_atomic_write_creates_temp_file() {
    let (repo, temp_dir) = create_test_repository().await;

    let settings = repo.get_all().await.unwrap();
    repo.save_all(&settings).await.unwrap();

    let settings_path = temp_dir.path().join(SETTINGS_FILE_NAME);
    assert!(settings_path.exists());

    let temp_path = settings_path.with_extension(TEMP_SUFFIX);
    assert!(!temp_path.exists());
}

#[tokio::test]
async fn test_validate_folder_path() {
    let (repo, temp_dir) = create_test_repository().await;

    // Valid directory
    assert!(repo.validate_folder_path(temp_dir.path().to_str().unwrap()));

    // Invalid path
    assert!(!repo.validate_folder_path("/nonexistent/path"));

    // File instead of directory
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "test").await.unwrap();
    assert!(!repo.validate_folder_path(file_path.to_str().unwrap()));
}

#[tokio::test]
async fn test_corrupted_file_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join(SETTINGS_FILE_NAME);

    fs::write(&settings_path, "{ corrupted json }")
        .await
        .unwrap();

    // Repository should recover with defaults
    let repo = SettingsRepository::new(temp_dir.path().to_path_buf())
        .await
        .unwrap();

    let settings = repo.get_all().await.unwrap();
    assert_eq!(settings.indexing.chunk_size, 800); // Default value
}

#[tokio::test]
async fn test_settings_file_version() {
    let (_repo, temp_dir) = create_test_repository().await;

    let settings_path = temp_dir.path().join(SETTINGS_FILE_NAME);
    let content = fs::read_to_string(&settings_path).await.unwrap();
    let settings_file: SettingsFile = serde_json::from_str(&content).unwrap();

    assert_eq!(settings_file.version, SETTINGS_VERSION);
}

/// The dims are remembered while the mode is `none`, so validation only
/// reads them when truncation is actually on. Rejecting a remembered value
/// nothing consumes would make the whole settings document unsaveable.
#[tokio::test]
async fn vector_compression_dims_are_only_checked_when_truncation_is_on() {
    let (repo, _dir) = create_test_repository().await;

    let mut settings = SettingsDto::default();
    settings.search.vector_index_compression.mode = VectorIndexCompressionModeDto::None;
    settings.search.vector_index_compression.dims = 1;
    assert!(repo.validate_settings(&settings).valid);

    settings.search.vector_index_compression.mode = VectorIndexCompressionModeDto::Truncated;
    let rejected = repo.validate_settings(&settings);
    assert!(!rejected.valid);
    assert!(rejected
        .errors
        .values()
        .flatten()
        .any(|message| message.contains("vector_index_compression.dims")));

    settings.search.vector_index_compression.dims = 512;
    assert!(repo.validate_settings(&settings).valid);

    settings.search.vector_index_compression.dims = MAX_VECTOR_COMPRESSION_DIMS + 1;
    assert!(!repo.validate_settings(&settings).valid);
}
