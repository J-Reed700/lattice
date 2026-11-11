//! URL identity for caches and request-local deduplication.
//!
//! `url::Url` canonicalizes the scheme and host (and removes a default port)
//! while preserving the case and slash structure of the path and query.

/// Return a stable identity for a parsed URL without its fragment.
///
/// Fragments identify a location in a fetched representation rather than a
/// distinct HTTP resource. Invalid URLs are returned trimmed but otherwise
/// untouched so malformed values cannot collide by case folding.
pub fn identity(input: &str) -> String {
    let trimmed = input.trim();
    let Ok(mut parsed) = url::Url::parse(trimmed) else {
        return trimmed.to_owned();
    };
    parsed.set_fragment(None);
    parsed.to_string()
}

/// The key a conversation files a cited web page under, in both the text
/// snapshot (`conversation_web_sources.normalized_url`) and the frozen copy
/// (`web_page_snapshots.normalized_url`): trimmed and lowercased, nothing
/// else, so the two tables always agree on which page is which.
pub fn normalize_web_source_url(url: &str) -> String {
    url.trim().to_ascii_lowercase()
}

/// The host a URL names, or `None` when it will not parse or has no host.
pub fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::{host_of, identity, normalize_web_source_url};

    #[test]
    fn the_host_of_a_url_is_its_lower_case_host_without_port_or_path() {
        assert_eq!(
            host_of("https://Example.com:8080/a?b=c").as_deref(),
            Some("example.com")
        );
    }

    #[test]
    fn a_url_that_will_not_parse_or_has_no_host_has_no_host() {
        assert_eq!(host_of("not a url"), None);
        assert_eq!(host_of("mailto:someone@example.com"), None);
    }

    #[test]
    fn canonicalizes_only_url_components_with_defined_equivalence() {
        assert_eq!(
            identity(" HTTPS://Example.COM:443/a/Case/?Key=Value#section "),
            "https://example.com/a/Case/?Key=Value"
        );
        assert_ne!(
            identity("https://example.com/a"),
            identity("https://example.com/a/")
        );
        assert_ne!(
            identity("https://example.com/Case"),
            identity("https://example.com/case")
        );
        assert_ne!(
            identity("https://example.com/?Q=A"),
            identity("https://example.com/?q=a")
        );
    }

    #[test]
    fn normalize_web_source_url_trims_and_lowercases() {
        assert_eq!(
            normalize_web_source_url(" HTTPS://Example.COM/Path/To/Resource "),
            "https://example.com/path/to/resource"
        );
    }
}
