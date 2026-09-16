//! Tag generation prompt and parsing helpers.
//!
//! Tag generation executes in `TagServiceImpl` using local LLMs. This module
//! keeps prompt construction and response parsing logic in one place.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// System prompt for tag generation (cached for 5 minutes).
pub(crate) const TAG_GENERATION_SYSTEM_PROMPT: &str = r#"You are a tag generation expert. Generate 3-5 relevant tags for documents.

Rules:
- Tags should be lowercase, hyphenated (e.g., "machine-learning")
- Focus on topics, technologies, and concepts
- Avoid overly generic tags like "interesting" or "important"
- Consider the document type and context
- Tags should be searchable and help categorize content

Return only comma-separated tags, nothing else. No explanations.

Examples:
- "machine-learning, neural-networks, deep-learning, python"
- "project-management, agile, scrum, team-collaboration"
- "tax-returns, finance, accounting, legal"
"#;

/// Document metadata for tag generation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub file_type: Option<String>,
    pub author: Option<String>,
}

/// Helper namespace for shared tag generation utilities.
pub struct TagGenerator;

impl TagGenerator {
    /// Build user message with document context.
    pub(crate) fn build_user_message(
        content: &str,
        metadata: &DocumentMetadata,
        max_tags: usize,
    ) -> String {
        let mut parts = Vec::new();

        if let Some(title) = &metadata.title {
            parts.push(format!("Document title: {}", title));
        }

        if let Some(file_type) = &metadata.file_type {
            parts.push(format!("File type: {}", file_type));
        }

        // Truncate content to first 10K chars to stay within token budget
        let content_preview = if content.len() > 10000 {
            format!(
                "{}... [truncated]",
                &content[..crate::shared::text_utils::floor_char_boundary(content, 10000)]
            )
        } else {
            content.to_string()
        };

        parts.push(format!("\nContent:\n{}", content_preview));
        parts.push(format!("\nGenerate up to {} tags.", max_tags));

        parts.join("\n")
    }

    /// Parse comma-separated tags from response.
    ///
    /// Cleans up tags, removes duplicates, and enforces max_tags limit.
    pub(crate) fn parse_tags(response: &str, max_tags: usize) -> Vec<String> {
        if response.is_empty() {
            return Vec::new();
        }

        // Split by comma and clean
        let tags: Vec<String> = response
            .split(',')
            .map(|tag| tag.trim().to_lowercase())
            .filter(|tag| !tag.is_empty())
            .collect();

        // Remove duplicates while preserving order
        let mut seen = HashSet::new();
        let mut unique_tags = Vec::new();

        for tag in tags {
            if seen.insert(tag.clone()) {
                unique_tags.push(tag);
            }
        }

        // Enforce max_tags limit
        unique_tags.truncate(max_tags);
        unique_tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tags() {
        // Normal case
        let tags = TagGenerator::parse_tags("machine-learning, python, ai", 5);
        assert_eq!(tags, vec!["machine-learning", "python", "ai"]);

        // With extra whitespace
        let tags = TagGenerator::parse_tags("  tag1  ,  tag2  ,  tag3  ", 5);
        assert_eq!(tags, vec!["tag1", "tag2", "tag3"]);

        // With duplicates
        let tags = TagGenerator::parse_tags("python, ml, python, ai", 5);
        assert_eq!(tags, vec!["python", "ml", "ai"]);

        // Enforce max_tags
        let tags = TagGenerator::parse_tags("tag1, tag2, tag3, tag4, tag5", 3);
        assert_eq!(tags, vec!["tag1", "tag2", "tag3"]);

        // Empty string
        let tags = TagGenerator::parse_tags("", 5);
        assert!(tags.is_empty());
    }

    #[test]
    fn test_build_user_message() {
        let metadata = DocumentMetadata {
            title: Some("My Doc".to_string()),
            file_type: Some("md".to_string()),
            author: None,
        };

        let message = TagGenerator::build_user_message("test content", &metadata, 5);

        assert!(message.contains("Document title: My Doc"));
        assert!(message.contains("File type: md"));
        assert!(message.contains("test content"));
        assert!(message.contains("Generate up to 5 tags"));
    }

    #[test]
    fn test_build_user_message_truncation() {
        let long_content = "a".repeat(15000);
        let message = TagGenerator::build_user_message(&long_content, &Default::default(), 5);

        assert!(message.contains("[truncated]"));
        assert!(message.len() < 11000); // Should be truncated
    }
}
