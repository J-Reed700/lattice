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

/// Settings that only existed to cap what a model is given were removed: the
/// KB result clamp, the tool-result ceiling and result count, and the web page
/// count and per-page ceiling. A settings file written before that still has
/// them, and must still load; serde ignores the keys it no longer knows.
#[test]
fn a_settings_file_with_the_removed_cap_settings_still_loads() {
    let mut value = serde_json::to_value(SettingsDto::default()).unwrap();
    let tuning = value
        .pointer_mut("/search/retrievalTuning")
        .and_then(serde_json::Value::as_object_mut)
        .expect("retrieval tuning is serialized under search");
    tuning.insert("kbSearchMinLimit".into(), serde_json::json!(16));
    tuning.insert("kbSearchMaxLimit".into(), serde_json::json!(48));
    tuning.insert("webFetchPageCount".into(), serde_json::json!(3));
    tuning.insert("webPageMaxChars".into(), serde_json::json!(50_000));
    let tool_output = value
        .pointer_mut("/llm/toolOutput")
        .and_then(serde_json::Value::as_object_mut)
        .expect("tool output is serialized under llm");
    tool_output.insert("maxChars".into(), serde_json::json!(50_000));
    tool_output.insert("maxResults".into(), serde_json::json!(12));

    let loaded: SettingsDto = serde_json::from_value(value).unwrap();
    assert_eq!(
        loaded.llm.tool_output.excerpt_chars,
        SettingsDto::default().llm.tool_output.excerpt_chars
    );
}

/// The page count's 0 meant "search snippets only"; that choice is a switch
/// now. A settings file without it reads pages, and one that turned it off
/// keeps it off.
#[test]
fn reading_web_pages_is_a_switch_that_defaults_on() {
    let mut value = serde_json::to_value(SettingsDto::default()).unwrap();
    let tuning = value
        .pointer_mut("/search/retrievalTuning")
        .and_then(serde_json::Value::as_object_mut)
        .expect("retrieval tuning is serialized under search");
    assert_eq!(tuning.remove("readWebPages"), Some(serde_json::json!(true)));
    let loaded: SettingsDto = serde_json::from_value(value.clone()).unwrap();
    assert!(loaded.search.retrieval_tuning.read_web_pages);

    if let Some(tuning) = value
        .pointer_mut("/search/retrievalTuning")
        .and_then(serde_json::Value::as_object_mut)
    {
        tuning.insert("readWebPages".into(), serde_json::json!(false));
    }
    let loaded: SettingsDto = serde_json::from_value(value).unwrap();
    assert!(!loaded.search.retrieval_tuning.read_web_pages);
}

/// The Learning model is unset until the user chooses one. A settings file
/// written before the field existed loads with it unset, and a chosen model
/// survives the round trip under `llm.learningModel`.
#[test]
fn the_learning_model_loads_with_or_without_the_field() {
    use crate::domain::models::model_ref::ModelRef;

    let mut value = serde_json::to_value(SettingsDto::default()).unwrap();
    let llm = value
        .pointer_mut("/llm")
        .and_then(serde_json::Value::as_object_mut)
        .expect("llm settings are serialized under llm");
    assert_eq!(llm.remove("learningModel"), Some(serde_json::Value::Null));
    let loaded: SettingsDto = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(loaded.llm.learning_model, None);

    let chosen = serde_json::json!({"kind": "local", "model_id": "qwen3.5-27b"});
    value["llm"]["learningModel"] = chosen.clone();
    let loaded: SettingsDto = serde_json::from_value(value).unwrap();
    assert_eq!(
        loaded.llm.learning_model,
        Some(ModelRef::Local {
            model_id: "qwen3.5-27b".into()
        })
    );
    assert_eq!(
        serde_json::to_value(&loaded).unwrap()["llm"]["learningModel"],
        chosen
    );
}
