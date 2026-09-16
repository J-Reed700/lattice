use crate::features::indexing::engine::error::{IndexingError, Result};
use crate::features::indexing::engine::metadata_extractor::DocumentMetadata;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokenizers::Tokenizer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextChunk {
    pub text: String,
    pub start_idx: usize,
    pub end_idx: usize,
    pub token_count: usize,

    /// Path to the source document
    #[serde(default)]
    pub document_path: String,

    /// Index of this chunk within the document
    #[serde(default)]
    pub chunk_index: usize,
}

#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    pub max_tokens: usize,
    pub overlap_tokens: usize,
    pub prefer_sentence_boundaries: bool,
}

impl ChunkerConfig {
    /// Validates the configuration, returning an error for invalid settings.
    /// This catches misconfigurations at construction time, not at runtime.
    ///
    /// # Invariants Enforced
    /// - `max_tokens > 0` (must produce at least 1 token per chunk)
    /// - `overlap_tokens < max_tokens` (must advance by at least 1 token per iteration)
    pub fn validate(&self) -> Result<()> {
        use crate::shared::error::AppError;

        if self.max_tokens == 0 {
            return Err(AppError::InvalidConfig(
                "max_tokens must be greater than 0".to_string(),
            ));
        }

        if self.overlap_tokens >= self.max_tokens {
            return Err(AppError::InvalidConfig(format!(
                "overlap_tokens ({}) must be less than max_tokens ({}) to ensure forward progress",
                self.overlap_tokens, self.max_tokens
            )));
        }

        Ok(())
    }
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            max_tokens: 800,
            overlap_tokens: 120,
            prefer_sentence_boundaries: true,
        }
    }
}

pub struct SemanticChunker {
    tokenizer: Arc<Tokenizer>,
    config: ChunkerConfig,
}

impl SemanticChunker {
    /// Creates a new SemanticChunker with the given tokenizer and configuration.
    ///
    /// # Errors
    /// Returns an error if the configuration is invalid (e.g., overlap >= max_tokens).
    pub fn new(tokenizer: Arc<Tokenizer>, config: ChunkerConfig) -> Result<Self> {
        config.validate()?;
        let mut source_tokenizer = (*tokenizer).clone();
        source_tokenizer
            .with_truncation(None)
            .map_err(|e| crate::shared::error::AppError::InvalidConfig(e.to_string()))?;
        source_tokenizer.with_padding(None);
        Ok(Self {
            tokenizer: Arc::new(source_tokenizer),
            config,
        })
    }

    pub fn chunk_text(&self, text: &str) -> Result<Vec<TextChunk>> {
        if text.is_empty() {
            return Ok(vec![]);
        }

        let encoding =
            self.tokenizer
                .encode(text, false)
                .map_err(|e| IndexingError::TokenizationError {
                    reason: e.to_string(),
                })?;

        let tokens = encoding.get_ids();
        let offsets = encoding.get_offsets();

        if tokens.is_empty() {
            return Ok(vec![]);
        }

        let mut chunks = Vec::new();
        let mut start_token_idx = 0;

        while start_token_idx < tokens.len() {
            let end_token_idx = (start_token_idx + self.config.max_tokens).min(tokens.len());

            let actual_end_idx =
                if self.config.prefer_sentence_boundaries && end_token_idx < tokens.len() {
                    self.find_sentence_boundary(text, offsets, start_token_idx, end_token_idx)
                        .unwrap_or(end_token_idx)
                } else {
                    end_token_idx
                };

            let start_char = offsets.get(start_token_idx).map(|o| o.0).unwrap_or(0);
            let end_char = if actual_end_idx < offsets.len() {
                offsets
                    .get(actual_end_idx - 1)
                    .map(|o| o.1)
                    .unwrap_or(text.len())
            } else {
                text.len()
            };

            let chunk_text = text[start_char..end_char].to_string();
            let token_count = actual_end_idx - start_token_idx;

            chunks.push(TextChunk {
                text: chunk_text,
                start_idx: start_char,
                end_idx: end_char,
                token_count,
                document_path: String::new(),
                chunk_index: chunks.len(),
            });

            if actual_end_idx >= tokens.len() {
                break;
            }

            // INVARIANT: Each iteration advances by at least 1 token.
            // This is guaranteed by design through:
            // 1. Config validation ensures overlap_tokens < max_tokens
            // 2. find_sentence_boundary() only returns positions where chunk_size > overlap_tokens
            // 3. Therefore: next_start = end - overlap > start (always advances)
            let next_start_idx = actual_end_idx.saturating_sub(self.config.overlap_tokens);

            // Debug assertion to catch invariant violations during development
            debug_assert!(
                next_start_idx > start_token_idx,
                "Chunker invariant violated: next_start ({}) must be > current_start ({}). \
                 actual_end_idx={}, overlap_tokens={}",
                next_start_idx,
                start_token_idx,
                actual_end_idx,
                self.config.overlap_tokens
            );

            start_token_idx = next_start_idx;
        }

        Ok(chunks)
    }

    /// Finds a sentence boundary within the search window.
    ///
    /// # Invariant
    /// The returned boundary position, if any, guarantees that the resulting chunk
    /// is large enough to ensure forward progress (chunk_size > overlap_tokens).
    /// This is enforced by only accepting boundaries at positions >= min_acceptable_end.
    fn find_sentence_boundary(
        &self,
        text: &str,
        offsets: &[(usize, usize)],
        start_token_idx: usize,
        target_token_idx: usize,
    ) -> Option<usize> {
        // ARCHITECTURAL FIX: Minimum chunk size must exceed overlap to guarantee forward progress.
        // This enforces the invariant at the source, rather than patching it in the loop.
        let min_chunk_size = self.config.overlap_tokens + 1;
        let min_acceptable_end = start_token_idx + min_chunk_size;

        // If target is already at or below minimum, no valid sentence boundary exists
        if target_token_idx <= min_acceptable_end {
            return None;
        }

        // Search window is limited to avoid searching too far back
        let search_window = 50.min(target_token_idx.saturating_sub(min_acceptable_end));
        if search_window == 0 {
            return None;
        }

        let search_start = target_token_idx.saturating_sub(search_window);

        for i in (search_start..target_token_idx).rev() {
            if i >= offsets.len() {
                continue;
            }

            // `offsets` holds **byte** offsets — the same values used to slice
            // `text[start_char..end_char]` when chunks are built. Indexing with
            // `chars().nth(byte_offset)` therefore inspected the wrong
            // character on any text containing accents, CJK, or emoji, giving
            // arbitrary mid-sentence splits that only degrade retrieval
            // quality, never fail loudly. `chars().nth()` was also O(n) per
            // probe, making boundary search quadratic on large documents.
            let byte_end = offsets.get(i).map(|o| o.1).unwrap_or(0);
            if byte_end > 0 && byte_end <= text.len() && text.is_char_boundary(byte_end) {
                // Last character *ending* at this byte offset.
                let char_at_pos = text[..byte_end].chars().next_back();
                if let Some(ch) = char_at_pos {
                    if self.is_sentence_ending(ch) {
                        let next_char = text[byte_end..].chars().next();
                        if next_char.is_none_or(|c| c.is_whitespace()) {
                            let boundary = i + 1;
                            // Only accept boundaries that result in valid chunk sizes
                            if boundary >= min_acceptable_end {
                                return Some(boundary);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    fn is_sentence_ending(&self, ch: char) -> bool {
        matches!(ch, '.' | '!' | '?' | '\n')
    }

    pub fn chunk_with_metadata(&self, text: &str) -> Result<Vec<ChunkWithMetadata>> {
        let chunks = self.chunk_text(text)?;

        Ok(chunks
            .into_iter()
            .enumerate()
            .map(|(idx, chunk)| ChunkWithMetadata {
                index: idx,
                chunk,
                has_overlap_before: idx > 0,
                has_overlap_after: false,
            })
            .collect())
    }

    pub fn chunk_with_context(
        &self,
        text: &str,
        metadata: &DocumentMetadata,
    ) -> Result<Vec<ContextualizedChunk>> {
        let base_chunks = self.chunk_text(text)?;

        let contextualized = base_chunks
            .into_iter()
            .enumerate()
            .map(|(idx, chunk)| {
                let context_prefix = self.build_context_prefix(metadata, idx);
                let contextualized_content = format!("{}\n\n{}", context_prefix, chunk.text);

                ContextualizedChunk {
                    original_content: chunk.text.clone(),
                    contextualized_content,
                    context_prefix,
                    chunk_index: idx,
                    token_count: chunk.token_count,
                    start_idx: chunk.start_idx,
                    end_idx: chunk.end_idx,
                }
            })
            .collect();

        Ok(contextualized)
    }

    fn build_context_prefix(&self, metadata: &DocumentMetadata, _chunk_index: usize) -> String {
        context_prefix(metadata)
    }
}

pub fn context_prefix(metadata: &DocumentMetadata) -> String {
    let mut parts = vec![format!("Document: {}", metadata.title)];
    if let Some(page) = metadata.page_number {
        parts.push(format!("Page: {page}"));
    }
    if let Some(section) = &metadata.section {
        parts.push(format!("Section: {section}"));
    }
    format!("[{}]", parts.join(" | "))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkWithMetadata {
    pub index: usize,
    pub chunk: TextChunk,
    pub has_overlap_before: bool,
    pub has_overlap_after: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextualizedChunk {
    pub original_content: String,
    pub contextualized_content: String,
    pub context_prefix: String,
    pub chunk_index: usize,
    pub token_count: usize,
    pub start_idx: usize,
    pub end_idx: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_tokenizer() -> Arc<Tokenizer> {
        use std::collections::HashMap;
        use tokenizers::models::bpe::BPE;
        use tokenizers::pre_tokenizers::whitespace::Whitespace;

        let mut vocab = HashMap::new();
        for c in b'a'..=b'z' {
            vocab.insert(String::from_utf8(vec![c]).unwrap(), c as u32);
        }
        for c in b'A'..=b'Z' {
            vocab.insert(String::from_utf8(vec![c]).unwrap(), (c + 26) as u32);
        }
        for c in b'0'..=b'9' {
            vocab.insert(String::from_utf8(vec![c]).unwrap(), (c + 52) as u32);
        }
        vocab.insert(" ".to_string(), 62);
        vocab.insert(".".to_string(), 63);
        vocab.insert("[UNK]".to_string(), 64);

        let merges = vec![];
        let bpe = BPE::builder()
            .vocab_and_merges(vocab, merges)
            .unk_token("[UNK]".to_string())
            .build()
            .expect("Failed to build BPE");

        let mut tokenizer = Tokenizer::new(bpe);
        tokenizer.with_pre_tokenizer(Whitespace {});

        Arc::new(tokenizer)
    }

    fn create_test_chunker() -> Result<SemanticChunker> {
        SemanticChunker::new(
            create_test_tokenizer(),
            ChunkerConfig {
                max_tokens: 50,
                overlap_tokens: 10,
                prefer_sentence_boundaries: true,
            },
        )
    }

    #[test]
    fn test_empty_text() -> Result<()> {
        let chunker = create_test_chunker()?;
        let chunks = chunker.chunk_text("")?;
        assert_eq!(chunks.len(), 0);
        Ok(())
    }

    #[test]
    fn test_short_text() -> Result<()> {
        let chunker = create_test_chunker()?;
        let text = "This is a short text.";
        let chunks = chunker.chunk_text(text)?;
        assert_eq!(chunks.len(), 1);
        Ok(())
    }

    #[test]
    fn test_chunking_preserves_content() -> Result<()> {
        let chunker = create_test_chunker()?;
        let text =
            "This is the first sentence. This is the second sentence. This is the third sentence.";
        let chunks = chunker.chunk_text(text)?;

        assert!(!chunks.is_empty());

        for chunk in &chunks {
            assert!(text.contains(&chunk.text));
        }

        Ok(())
    }

    /// Boundary detection indexes `text` by byte offset. Probing with
    /// `chars().nth(byte_offset)` silently inspected the wrong character on
    /// any multi-byte text, and could not be caught by ASCII-only fixtures.
    /// The chunker must at minimum never panic and never lose content on
    /// non-ASCII input.
    #[test]
    fn chunking_handles_multibyte_text_without_panicking() -> Result<()> {
        let chunker = create_test_chunker()?;

        for text in [
            // Accents: multi-byte characters adjacent to sentence endings.
            "Le café était très bon. Nous sommes restés longtemps. Puis nous \
             sommes partis à la maison.",
            // CJK: every character is 3 bytes.
            "这是第一句话。这是第二句话。这是第三句话。这是第四句话。",
            // Emoji: 4-byte characters, including right before a period.
            "I love this 🎉. It works great 🚀. Ship it 🔥. Done ✅.",
            // Mixed scripts in one document.
            "Hello world. こんにちは世界。Café ☕ time. Ω≈ç√∫˜µ≤≥÷.",
        ] {
            let chunks = chunker.chunk_text(text)?;

            for chunk in &chunks {
                // Byte offsets must land on real character boundaries;
                // otherwise slicing `text` with them would panic.
                assert!(
                    text.is_char_boundary(chunk.start_idx),
                    "start_idx {} is not a char boundary in {:?}",
                    chunk.start_idx,
                    text
                );
                assert!(
                    text.is_char_boundary(chunk.end_idx),
                    "end_idx {} is not a char boundary in {:?}",
                    chunk.end_idx,
                    text
                );
                assert!(chunk.start_idx <= chunk.end_idx);
                assert!(chunk.end_idx <= text.len());
            }

            // Content must survive chunking intact.
            if !chunks.is_empty() {
                let reassembled: String = chunks.iter().map(|c| c.text.as_str()).collect();
                for word in text.split_whitespace().take(3) {
                    assert!(
                        reassembled.contains(word),
                        "chunking dropped {:?} from {:?}",
                        word,
                        text
                    );
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_config_validation_zero_max_tokens() {
        let config = ChunkerConfig {
            max_tokens: 0,
            overlap_tokens: 10,
            prefer_sentence_boundaries: true,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("max_tokens must be greater than 0"));
    }

    #[test]
    fn test_config_validation_overlap_equals_max() {
        let config = ChunkerConfig {
            max_tokens: 50,
            overlap_tokens: 50,
            prefer_sentence_boundaries: true,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("overlap_tokens"));
    }

    #[test]
    fn test_config_validation_overlap_exceeds_max() {
        let config = ChunkerConfig {
            max_tokens: 50,
            overlap_tokens: 100,
            prefer_sentence_boundaries: true,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("overlap_tokens"));
    }

    #[test]
    fn test_chunker_rejects_invalid_config() {
        let tokenizer = create_test_tokenizer();
        let result = SemanticChunker::new(
            tokenizer,
            ChunkerConfig {
                max_tokens: 20,
                overlap_tokens: 30, // Invalid: overlap >= max_tokens
                prefer_sentence_boundaries: true,
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_config_accepted() {
        let config = ChunkerConfig {
            max_tokens: 512,
            overlap_tokens: 50,
            prefer_sentence_boundaries: true,
        };
        assert!(config.validate().is_ok());
    }
}
