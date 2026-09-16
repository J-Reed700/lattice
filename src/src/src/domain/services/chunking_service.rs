//! # Pure Chunking Service
//!
//! Domain service for chunking text with NO external dependencies.
//!
//! This is a pure domain service that implements text chunking algorithms
//! using only standard library functionality. No tokenizers, no ML libraries,
//! no external crates - just pure Rust.

/// Pure text chunking service.
///
/// Provides various text chunking strategies without external dependencies:
/// - Fixed-size chunking by characters
/// - Sentence boundary detection
/// - Paragraph-based chunking
/// - Sliding window with overlap
///
/// ## Pure Domain Logic
///
/// All algorithms use only `std` library. Infrastructure-specific chunking
/// (e.g., using tokenizers or ML models) should be implemented in the
/// infrastructure layer.
pub struct ChunkingService;

impl ChunkingService {
    /// Create a new chunking service.
    pub fn new() -> Self {
        Self
    }

    /// Chunk text by fixed size with overlap.
    ///
    /// # Arguments
    ///
    /// * `text` - Input text to chunk
    /// * `chunk_size` - Size of each chunk in characters
    /// * `overlap` - Number of characters to overlap between chunks
    ///
    /// # Returns
    ///
    /// Vector of text chunks as strings
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::services::chunking_service::ChunkingService;
    ///
    /// let service = ChunkingService::new();
    /// let chunks = service.chunk_fixed_size("Long text here", 10, 2);
    /// ```
    pub fn chunk_fixed_size(&self, text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
        if text.is_empty() || chunk_size == 0 {
            return vec![];
        }

        let chars: Vec<char> = text.chars().collect();
        let mut chunks = Vec::new();
        let mut start = 0;

        while start < chars.len() {
            let end = (start + chunk_size).min(chars.len());
            let chunk: String = chars.get(start..end).unwrap_or(&[]).iter().collect();

            // Only add non-empty chunks
            if !chunk.trim().is_empty() {
                chunks.push(chunk);
            }

            // Move to next chunk with overlap
            if end >= chars.len() {
                break;
            }

            start += chunk_size.saturating_sub(overlap);

            // Prevent infinite loop
            if chunk_size <= overlap {
                break;
            }
        }

        chunks
    }

    /// Chunk text by sentence boundaries.
    ///
    /// Detects sentence endings (., !, ?) and splits text accordingly.
    /// Handles common abbreviations (Dr., Mr., etc.) to avoid false splits.
    ///
    /// # Arguments
    ///
    /// * `text` - Input text to chunk
    ///
    /// # Returns
    ///
    /// Vector of sentences as strings
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::services::chunking_service::ChunkingService;
    ///
    /// let service = ChunkingService::new();
    /// let sentences = service.chunk_by_sentences("First sentence. Second sentence!");
    /// assert_eq!(sentences.len(), 2);
    /// ```
    pub fn chunk_by_sentences(&self, text: &str) -> Vec<String> {
        if text.is_empty() {
            return vec![];
        }

        let mut sentences = Vec::new();
        let mut current = String::new();

        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let ch = *chars.get(i).unwrap_or(&' ');
            current.push(ch);

            if self.is_sentence_ending(ch) {
                // Look ahead to see if this is a real sentence boundary
                if i + 1 < chars.len() {
                    let next = *chars.get(i + 1).unwrap_or(&' ');

                    // If next char is whitespace or uppercase, likely a sentence boundary
                    if (next.is_whitespace() || next.is_uppercase())
                        && !self.is_abbreviation(&current)
                    {
                        let trimmed = current.trim().to_string();
                        if !trimmed.is_empty() {
                            sentences.push(trimmed);
                        }
                        current.clear();
                    }
                } else {
                    // End of text - add remaining
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        sentences.push(trimmed);
                    }
                    current.clear();
                }
            }

            i += 1;
        }

        if !current.trim().is_empty() {
            sentences.push(current.trim().to_string());
        }

        sentences
    }

    /// Chunk text by paragraphs.
    ///
    /// Splits text on double newlines or paragraph breaks.
    ///
    /// # Arguments
    ///
    /// * `text` - Input text to chunk
    ///
    /// # Returns
    ///
    /// Vector of paragraphs as strings
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::services::chunking_service::ChunkingService;
    ///
    /// let service = ChunkingService::new();
    /// let text = "First paragraph.\n\nSecond paragraph.";
    /// let paragraphs = service.chunk_by_paragraphs(text);
    /// assert_eq!(paragraphs.len(), 2);
    /// ```
    pub fn chunk_by_paragraphs(&self, text: &str) -> Vec<String> {
        text.split("\n\n")
            .map(|para| para.trim().to_string())
            .filter(|para| !para.is_empty())
            .collect()
    }

    /// Chunk text by word count.
    ///
    /// Splits text into chunks of approximately equal word count.
    ///
    /// # Arguments
    ///
    /// * `text` - Input text to chunk
    /// * `words_per_chunk` - Target number of words per chunk
    ///
    /// # Returns
    ///
    /// Vector of text chunks
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::services::chunking_service::ChunkingService;
    ///
    /// let service = ChunkingService::new();
    /// let chunks = service.chunk_by_word_count("word1 word2 word3 word4", 2);
    /// assert_eq!(chunks.len(), 2);
    /// ```
    pub fn chunk_by_word_count(&self, text: &str, words_per_chunk: usize) -> Vec<String> {
        if text.is_empty() || words_per_chunk == 0 {
            return vec![];
        }

        let words: Vec<&str> = text.split_whitespace().collect();
        let mut chunks = Vec::new();

        for chunk_words in words.chunks(words_per_chunk) {
            let chunk = chunk_words.join(" ");
            if !chunk.is_empty() {
                chunks.push(chunk);
            }
        }

        chunks
    }

    /// Check if a character is a sentence ending.
    fn is_sentence_ending(&self, ch: char) -> bool {
        matches!(ch, '.' | '!' | '?' | '\n')
    }

    /// Check if text ends with a common abbreviation.
    ///
    /// This helps prevent false sentence splits.
    fn is_abbreviation(&self, text: &str) -> bool {
        let abbrevs = [
            "Dr.", "Mr.", "Mrs.", "Ms.", "Prof.", "Sr.", "Jr.", "vs.", "etc.", "Inc.", "Ltd.",
            "Co.", "Corp.",
        ];

        let trimmed = text.trim();
        abbrevs.iter().any(|abbrev| trimmed.ends_with(abbrev))
    }

    /// Count words in text.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::services::chunking_service::ChunkingService;
    ///
    /// let service = ChunkingService::new();
    /// assert_eq!(service.word_count("hello world"), 2);
    /// ```
    pub fn word_count(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }

    /// Count characters in text.
    pub fn char_count(&self, text: &str) -> usize {
        text.chars().count()
    }
}

impl Default for ChunkingService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_fixed_size() {
        let service = ChunkingService::new();
        let text = "Hello, World! This is a test.";
        let chunks = service.chunk_fixed_size(text, 10, 0);

        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.chars().count() <= 10));
    }

    #[test]
    fn test_chunk_fixed_size_with_overlap() {
        let service = ChunkingService::new();
        let text = "Hello World Test";
        let chunks = service.chunk_fixed_size(text, 10, 3);

        assert!(!chunks.is_empty());
        // With overlap, we should get more chunks
        assert!(chunks.len() > 1);
    }

    #[test]
    fn test_chunk_by_sentences() {
        let service = ChunkingService::new();
        let text = "First sentence. Second sentence! Third sentence?";
        let sentences = service.chunk_by_sentences(text);

        assert_eq!(sentences.len(), 3);
        assert!(sentences[0].contains("First"));
        assert!(sentences[1].contains("Second"));
        assert!(sentences[2].contains("Third"));
    }

    #[test]
    fn test_chunk_by_sentences_with_abbreviation() {
        let service = ChunkingService::new();
        let text = "Dr. Smith is here. This is a test.";
        let sentences = service.chunk_by_sentences(text);

        assert!(sentences.len() <= 2);
    }

    #[test]
    fn test_chunk_by_paragraphs() {
        let service = ChunkingService::new();
        let text = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
        let paragraphs = service.chunk_by_paragraphs(text);

        assert_eq!(paragraphs.len(), 3);
        assert!(paragraphs[0].contains("First"));
        assert!(paragraphs[1].contains("Second"));
        assert!(paragraphs[2].contains("Third"));
    }

    #[test]
    fn test_chunk_by_word_count() {
        let service = ChunkingService::new();
        let text = "one two three four five six seven eight";
        let chunks = service.chunk_by_word_count(text, 3);

        assert_eq!(chunks.len(), 3); // 3, 3, 2 words
        assert_eq!(service.word_count(&chunks[0]), 3);
        assert_eq!(service.word_count(&chunks[1]), 3);
        assert_eq!(service.word_count(&chunks[2]), 2);
    }

    #[test]
    fn test_empty_text() {
        let service = ChunkingService::new();

        assert!(service.chunk_fixed_size("", 10, 0).is_empty());
        assert!(service.chunk_by_sentences("").is_empty());
        assert!(service.chunk_by_paragraphs("").is_empty());
        assert!(service.chunk_by_word_count("", 10).is_empty());
    }

    #[test]
    fn test_word_count() {
        let service = ChunkingService::new();

        assert_eq!(service.word_count("hello world"), 2);
        assert_eq!(service.word_count("   one   two   three   "), 3);
        assert_eq!(service.word_count(""), 0);
    }

    #[test]
    fn test_char_count() {
        let service = ChunkingService::new();

        assert_eq!(service.char_count("hello"), 5);
        assert_eq!(service.char_count(""), 0);
        assert_eq!(service.char_count("🦀"), 1); // Unicode emoji is 1 char
    }

    #[test]
    fn test_is_abbreviation() {
        let service = ChunkingService::new();

        assert!(service.is_abbreviation("Hello Dr."));
        assert!(service.is_abbreviation("Company Inc."));
        assert!(!service.is_abbreviation("End of sentence."));
    }
}
