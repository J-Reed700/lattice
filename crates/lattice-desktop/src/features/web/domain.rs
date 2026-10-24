//! Web archive domain models.

use chrono::{DateTime, Utc};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::types::DocumentId;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use url::Url;

/// Validate that a domain is safe for use as a directory name.
pub fn validate_domain(domain: &str) -> Result<()> {
    let domain = domain.trim();

    if domain.is_empty() {
        return Err(AppError::InvalidInput("Domain cannot be empty".to_string()));
    }

    if domain.split('/').any(|part| part == ".." || part == ".") {
        return Err(AppError::InvalidInput(
            "Domain cannot contain path traversal sequences (. or ..)".to_string(),
        ));
    }

    if domain.split('\\').any(|part| part == ".." || part == ".") {
        return Err(AppError::InvalidInput(
            "Domain cannot contain path traversal sequences (. or ..)".to_string(),
        ));
    }

    if domain.chars().any(|c| c.is_control()) {
        return Err(AppError::InvalidInput(
            "Domain contains control characters".to_string(),
        ));
    }

    const INVALID_CHARS: &[char] = &['/', '\\', '\0', '<', '>', ':', '"', '|', '?', '*'];
    if let Some(invalid_char) = domain.chars().find(|c| INVALID_CHARS.contains(c)) {
        return Err(AppError::InvalidInput(format!(
            "Domain contains invalid character: '{}'",
            invalid_char
        )));
    }

    if domain.len() > 255 {
        return Err(AppError::InvalidInput(format!(
            "Domain exceeds maximum length of 255 characters (got {})",
            domain.len()
        )));
    }

    if domain.chars().all(|c| c == '.') {
        return Err(AppError::InvalidInput(
            "Domain cannot consist only of dots".to_string(),
        ));
    }

    Ok(())
}

/// Sanitize a title for use in a filename (lowercase alphanumeric + hyphens, max 50 chars).
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
    result.chars().take(50).collect()
}

/// Sanitized, unique file path for storing web articles ({domain}/{sanitized-title}-{short-uuid}.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebArchivePath {
    relative_path: PathBuf,
}

impl WebArchivePath {
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

        let sanitized_title = sanitize_title(title);
        let short_uuid = &doc_id.as_str()[..8];
        let filename = format!("{}-{}.md", sanitized_title, short_uuid);
        let relative_path = PathBuf::from(domain).join(filename);

        Ok(Self { relative_path })
    }

    /// Convert to absolute path given a base directory with traversal prevention.
    pub fn to_absolute(&self, base_dir: &Path) -> Result<PathBuf> {
        use std::path::Component;

        // Validate path components before filesystem operations to prevent TOCTOU
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
                Component::CurDir => continue,
            }
        }

        let canonical_base = base_dir
            .canonicalize()
            .map_err(|e| AppError::InvalidInput(format!("Invalid base directory: {}", e)))?;

        let mut resolved = canonical_base.clone();
        for component in self.relative_path.components() {
            match component {
                Component::Normal(name) => resolved.push(name),
                Component::ParentDir => {
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

        if !resolved.starts_with(&canonical_base) {
            return Err(AppError::Security(
                "Path escapes base directory".to_string(),
            ));
        }

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

    pub fn as_path(&self) -> &Path {
        &self.relative_path
    }

    pub fn as_str(&self) -> &str {
        self.relative_path.to_str().unwrap_or("")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleMetadata {
    pub published_date: Option<DateTime<Utc>>,
    pub word_count: usize,
    pub tags: Vec<String>,
    pub domain: String,
    pub imported_date: DateTime<Utc>,
}

impl ArticleMetadata {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestedContent {
    pub html: String,
    pub text: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub published_date: Option<DateTime<Utc>>,
}

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

    pub fn archive_path(&self) -> Result<WebArchivePath> {
        WebArchivePath::new(&self.source_url, &self.title, &self.doc_id)
    }

    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

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

        output.push_str(&self.content);
        output.push_str("\n\n");

        output.push_str("---\n");
        output.push_str(&format!("Source: {}\n", self.source_url));

        output
    }

    pub fn doc_id(&self) -> &DocumentId {
        &self.doc_id
    }

    pub fn source_url(&self) -> &Url {
        &self.source_url
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn metadata(&self) -> &ArticleMetadata {
        &self.metadata
    }
}

#[cfg(test)]
mod tests;
