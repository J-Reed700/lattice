use super::{SettingsDto, VectorIndexCompressionModeDto, MAX_VECTOR_COMPRESSION_DIMS};

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

/// The dims are remembered while the mode is `none`, so validation only
/// reads them when truncation is actually on. Rejecting a remembered value
/// nothing consumes would make the whole settings document unsaveable.
#[test]
fn vector_compression_dims_are_only_checked_when_truncation_is_on() {
    let mut settings = SettingsDto::default();
    settings.search.vector_index_compression.mode = VectorIndexCompressionModeDto::None;
    settings.search.vector_index_compression.dims = 1;
    assert!(settings.validate().valid);

    settings.search.vector_index_compression.mode = VectorIndexCompressionModeDto::Truncated;
    let rejected = settings.validate();
    assert!(!rejected.valid);
    assert!(rejected
        .errors
        .values()
        .flatten()
        .any(|message| message.contains("vector_index_compression.dims")));

    settings.search.vector_index_compression.dims = 512;
    assert!(settings.validate().valid);

    settings.search.vector_index_compression.dims = MAX_VECTOR_COMPRESSION_DIMS + 1;
    assert!(!settings.validate().valid);
}

/// The page-fetch timeout bounds the pipeline's own page reads. With page
/// reading off nothing uses it, and a 0 left there made every save fail.
#[test]
fn the_page_fetch_timeout_is_only_checked_when_pages_are_read() {
    let mut settings = SettingsDto::default();
    settings.search.retrieval_tuning.read_web_pages = false;
    settings.search.retrieval_tuning.web_page_fetch_timeout_secs = 0;
    assert!(settings.validate().valid);

    settings.search.retrieval_tuning.read_web_pages = true;
    let rejected = settings.validate();
    assert!(!rejected.valid);
    assert!(rejected
        .errors
        .values()
        .flatten()
        .any(|message| message.contains("web_page_fetch_timeout_secs")));
}
