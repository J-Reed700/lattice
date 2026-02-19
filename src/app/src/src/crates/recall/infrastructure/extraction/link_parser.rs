//! Wikilink parser for extracting `[[links]]` from markdown documents.
//!
//! Supports multiple wikilink formats:
//! - `[[note]]`: Simple link to note
//! - `[[note|display text]]`: Link with custom display text
//! - `[[note#header]]`: Link to specific header
//! - `[[note#header|display]]`: Link to header with display text
//! - `[[path/to/note]]`: Link with relative path

use lazy_regex::regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::warn;

/// Compiled regex for matching wikilinks (compile-time verified).
/// Pattern: `[[target#header|display]]`
///   - Group 1: target (required)
///   - Group 2: header (optional, after #)
///   - Group 3: display text (optional, after |)
///
/// Note: lazy_regex::regex! validates at compile-time, eliminating runtime panics
fn wikilink_regex() -> &'static regex::Regex {
    regex!(r"\[\[([^\]|#]+)(?:#([^\]|]+))?(?:\|([^\]]+))?\]\]")
}

/// Represents a `[[wikilink]]` in markdown.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WikiLink {
    /// File containing the link
    pub source_path: String,

    /// Target note path/title
    pub target: String,

    /// Custom display text (from `[[target|display]]`)
    pub display_text: Option<String>,

    /// Header anchor (from `[[target#header]]`)
    pub header: Option<String>,

    /// Line number in source file
    pub line_number: usize,

    /// Surrounding text for preview
    pub context: String,
}

/// Document metadata for link resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentInfo {
    pub file_path: String,
    pub title: Option<String>,
}

/// Parser for extracting wikilinks from markdown documents.
///
/// # Example
///
/// ```
/// use vault_desktop::extraction::LinkParser;
///
/// let parser = LinkParser::new();
/// let content = "See [[todo]] and [[projects/ml|Machine Learning]]";
/// let links = parser.parse_document(content, "notes/index.md");
///
/// assert_eq!(links.len(), 2);
/// assert_eq!(links[0].target, "todo");
/// assert_eq!(links[1].target, "projects/ml");
/// assert_eq!(links[1].display_text, Some("Machine Learning".to_string()));
/// ```
pub struct LinkParser;

impl LinkParser {
    /// Create a new link parser.
    pub fn new() -> Self {
        Self
    }

    /// Extract all wikilinks from document content.
    ///
    /// # Arguments
    ///
    /// * `content` - Markdown document content
    /// * `source_path` - Path to the source document
    ///
    /// # Returns
    ///
    /// List of WikiLink objects found in the document
    ///
    /// # Example
    ///
    /// ```
    /// use vault_desktop::extraction::LinkParser;
    ///
    /// let parser = LinkParser::new();
    /// let content = r#"
    /// # My Notes
    /// See [[todo]] for tasks.
    /// Check [[projects/ml#architecture|ML Architecture]].
    /// "#;
    /// let links = parser.parse_document(content, "index.md");
    ///
    /// assert_eq!(links.len(), 2);
    /// assert_eq!(links[0].target, "todo");
    /// assert_eq!(links[1].target, "projects/ml");
    /// assert_eq!(links[1].header, Some("architecture".to_string()));
    /// ```
    pub fn parse_document(&self, content: &str, source_path: &str) -> Vec<WikiLink> {
        let mut links = Vec::new();

        for capture in wikilink_regex().captures_iter(content) {
            // Defensive: skip malformed wikilinks instead of panicking
            // These should always exist for valid regex matches, but we handle gracefully
            let full_match = match capture.get(0) {
                Some(m) => m,
                None => {
                    warn!("Regex capture missing full match - skipping malformed wikilink");
                    continue;
                }
            };
            let target = match capture.get(1) {
                Some(m) => m.as_str().trim().to_string(),
                None => {
                    warn!("Regex capture missing target - skipping malformed wikilink");
                    continue;
                }
            };
            let header = capture.get(2).map(|m| m.as_str().trim().to_string());
            let display = capture.get(3).map(|m| m.as_str().trim().to_string());

            // Calculate line number (count newlines before match, then add 1)
            let line_number = content[..full_match.start()]
                .chars()
                .filter(|&c| c == '\n')
                .count()
                + 1;

            // Extract context (50 chars before and after)
            let start = full_match.start().saturating_sub(50);
            let end = (full_match.end() + 50).min(content.len());
            let context = content[start..end].replace("\n", " ");

            links.push(WikiLink {
                source_path: source_path.to_string(),
                target,
                display_text: display,
                header,
                line_number,
                context,
            });
        }

        links
    }

    /// Resolve a link target to actual file path.
    ///
    /// Resolution strategy:
    /// 1. Exact path match: [[notes/todo.md]] -> notes/todo.md
    /// 2. Path without extension: [[notes/todo]] -> notes/todo.md
    /// 3. Title match: [[Todo List]] -> notes/todo.md (if title matches)
    /// 4. Fuzzy match: [[todo]] -> notes/todo.md (filename contains target)
    ///
    /// # Arguments
    ///
    /// * `target` - Link target text from `[[target]]`
    /// * `source_path` - Path of document containing the link
    /// * `all_documents` - List of available documents with paths and titles
    ///
    /// # Returns
    ///
    /// Resolved file path or None if not found
    ///
    /// # Example
    ///
    /// ```
    /// use vault_desktop::extraction::{LinkParser, DocumentInfo};
    ///
    /// let parser = LinkParser::new();
    /// let docs = vec![
    ///     DocumentInfo {
    ///         file_path: "notes/todo.md".to_string(),
    ///         title: Some("Todo List".to_string()),
    ///     },
    ///     DocumentInfo {
    ///         file_path: "projects/ml.md".to_string(),
    ///         title: Some("Machine Learning".to_string()),
    ///     },
    /// ];
    ///
    /// // Exact match
    /// assert_eq!(
    ///     parser.resolve_link("notes/todo.md", "index.md", &docs),
    ///     Some("notes/todo.md".to_string())
    /// );
    ///
    /// // Title match
    /// assert_eq!(
    ///     parser.resolve_link("Todo List", "index.md", &docs),
    ///     Some("notes/todo.md".to_string())
    /// );
    ///
    /// // Fuzzy match
    /// assert_eq!(
    ///     parser.resolve_link("todo", "index.md", &docs),
    ///     Some("notes/todo.md".to_string())
    /// );
    /// ```
    pub fn resolve_link(
        &self,
        target: &str,
        source_path: &str,
        all_documents: &[DocumentInfo],
    ) -> Option<String> {
        if all_documents.is_empty() {
            return None;
        }

        let target_lower = target.to_lowercase();
        let source_dir = Path::new(source_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        // 1. Exact path match (full path or path without extension)
        for doc in all_documents {
            let file_path_lower = doc.file_path.to_lowercase();

            // Exact match
            if file_path_lower == target_lower {
                return Some(doc.file_path.clone());
            }

            // Match with .md extension added (for exact path)
            if file_path_lower == format!("{}.md", target_lower) {
                return Some(doc.file_path.clone());
            }

            // Relative path from source directory
            let relative_path = if source_dir.is_empty() {
                target.to_string()
            } else {
                format!("{}/{}", source_dir, target)
            };
            let relative_lower = relative_path.to_lowercase();

            if file_path_lower == relative_lower
                || file_path_lower == format!("{}.md", relative_lower)
            {
                return Some(doc.file_path.clone());
            }
        }

        // 2. Title match
        for doc in all_documents {
            if let Some(title) = &doc.title {
                if title.to_lowercase() == target_lower {
                    return Some(doc.file_path.clone());
                }
            }
        }

        // 3. Fuzzy filename match
        for doc in all_documents {
            let file_name = Path::new(&doc.file_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();

            if target_lower.contains(&file_name) || file_name.contains(&target_lower) {
                return Some(doc.file_path.clone());
            }
        }

        None
    }

    /// Extract document title from content.
    ///
    /// Tries to extract title from:
    /// 1. First H1 heading (# Title)
    /// 2. YAML frontmatter (title: ...)
    /// 3. First line if it looks like a title
    ///
    /// # Arguments
    ///
    /// * `content` - Document content
    ///
    /// # Returns
    ///
    /// Extracted title or None
    ///
    /// # Example
    ///
    /// ```
    /// use vault_desktop::extraction::LinkParser;
    ///
    /// let parser = LinkParser::new();
    ///
    /// let content = "# My Note\n\nContent here...";
    /// assert_eq!(parser.extract_title(content), Some("My Note".to_string()));
    ///
    /// let content = "---\ntitle: My Note\n---\n\nContent...";
    /// assert_eq!(parser.extract_title(content), Some("My Note".to_string()));
    /// ```
    pub fn extract_title(&self, content: &str) -> Option<String> {
        let lines: Vec<&str> = content.lines().collect();

        // Check for YAML frontmatter
        if content.starts_with("---") {
            let in_frontmatter = true;
            for line in lines.iter().skip(1) {
                if line.trim() == "---" {
                    break;
                }
                if line.trim().starts_with("title:") {
                    let title = line.split(':').nth(1)?.trim();
                    let title = title.trim_matches('"').trim_matches('\'');
                    return Some(title.to_string());
                }
            }
        }

        // Check for H1 heading
        for line in &lines {
            let trimmed = line.trim();
            if trimmed.starts_with("# ") {
                return Some(trimmed.get(2..).unwrap_or(trimmed).trim().to_string());
            }
        }

        // Check first line as title (if it's short and doesn't look like code)
        if !lines.is_empty() {
            let first_line = lines.first().map(|s| s.trim()).unwrap_or("");
            if !first_line.is_empty()
                && first_line.len() < 100
                && !first_line.starts_with('[')
                && !first_line.starts_with('`')
            {
                return Some(first_line.to_string());
            }
        }

        None
    }
}

impl Default for LinkParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_link() {
        let parser = LinkParser::new();
        let content = "See [[todo]] for tasks.";
        let links = parser.parse_document(content, "index.md");

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "todo");
        assert_eq!(links[0].display_text, None);
        assert_eq!(links[0].header, None);
        assert_eq!(links[0].line_number, 1);
    }

    #[test]
    fn test_parse_link_with_display() {
        let parser = LinkParser::new();
        let content = "Check [[projects/ml|Machine Learning]]";
        let links = parser.parse_document(content, "index.md");

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "projects/ml");
        assert_eq!(links[0].display_text, Some("Machine Learning".to_string()));
    }

    #[test]
    fn test_parse_link_with_header() {
        let parser = LinkParser::new();
        let content = "See [[doc#section]]";
        let links = parser.parse_document(content, "index.md");

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "doc");
        assert_eq!(links[0].header, Some("section".to_string()));
    }

    #[test]
    fn test_parse_link_with_header_and_display() {
        let parser = LinkParser::new();
        let content = "[[projects/ml#architecture|ML Architecture]]";
        let links = parser.parse_document(content, "index.md");

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "projects/ml");
        assert_eq!(links[0].header, Some("architecture".to_string()));
        assert_eq!(links[0].display_text, Some("ML Architecture".to_string()));
    }

    #[test]
    fn test_parse_multiple_links() {
        let parser = LinkParser::new();
        let content = "See [[todo]] and [[projects/ml|ML]]";
        let links = parser.parse_document(content, "index.md");

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "todo");
        assert_eq!(links[1].target, "projects/ml");
    }

    #[test]
    fn test_line_numbers() {
        let parser = LinkParser::new();
        let content = "Line 1\nLine 2 [[link1]]\nLine 3\nLine 4 [[link2]]";
        let links = parser.parse_document(content, "test.md");

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].line_number, 2);
        assert_eq!(links[1].line_number, 4);
    }

    #[test]
    fn test_resolve_exact_match() {
        let parser = LinkParser::new();
        let docs = vec![DocumentInfo {
            file_path: "notes/todo.md".to_string(),
            title: Some("Todo".to_string()),
        }];

        assert_eq!(
            parser.resolve_link("notes/todo.md", "index.md", &docs),
            Some("notes/todo.md".to_string())
        );
    }

    #[test]
    fn test_resolve_without_extension() {
        let parser = LinkParser::new();
        let docs = vec![DocumentInfo {
            file_path: "notes/todo.md".to_string(),
            title: None,
        }];

        assert_eq!(
            parser.resolve_link("notes/todo", "index.md", &docs),
            Some("notes/todo.md".to_string())
        );
    }

    #[test]
    fn test_resolve_title_match() {
        let parser = LinkParser::new();
        let docs = vec![DocumentInfo {
            file_path: "notes/todo.md".to_string(),
            title: Some("Todo List".to_string()),
        }];

        assert_eq!(
            parser.resolve_link("Todo List", "index.md", &docs),
            Some("notes/todo.md".to_string())
        );
    }

    #[test]
    fn test_resolve_fuzzy_match() {
        let parser = LinkParser::new();
        let docs = vec![DocumentInfo {
            file_path: "notes/todo.md".to_string(),
            title: None,
        }];

        assert_eq!(
            parser.resolve_link("todo", "index.md", &docs),
            Some("notes/todo.md".to_string())
        );
    }

    #[test]
    fn test_extract_title_from_h1() {
        let parser = LinkParser::new();
        let content = "# My Note\n\nContent here...";
        assert_eq!(parser.extract_title(content), Some("My Note".to_string()));
    }

    #[test]
    fn test_extract_title_from_frontmatter() {
        let parser = LinkParser::new();
        let content = "---\ntitle: My Note\n---\n\nContent...";
        assert_eq!(parser.extract_title(content), Some("My Note".to_string()));
    }

    #[test]
    fn test_extract_title_from_frontmatter_with_quotes() {
        let parser = LinkParser::new();
        let content = "---\ntitle: \"My Note\"\n---\n\nContent...";
        assert_eq!(parser.extract_title(content), Some("My Note".to_string()));
    }

    #[test]
    fn test_extract_title_from_first_line() {
        let parser = LinkParser::new();
        let content = "My Note\n\nContent here...";
        assert_eq!(parser.extract_title(content), Some("My Note".to_string()));
    }
}
