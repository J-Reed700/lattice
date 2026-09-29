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

#[cfg(test)]
mod tests {
    use super::identity;

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
}
