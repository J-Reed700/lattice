//! Where an article goes: its domain directory, a filesystem-safe slug, and
//! a fallback title when the extracted one is poor.

use chrono::Utc;
use url::Url;

use super::WebArchiveService;
use lattice_core::shared::error::{AppError, Result};

impl WebArchiveService {
    /// The domain of `url`, for organizing articles
    /// (`https://www.example.com/a` gives `www.example.com`).
    pub(super) fn extract_domain(url: &str) -> Result<String> {
        let parsed =
            Url::parse(url).map_err(|e| AppError::InvalidInput(format!("Invalid URL: {}", e)))?;

        parsed
            .host_str()
            .map(|h| h.to_string())
            .ok_or_else(|| AppError::InvalidInput("URL has no host".to_string()))
    }

    /// Create a filesystem-safe filename slug from article title
    ///
    /// Converts title to lowercase, replaces spaces with hyphens,
    /// removes special characters, and truncates to 50 chars.
    pub(super) fn slugify(title: &str) -> String {
        title
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c
                } else {
                    '-' // Replace whitespace and special chars with dash (will be deduplicated)
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-")
            .chars()
            .take(50)
            .collect()
    }

    /// The extracted title, or one generated from the URL when the extracted one
    /// is generic.
    pub(super) fn generate_fallback_title(url: &str, extracted_title: &str) -> String {
        // Generic/bad titles to detect
        let bad_titles = [
            "the heart of the internet",
            "reddit",
            "loading",
            "error",
            "page not found",
            "untitled",
        ];

        let title_lower = extracted_title.to_lowercase();
        let is_bad_title =
            bad_titles.iter().any(|&bad| title_lower.contains(bad)) || extracted_title.len() < 10;

        if is_bad_title {
            if let Ok(parsed) = Url::parse(url) {
                let domain = parsed.host_str().unwrap_or("unknown");

                // Try to extract last path segment as title
                if let Some(segments) = parsed.path_segments() {
                    let path_parts: Vec<&str> = segments.collect();
                    if let Some(last_segment) = path_parts.last() {
                        if !last_segment.is_empty() && last_segment.len() > 3 {
                            let clean_segment = last_segment
                                .replace(".html", "")
                                .replace(".php", "")
                                .replace("-", " ");
                            return format!("{} - {}", clean_segment, domain);
                        }
                    }
                }

                // Fallback: domain + timestamp
                return format!("{}-{}", domain, Utc::now().timestamp());
            }
        }

        // Title is good enough
        extracted_title.to_string()
    }
}
