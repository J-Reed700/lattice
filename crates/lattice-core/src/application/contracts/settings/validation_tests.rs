use super::SettingsDto;

#[test]
fn default_settings_validate_without_errors() {
    let result = SettingsDto::default().validate();

    assert!(result.valid);
    assert!(!result.has_errors());
}

#[test]
fn invalid_values_are_reported_under_their_category() {
    let mut settings = SettingsDto::default();
    settings.indexing.chunk_size = 0;
    settings.search.similarity_threshold = 1.5;

    let result = settings.validate();

    assert!(!result.valid);
    assert!(result.has_errors());
    assert!(result.errors.contains_key("indexing"));
    assert!(result.errors.contains_key("search"));
}

#[test]
fn a_very_large_chunk_size_is_a_warning_not_an_error() {
    let mut settings = SettingsDto::default();
    settings.indexing.chunk_size = 15000;

    let result = settings.validate();

    assert!(result.valid);
    assert!(result.has_warnings());
    assert!(result.warnings.contains_key("indexing"));
}
