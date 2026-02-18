//! Metadata Extraction Service
//!
//! Provides intelligent metadata extraction for documents and chunks:
//! - Language detection (natural language and programming language)
//! - Category classification
//! - Quality score calculation
//! - Word and token counting

use crate::domain::entities::document::{Category, Language};
use chrono::{DateTime, Utc};

/// Service for extracting rich metadata from document content.
///
/// This service provides heuristic-based detection and scoring without
/// external dependencies. It can be enhanced with ML-based detection
/// in future phases.
#[derive(Debug, Clone)]
pub struct MetadataExtractor;

impl MetadataExtractor {
    /// Create a new metadata extractor.
    pub fn new() -> Self {
        Self
    }

    /// Detect language from content and file extension.
    ///
    /// Priority:
    /// 1. Programming language (from file extension)
    /// 2. Natural language (from content heuristics)
    ///
    /// # Arguments
    ///
    /// * `content` - Document text content
    /// * `file_ext` - File extension (e.g., "rs", "py", "txt")
    ///
    /// # Returns
    ///
    /// Detected language enum
    pub fn detect_language(&self, content: &str, file_ext: &str) -> Language {
        // Check file extension first for programming languages
        match file_ext.to_lowercase().as_str() {
            "rs" => Language::Rust,
            "py" | "pyw" => Language::Python,
            "js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
            "ts" | "tsx" | "mts" | "cts" => Language::TypeScript,
            "go" => Language::Go,
            "java" => Language::Java,
            "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" => Language::Cpp,
            "cs" => Language::CSharp,
            _ => {
                // Detect natural language using simple heuristics
                // In future: can use 'whatlang' crate for better detection
                if content.chars().filter(|c| c.is_alphabetic()).count() > 10 {
                    // Basic detection - assume English for now
                    // Could be enhanced with character frequency analysis
                    Language::English
                } else {
                    Language::Unknown
                }
            }
        }
    }

    /// Categorize document based on content and metadata.
    ///
    /// Uses keyword-based classification to determine document category.
    ///
    /// # Arguments
    ///
    /// * `content` - Document text content
    /// * `file_name` - Name of the file
    /// * `mime_type` - MIME type (e.g., "text/plain")
    ///
    /// # Returns
    ///
    /// Category enum
    pub fn categorize(&self, content: &str, file_name: &str, mime_type: &str) -> Category {
        let content_lower = content.to_lowercase();
        let name_lower = file_name.to_lowercase();

        // Code files
        if mime_type.starts_with("text/x-")
            || name_lower.ends_with(".rs")
            || name_lower.ends_with(".py")
            || name_lower.ends_with(".js")
            || name_lower.ends_with(".ts")
            || name_lower.ends_with(".go")
            || name_lower.ends_with(".java")
            || name_lower.ends_with(".cpp")
            || name_lower.ends_with(".cs")
            || content_lower.contains("fn main")
            || content_lower.contains("def ")
            || content_lower.contains("function ")
            || content_lower.contains("public class")
            || content_lower.contains("impl ")
        {
            return Category::Code;
        }

        // Documentation
        if name_lower.contains("readme")
            || name_lower.contains("doc")
            || name_lower.starts_with("api")
            || mime_type == "text/markdown"
            || name_lower.ends_with(".md")
        {
            return Category::Documentation;
        }

        // Research papers
        if (content_lower.contains("abstract") || content_lower.contains("introduction"))
            && content_lower.contains("conclusion")
            && (content_lower.contains("references") || content_lower.contains("bibliography"))
        {
            return Category::Research;
        }

        // Tutorial
        if content_lower.contains("step 1")
            || content_lower.contains("step-by-step")
            || content_lower.contains("tutorial")
            || content_lower.contains("how to")
            || content_lower.contains("getting started")
        {
            return Category::Tutorial;
        }

        // Notes
        if name_lower.contains("note")
            || name_lower.contains("journal")
            || name_lower.contains("diary")
        {
            return Category::Notes;
        }

        // Reference
        if content_lower.contains("table of contents")
            || content_lower.contains("index")
            || content_lower.contains("glossary")
            || name_lower.contains("reference")
        {
            return Category::Reference;
        }

        Category::Uncategorized
    }

    /// Calculate document quality score (0.0-1.0).
    ///
    /// Factors considered:
    /// - Content length (longer is generally better, up to a point)
    /// - Readability (alphabetic character ratio)
    /// - Structure (paragraphs, formatting)
    /// - Recency (newer documents scored higher)
    ///
    /// # Arguments
    ///
    /// * `content` - Document text content
    /// * `file_size` - File size in bytes
    /// * `modified_at` - Last modification timestamp
    ///
    /// # Returns
    ///
    /// Quality score between 0.0 and 1.0
    pub fn calculate_quality_score(
        &self,
        content: &str,
        file_size: i64,
        modified_at: DateTime<Utc>,
    ) -> f32 {
        let mut score: f32 = 0.5;

        let word_count = content.split_whitespace().count();

        // Length factor (sweet spot: 100-2000 words)
        if word_count > 50 {
            score += 0.1;
        }
        if word_count > 100 {
            score += 0.1;
        }
        if word_count > 500 {
            score += 0.05;
        }
        if word_count > 2000 {
            score -= 0.05; // Very long documents may be harder to use
        }

        // Readability - high alphabetic character ratio indicates readable text
        let total_chars = content.len().max(1);
        let alpha_ratio =
            content.chars().filter(|c| c.is_alphabetic()).count() as f32 / total_chars as f32;

        if alpha_ratio > 0.5 {
            score += 0.1;
        }
        if alpha_ratio > 0.7 {
            score += 0.1;
        }

        // Structure - presence of paragraphs indicates well-formatted content
        if content.contains("\n\n") {
            score += 0.05;
        }

        // Line count - reasonable number of lines suggests good structure
        let line_count = content.lines().count();
        if line_count > 5 && line_count < 1000 {
            score += 0.05;
        }

        // Recency factor - newer content is often more valuable
        let age_days = (Utc::now() - modified_at).num_days();
        if age_days < 7 {
            score += 0.1;
        } else if age_days < 30 {
            score += 0.05;
        } else if age_days > 365 {
            score -= 0.05; // Older content may be outdated
        }

        // File size sanity check - very small or empty files are lower quality
        if file_size < 100 {
            score -= 0.2;
        }

        score.clamp(0.0, 1.0)
    }

    /// Count words in content.
    ///
    /// Uses whitespace-based splitting, which works well for most text.
    ///
    /// # Arguments
    ///
    /// * `content` - Text content to count
    ///
    /// # Returns
    ///
    /// Word count as i32
    pub fn count_words(&self, content: &str) -> i32 {
        content.split_whitespace().count() as i32
    }

    /// Estimate token count from content.
    ///
    /// Uses approximation: ~1.3 tokens per word (based on typical English text).
    /// This is a rough heuristic and could be improved with actual tokenization.
    ///
    /// # Arguments
    ///
    /// * `content` - Text content to count tokens
    ///
    /// # Returns
    ///
    /// Estimated token count as i32
    pub fn count_tokens(&self, content: &str) -> i32 {
        let word_count = content.split_whitespace().count();
        (word_count as f32 * 1.3) as i32
    }

    /// Detect if content contains code blocks or code patterns.
    pub fn contains_code(&self, content: &str) -> bool {
        // Check for common code patterns
        content.contains("```") ||           // Markdown code blocks
        content.contains("fn ") ||           // Rust functions
        content.contains("def ") ||          // Python functions
        content.contains("function ") ||     // JavaScript functions
        content.contains("class ") ||        // Classes
        content.contains("impl ") ||         // Rust impl
        content.contains("public ") ||       // Java/C# public
        content.contains("    ") ||          // Indentation (4 spaces)
        content.contains("\t") ||            // Tab indentation
        content.matches("->").count() > 2 || // Return types
        content.matches("=>").count() > 2 // Arrow functions
    }

    /// Extract section/heading from content.
    ///
    /// Checks the first 10 lines for markdown headings, colon headings,
    /// or ALL CAPS headings.
    pub fn extract_section(&self, content: &str) -> Option<String> {
        content
            .lines()
            .take(10)
            .find(|line| {
                let trimmed = line.trim();
                trimmed.starts_with('#') ||  // Markdown headings
                trimmed.ends_with(':') ||    // Colon headings
                (trimmed.len() > 3 &&
                 trimmed.chars().all(|c| c.is_uppercase() || c.is_whitespace() || c == '_'))
            })
            .map(|line| {
                line.trim()
                    .trim_start_matches('#')
                    .trim_end_matches(':')
                    .trim()
                    .to_string()
            })
    }
}

impl Default for MetadataExtractor {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language_rust() {
        let extractor = MetadataExtractor::new();
        let content = "fn main() { println!(\"Hello\"); }";
        let language = extractor.detect_language(content, "rs");
        assert_eq!(language, Language::Rust);
    }

    #[test]
    fn test_detect_language_python() {
        let extractor = MetadataExtractor::new();
        let content = "def main():\n    print('Hello')";
        let language = extractor.detect_language(content, "py");
        assert_eq!(language, Language::Python);
    }

    #[test]
    fn test_detect_language_english() {
        let extractor = MetadataExtractor::new();
        let content = "This is a regular English text document with many words.";
        let language = extractor.detect_language(content, "txt");
        assert_eq!(language, Language::English);
    }

    #[test]
    fn test_categorize_code() {
        let extractor = MetadataExtractor::new();
        let content = "fn main() {\n    println!(\"test\");\n}";
        let category = extractor.categorize(content, "main.rs", "text/plain");
        assert_eq!(category, Category::Code);
    }

    #[test]
    fn test_categorize_documentation() {
        let extractor = MetadataExtractor::new();
        let content = "# API Documentation\n\nThis is the API reference.";
        let category = extractor.categorize(content, "README.md", "text/markdown");
        assert_eq!(category, Category::Documentation);
    }

    #[test]
    fn test_categorize_research() {
        let extractor = MetadataExtractor::new();
        let content = "Abstract\nThis paper explores...\nConclusion\nReferences\n";
        let category = extractor.categorize(content, "paper.pdf", "application/pdf");
        assert_eq!(category, Category::Research);
    }

    #[test]
    fn test_categorize_tutorial() {
        let extractor = MetadataExtractor::new();
        let content = "How to build a web app\nStep 1: Install dependencies";
        let category = extractor.categorize(content, "tutorial.txt", "text/plain");
        assert_eq!(category, Category::Tutorial);
    }

    #[test]
    fn test_quality_score_good_content() {
        let extractor = MetadataExtractor::new();
        let content = "This is a well-written document with multiple paragraphs.\n\n\
                      It has good structure and reasonable length.\n\n\
                      The content is readable and informative.";
        let modified = Utc::now();
        let score = extractor.calculate_quality_score(content, 1000, modified);
        assert!(score > 0.7);
    }

    #[test]
    fn test_quality_score_poor_content() {
        let extractor = MetadataExtractor::new();
        // Old content (> 365 days) with mostly non-alphabetic characters
        let content = "123 456 789";
        let modified = Utc::now() - chrono::Duration::days(400);
        let score = extractor.calculate_quality_score(content, 10, modified);
        // Starting score 0.5, old content gets -0.1 = 0.4
        // Low alpha ratio (3/11 = 0.27) gets no bonuses
        assert!(score <= 0.5, "Score was {}", score);
    }

    #[test]
    fn test_count_words() {
        let extractor = MetadataExtractor::new();
        let content = "This is a test with five words";
        assert_eq!(extractor.count_words(content), 7);
    }

    #[test]
    fn test_count_tokens() {
        let extractor = MetadataExtractor::new();
        let content = "This is a test";
        let tokens = extractor.count_tokens(content);
        // Should be approximately 4 * 1.3 = 5
        assert!((4..=6).contains(&tokens));
    }
}
