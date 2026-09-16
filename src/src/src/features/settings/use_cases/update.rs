use crate::application::ports::{
    NoopSettingsSideEffects, SettingsRepositoryPort, SettingsSideEffectsPort,
};
use crate::features::settings::dto::{SettingsCategory, SettingsDto, UpdateSettingsRequestDto};
use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::infrastructure::security::InputValidator;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;
use url::Url;

pub struct UpdateSettingsUseCase {
    repository: Arc<dyn SettingsRepositoryPort>,
    side_effects: Arc<dyn SettingsSideEffectsPort>,
}

impl UpdateSettingsUseCase {
    pub fn new(repository: Arc<dyn SettingsRepositoryPort>) -> Self {
        Self {
            repository,
            side_effects: Arc::new(NoopSettingsSideEffects),
        }
    }

    pub fn with_side_effects(
        repository: Arc<dyn SettingsRepositoryPort>,
        side_effects: Arc<dyn SettingsSideEffectsPort>,
    ) -> Self {
        Self {
            repository,
            side_effects,
        }
    }

    pub async fn execute(&self, request: UpdateSettingsRequestDto) -> Result<SettingsDto> {
        // Validate before merge so hostile values never reach the repository.
        validate_security_constraints(&request)?;

        let previous = self.repository.get_all().await.ok();
        let previous_ollama_url = previous
            .as_ref()
            .map(|s| s.llm.ollama_url.clone())
            .unwrap_or_default();
        let previous_vault_enabled = previous.as_ref().map(|s| s.vault.enabled).unwrap_or(false);

        // Pre-flight vault writability on enable flip.
        if let Some(proposed) = compute_proposed_vault(&request, previous.as_ref()) {
            if !previous_vault_enabled && proposed.enabled {
                let root =
                    crate::features::vault::writeback::resolve_vault_root(&proposed.vault_path)
                        .ok_or_else(|| {
                            AppError::InvalidInput(
                                "Vault root could not be resolved (no home directory available). \
                         Pick an explicit folder in Vault settings."
                                    .to_string(),
                            )
                        })?;
                probe_vault_writable(&root).await?;
            }
        }

        let updated_settings = self
            .repository
            .update(request.category, request.updates.clone())
            .await?;

        let validation = self.repository.validate(&updated_settings);
        if !validation.valid {
            return Err(AppError::InvalidInput(format!(
                "Settings validation failed: {:?}",
                validation.errors
            )));
        }

        // `repository.update()` already persisted, under its write lock.
        // Saving again here re-ran the whole write outside that lock, which
        // widened the window for a concurrent update to be lost.

        if updated_settings.llm.ollama_url != previous_ollama_url {
            let event = AuditEvent::new(AuditAction::ConfigChanged, AuditResult::success())
                .with_resource_id("settings.llm.ollama_url")
                .with_metadata("operation", "update_settings")
                .with_metadata("ollama_url", &updated_settings.llm.ollama_url)
                .with_metadata("previous_ollama_url", &previous_ollama_url);

            if let Err(e) = get_audit_logger().log(event).await {
                tracing::warn!("Failed to write audit log for settings update: {}", e);
            }
        }

        // Side effects after persist. Best-effort — failures don't roll
        // back already-committed settings.
        let hints =
            crate::application::ports::settings_side_effects_port::SettingsTransitionHints {
                vault_just_enabled: !previous_vault_enabled && updated_settings.vault.enabled,
            };
        self.side_effects
            .on_settings_updated(request.category, hints)
            .await;

        Ok(updated_settings)
    }

    pub async fn update_category(
        &self,
        category: SettingsCategory,
        updates: std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<SettingsDto> {
        let request = UpdateSettingsRequestDto {
            category: Some(category),
            updates,
        };
        self.execute(request).await
    }

    pub async fn update_global(
        &self,
        updates: std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<SettingsDto> {
        let request = UpdateSettingsRequestDto {
            category: None,
            updates,
        };
        self.execute(request).await
    }
}

fn validate_security_constraints(request: &UpdateSettingsRequestDto) -> Result<()> {
    match request.category {
        Some(SettingsCategory::Llm) => {
            if let Some(value) = request.updates.get("ollamaUrl") {
                if let Some(url) = value.as_str() {
                    validate_http_url(url).map_err(AppError::InvalidInput)?;
                }
            }
        }
        Some(SettingsCategory::Indexing) => {
            if let Some(value) = request.updates.get("indexedPaths") {
                validate_indexed_paths_payload(value)?;
            }
        }
        Some(SettingsCategory::Vault) => {
            if let Some(value) = request.updates.get("vaultPath") {
                validate_vault_path_payload(value)?;
            }
        }
        None => {
            if let Some(llm) = request.updates.get("llm") {
                if let Some(url) = llm.get("ollamaUrl").and_then(|v| v.as_str()) {
                    validate_http_url(url).map_err(AppError::InvalidInput)?;
                }
            }
            if let Some(indexing) = request.updates.get("indexing") {
                if let Some(paths) = indexing.get("indexedPaths") {
                    validate_indexed_paths_payload(paths)?;
                }
            }
            if let Some(vault) = request.updates.get("vault") {
                if let Some(path) = vault.get("vaultPath") {
                    validate_vault_path_payload(path)?;
                }
            }
        }
        _ => {}
    }

    Ok(())
}

struct ProposedVault {
    enabled: bool,
    vault_path: String,
}

/// Returns `None` for updates that can't affect vault config.
fn compute_proposed_vault(
    request: &UpdateSettingsRequestDto,
    previous: Option<&SettingsDto>,
) -> Option<ProposedVault> {
    let prev_enabled = previous.map(|s| s.vault.enabled).unwrap_or(false);
    let prev_path = previous
        .map(|s| s.vault.vault_path.clone())
        .unwrap_or_default();

    match request.category {
        Some(SettingsCategory::Vault) => {
            let enabled = request
                .updates
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(prev_enabled);
            let vault_path = request
                .updates
                .get("vaultPath")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or(prev_path);
            Some(ProposedVault {
                enabled,
                vault_path,
            })
        }
        None => {
            let vault = request.updates.get("vault")?;
            let enabled = vault
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(prev_enabled);
            let vault_path = vault
                .get("vaultPath")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or(prev_path);
            Some(ProposedVault {
                enabled,
                vault_path,
            })
        }
        Some(_) => None,
    }
}

async fn probe_vault_writable(root: &std::path::Path) -> Result<()> {
    if let Err(e) = tokio::fs::create_dir_all(root).await {
        return Err(AppError::InvalidInput(format!(
            "Vault folder '{}' could not be created: {}. Pick a different folder.",
            root.display(),
            e
        )));
    }
    let sentinel = root.join(".lattice-vault-probe");
    if let Err(e) = tokio::fs::write(&sentinel, b"probe").await {
        return Err(AppError::InvalidInput(format!(
            "Vault folder '{}' is not writable: {}. Check permissions, or pick a different folder.",
            root.display(),
            e
        )));
    }
    // Cleanup is best-effort.
    if let Err(e) = tokio::fs::remove_file(&sentinel).await {
        tracing::warn!(
            sentinel = %sentinel.display(),
            error = %e,
            "vault writability probe: failed to clean up sentinel file"
        );
    }
    Ok(())
}

fn validate_vault_path_payload(value: &serde_json::Value) -> Result<()> {
    let path_str = value
        .as_str()
        .ok_or_else(|| AppError::InvalidInput("vaultPath must be a string".to_string()))?;

    if path_str.is_empty() {
        return Ok(());
    }

    InputValidator::new()
        .validate_directory_path(path_str, false)
        .map_err(|e| AppError::InvalidInput(format!("Invalid vaultPath: {e}")))?;

    Ok(())
}

fn validate_indexed_paths_payload(value: &serde_json::Value) -> Result<()> {
    let arr = value.as_array().ok_or_else(|| {
        AppError::InvalidInput("indexedPaths must be an array of absolute path strings".to_string())
    })?;

    let validator = InputValidator::new();
    for entry in arr {
        let path_str = entry.as_str().ok_or_else(|| {
            AppError::InvalidInput("indexedPaths entries must be strings".to_string())
        })?;
        validator
            .validate_directory_path(path_str, false)
            .map_err(|e| AppError::InvalidInput(format!("Invalid path in indexedPaths: {e}")))?;
    }

    Ok(())
}

fn validate_http_url(url_str: &str) -> std::result::Result<(), String> {
    let url = Url::parse(url_str).map_err(|e| format!("Invalid URL: {e}"))?;

    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("URL must use http:// or https://".to_string());
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err("URL must not contain username or password".to_string());
    }

    let host = url.host_str().ok_or("URL must have a valid host")?;
    if host.is_empty() {
        return Err("URL must have a non-empty host".to_string());
    }

    if let Some(port) = url.port() {
        if port == 0 {
            return Err("Invalid port number".to_string());
        }
    }

    Ok(())
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::MockSettingsRepository;
    use serde_json::json;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_ssrf_authority_component_attack_rejected() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("ollamaUrl".to_string(), json!("http://localhost@evil.com"));

        let result = use_case
            .update_category(SettingsCategory::Llm, updates)
            .await;
        assert!(
            result.is_err(),
            "SSRF authority-component attack must be rejected"
        );
    }

    #[tokio::test]
    async fn test_ssrf_credentials_in_url_rejected() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert(
            "ollamaUrl".to_string(),
            json!("https://admin:password@internal.host"),
        );

        let result = use_case
            .update_category(SettingsCategory::Llm, updates)
            .await;
        assert!(result.is_err(), "Credentials in URL must be rejected");
    }

    #[tokio::test]
    async fn test_ssrf_file_protocol_rejected() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("ollamaUrl".to_string(), json!("file:///etc/passwd"));

        let result = use_case
            .update_category(SettingsCategory::Llm, updates)
            .await;
        assert!(result.is_err(), "file:// scheme must be rejected");
    }

    #[tokio::test]
    async fn test_valid_ollama_url_accepted() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("ollamaUrl".to_string(), json!("http://localhost:11434"));

        let result = use_case
            .update_category(SettingsCategory::Llm, updates)
            .await
            .expect("valid URL should be accepted");

        assert_eq!(result.llm.ollama_url, "http://localhost:11434");
    }

    #[tokio::test]
    async fn test_path_traversal_in_indexed_paths_rejected() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("indexedPaths".to_string(), json!(["/Users/josh/../../etc"]));

        let result = use_case
            .update_category(SettingsCategory::Indexing, updates)
            .await;
        assert!(
            result.is_err(),
            "CWE-22 path traversal must be rejected at the use-case layer"
        );
    }

    #[tokio::test]
    async fn test_relative_path_in_indexed_paths_rejected() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("indexedPaths".to_string(), json!(["Documents/folder"]));

        let result = use_case
            .update_category(SettingsCategory::Indexing, updates)
            .await;
        assert!(result.is_err(), "Relative paths must be rejected");
    }

    #[tokio::test]
    async fn test_indexed_paths_must_be_array_of_strings() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("indexedPaths".to_string(), json!("/just/a/string"));

        let result = use_case
            .update_category(SettingsCategory::Indexing, updates)
            .await;
        assert!(
            result.is_err(),
            "Non-array payload should be rejected with a clear message"
        );
    }

    #[tokio::test]
    async fn test_valid_absolute_paths_accepted() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert(
            "indexedPaths".to_string(),
            json!(["/Users/josh/Documents", "/Users/josh/Projects"]),
        );

        let result = use_case
            .update_category(SettingsCategory::Indexing, updates)
            .await
            .expect("valid absolute paths must be accepted");

        assert_eq!(result.indexing.indexed_paths.len(), 2);
    }

    #[tokio::test]
    async fn test_global_update_still_checks_ollama_url() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert(
            "llm".to_string(),
            json!({ "ollamaUrl": "http://localhost@evil.com" }),
        );

        let result = use_case.update_global(updates).await;
        assert!(
            result.is_err(),
            "global update must not bypass SSRF check on nested ollamaUrl"
        );
    }

    // SSRF and path-traversal regression tests.

    #[tokio::test]
    async fn audit_p03_side_effects_fire_on_successful_update() {
        use crate::application::ports::settings_side_effects_port::RecordingSettingsSideEffects;

        let repository = Arc::new(MockSettingsRepository::new());
        let side_effects = Arc::new(RecordingSettingsSideEffects::new());
        let use_case = UpdateSettingsUseCase::with_side_effects(repository, side_effects.clone());

        let mut updates = HashMap::new();
        updates.insert("temperature".to_string(), json!(0.5));
        let request = UpdateSettingsRequestDto {
            category: Some(SettingsCategory::Llm),
            updates,
        };

        use_case
            .execute(request)
            .await
            .expect("update should succeed");

        let calls = side_effects.calls();
        assert_eq!(
            calls,
            vec![Some(SettingsCategory::Llm)],
            "side effect must fire exactly once with the updated category"
        );
    }

    #[tokio::test]
    async fn audit_p03_side_effects_skipped_on_validation_failure() {
        use crate::application::ports::settings_side_effects_port::RecordingSettingsSideEffects;

        let repository = Arc::new(MockSettingsRepository::new());
        let side_effects = Arc::new(RecordingSettingsSideEffects::new());
        let use_case = UpdateSettingsUseCase::with_side_effects(repository, side_effects.clone());

        // Path-traversal payload — rejected before repository write.
        let mut updates = HashMap::new();
        updates.insert("indexedPaths".to_string(), json!(["/Users/josh/../../etc"]));
        let request = UpdateSettingsRequestDto {
            category: Some(SettingsCategory::Indexing),
            updates,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err(), "validation should reject traversal");

        let calls = side_effects.calls();
        assert!(
            calls.is_empty(),
            "side effect must not fire when the write was rejected (got {calls:?})"
        );
    }

    #[tokio::test]
    async fn audit_p03_side_effects_carry_global_category_for_global_update() {
        use crate::application::ports::settings_side_effects_port::RecordingSettingsSideEffects;

        let repository = Arc::new(MockSettingsRepository::new());
        let side_effects = Arc::new(RecordingSettingsSideEffects::new());
        let use_case = UpdateSettingsUseCase::with_side_effects(repository, side_effects.clone());

        let mut updates = HashMap::new();
        updates.insert("search".to_string(), json!({ "maxResults": 5 }));
        let request = UpdateSettingsRequestDto {
            category: None, // global
            updates,
        };

        use_case
            .execute(request)
            .await
            .expect("update should succeed");

        let calls = side_effects.calls();
        assert_eq!(
            calls,
            vec![None],
            "global update should pass None so the impl can invalidate everything"
        );
    }

    #[tokio::test]
    async fn test_global_update_still_checks_indexed_paths() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert(
            "indexing".to_string(),
            json!({ "indexedPaths": ["/Users/josh/../../etc"] }),
        );

        let result = use_case.update_global(updates).await;
        assert!(
            result.is_err(),
            "global update must not bypass path validation on nested indexedPaths"
        );
    }

    /// The first-run gate reads this flag instead of `localStorage`, so the
    /// category has to round-trip through the repository like any other.
    #[tokio::test]
    async fn onboarding_first_run_dismissed_round_trips() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository.clone());

        let before = repository.get_all().await.expect("defaults readable");
        assert!(
            !before.onboarding.first_run_dismissed,
            "a fresh install has not dismissed first run"
        );

        let mut updates = HashMap::new();
        updates.insert("firstRunDismissed".to_string(), json!(true));

        let updated = use_case
            .update_category(SettingsCategory::Onboarding, updates)
            .await
            .expect("onboarding is a real category");

        assert!(updated.onboarding.first_run_dismissed);
        assert!(
            repository
                .get_all()
                .await
                .expect("readable")
                .onboarding
                .first_run_dismissed,
            "the flag must be persisted, not just echoed back"
        );
    }

    #[test]
    fn onboarding_parses_as_a_settings_category() {
        use std::str::FromStr;
        assert_eq!(
            SettingsCategory::from_str("onboarding").expect("known category"),
            SettingsCategory::Onboarding
        );
        assert!(SettingsCategory::all().contains(&SettingsCategory::Onboarding));
    }
}
