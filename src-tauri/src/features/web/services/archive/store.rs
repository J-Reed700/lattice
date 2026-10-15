//! Writing, reading and listing archived articles on disk.

use async_trait::async_trait;
use chrono::Utc;
use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;

use super::{ArticleMetadata, WebArchiveService};
use crate::features::web::WebArchiveServiceTrait;
use lattice_core::application::contracts::web::CleanArticle;
use lattice_core::shared::error::{AppError, Result};

#[async_trait]
impl WebArchiveServiceTrait for WebArchiveService {
    async fn archive_article(&self, article: CleanArticle, url: &str) -> Result<PathBuf> {
        // Ensure archive is initialized
        self.initialize().await?;

        let domain = Self::extract_domain(url)?;
        let domain_dir = self.base_dir.join(&domain);

        fs::create_dir_all(&domain_dir)
            .await
            .map_err(|e| AppError::Io {
                message: format!("Failed to create domain directory: {}", e),
                kind: format!("{:?}", e.kind()),
            })?;

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

        let slug = Self::slugify(&title_to_use);
        let uuid = Uuid::new_v4();
        let dir_name = format!("{}-{}", slug, uuid.simple());
        let article_dir = domain_dir.join(&dir_name);

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

        Self::markdown_to_article(&content).await
    }

    async fn list_articles(&self) -> Result<Vec<PathBuf>> {
        let mut articles = Vec::new();

        // Ensure directory exists
        if !self.base_dir.exists() {
            return Ok(articles);
        }

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

                    if article_path.is_dir() {
                        let article_md = article_path.join("article.md");
                        if article_md.exists() {
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
