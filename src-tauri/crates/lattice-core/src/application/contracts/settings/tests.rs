//! Tests for settings defaults, serialization, categories and validation results.

#![cfg(test)]

use super::*;

#[test]
fn test_settings_dto_default() {
    let settings = SettingsDto::default();

    assert_eq!(settings.indexing.chunk_size, 800);
    assert_eq!(settings.search.max_results, 10);
    assert_eq!(
        settings.search.vector_index_compression.mode,
        VectorIndexCompressionModeDto::None
    );
    assert_eq!(
        settings.search.vector_index_compression.dims,
        DEFAULT_VECTOR_COMPRESSION_DIMS
    );
    assert_eq!(settings.llm.model, "llama3.2:latest");
    assert!(settings.llm.verification.enabled);
    // A judge that samples is a judge that disagrees with itself.
    assert_eq!(settings.llm.verification.temperature, 0.0);
    assert_eq!(settings.llm.verification.top_k, 1);
    // Answers sample with the Qwen 3.5 card's non-thinking profile and
    // never think unless the user turns it on.
    assert_eq!(
        (
            settings.llm.temperature,
            settings.llm.top_p,
            settings.llm.top_k,
            settings.llm.presence_penalty,
            settings.llm.repeat_penalty
        ),
        (0.7, 0.8, 20, 1.5, 1.0)
    );
    assert!(!settings.llm.answer_thinking);
    assert_eq!(
        settings.llm.thinking_sampling,
        SamplingProfileDto {
            temperature: 1.0,
            top_p: 0.95,
            top_k: 20,
            presence_penalty: 1.5,
            repeat_penalty: 1.0,
        }
    );
    assert_eq!(
        settings.llm.speculative_decoding,
        SpeculativeDecodingDto::Off
    );
    assert_eq!(settings.ui.theme, "system");
}

#[test]
fn test_settings_dto_serialization() {
    let settings = SettingsDto::default();
    let json = serde_json::to_string(&settings).unwrap();
    let deserialized: SettingsDto = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.indexing.chunk_size, 800);
}

#[test]
fn test_settings_category_from_str() {
    assert_eq!("indexing".parse(), Ok(SettingsCategory::Indexing));
    assert_eq!("search".parse(), Ok(SettingsCategory::Search));
    assert!("invalid".parse::<SettingsCategory>().is_err());
}

#[test]
fn test_settings_category_as_str() {
    assert_eq!(SettingsCategory::Indexing.as_str(), "indexing");
    assert_eq!(SettingsCategory::Search.as_str(), "search");
}

#[test]
fn test_validation_result_success() {
    let result = ValidationResult::success();
    assert!(result.valid);
    assert!(!result.has_errors());
    assert!(!result.has_warnings());
}

#[test]
fn test_validation_result_add_error() {
    let mut result = ValidationResult::success();
    result.add_error("indexing", "Invalid chunk size".to_string());

    assert!(!result.valid);
    assert!(result.has_errors());
    assert_eq!(result.errors.get("indexing").unwrap().len(), 1);
}

#[test]
fn test_validation_result_add_warning() {
    let mut result = ValidationResult::success();
    result.add_warning("llm", "High temperature value".to_string());

    assert!(result.valid);
    assert!(result.has_warnings());
    assert_eq!(result.warnings.get("llm").unwrap().len(), 1);
}
