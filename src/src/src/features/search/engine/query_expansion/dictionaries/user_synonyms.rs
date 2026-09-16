//! User-defined synonym dictionary management.
//!
//! Handles loading, saving, and managing custom user-defined synonyms
//! that take priority over domain dictionary expansions.

use crate::shared::error::Result;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Get the path to the user synonyms configuration file.
///
/// Returns: `~/.config/lattice-desktop/synonyms.json`
pub fn get_user_synonyms_path() -> PathBuf {
    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home_dir
        .join(".config")
        .join("lattice-desktop")
        .join("synonyms.json")
}

/// Load user-defined synonyms from configuration file.
///
/// Returns an empty HashMap if the file doesn't exist.
/// Returns an error if the file exists but cannot be parsed.
pub fn load_user_synonyms() -> Result<HashMap<String, Vec<String>>> {
    let config_path = get_user_synonyms_path();

    if !config_path.exists() {
        return Ok(HashMap::new());
    }

    let content = fs::read_to_string(&config_path)?;
    let synonyms: HashMap<String, Vec<String>> = serde_json::from_str(&content)?;

    Ok(synonyms)
}

/// Save user-defined synonyms to configuration file.
///
/// Creates parent directories if they don't exist.
pub fn save_user_synonyms(synonyms: &HashMap<String, Vec<String>>) -> Result<()> {
    let config_path = get_user_synonyms_path();

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(synonyms)?;
    fs::write(&config_path, content)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_get_user_synonyms_path() {
        let path = get_user_synonyms_path();
        assert!(path.to_string_lossy().contains("lattice-desktop"));
        assert!(path.to_string_lossy().ends_with("synonyms.json"));
    }

    #[test]
    fn test_load_nonexistent_file() {
        env::set_var("HOME", "/nonexistent_test_dir_12345");
        let result = load_user_synonyms();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), HashMap::new());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let temp_dir = env::temp_dir().join("vault_test_synonyms");
        fs::create_dir_all(&temp_dir).unwrap();

        let temp_file = temp_dir.join("test_synonyms.json");

        let mut synonyms = HashMap::new();
        synonyms.insert(
            "test".to_string(),
            vec!["exam".to_string(), "trial".to_string()],
        );

        let content = serde_json::to_string_pretty(&synonyms).unwrap();
        fs::write(&temp_file, content).unwrap();

        let loaded_content = fs::read_to_string(&temp_file).unwrap();
        let loaded: HashMap<String, Vec<String>> = serde_json::from_str(&loaded_content).unwrap();

        assert_eq!(loaded, synonyms);

        fs::remove_file(&temp_file).ok();
        fs::remove_dir(&temp_dir).ok();
    }
}
