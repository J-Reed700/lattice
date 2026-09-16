use crate::infrastructure::security::SecureStorage;
use std::error::Error;
use std::path::Path;
use tauri::Manager;
use tracing::{info, warn};

pub struct CredentialMigration;

impl CredentialMigration {
    pub fn migrate_from_config(config_path: &Path) -> Result<MigrationReport, Box<dyn Error>> {
        let mut report = MigrationReport::default();

        if !config_path.exists() {
            info!(
                "No config file found at {:?}, skipping migration",
                config_path
            );
            return Ok(report);
        }

        let contents = std::fs::read_to_string(config_path)?;

        let config_json: serde_json::Value = match serde_json::from_str(&contents) {
            Ok(json) => json,
            Err(e) => {
                warn!("Failed to parse config file: {}. Skipping migration.", e);
                return Ok(report);
            }
        };

        Self::migrate_key(&config_json, "ollama_api_key", &mut report)?;
        Self::migrate_key(&config_json, "openai_api_key", &mut report)?;
        Self::migrate_endpoint(&config_json, "custom_api_endpoint", &mut report)?;

        if report.keys_migrated > 0 {
            info!(
                "Migration complete: {} keys migrated, {} keys already secure",
                report.keys_migrated, report.keys_already_secure
            );
        }

        Ok(report)
    }

    fn migrate_key(
        config_json: &serde_json::Value,
        key_name: &str,
        report: &mut MigrationReport,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(value) = config_json.get(key_name) {
            if let Some(api_key) = value.as_str() {
                if !api_key.is_empty() {
                    match key_name {
                        "ollama_api_key" => {
                            SecureStorage::set_ollama_key(api_key)?;
                            report.keys_migrated += 1;
                            info!("Migrated {} to secure storage", key_name);
                        }
                        "openai_api_key" => {
                            SecureStorage::set_openai_key(api_key)?;
                            report.keys_migrated += 1;
                            info!("Migrated {} to secure storage", key_name);
                        }
                        _ => {}
                    }
                }
            }
        } else if Self::check_if_key_already_secure(key_name)? {
            report.keys_already_secure += 1;
        }
        Ok(())
    }

    fn migrate_endpoint(
        config_json: &serde_json::Value,
        key_name: &str,
        report: &mut MigrationReport,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(value) = config_json.get(key_name) {
            if let Some(endpoint) = value.as_str() {
                if !endpoint.is_empty() {
                    SecureStorage::set_custom_endpoint(endpoint)?;
                    report.keys_migrated += 1;
                    info!("Migrated {} to secure storage", key_name);
                }
            }
        }
        Ok(())
    }

    fn check_if_key_already_secure(key_name: &str) -> Result<bool, Box<dyn Error>> {
        match key_name {
            "ollama_api_key" => Ok(SecureStorage::get_ollama_key()?.is_some()),
            "openai_api_key" => Ok(SecureStorage::get_openai_key()?.is_some()),
            _ => Ok(false),
        }
    }

    pub fn remove_plaintext_keys_from_config(config_path: &Path) -> Result<(), Box<dyn Error>> {
        if !config_path.exists() {
            return Ok(());
        }

        let contents = std::fs::read_to_string(config_path)?;
        let mut config_json: serde_json::Value = serde_json::from_str(&contents)?;

        let config_obj = config_json
            .as_object_mut()
            .ok_or("Config file is not a valid JSON object")?;

        let mut modified = false;

        if config_obj.contains_key("ollama_api_key") {
            config_obj.remove("ollama_api_key");
            modified = true;
            info!("Removed ollama_api_key from plaintext config");
        }

        if config_obj.contains_key("openai_api_key") {
            config_obj.remove("openai_api_key");
            modified = true;
            info!("Removed openai_api_key from plaintext config");
        }

        if config_obj.contains_key("custom_api_endpoint") {
            config_obj.remove("custom_api_endpoint");
            modified = true;
            info!("Removed custom_api_endpoint from plaintext config");
        }

        if modified {
            let new_contents = serde_json::to_string_pretty(&config_json)?;
            std::fs::write(config_path, new_contents)?;
            info!("Updated config file to remove plaintext credentials");
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct MigrationReport {
    pub keys_migrated: usize,
    pub keys_already_secure: usize,
}

#[tauri::command]
pub async fn migrate_credentials(app_handle: tauri::AppHandle) -> Result<String, String> {
    let app_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    let config_path = app_dir.join("config.json");

    let report = CredentialMigration::migrate_from_config(&config_path)
        .map_err(|e| format!("Migration failed: {}", e))?;

    if report.keys_migrated > 0 {
        CredentialMigration::remove_plaintext_keys_from_config(&config_path)
            .map_err(|e| format!("Failed to clean up config file: {}", e))?;

        Ok(format!(
            "Migration complete: {} credentials moved to secure storage",
            report.keys_migrated
        ))
    } else if report.keys_already_secure > 0 {
        Ok(format!(
            "No migration needed: {} credentials already secure",
            report.keys_already_secure
        ))
    } else {
        Ok("No credentials found to migrate".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    #[ignore = "Requires OS keyring access - may not be available in CI"]
    fn test_migration_from_plaintext_config() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_path = temp_file.path();

        let config_content = r#"{
            "ollama_api_key": "test-ollama-key-123",
            "openai_api_key": "test-openai-key-456",
            "custom_api_endpoint": "https://custom.example.com",
            "other_setting": "value"
        }"#;

        std::fs::write(config_path, config_content).unwrap();

        let report = CredentialMigration::migrate_from_config(config_path).unwrap();
        assert!(report.keys_migrated >= 2);

        let ollama_key = SecureStorage::get_ollama_key().unwrap();
        assert_eq!(ollama_key, Some("test-ollama-key-123".to_string()));

        let openai_key = SecureStorage::get_openai_key().unwrap();
        assert_eq!(openai_key, Some("test-openai-key-456".to_string()));

        SecureStorage::clear_all().unwrap();
    }

    #[test]
    fn test_remove_plaintext_keys() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_path = temp_file.path();

        let config_content = r#"{
            "ollama_api_key": "test-key",
            "other_setting": "value"
        }"#;

        std::fs::write(config_path, config_content).unwrap();

        CredentialMigration::remove_plaintext_keys_from_config(config_path).unwrap();

        let updated_content = std::fs::read_to_string(config_path).unwrap();
        let updated_json: serde_json::Value = serde_json::from_str(&updated_content).unwrap();

        assert!(updated_json.get("ollama_api_key").is_none());
        assert!(updated_json.get("other_setting").is_some());
    }
}
