use crate::infrastructure::web::ingestion::types::WebMetadata;
use chrono::{DateTime, Utc};
use scraper::{Html, Selector};
use tracing::{debug, warn};
use url::Url;

pub struct MetadataExtractor;

impl MetadataExtractor {
    pub fn extract(html: &Html, base_url: &str) -> WebMetadata {
        let mut metadata = WebMetadata::new();

        metadata.site_name = Self::extract_site_name(html);
        metadata.author = Self::extract_author(html);
        metadata.published_time = Self::extract_published_time(html);
        metadata.modified_time = Self::extract_modified_time(html);
        metadata.description = Self::extract_description(html);
        metadata.keywords = Self::extract_keywords(html);
        metadata.image_url = Self::extract_image_url(html, base_url);
        metadata.favicon_url = Self::extract_favicon_url(html, base_url);
        metadata.canonical_url = Self::extract_canonical_url(html);

        debug!("Extracted metadata for {}: {:?}", base_url, metadata);

        metadata
    }

    fn extract_site_name(html: &Html) -> Option<String> {
        Self::get_meta_content(html, "og:site_name")
            .or_else(|| Self::get_meta_content(html, "twitter:site"))
            .or_else(|| Self::get_meta_content(html, "application-name"))
    }

    fn extract_author(html: &Html) -> Option<String> {
        Self::get_meta_content(html, "author")
            .or_else(|| Self::get_meta_content(html, "article:author"))
            .or_else(|| Self::get_meta_content(html, "twitter:creator"))
            .or_else(|| Self::extract_byline(html))
    }

    fn extract_byline(html: &Html) -> Option<String> {
        let selectors = [
            "[rel='author']",
            ".author",
            ".byline",
            "[class*='author']",
            "[itemprop='author']",
        ];

        for selector_str in &selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                if let Some(element) = html.select(&selector).next() {
                    let text = element.text().collect::<String>().trim().to_string();
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
            }
        }

        None
    }

    fn extract_published_time(html: &Html) -> Option<DateTime<Utc>> {
        Self::get_meta_content(html, "article:published_time")
            .or_else(|| Self::get_meta_content(html, "datePublished"))
            .or_else(|| Self::get_meta_content(html, "publishdate"))
            .and_then(|s| Self::parse_datetime(&s))
    }

    fn extract_modified_time(html: &Html) -> Option<DateTime<Utc>> {
        Self::get_meta_content(html, "article:modified_time")
            .or_else(|| Self::get_meta_content(html, "dateModified"))
            .or_else(|| Self::get_meta_content(html, "last-modified"))
            .and_then(|s| Self::parse_datetime(&s))
    }

    fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|| {
                DateTime::parse_from_rfc2822(s)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            })
    }

    fn extract_description(html: &Html) -> Option<String> {
        Self::get_meta_content(html, "og:description")
            .or_else(|| Self::get_meta_content(html, "twitter:description"))
            .or_else(|| Self::get_meta_content(html, "description"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn extract_keywords(html: &Html) -> Vec<String> {
        Self::get_meta_content(html, "keywords")
            .map(|s| {
                s.split(',')
                    .map(|k| k.trim().to_string())
                    .filter(|k| !k.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn extract_image_url(html: &Html, base_url: &str) -> Option<String> {
        Self::get_meta_content(html, "og:image")
            .or_else(|| Self::get_meta_content(html, "twitter:image"))
            .or_else(|| Self::get_meta_content(html, "image"))
            .and_then(|url| Self::resolve_url(base_url, &url))
    }

    fn extract_favicon_url(html: &Html, base_url: &str) -> Option<String> {
        let selectors = [
            "link[rel='icon']",
            "link[rel='shortcut icon']",
            "link[rel='apple-touch-icon']",
        ];

        for selector_str in &selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                if let Some(element) = html.select(&selector).next() {
                    if let Some(href) = element.value().attr("href") {
                        if let Some(resolved) = Self::resolve_url(base_url, href) {
                            return Some(resolved);
                        }
                    }
                }
            }
        }

        if let Ok(base) = Url::parse(base_url) {
            if let Some(domain) = base.domain() {
                return Some(format!("https://{}/favicon.ico", domain));
            }
        }

        None
    }

    fn extract_canonical_url(html: &Html) -> Option<String> {
        Self::get_meta_content(html, "og:url")
            .or_else(|| {
                if let Ok(selector) = Selector::parse("link[rel='canonical']") {
                    html.select(&selector)
                        .next()
                        .and_then(|el| el.value().attr("href"))
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
    }

    fn get_meta_content(html: &Html, property: &str) -> Option<String> {
        let selectors = [
            format!("meta[property='{}']", property),
            format!("meta[name='{}']", property),
            format!("meta[itemprop='{}']", property),
        ];

        for selector_str in &selectors {
            if let Ok(selector) = Selector::parse(&selector_str) {
                if let Some(element) = html.select(&selector).next() {
                    if let Some(content) = element.value().attr("content") {
                        let trimmed = content.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
            }
        }

        None
    }

    fn resolve_url(base: &str, relative: &str) -> Option<String> {
        if relative.starts_with("http://") || relative.starts_with("https://") {
            return Some(relative.to_string());
        }

        match Url::parse(base) {
            Ok(base_url) => match base_url.join(relative) {
                Ok(resolved) => Some(resolved.to_string()),
                Err(e) => {
                    warn!("Failed to resolve URL {}/{}: {}", base, relative, e);
                    None
                }
            },
            Err(e) => {
                warn!("Invalid base URL {}: {}", base, e);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_basic_metadata() {
        let html = Html::parse_document(
            r#"
            <html>
            <head>
                <meta property="og:site_name" content="Example Site" />
                <meta name="author" content="John Doe" />
                <meta name="description" content="Test description" />
                <meta name="keywords" content="test, example, metadata" />
            </head>
            </html>
            "#,
        );

        let metadata = MetadataExtractor::extract(&html, "https://example.com");

        assert_eq!(metadata.site_name, Some("Example Site".to_string()));
        assert_eq!(metadata.author, Some("John Doe".to_string()));
        assert_eq!(metadata.description, Some("Test description".to_string()));
        assert_eq!(metadata.keywords.len(), 3);
    }

    #[test]
    fn test_extract_opengraph() {
        let html = Html::parse_document(
            r#"
            <html>
            <head>
                <meta property="og:image" content="https://example.com/image.jpg" />
                <meta property="og:url" content="https://example.com/article" />
            </head>
            </html>
            "#,
        );

        let metadata = MetadataExtractor::extract(&html, "https://example.com");

        assert_eq!(
            metadata.image_url,
            Some("https://example.com/image.jpg".to_string())
        );
        assert_eq!(
            metadata.canonical_url,
            Some("https://example.com/article".to_string())
        );
    }

    #[test]
    fn test_resolve_relative_url() {
        let resolved = MetadataExtractor::resolve_url("https://example.com/page", "/image.jpg");
        assert_eq!(resolved, Some("https://example.com/image.jpg".to_string()));

        let absolute =
            MetadataExtractor::resolve_url("https://example.com", "https://other.com/image.jpg");
        assert_eq!(absolute, Some("https://other.com/image.jpg".to_string()));
    }
}
