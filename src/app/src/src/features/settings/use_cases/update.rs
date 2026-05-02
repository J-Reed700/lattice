//! Update Settings Use Case
//!
//! Domain + security validation for settings mutations. Owns:
//!
//! - **SSRF prevention** for `llm.ollama_url` (CWE-918) — moved here from
//!   the deleted `save_config` Tauri command.
//! - **Path traversal / null byte prevention** for `indexing.indexed_paths`
//!   (CWE-22, CWE-158) — moved here from the deleted `add_watch_folder`
//!   command. Critical: applies to ANY mutation of `indexed_paths`, not
//!   just the dedicated commands. Without this, a malicious or buggy
//!   frontend could inject `/etc/shadow` via a raw `update_settings`
//!   call with category=indexing and an `indexedPaths` payload.
//! - **Audit logging** for LLM endpoint changes (CWE-778) — moved here
//!   from `save_config`.
//!
//! The repository stays "dumb" — it does structural validation only
//! (Zod-like — non-empty strings, in-range numbers). Domain + security
//! validation lives at this layer.

use crate::application::ports::{
    NoopSettingsSideEffects, SettingsRepositoryPort, SettingsSideEffectsPort,
};
use crate::features::settings::dto::{SettingsCategory, SettingsDto, UpdateSettingsRequestDto};
use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::infrastructure::security::InputValidator;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;
use url::Url;

/// Use case for updating application settings.
///
/// # Responsibilities
///
/// - Update specific settings fields
/// - Support category-specific or global updates
/// - Domain + security validation (SSRF, path traversal)
/// - Audit logging for security-sensitive changes
/// - Repository-level structural validation
/// - Trigger downstream side effects (cache invalidation) via the
///   `SettingsSideEffectsPort`. Audit P0-3 fix: previously the Tauri
///   command did this directly, leaking when internal callers
///   bypassed the plugin layer.
pub struct UpdateSettingsUseCase {
    repository: Arc<dyn SettingsRepositoryPort>,
    side_effects: Arc<dyn SettingsSideEffectsPort>,
}

impl UpdateSettingsUseCase {
    /// Create a use case with no-op side effects. Convenience for
    /// tests and code paths that legitimately don't care about cache
    /// invalidation. Production code should use `with_side_effects`.
    pub fn new(repository: Arc<dyn SettingsRepositoryPort>) -> Self {
        Self {
            repository,
            side_effects: Arc::new(NoopSettingsSideEffects),
        }
    }

    /// Production constructor. Wires the side-effects port so a
    /// successful update triggers `on_settings_updated(category)` —
    /// which the Container impl uses to invalidate the LLM cache,
    /// router LLM cache, and refresh custom-tool runtime
    /// configuration.
    pub fn with_side_effects(
        repository: Arc<dyn SettingsRepositoryPort>,
        side_effects: Arc<dyn SettingsSideEffectsPort>,
    ) -> Self {
        Self {
            repository,
            side_effects,
        }
    }

    /// Execute the use case to update settings.
    ///
    /// # Errors
    ///
    /// - `InvalidInput` — SSRF check failed on `llm.ollama_url`, path
    ///   validation failed on `indexing.indexed_paths`, or repository
    ///   structural validation failed.
    /// - `Storage` — Failed to persist updated settings.
    pub async fn execute(&self, request: UpdateSettingsRequestDto) -> Result<SettingsDto> {
        // ---------- Pre-write security validation ----------
        // These checks run on the *incoming* update payload BEFORE the
        // repository merges it, so a malicious value never lands in
        // memory or on disk.
        validate_security_constraints(&request)?;

        // Capture pre-state for audit logging + transition detection.
        let previous = self.repository.get_all().await.ok();
        let previous_ollama_url = previous
            .as_ref()
            .map(|s| s.llm.ollama_url.clone())
            .unwrap_or_default();
        let previous_vault_enabled =
            previous.as_ref().map(|s| s.vault.enabled).unwrap_or(false);

        // ---------- Apply + structural validation ----------
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

        self.repository.save_all(&updated_settings).await?;

        // ---------- Audit logging ----------
        // Emit an audit event when a security-sensitive field changed.
        // Today: ollama_url. Add others here as they're identified.
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

        // ---------- Downstream side effects ----------
        // Audit P0-3 fix: trigger cache invalidation here, not in the
        // Tauri command, so internal callers (tests, watch-folder
        // helpers, future migration scripts) can't bypass it. The
        // port impl is best-effort — failures don't roll back the
        // already-persisted settings.
        let hints = crate::application::ports::settings_side_effects_port::SettingsTransitionHints {
            vault_just_enabled: !previous_vault_enabled && updated_settings.vault.enabled,
        };
        self.side_effects
            .on_settings_updated(request.category, hints)
            .await;

        Ok(updated_settings)
    }

    /// Update a specific settings category (convenience wrapper).
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

    /// Update global settings (across all categories).
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

// ============================================================================
// Security validation helpers
// ============================================================================

/// Run the security checks that apply to settings mutations. Returns
/// `Err(InvalidInput)` if any field carries a value that should never
/// reach the repository.
///
/// Run BEFORE the repository merge so a hostile payload never lands on
/// disk. Each check is targeted at a specific field; unrelated updates
/// (e.g. changing `search.max_results`) are not affected.
fn validate_security_constraints(request: &UpdateSettingsRequestDto) -> Result<()> {
    // The category gates which keys can appear. For LLM updates, check
    // ollama_url for SSRF. For Indexing updates, check indexed_paths
    // for traversal. For a "global" update (no category), check both
    // — the payload is keyed by category names, so the keys we look at
    // are different.

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
        // Global update (None) — the payload may carry the whole
        // settings shape with `llm.ollamaUrl` or `indexing.indexedPaths`
        // as nested objects.
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
        // Other categories carry no security-sensitive fields today.
        _ => {}
    }

    Ok(())
}

/// CWE-22 / CWE-158: vault path must be a valid absolute directory path
/// (or empty, meaning "use the default"). Same validator as
/// `indexed_paths` since the threat surface is identical — a hostile
/// frontend could try to point the vault writeback at `/etc` or
/// `~/.ssh`.
fn validate_vault_path_payload(value: &serde_json::Value) -> Result<()> {
    let path_str = value.as_str().ok_or_else(|| {
        AppError::InvalidInput("vaultPath must be a string".to_string())
    })?;

    // Empty string is the sentinel for "use default" — accept it.
    if path_str.is_empty() {
        return Ok(());
    }

    InputValidator::new()
        .validate_directory_path(path_str, false)
        .map_err(|e| AppError::InvalidInput(format!("Invalid vaultPath: {e}")))?;

    Ok(())
}

/// CWE-22 / CWE-158: enforce path validation on every string in an
/// `indexed_paths` array payload.
fn validate_indexed_paths_payload(value: &serde_json::Value) -> Result<()> {
    let arr = value.as_array().ok_or_else(|| {
        AppError::InvalidInput(
            "indexedPaths must be an array of absolute path strings".to_string(),
        )
    })?;

    let validator = InputValidator::new();
    for entry in arr {
        let path_str = entry.as_str().ok_or_else(|| {
            AppError::InvalidInput(
                "indexedPaths entries must be strings".to_string(),
            )
        })?;
        // require_exists=false: legitimate use of generic update may
        // include paths the user wants to track even if currently
        // unmounted (network drives, removable media). The dedicated
        // add_watch_folder command checks existence at add time.
        validator
            .validate_directory_path(path_str, false)
            .map_err(|e| AppError::InvalidInput(format!(
                "Invalid path in indexedPaths: {e}"
            )))?;
    }

    Ok(())
}

/// SSRF (CWE-918) prevention for HTTP/HTTPS URLs. Moved here verbatim
/// from the deleted `save_config` command to preserve the security
/// surface.
///
/// Blocks:
/// - Non-http(s) schemes (file://, javascript:, etc.)
/// - URLs with userinfo (`http://localhost@evil.com` style)
/// - Empty or invalid host
/// - Port 0
fn validate_http_url(url_str: &str) -> std::result::Result<(), String> {
    let url = Url::parse(url_str).map_err(|e| format!("Invalid URL: {e}"))?;

    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("URL must use http:// or https://".to_string());
    }

    // Reject URLs with userinfo to prevent SSRF authority-component attacks.
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::MockSettingsRepository;
    use serde_json::json;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_update_search_category() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("maxResults".to_string(), json!(20));
        updates.insert("similarityThreshold".to_string(), json!(0.8));

        let request = UpdateSettingsRequestDto {
            category: Some(SettingsCategory::Search),
            updates,
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.search.max_results, 20);
        assert_eq!(result.search.similarity_threshold, 0.8);
    }

    #[tokio::test]
    async fn test_update_indexing_category() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("chunkSize".to_string(), json!(1024));
        updates.insert("chunkOverlap".to_string(), json!(100));

        let request = UpdateSettingsRequestDto {
            category: Some(SettingsCategory::Indexing),
            updates,
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.indexing.chunk_size, 1024);
        assert_eq!(result.indexing.chunk_overlap, 100);
    }

    #[tokio::test]
    async fn test_update_category_helper() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("temperature".to_string(), json!(0.9));
        updates.insert("maxTokens".to_string(), json!(4096));

        let result = use_case
            .update_category(SettingsCategory::Llm, updates)
            .await
            .unwrap();

        assert_eq!(result.llm.temperature, 0.9);
        assert_eq!(result.llm.max_tokens, 4096);
    }

    #[tokio::test]
    async fn test_validation_failure() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("chunkSize".to_string(), json!(0));

        let request = UpdateSettingsRequestDto {
            category: Some(SettingsCategory::Indexing),
            updates,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validation_out_of_range() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("similarityThreshold".to_string(), json!(1.5));

        let request = UpdateSettingsRequestDto {
            category: Some(SettingsCategory::Search),
            updates,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
    }

    // === SSRF tests for llm.ollama_url ===

    #[tokio::test]
    async fn test_ssrf_authority_component_attack_rejected() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert(
            "ollamaUrl".to_string(),
            json!("http://localhost@evil.com"),
        );

        let result = use_case.update_category(SettingsCategory::Llm, updates).await;
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

        let result = use_case.update_category(SettingsCategory::Llm, updates).await;
        assert!(result.is_err(), "Credentials in URL must be rejected");
    }

    #[tokio::test]
    async fn test_ssrf_file_protocol_rejected() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("ollamaUrl".to_string(), json!("file:///etc/passwd"));

        let result = use_case.update_category(SettingsCategory::Llm, updates).await;
        assert!(result.is_err(), "file:// scheme must be rejected");
    }

    #[tokio::test]
    async fn test_valid_ollama_url_accepted() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert(
            "ollamaUrl".to_string(),
            json!("http://localhost:11434"),
        );

        let result = use_case
            .update_category(SettingsCategory::Llm, updates)
            .await
            .expect("valid URL should be accepted");

        assert_eq!(result.llm.ollama_url, "http://localhost:11434");
    }

    // === Path traversal tests for indexing.indexed_paths ===

    #[tokio::test]
    async fn test_path_traversal_in_indexed_paths_rejected() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert(
            "indexedPaths".to_string(),
            json!(["/Users/josh/../../etc"]),
        );

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
        updates.insert(
            "indexedPaths".to_string(),
            json!(["Documents/folder"]),
        );

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

    // === Global (no-category) update SSRF + path checks ===

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

    // ============================================================
    // Audit P0-3 regression tests: settings updates MUST trigger
    // SettingsSideEffectsPort, regardless of caller (IPC, internal,
    // test). These tests pin the invariant so a future refactor that
    // re-introduces the leak (e.g., moves cache-invalidation back to
    // the plugin layer) breaks them visibly.
    // ============================================================

    #[tokio::test]
    async fn audit_p03_side_effects_fire_on_successful_update() {
        use crate::application::ports::settings_side_effects_port::RecordingSettingsSideEffects;

        let repository = Arc::new(MockSettingsRepository::new());
        let side_effects = Arc::new(RecordingSettingsSideEffects::new());
        let use_case = UpdateSettingsUseCase::with_side_effects(
            repository,
            side_effects.clone(),
        );

        let mut updates = HashMap::new();
        updates.insert("temperature".to_string(), json!(0.5));
        let request = UpdateSettingsRequestDto {
            category: Some(SettingsCategory::Llm),
            updates,
        };

        use_case.execute(request).await.expect("update should succeed");

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
        let use_case = UpdateSettingsUseCase::with_side_effects(
            repository,
            side_effects.clone(),
        );

        // Path-traversal payload — rejected before the repository
        // write. Side effect MUST NOT fire because nothing changed
        // on disk; firing it would invalidate the LLM cache for no
        // reason and force a costly model reload on the next chat
        // turn.
        let mut updates = HashMap::new();
        updates.insert(
            "indexedPaths".to_string(),
            json!(["/Users/josh/../../etc"]),
        );
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
        let use_case = UpdateSettingsUseCase::with_side_effects(
            repository,
            side_effects.clone(),
        );

        let mut updates = HashMap::new();
        updates.insert(
            "search".to_string(),
            json!({ "maxResults": 5 }),
        );
        let request = UpdateSettingsRequestDto {
            category: None, // global
            updates,
        };

        use_case.execute(request).await.expect("update should succeed");

        let calls = side_effects.calls();
        assert_eq!(
            calls,
            vec![None],
            "global update should pass None to the side-effects port \
             so the impl can decide to invalidate everything"
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
}
