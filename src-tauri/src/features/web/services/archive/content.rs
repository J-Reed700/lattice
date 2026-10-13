//! An article's files: markdown with YAML frontmatter and back, the HTML
//! snapshot, and the outgoing links recorded in its metadata.

use chrono::Utc;
use scraper::{Html, Selector};
use std::collections::HashSet;
use url::Url;

use super::WebArchiveService;
use crate::application::contracts::web::CleanArticle;
use crate::shared::error::{AppError, Result};

impl WebArchiveService {
    /// Convert CleanArticle to markdown with YAML frontmatter
    pub(super) fn article_to_markdown(article: &CleanArticle, url: &str) -> String {
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
    pub(super) fn create_html_snapshot(article: &CleanArticle) -> String {
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

    pub(super) fn extract_links(html: &str, base_url: &str) -> Vec<String> {
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
    pub(super) async fn markdown_to_article(content: &str) -> Result<CleanArticle> {
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
