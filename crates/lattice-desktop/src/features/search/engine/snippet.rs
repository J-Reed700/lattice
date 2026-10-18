use unicode_segmentation::UnicodeSegmentation;

const DEFAULT_SNIPPET_LENGTH: usize = 300;
const CONTEXT_CHARS: usize = 50;

pub struct SnippetExtractor {
    max_length: usize,
}

impl SnippetExtractor {
    pub fn new(max_length: usize) -> Self {
        Self { max_length }
    }
}

impl Default for SnippetExtractor {
    fn default() -> Self {
        Self::new(DEFAULT_SNIPPET_LENGTH)
    }
}

impl SnippetExtractor {
    pub fn extract(&self, text: &str, query_terms: &[&str]) -> String {
        if text.is_empty() {
            return String::new();
        }

        if query_terms.is_empty() {
            return self.extract_beginning(text);
        }

        let best_match_pos = self.find_best_match_position(text, query_terms);

        match best_match_pos {
            Some(pos) => self.extract_around_position(text, pos),
            None => self.extract_beginning(text),
        }
    }

    pub fn extract_with_highlights(
        &self,
        text: &str,
        query_terms: &[&str],
    ) -> (String, Vec<String>) {
        let snippet = self.extract(text, query_terms);
        let highlights = self.find_highlights(&snippet, query_terms);
        (snippet, highlights)
    }

    fn find_best_match_position(&self, text: &str, query_terms: &[&str]) -> Option<usize> {
        let text_lower = text.to_lowercase();

        for term in query_terms {
            let term_lower = term.to_lowercase();
            if let Some(pos) = text_lower.find(&term_lower) {
                return Some(pos);
            }
        }

        None
    }

    fn extract_around_position(&self, text: &str, pos: usize) -> String {
        let start = pos.saturating_sub(CONTEXT_CHARS);
        let end = (pos + self.max_length).min(text.len());

        let mut snippet = String::new();

        if start > 0 {
            snippet.push_str("...");
        }

        let substr = &text[start..end];
        let trimmed = self.trim_to_word_boundaries(substr, start > 0, end < text.len());
        snippet.push_str(&trimmed);

        if end < text.len() {
            snippet.push_str("...");
        }

        snippet
    }

    fn extract_beginning(&self, text: &str) -> String {
        let end = self.max_length.min(text.len());
        let substr = &text[..end];
        let trimmed = self.trim_to_word_boundaries(substr, false, end < text.len());

        if end < text.len() {
            format!("{}...", trimmed)
        } else {
            trimmed
        }
    }

    fn trim_to_word_boundaries(&self, text: &str, trim_start: bool, trim_end: bool) -> String {
        let mut result = text.to_string();

        if trim_start {
            if let Some(first_space) = result.find(|c: char| c.is_whitespace()) {
                result = result[first_space..].trim_start().to_string();
            }
        }

        if trim_end {
            let graphemes: Vec<&str> = result.graphemes(true).collect();
            if let Some(last_word_end) = graphemes
                .iter()
                .rposition(|&g| g.chars().all(char::is_whitespace))
            {
                result = graphemes
                    .get(..last_word_end)
                    .map(|slice| slice.concat())
                    .unwrap_or(result);
            }
        }

        result
    }

    fn find_highlights(&self, snippet: &str, query_terms: &[&str]) -> Vec<String> {
        let mut highlights = Vec::new();
        let snippet_lower = snippet.to_lowercase();

        for term in query_terms {
            let term_lower = term.to_lowercase();
            if snippet_lower.contains(&term_lower) {
                highlights.push(term.to_string());
            }
        }

        highlights.sort();
        highlights.dedup();
        highlights
    }
}

pub fn extract_snippet(text: &str, max_length: usize) -> String {
    let extractor = SnippetExtractor::new(max_length);
    extractor.extract_beginning(text)
}

pub fn extract_snippet_with_context(text: &str, query: &str, max_length: usize) -> String {
    let extractor = SnippetExtractor::new(max_length);
    let terms: Vec<&str> = query.split_whitespace().collect();
    extractor.extract(text, &terms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_beginning() {
        let extractor = SnippetExtractor::new(50);
        let text =
            "This is a long piece of text that should be truncated properly at word boundaries.";
        let snippet = extractor.extract_beginning(text);

        assert!(snippet.len() <= 54);
        assert!(snippet.ends_with("..."));
    }

    #[test]
    fn test_extract_with_match() {
        let extractor = SnippetExtractor::new(100);
        let text =
            "The quick brown fox jumps over the lazy dog. This is some more text that continues.";
        let terms = vec!["lazy", "dog"];
        let snippet = extractor.extract(text, &terms);

        assert!(snippet.contains("lazy"));
        assert!(snippet.contains("dog"));
    }

    #[test]
    fn test_extract_with_highlights() {
        let extractor = SnippetExtractor::new(100);
        let text = "Rust is a systems programming language focused on safety and performance.";
        let terms = vec!["Rust", "programming"];
        let (_snippet, highlights) = extractor.extract_with_highlights(text, &terms);

        assert!(highlights.contains(&"Rust".to_string()));
        assert!(highlights.contains(&"programming".to_string()));
    }

    #[test]
    fn test_empty_text() {
        let extractor = SnippetExtractor::new(100);
        let snippet = extractor.extract("", &[]);
        assert!(snippet.is_empty());
    }
}
