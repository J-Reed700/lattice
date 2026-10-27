//! Web archive domain models: [`WebArchivePath`], a sanitized unique file
//! path, and [`WebArticleAggregate`], an ingested article with its metadata
//! and content.

use chrono::{DateTime, Utc};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::types::DocumentId;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use url::Url;

/// Validates that a domain is safe as a directory name.
///
/// # Errors
///
/// Returns `AppError::InvalidInput` if domain contains:
/// - Path traversal sequences (.., .)
/// - Filesystem special characters (/, \, :, etc.)
/// - Empty or overly long strings
pub fn validate_domain(domain: &str) -> Result<()> {
    // Trim whitespace and check for empty
    let domain = domain.trim();

    if domain.is_empty() {
        return Err(AppError::InvalidInput("Domain cannot be empty".to_string()));
    }

    // Reject path traversal - component-based check is more robust
    if domain.split('/').any(|part| part == ".." || part == ".") {
        return Err(AppError::InvalidInput(
            "Domain cannot contain path traversal sequences (. or ..)".to_string(),
        ));
    }

    // Also check for backslash-separated (Windows-style)
    if domain.split('\\').any(|part| part == ".." || part == ".") {
        return Err(AppError::InvalidInput(
            "Domain cannot contain path traversal sequences (. or ..)".to_string(),
        ));
    }

    // Reject control characters (including null byte)
    if domain.chars().any(|c| c.is_control()) {
        return Err(AppError::InvalidInput(
            "Domain contains control characters".to_string(),
        ));
    }

    // Reject filesystem special characters
    const INVALID_CHARS: &[char] = &['/', '\\', '\0', '<', '>', ':', '"', '|', '?', '*'];
    if let Some(invalid_char) = domain.chars().find(|c| INVALID_CHARS.contains(c)) {
        return Err(AppError::InvalidInput(format!(
            "Domain contains invalid character: '{}'",
            invalid_char
        )));
    }

    // Reject overly long domains (filesystem limits)
    if domain.len() > 255 {
        return Err(AppError::InvalidInput(format!(
            "Domain exceeds maximum length of 255 characters (got {})",
            domain.len()
        )));
    }

    // Reject domains that are just dots
    if domain.chars().all(|c| c == '.') {
        return Err(AppError::InvalidInput(
            "Domain cannot consist only of dots".to_string(),
        ));
    }

    Ok(())
}

/// Sanitizes a title for a filename: lowercase alphanumerics and single
/// hyphens, at most 50 characters.
pub fn sanitize_title(title: &str) -> String {
    let sanitized = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>();

    let mut result = String::new();
    let mut prev_hyphen = false;
    for c in sanitized.chars() {
        if c == '-' {
            if !prev_hyphen {
                result.push(c);
                prev_hyphen = true;
            }
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }

    let result = result.trim_matches('-');

    // Truncate to max 50 characters
    result.chars().take(50).collect()
}

/// A sanitized, unique file path for a web article:
/// `{domain}/{sanitized-title}-{short-uuid}.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebArchivePath {
    relative_path: PathBuf,
}

impl WebArchivePath {
    /// Create new web archive path from URL, title, and document ID.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - URL has no host
    /// - Title is empty
    ///
    /// # Invariants
    ///
    /// - Generated path is unique (uses doc_id)
    /// - Filename is sanitized
    pub fn new(url: &Url, title: &str, doc_id: &DocumentId) -> Result<Self> {
        let domain = url
            .host_str()
            .ok_or_else(|| AppError::InvalidInput("URL must have a host".to_string()))?;

        validate_domain(domain)?;

        if title.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Article title cannot be empty".to_string(),
            ));
        }

        // Sanitize title
        let sanitized_title = sanitize_title(title);

        let short_uuid = &doc_id.as_str()[..8];

        // Format: {domain}/{sanitized-title}-{short-uuid}.md
        let filename = format!("{}-{}.md", sanitized_title, short_uuid);
        let relative_path = PathBuf::from(domain).join(filename);

        Ok(Self { relative_path })
    }

    /// Converts to an absolute path under `base_dir`, validating every component
    /// before creating any directory (CWE-22, TOCTOU).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Base directory is invalid or doesn't exist
    /// - Path contains traversal attempts (..)
    /// - Path escapes the base directory (directory traversal attempt)
    /// - Parent directory cannot be created
    pub fn to_absolute(&self, base_dir: &Path) -> Result<PathBuf> {
        use std::path::Component;

        // SECURITY: Validate path components BEFORE any filesystem operations
        // This prevents TOCTOU (Time-of-Check Time-of-Use) vulnerabilities
        for component in self.relative_path.components() {
            match component {
                Component::Normal(_) => continue,
                Component::ParentDir => {
                    return Err(AppError::Security(
                        "Path traversal detected: relative path contains ..".to_string(),
                    ));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(AppError::InvalidInput(
                        "Relative path cannot contain absolute components".to_string(),
                    ));
                }
                Component::CurDir => continue, // Allow "." (current dir)
            }
        }

        // Canonicalize base directory (must exist)
        let canonical_base = base_dir
            .canonicalize()
            .map_err(|e| AppError::InvalidInput(format!("Invalid base directory: {}", e)))?;

        // Manually resolve relative path against canonical base
        // We can't use canonicalize on the full path since it might not exist yet
        let mut resolved = canonical_base.clone();
        for component in self.relative_path.components() {
            match component {
                Component::Normal(name) => resolved.push(name),
                Component::ParentDir => {
                    // Already validated above, this is defensive redundancy
                    return Err(AppError::Security("Unexpected .. in path".to_string()));
                }
                Component::CurDir => continue,
                _ => {
                    return Err(AppError::InvalidInput(
                        "Unexpected path component".to_string(),
                    ));
                }
            }
        }

        // Final security check: resolved path must be within base directory
        if !resolved.starts_with(&canonical_base) {
            return Err(AppError::Security(
                "Path escapes base directory".to_string(),
            ));
        }

        // NOW it's safe to create directories (validation happened first)
        if let Some(parent) = resolved.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| AppError::Io {
                    message: format!("Failed to create directory: {}", e),
                    kind: e.kind().to_string(),
                })?;
            }
        }

        Ok(resolved)
    }

    /// Get the relative path.
    pub fn as_path(&self) -> &Path {
        &self.relative_path
    }

    /// Get the relative path as a string.
    pub fn as_str(&self) -> &str {
        self.relative_path.to_str().unwrap_or("")
    }
}

/// Article metadata extracted from web content.
///
/// Contains optional fields that may or may not be present
/// depending on the source article.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleMetadata {
    /// Published date (if available)
    pub published_date: Option<DateTime<Utc>>,

    /// Word count of article content
    pub word_count: usize,

    /// Tags or keywords (if extracted)
    pub tags: Vec<String>,

    /// Domain of source URL
    pub domain: String,

    /// Date when article was imported
    pub imported_date: DateTime<Utc>,
}

impl ArticleMetadata {
    /// Create metadata from content and URL.
    fn from_content(content: &str, url: &Url, published_date: Option<DateTime<Utc>>) -> Self {
        let word_count = content.split_whitespace().count();
        let domain = url.host_str().unwrap_or("unknown").to_string();

        Self {
            published_date,
            word_count,
            tags: Vec::new(),
            domain,
            imported_date: Utc::now(),
        }
    }
}

/// Content ingested from a web page.
///
/// Contains the raw HTML, extracted text, and metadata
/// from the web capture process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestedContent {
    /// Raw HTML content
    pub html: String,

    /// Extracted text content
    pub text: String,

    /// Article title (if extracted)
    pub title: Option<String>,

    /// Article author (if extracted)
    pub author: Option<String>,

    /// Published date (if extracted)
    pub published_date: Option<DateTime<Utc>>,
}

/// An ingested web article, ready to be archived as a markdown file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebArticleAggregate {
    doc_id: DocumentId,
    source_url: Url,
    title: String,
    author: Option<String>,
    content: String,
    metadata: ArticleMetadata,
}

impl WebArticleAggregate {
    /// Create from ingested content.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Content text is empty
    /// - Title is missing or empty
    ///
    /// # Invariants
    ///
    /// - Generated doc_id is unique
    /// - Content is not empty
    /// - Title is not empty
    pub fn from_ingestion(url: Url, content: IngestedContent) -> Result<Self> {
        if content.text.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Article content cannot be empty".to_string(),
            ));
        }

        let title = content
            .title
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| AppError::InvalidInput("Article title is required".to_string()))?;

        let doc_id = DocumentId::new();

        let metadata = ArticleMetadata::from_content(&content.text, &url, content.published_date);

        Ok(Self {
            doc_id,
            source_url: url,
            title,
            author: content.author,
            content: content.text,
            metadata,
        })
    }

    /// Generate archive file path.
    ///
    /// # Errors
    ///
    /// Returns an error if path generation fails (invalid URL or title).
    pub fn archive_path(&self) -> Result<WebArchivePath> {
        WebArchivePath::new(&self.source_url, &self.title, &self.doc_id)
    }

    /// Serializes to markdown with YAML frontmatter.
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

        // YAML frontmatter
        output.push_str("---\n");
        output.push_str(&format!("source_url: {}\n", self.source_url));
        output.push_str(&format!("title: {}\n", self.title));

        if let Some(ref author) = self.author {
            output.push_str(&format!("author: {}\n", author));
        }

        output.push_str(&format!(
            "imported_date: {}\n",
            self.metadata.imported_date.to_rfc3339()
        ));
        output.push_str(&format!("domain: {}\n", self.metadata.domain));

        if let Some(published) = self.metadata.published_date {
            output.push_str(&format!("published_date: {}\n", published.to_rfc3339()));
        }

        output.push_str(&format!("word_count: {}\n", self.metadata.word_count));
        output.push_str("---\n\n");

        // Content
        output.push_str(&self.content);
        output.push_str("\n\n");

        // Footer with source attribution
        output.push_str("---\n");
        output.push_str(&format!("Source: {}\n", self.source_url));

        output
    }

    /// Get document ID.
    pub fn doc_id(&self) -> &DocumentId {
        &self.doc_id
    }

    /// Get source URL.
    pub fn source_url(&self) -> &Url {
        &self.source_url
    }

    /// Get article title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Get article author.
    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    /// Get article content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get article metadata.
    pub fn metadata(&self) -> &ArticleMetadata {
        &self.metadata
    }
}

#[cfg(test)]
mod tests;
