//! Web archive service implementation
//!
//! Manages full-fidelity web article archives on the local filesystem.
//!
//! ## Architecture
//!
//! This service provides filesystem-based archival of web articles:
//! - Stores each article in its own directory
//! - Preserves both markdown AND full HTML snapshots
//! - Includes JSON metadata for programmatic access
//! - Organizes articles by domain (e.g., `github.com/`, `arxiv.org/`)
//! - Creates helpful README.md explaining the archive structure
//!
//! ## Storage Structure
//!
//! Each article is stored as a directory:
//! ```
//! web-archive/
//! ├── github.com/
//! │   └── rust-best-practices-abc123/
//! │       ├── article.md          # Markdown with frontmatter
//! │       ├── page.html           # Full HTML snapshot
//! │       └── metadata.json       # Structured metadata
//! ```
//!
//! ### article.md Format
//! ```markdown
//! ---
//! title: "Article Title"
//! author: "Author Name"
//! url: "https://example.com/article"
//! published_date: "2024-01-15T10:30:00Z"
//! word_count: 1500
//! reading_time_minutes: 8
//! archived_at: "2024-01-20T14:22:00Z"
//! ---
//!
//! # Article Title
//!
//! Article content here...
//! ```
//!
//! ### metadata.json Format
//! ```json
//! {
//!   "url": "https://example.com/article",
//!   "title": "Article Title",
//!   "author": "Author Name",
//!   "published_date": "2024-01-15T10:30:00Z",
//!   "archived_at": "2024-01-20T14:22:00Z",
//!   "word_count": 1500,
//!   "reading_time_minutes": 8,
//!   "links": [
//!     "https://example.com/related-article",
//!     "https://example.com/reference"
//!   ]
//! }
//! ```

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;
use url::Url;
use uuid::Uuid;

use crate::features::function_calling::dto::CleanArticle;
use crate::infrastructure::services::traits::WebArchiveServiceTrait;
use crate::shared::error::{AppError, Result};

/// Structured metadata stored in metadata.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleMetadata {
    /// Original URL
    pub url: String,

    /// Article title
    pub title: String,

    /// Author name (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// Publication date (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_date: Option<DateTime<Utc>>,

    /// When article was archived
    pub archived_at: DateTime<Utc>,

    /// Word count
    pub word_count: usize,

    /// Estimated reading time in minutes
    pub reading_time_minutes: i64,

    /// Outgoing links captured from the article HTML
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
}

/// README content explaining the web archive structure
const README_CONTENT: &str = r#"# Recall Web Archive

This directory contains web articles you've imported into Recall.

## Structure

- Articles are organized by domain (e.g., `github.com/`, `arxiv.org/`)
- Each article gets its own directory: `{title-slug}-{uuid}/`
- Each directory contains:
  - `article.md` - Markdown with YAML frontmatter
  - `page.html` - Full HTML snapshot for offline viewing
  - `metadata.json` - Structured metadata

## Article Directory Format

Each article directory contains three files:

### article.md
Markdown with YAML frontmatter:
```markdown
---
title: "Article Title"
author: "Author Name"
url: "https://example.com/article"
published_date: "2024-01-15T10:30:00Z"
word_count: 1500
reading_time_minutes: 8
archived_at: "2024-01-20T14:22:00Z"
---

# Article Title

Article content here...
```

### page.html
Full HTML snapshot with cleaned article content (boilerplate removed).
Open this in a browser for offline reading.

### metadata.json
Structured metadata for programmatic access:
```json
{
  "url": "https://example.com/article",
  "title": "Article Title",
  "author": "Author Name",
  "published_date": "2024-01-15T10:30:00Z",
  "archived_at": "2024-01-20T14:22:00Z",
  "word_count": 1500,
  "reading_time_minutes": 8,
  "links": [
    "https://example.com/related-article",
    "https://example.com/reference"
  ]
}
```

## Editing

You can safely edit these files manually. Changes will be reflected in Recall after re-indexing the directory.

## Backup

To backup your web archive, simply copy this directory to another location.

## Directory Structure Example

```
web-archive/
├── README.md (this file)
├── github.com/
│   ├── rust-best-practices-abc123/
│   │   ├── article.md
│   │   ├── page.html
│   │   └── metadata.json
│   └── tauri-tutorial-def456/
│       ├── article.md
│       ├── page.html
│       └── metadata.json
└── arxiv.org/
    └── machine-learning-paper-ghi789/
        ├── article.md
        ├── page.html
        └── metadata.json
```

## Re-indexing

To make Recall aware of manual changes:
1. Open Recall settings
2. Go to "Indexing" tab
3. Click "Re-index directory"
4. Select this web-archive folder

---

Generated by Recall - Your Personal Knowledge System
https://github.com/RecallGraph/Recall
"#;

/// Web archive service for managing markdown-based article archives
pub struct WebArchiveService {
    /// Base directory for archive (~/.recall/web-archive)
    base_dir: PathBuf,
}

impl WebArchiveService {
    /// Create a new web archive service
    ///
    /// Initializes the archive directory at `~/.recall/web-archive` and
    /// ensures the README.md exists.
    ///
    /// # Returns
    /// `WebArchiveService` instance ready to use
    ///
    /// # Errors
    /// - `AppError::ConfigError` if home directory cannot be determined
    /// - `AppError::Io` if directory creation fails
    pub fn new() -> Result<Self> {
        let base_dir = dirs::home_dir()
            .ok_or_else(|| AppError::InvalidConfig("No home directory found".to_string()))?
            .join(".recall")
            .join("web-archive");

        Ok(Self { base_dir })
    }

    /// Initialize the archive directory structure
    ///
    /// Creates the base directory and README.md if they don't exist.
    /// This is called automatically by commands that use the service.
    ///
    /// # Errors
    /// - `AppError::Io` if directory creation or file writing fails
    pub async fn initialize(&self) -> Result<()> {
        // Create base directory
        fs::create_dir_all(&self.base_dir)
            .await
            .map_err(|e| AppError::Io {
                message: format!("Failed to create archive directory: {}", e),
                kind: format!("{:?}", e.kind()),
            })?;

        // Create README if it doesn't exist
        self.ensure_readme().await?;

        Ok(())
    }

    /// Ensure README.md exists in the archive directory
    async fn ensure_readme(&self) -> Result<()> {
        let readme_path = self.base_dir.join("README.md");

        if !readme_path.exists() {
            fs::write(&readme_path, README_CONTENT)
                .await
                .map_err(|e| AppError::Io {
                    message: format!("Failed to write README: {}", e),
                    kind: format!("{:?}", e.kind()),
                })?;
        }

        Ok(())
    }

    /// Extract domain from URL
    ///
    /// Returns the domain portion of a URL for organizing articles.
    ///
    /// # Example
    /// - `https://github.com/user/repo` → `github.com`
    /// - `https://www.example.com/article` → `www.example.com`
    fn extract_domain(url: &str) -> Result<String> {
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
    fn slugify(title: &str) -> String {
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

    /// Generate a fallback title from URL when extracted title is poor quality
    ///
    /// Detects generic/bad titles and generates meaningful alternatives from the URL.
    ///
    /// # Arguments
    /// - `url` - The source URL to extract a title from
    /// - `extracted_title` - The title extracted by the article parser
    ///
    /// # Returns
    /// Either the original title (if good) or a generated fallback from URL
    fn generate_fallback_title(url: &str, extracted_title: &str) -> String {
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
            // Extract meaningful parts from URL
            if let Ok(parsed) = Url::parse(url) {
                let domain = parsed.host_str().unwrap_or("unknown");

                // Try to extract last path segment as title
                if let Some(segments) = parsed.path_segments() {
                    let path_parts: Vec<&str> = segments.collect();
                    if let Some(last_segment) = path_parts.last() {
                        if !last_segment.is_empty() && last_segment.len() > 3 {
                            // Use last URL segment + domain
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

    /// Convert CleanArticle to markdown with YAML frontmatter
    fn article_to_markdown(article: &CleanArticle, url: &str) -> String {
        let mut markdown = String::new();

        // YAML frontmatter
        markdown.push_str("---\n");
        markdown.push_str(&format!(
            "title: \"{}\"\n",
            article.title.replace('"', "\\\"")
        ));

        if let Some(ref author) = article.author {
            markdown.push_str(&format!("author: \"{}\"\n", author.replace('"', "\\\"")));
        }

        markdown.push_str(&format!("url: \"{}\"\n", url));

        if let Some(ref published) = article.published_date {
            markdown.push_str(&format!("published_date: \"{}\"\n", published.to_rfc3339()));
        }

        markdown.push_str(&format!("word_count: {}\n", article.word_count));
        markdown.push_str(&format!(
            "reading_time_minutes: {}\n",
            article.reading_time_minutes
        ));
        markdown.push_str(&format!("archived_at: \"{}\"\n", Utc::now().to_rfc3339()));
        markdown.push_str("---\n\n");

        // Article content
        markdown.push_str(&format!("# {}\n\n", article.title));

        if let Some(ref author) = article.author {
            markdown.push_str(&format!("*By {}*\n\n", author));
        }

        if let Some(ref excerpt) = article.excerpt {
            markdown.push_str(&format!("> {}\n\n", excerpt));
        }

        markdown.push_str(&article.text_content);

        markdown
    }

    /// Create a standalone HTML snapshot for offline viewing
    ///
    /// Generates a complete HTML document with:
    /// - Metadata in header
    /// - Clean article content
    /// - Simple, readable styling
    fn create_html_snapshot(article: &CleanArticle) -> String {
        let mut html = String::new();

        html.push_str("<!DOCTYPE html>\n");
        html.push_str("<html lang=\"en\">\n");
        html.push_str("<head>\n");
        html.push_str("  <meta charset=\"UTF-8\">\n");
        html.push_str(
            "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n",
        );
        html.push_str(&format!("  <title>{}</title>\n", article.title));

        // Simple, readable CSS
        html.push_str("  <style>\n");
        html.push_str("    body {\n");
        html.push_str("      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;\n");
        html.push_str("      max-width: 800px;\n");
        html.push_str("      margin: 40px auto;\n");
        html.push_str("      padding: 0 20px;\n");
        html.push_str("      line-height: 1.6;\n");
        html.push_str("      color: #333;\n");
        html.push_str("    }\n");
        html.push_str("    .metadata {\n");
        html.push_str("      color: #666;\n");
        html.push_str("      font-size: 0.9em;\n");
        html.push_str("      border-bottom: 1px solid #eee;\n");
        html.push_str("      padding-bottom: 10px;\n");
        html.push_str("      margin-bottom: 30px;\n");
        html.push_str("    }\n");
        html.push_str("    h1 { margin-top: 0; }\n");
        html.push_str("    img { max-width: 100%; height: auto; }\n");
        html.push_str("    pre { background: #f5f5f5; padding: 15px; overflow-x: auto; }\n");
        html.push_str("    code { background: #f5f5f5; padding: 2px 4px; }\n");
        html.push_str("    blockquote { border-left: 4px solid #ddd; margin: 0; padding-left: 20px; color: #666; }\n");
        html.push_str("  </style>\n");
        html.push_str("</head>\n");
        html.push_str("<body>\n");

        // Metadata section
        html.push_str("  <div class=\"metadata\">\n");
        if let Some(ref author) = article.author {
            html.push_str(&format!("    <p><strong>Author:</strong> {}</p>\n", author));
        }
        if let Some(ref published) = article.published_date {
            html.push_str(&format!(
                "    <p><strong>Published:</strong> {}</p>\n",
                published.format("%B %d, %Y")
            ));
        }
        html.push_str(&format!(
            "    <p><strong>Reading time:</strong> {} min</p>\n",
            article.reading_time_minutes
        ));
        html.push_str(&format!(
            "    <p><strong>Word count:</strong> {}</p>\n",
            article.word_count
        ));
        html.push_str(&format!(
            "    <p><strong>Archived:</strong> {}</p>\n",
            Utc::now().format("%B %d, %Y at %H:%M UTC")
        ));
        html.push_str("  </div>\n\n");

        // Article content (use the clean HTML content from article.content)
        html.push_str("  <article>\n");
        html.push_str(&article.content);
        html.push_str("\n  </article>\n");

        html.push_str("</body>\n");
        html.push_str("</html>\n");

        html
    }

    fn extract_links(html: &str, base_url: &str) -> Vec<String> {
        const MAX_LINKS: usize = 200;

        let document = Html::parse_fragment(html);
        let selector = match Selector::parse("a[href]") {
            Ok(selector) => selector,
            Err(_) => return Vec::new(),
        };

        let mut seen = HashSet::new();
        let mut links = Vec::new();

        for element in document.select(&selector) {
            if let Some(href) = element.value().attr("href") {
                if let Some(resolved) = Self::normalize_link(base_url, href) {
                    if seen.insert(resolved.clone()) {
                        links.push(resolved);
                        if links.len() >= MAX_LINKS {
                            break;
                        }
                    }
                }
            }
        }

        links
    }

    fn normalize_link(base_url: &str, href: &str) -> Option<String> {
        let trimmed = href.trim();
        if trimmed.is_empty() {
            return None;
        }

        if trimmed.starts_with('#')
            || trimmed.starts_with("mailto:")
            || trimmed.starts_with("tel:")
            || trimmed.starts_with("javascript:")
        {
            return None;
        }

        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return Some(trimmed.to_string());
        }

        let base = Url::parse(base_url).ok()?;
        base.join(trimmed).ok().map(|url| url.to_string())
    }

    /// Parse markdown file back to CleanArticle
    ///
    /// Extracts YAML frontmatter and markdown content.
    async fn markdown_to_article(content: &str) -> Result<CleanArticle> {
        // Split frontmatter and content
        let parts: Vec<&str> = content.split("---").collect();

        if parts.len() < 3 {
            return Err(AppError::InvalidInput(
                "Invalid markdown format: missing frontmatter".to_string(),
            ));
        }

        let frontmatter = parts
            .get(1)
            .ok_or_else(|| AppError::InvalidInput("Missing frontmatter section".to_string()))?;
        let markdown_content = parts
            .get(2..)
            .map(|slice| slice.join("---").trim().to_string())
            .unwrap_or_default();

        // Parse frontmatter
        let mut title = String::new();
        let mut author = None;
        let mut word_count = 0;
        let mut reading_time = 0;
        let mut published_date = None;

        for line in frontmatter.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');

                match key {
                    "title" => title = value.to_string(),
                    "author" => author = Some(value.to_string()),
                    "word_count" => word_count = value.parse().unwrap_or(0),
                    "reading_time_minutes" => reading_time = value.parse().unwrap_or(0),
                    "published_date" => {
                        published_date = chrono::DateTime::parse_from_rfc3339(value)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc));
                    }
                    _ => {}
                }
            }
        }

        // Extract excerpt (first paragraph or first 200 chars)
        let excerpt = markdown_content
            .lines()
            .skip_while(|l| l.trim().is_empty() || l.starts_with('#'))
            .take_while(|l| !l.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(200)
            .collect::<String>();

        Ok(CleanArticle {
            title,
            author,
            content: markdown_content.clone(), // Use markdown as HTML content
            text_content: markdown_content,
            word_count,
            reading_time_minutes: reading_time,
            published_date,
            excerpt: Some(excerpt),
        })
    }
}

#[async_trait]
impl WebArchiveServiceTrait for WebArchiveService {
    async fn archive_article(&self, article: CleanArticle, url: &str) -> Result<PathBuf> {
        // Ensure archive is initialized
        self.initialize().await?;

        // Extract domain for subdirectory
        let domain = Self::extract_domain(url)?;
        let domain_dir = self.base_dir.join(&domain);

        // Create domain subdirectory
        fs::create_dir_all(&domain_dir)
            .await
            .map_err(|e| AppError::Io {
                message: format!("Failed to create domain directory: {}", e),
                kind: format!("{:?}", e.kind()),
            })?;

        // Validate and generate title
        let title_to_use = Self::generate_fallback_title(url, &article.title);

        // Log if we used fallback
        if title_to_use != article.title {
            tracing::warn!(
                original_title = %article.title,
                fallback_title = %title_to_use,
                url = url,
                "Using fallback title due to poor extraction"
            );
        }

        // Generate article directory: {slug}-{uuid}/
        let slug = Self::slugify(&title_to_use);
        let uuid = Uuid::new_v4();
        let dir_name = format!("{}-{}", slug, uuid.simple());
        let article_dir = domain_dir.join(&dir_name);

        // Create article directory
        fs::create_dir_all(&article_dir)
            .await
            .map_err(|e| AppError::Io {
                message: format!("Failed to create article directory: {}", e),
                kind: format!("{:?}", e.kind()),
            })?;

        // 1. Write article.md (markdown with frontmatter)
        let markdown = Self::article_to_markdown(&article, url);
        let markdown_path = article_dir.join("article.md");
        fs::write(&markdown_path, markdown)
            .await
            .map_err(|e| AppError::Io {
                message: format!("Failed to write article.md: {}", e),
                kind: format!("{:?}", e.kind()),
            })?;

        // 2. Write page.html (full HTML snapshot)
        let html_path = article_dir.join("page.html");
        let html_content = Self::create_html_snapshot(&article);
        fs::write(&html_path, html_content)
            .await
            .map_err(|e| AppError::Io {
                message: format!("Failed to write page.html: {}", e),
                kind: format!("{:?}", e.kind()),
            })?;

        // 3. Write metadata.json
        let metadata = ArticleMetadata {
            url: url.to_string(),
            title: article.title.clone(),
            author: article.author.clone(),
            published_date: article.published_date,
            archived_at: Utc::now(),
            word_count: article.word_count,
            reading_time_minutes: article.reading_time_minutes,
            links: Self::extract_links(&article.content, url),
        };
        let metadata_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize metadata: {}", e)))?;
        let metadata_path = article_dir.join("metadata.json");
        fs::write(&metadata_path, metadata_json)
            .await
            .map_err(|e| AppError::Io {
                message: format!("Failed to write metadata.json: {}", e),
                kind: format!("{:?}", e.kind()),
            })?;

        // Return path to article.md (not directory) for indexing
        Ok(markdown_path)
    }

    async fn load_article(&self, path: &Path) -> Result<CleanArticle> {
        // Determine if this is a directory (new format) or file (legacy format)
        let article_md_path = if path.is_dir() {
            // New format: path/article.md
            path.join("article.md")
        } else {
            // Legacy format: direct .md file
            path.to_path_buf()
        };

        // Read markdown file
        let content = fs::read_to_string(&article_md_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::NotFound(format!("Article not found: {}", path.display()))
            } else {
                AppError::Io {
                    message: format!("Failed to read article: {}", e),
                    kind: format!("{:?}", e.kind()),
                }
            }
        })?;

        // Parse markdown
        Self::markdown_to_article(&content).await
    }

    async fn delete_article(&self, path: &Path) -> Result<()> {
        // Determine what to delete based on path type
        let path_to_delete = if path.is_dir() {
            // Directory path: remove entire directory (legacy)
            path.to_path_buf()
        } else if path.file_name() == Some(std::ffi::OsStr::new("article.md")) {
            // article.md path: remove parent directory (new format)
            path.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| path.to_path_buf())
        } else {
            // Single file: remove just the file (legacy)
            match fs::remove_file(path).await {
                Ok(()) => return Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => {
                    return Err(AppError::Io {
                        message: format!("Failed to delete article file: {}", e),
                        kind: format!("{:?}", e.kind()),
                    })
                }
            }
        };

        // Remove directory
        match fs::remove_dir_all(&path_to_delete).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::Io {
                message: format!("Failed to delete article directory: {}", e),
                kind: format!("{:?}", e.kind()),
            }),
        }
    }

    async fn list_articles(&self) -> Result<Vec<PathBuf>> {
        let mut articles = Vec::new();

        // Ensure directory exists
        if !self.base_dir.exists() {
            return Ok(articles);
        }

        // Read all entries in base directory
        let mut entries = fs::read_dir(&self.base_dir)
            .await
            .map_err(|e| AppError::Io {
                message: format!("Failed to read archive directory: {}", e),
                kind: format!("{:?}", e.kind()),
            })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| AppError::Io {
            message: format!("Failed to read directory entry: {}", e),
            kind: format!("{:?}", e.kind()),
        })? {
            let path = entry.path();

            // Skip README.md
            if path.file_name() == Some(std::ffi::OsStr::new("README.md")) {
                continue;
            }

            // If it's a directory (domain directory like github.com/)
            if path.is_dir() {
                let mut domain_entries = fs::read_dir(&path).await.map_err(|e| AppError::Io {
                    message: format!("Failed to read domain directory: {}", e),
                    kind: format!("{:?}", e.kind()),
                })?;

                while let Some(domain_entry) =
                    domain_entries
                        .next_entry()
                        .await
                        .map_err(|e| AppError::Io {
                            message: format!("Failed to read domain entry: {}", e),
                            kind: format!("{:?}", e.kind()),
                        })?
                {
                    let article_path = domain_entry.path();

                    // Check if this is an article directory (contains article.md)
                    if article_path.is_dir() {
                        let article_md = article_path.join("article.md");
                        if article_md.exists() {
                            // Return path to article.md (new format)
                            articles.push(article_md);
                        }
                    } else if article_path.extension() == Some(std::ffi::OsStr::new("md")) {
                        // Legacy: Support old single-file markdown format
                        articles.push(article_path);
                    }
                }
            }
        }

        Ok(articles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            WebArchiveService::extract_domain("https://github.com/user/repo").unwrap(),
            "github.com"
        );
        assert_eq!(
            WebArchiveService::extract_domain("https://www.example.com/article").unwrap(),
            "www.example.com"
        );
        assert_eq!(
            WebArchiveService::extract_domain("http://arxiv.org/abs/1234").unwrap(),
            "arxiv.org"
        );
    }

    #[test]
    fn test_slugify() {
        assert_eq!(
            WebArchiveService::slugify("Rust Best Practices"),
            "rust-best-practices"
        );
        assert_eq!(
            WebArchiveService::slugify("Hello, World! 123"),
            "hello-world-123"
        );
        assert_eq!(
            WebArchiveService::slugify("Multiple   Spaces"),
            "multiple-spaces"
        );

        // Test truncation
        let long_title = "a".repeat(100);
        assert_eq!(WebArchiveService::slugify(&long_title).len(), 50);
    }

    #[test]
    fn test_article_to_markdown() {
        let article = CleanArticle {
            title: "Test Article".to_string(),
            author: Some("Test Author".to_string()),
            content: "<p>Content</p>".to_string(),
            text_content: "Content here".to_string(),
            word_count: 2,
            reading_time_minutes: 1,
            published_date: None,
            excerpt: Some("Brief excerpt".to_string()),
        };

        let markdown = WebArchiveService::article_to_markdown(&article, "https://example.com");

        assert!(markdown.contains("---"));
        assert!(markdown.contains("title: \"Test Article\""));
        assert!(markdown.contains("author: \"Test Author\""));
        assert!(markdown.contains("url: \"https://example.com\""));
        assert!(markdown.contains("word_count: 2"));
        assert!(markdown.contains("# Test Article"));
        assert!(markdown.contains("*By Test Author*"));
        assert!(markdown.contains("> Brief excerpt"));
        assert!(markdown.contains("Content here"));
    }

    #[tokio::test]
    async fn test_markdown_to_article() {
        let markdown = r#"---
title: "Test Article"
author: "Test Author"
url: "https://example.com"
word_count: 150
reading_time_minutes: 1
archived_at: "2024-01-20T14:22:00Z"
---

# Test Article

*By Test Author*

> Brief excerpt

Article content here.
"#;

        let article = WebArchiveService::markdown_to_article(markdown)
            .await
            .unwrap();

        assert_eq!(article.title, "Test Article");
        assert_eq!(article.author, Some("Test Author".to_string()));
        assert_eq!(article.word_count, 150);
        assert_eq!(article.reading_time_minutes, 1);
    }

    #[test]
    fn test_generate_fallback_title_with_bad_title() {
        // Test generic bad title
        let result = WebArchiveService::generate_fallback_title(
            "https://reddit.com/r/rust/comments/abc123/some_discussion",
            "The heart of the internet",
        );
        assert!(result.contains("reddit.com"));
        assert_ne!(result, "The heart of the internet");

        // Test short title
        let result =
            WebArchiveService::generate_fallback_title("https://example.com/article", "Short");
        assert!(result.contains("example.com"));

        // Test URL with path segment
        let result = WebArchiveService::generate_fallback_title(
            "https://github.com/rust-lang/rust/pull/12345",
            "Loading",
        );
        assert!(result.contains("12345"));
        assert!(result.contains("github.com"));
    }

    #[test]
    fn test_generate_fallback_title_with_good_title() {
        // Good title should pass through unchanged
        let good_title = "A Comprehensive Guide to Rust Programming";
        let result = WebArchiveService::generate_fallback_title("https://example.com", good_title);
        assert_eq!(result, good_title);
    }
}
