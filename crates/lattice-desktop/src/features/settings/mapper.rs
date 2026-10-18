//! # Settings Mapper
//!
//! Converts between domain settings models and DTOs.
//!
//! This mapper handles conversion between settings domain models and their DTO representations.
//! Currently, settings use DTOs directly without separate domain models, but this mapper
//! provides a place for future domain model conversion if needed.

use crate::features::settings::dto::{
    ExportSettingsRequestDto, ExportSettingsResponseDto, ImportSettingsRequestDto,
    ImportSettingsResponseDto, ResetSettingsRequestDto, SettingsCategory, SettingsDto,
    UpdateSettingsRequestDto, ValidationResult,
};
use std::collections::HashMap;
use std::path::PathBuf;

/// Mapper for settings-related conversions.
pub struct SettingsMapper;

impl SettingsMapper {
    /// Create an update request DTO.
    ///
    /// # Arguments
    ///
    /// * `category` - Optional category to update
    /// * `updates` - Key-value pairs of updates
    ///
    /// # Returns
    ///
    /// Update request DTO
    pub fn create_update_request(
        category: Option<SettingsCategory>,
        updates: HashMap<String, serde_json::Value>,
    ) -> UpdateSettingsRequestDto {
        UpdateSettingsRequestDto { category, updates }
    }

    /// Create a reset request DTO.
    ///
    /// # Arguments
    ///
    /// * `category` - Optional category to reset (None = reset all)
    ///
    /// # Returns
    ///
    /// Reset request DTO
    pub fn create_reset_request(category: Option<SettingsCategory>) -> ResetSettingsRequestDto {
        ResetSettingsRequestDto { category }
    }

    /// Create an export request DTO.
    ///
    /// # Arguments
    ///
    /// * `path` - Export destination path
    ///
    /// # Returns
    ///
    /// Export request DTO
    pub fn create_export_request(path: String) -> ExportSettingsRequestDto {
        ExportSettingsRequestDto { path }
    }

    /// Create an export response DTO.
    ///
    /// # Arguments
    ///
    /// * `path` - Path where settings were exported
    /// * `status` - Status message
    ///
    /// # Returns
    ///
    /// Export response DTO
    pub fn create_export_response(path: PathBuf, status: String) -> ExportSettingsResponseDto {
        ExportSettingsResponseDto { path, status }
    }

    /// Create an import request DTO.
    ///
    /// # Arguments
    ///
    /// * `path` - Import source path
    /// * `merge` - Whether to merge with existing settings
    ///
    /// # Returns
    ///
    /// Import request DTO
    pub fn create_import_request(path: String, merge: bool) -> ImportSettingsRequestDto {
        ImportSettingsRequestDto { path, merge }
    }

    /// Create an import response DTO.
    ///
    /// # Arguments
    ///
    /// * `settings` - Imported settings
    /// * `status` - Status message
    ///
    /// # Returns
    ///
    /// Import response DTO
    pub fn create_import_response(
        settings: SettingsDto,
        status: String,
    ) -> ImportSettingsResponseDto {
        ImportSettingsResponseDto { settings, status }
    }

    /// Parse settings category from string.
    ///
    /// # Arguments
    ///
    /// * `category_str` - Category name as string
    ///
    /// # Returns
    ///
    /// Settings category enum or None if invalid
    pub fn parse_category(category_str: &str) -> Option<SettingsCategory> {
        category_str.parse().ok()
    }

    /// Convert settings category to string.
    ///
    /// # Arguments
    ///
    /// * `category` - Settings category enum
    ///
    /// # Returns
    ///
    /// Category name as string
    pub fn category_to_string(category: SettingsCategory) -> String {
        category.as_str().to_string()
    }

    /// Extract validation errors as a formatted string.
    ///
    /// # Arguments
    ///
    /// * `validation` - Validation result
    ///
    /// # Returns
    ///
    /// Formatted error message
    pub fn format_validation_errors(validation: &ValidationResult) -> String {
        if !validation.has_errors() {
            return String::new();
        }

        let mut errors = Vec::new();
        for (category, category_errors) in &validation.errors {
            for error in category_errors {
                errors.push(format!("[{}] {}", category, error));
            }
        }

        errors.join("; ")
    }

    /// Extract validation warnings as a formatted string.
    ///
    /// # Arguments
    ///
    /// * `validation` - Validation result
    ///
    /// # Returns
    ///
    /// Formatted warning message
    pub fn format_validation_warnings(validation: &ValidationResult) -> String {
        if !validation.has_warnings() {
            return String::new();
        }

        let mut warnings = Vec::new();
        for (category, category_warnings) in &validation.warnings {
            for warning in category_warnings {
                warnings.push(format!("[{}] {}", category, warning));
            }
        }

        warnings.join("; ")
    }

    /// Create a successful validation result.
    pub fn create_success_validation() -> ValidationResult {
        ValidationResult::success()
    }

    /// Create a failed validation result.
    pub fn create_failure_validation() -> ValidationResult {
        ValidationResult::failure()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_create_update_request() {
        let mut updates = HashMap::new();
        updates.insert("maxResults".to_string(), json!(20));

        let request =
            SettingsMapper::create_update_request(Some(SettingsCategory::Search), updates);

        assert_eq!(request.category, Some(SettingsCategory::Search));
        assert_eq!(request.updates.len(), 1);
    }

    #[test]
    fn test_create_reset_request() {
        let request = SettingsMapper::create_reset_request(Some(SettingsCategory::Indexing));
        assert_eq!(request.category, Some(SettingsCategory::Indexing));

        let request = SettingsMapper::create_reset_request(None);
        assert_eq!(request.category, None);
    }

    #[test]
    fn test_create_export_request() {
        let request = SettingsMapper::create_export_request("/tmp/settings.json".to_string());
        assert_eq!(request.path, "/tmp/settings.json");
    }

    #[test]
    fn test_create_export_response() {
        let response = SettingsMapper::create_export_response(
            PathBuf::from("/tmp/settings.json"),
            "Success".to_string(),
        );
        assert_eq!(response.path, PathBuf::from("/tmp/settings.json"));
        assert_eq!(response.status, "Success");
    }

    #[test]
    fn test_create_import_request() {
        let request = SettingsMapper::create_import_request("/tmp/settings.json".to_string(), true);
        assert_eq!(request.path, "/tmp/settings.json");
        assert!(request.merge);
    }

    #[test]
    fn test_create_import_response() {
        let settings = SettingsDto::default();
        let response = SettingsMapper::create_import_response(settings, "Success".to_string());
        assert_eq!(response.status, "Success");
        assert_eq!(response.settings.indexing.chunk_size, 800);
    }

    #[test]
    fn test_parse_category() {
        assert_eq!(
            SettingsMapper::parse_category("indexing"),
            Some(SettingsCategory::Indexing)
        );
        assert_eq!(
            SettingsMapper::parse_category("search"),
            Some(SettingsCategory::Search)
        );
        assert_eq!(SettingsMapper::parse_category("invalid"), None);
    }

    #[test]
    fn test_category_to_string() {
        assert_eq!(
            SettingsMapper::category_to_string(SettingsCategory::Indexing),
            "indexing"
        );
        assert_eq!(
            SettingsMapper::category_to_string(SettingsCategory::Search),
            "search"
        );
    }

    #[test]
    fn test_format_validation_errors() {
        let mut validation = ValidationResult::success();
        validation.add_error("indexing", "chunk_size must be > 0".to_string());
        validation.add_error("search", "invalid threshold".to_string());

        let formatted = SettingsMapper::format_validation_errors(&validation);

        assert!(formatted.contains("[indexing]"));
        assert!(formatted.contains("[search]"));
        assert!(formatted.contains("chunk_size must be > 0"));
        assert!(formatted.contains("invalid threshold"));
    }

    #[test]
    fn test_format_validation_warnings() {
        let mut validation = ValidationResult::success();
        validation.add_warning("llm", "High temperature value".to_string());

        let formatted = SettingsMapper::format_validation_warnings(&validation);

        assert!(formatted.contains("[llm]"));
        assert!(formatted.contains("High temperature value"));
    }

    #[test]
    fn test_format_empty_errors() {
        let validation = ValidationResult::success();
        let formatted = SettingsMapper::format_validation_errors(&validation);
        assert!(formatted.is_empty());
    }

    #[test]
    fn test_create_validation_results() {
        let success = SettingsMapper::create_success_validation();
        assert!(success.valid);

        let failure = SettingsMapper::create_failure_validation();
        assert!(!failure.valid);
    }
}
