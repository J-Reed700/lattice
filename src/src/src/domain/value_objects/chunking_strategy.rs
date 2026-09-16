//! # Chunking Strategy Value Object
//!
//! Strategy for splitting document content into chunks.
//!
//! This value object defines HOW to chunk content, delegating the actual
//! implementation to the chunking service.

use crate::domain::entities::chunk::Chunk;
use crate::domain::services::ChunkingService;
use crate::shared::domain_types::DocumentId;
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Chunking strategy for splitting document content.
///
/// Different strategies optimize for different use cases:
/// - `FixedSize`: Simple, predictable chunks by character count
/// - `Semantic`: Context-aware chunking (requires external service)
///
/// ## Example
///
/// ```rust,no_run
/// use lattice::domain::value_objects::chunking_strategy::ChunkingStrategy;
/// use lattice::shared::domain_types::DocumentId;
///
/// let strategy = ChunkingStrategy::FixedSize { size: 512 };
/// let doc_id = DocumentId::new();
/// let chunks = strategy.chunk("Long document content...", &doc_id)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkingStrategy {
    /// Fixed-size chunking by character count.
    FixedSize { size: usize },
    /// Semantic chunking by meaning (future implementation).
    Semantic { max_tokens: usize },
}

impl ChunkingStrategy {
    /// Adapt semantic chunk size for document length.
    ///
    /// This only scales semantic chunking upward for large documents to reduce
    /// over-fragmentation; fixed-size chunking is left unchanged.
    pub fn adapt_for_content(self, content: &str) -> Self {
        match self {
            ChunkingStrategy::Semantic { max_tokens } => {
                let baseline = if max_tokens == 0 { 512 } else { max_tokens };
                let word_count = content.split_whitespace().count();
                let adaptive_floor = match word_count {
                    w if w >= 24_000 => 1600,
                    w if w >= 12_000 => 1200,
                    w if w >= 6_000 => 900,
                    w if w >= 2_500 => 700,
                    w if w >= 1_200 => 600,
                    _ => baseline,
                };

                ChunkingStrategy::Semantic {
                    max_tokens: baseline.max(adaptive_floor),
                }
            }
            other => other,
        }
    }

    /// Infer a semantic chunking strategy from existing chunk sizes.
    ///
    /// Used by reindexing to avoid resetting documents to a one-size-fits-all
    /// default strategy.
    pub fn infer_from_existing_chunks(chunks: &[Chunk]) -> Self {
        if chunks.is_empty() {
            return ChunkingStrategy::default();
        }

        let mut observed_sizes: Vec<usize> = chunks
            .iter()
            .map(|chunk| {
                if chunk.token_count() > 0 {
                    chunk.token_count() as usize
                } else if chunk.word_count() > 0 {
                    chunk.word_count()
                } else {
                    chunk.content().split_whitespace().count()
                }
            })
            .filter(|size| *size > 0)
            .collect();

        if observed_sizes.is_empty() {
            return ChunkingStrategy::default();
        }

        observed_sizes.sort_unstable();
        let p75_index = ((observed_sizes.len() * 3) / 4).min(observed_sizes.len() - 1);
        let p75 = observed_sizes.get(p75_index).copied().unwrap_or_default();
        let inferred = ((p75 as f32) * 1.2).round() as usize;

        ChunkingStrategy::Semantic {
            max_tokens: inferred.clamp(256, 2048),
        }
    }

    /// Chunk content into text chunks.
    ///
    /// # Errors
    ///
    /// Returns an error if chunking fails or produces no chunks.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::value_objects::chunking_strategy::ChunkingStrategy;
    /// use lattice::shared::domain_types::DocumentId;
    ///
    /// let strategy = ChunkingStrategy::FixedSize { size: 100 };
    /// let chunks = strategy.chunk("Document text here", &DocumentId::new())?;
    /// assert!(!chunks.is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn chunk(&self, content: &str, document_id: &DocumentId) -> Result<Vec<Chunk>> {
        match self {
            ChunkingStrategy::FixedSize { size } => {
                self.chunk_fixed_size(content, *size, document_id)
            }
            ChunkingStrategy::Semantic { max_tokens } => {
                self.chunk_semantic(content, *max_tokens, document_id)
            }
        }
    }

    /// Fixed-size chunking implementation.
    ///
    /// Splits content into chunks of approximately equal size.
    fn chunk_fixed_size(
        &self,
        content: &str,
        size: usize,
        document_id: &DocumentId,
    ) -> Result<Vec<Chunk>> {
        if content.is_empty() {
            return Err(AppError::InvalidInput("Cannot chunk empty content".into()));
        }

        if size == 0 {
            return Err(AppError::InvalidInput("Chunk size must be > 0".into()));
        }

        // Split content into fixed-size chunks
        let chunks: Vec<String> = content
            .chars()
            .collect::<Vec<_>>()
            .chunks(size)
            .map(|chunk| chunk.iter().collect())
            .filter(|s: &String| !s.trim().is_empty())
            .collect();

        if chunks.is_empty() {
            return Err(AppError::InvalidState(
                "Chunking produced no non-empty chunks".into(),
            ));
        }

        Ok(chunks
            .into_iter()
            .enumerate()
            .map(|(i, content)| Chunk::new(document_id.clone(), content, i))
            .collect())
    }

    fn chunk_semantic(
        &self,
        content: &str,
        max_words: usize,
        document_id: &DocumentId,
    ) -> Result<Vec<Chunk>> {
        if content.is_empty() {
            return Err(AppError::InvalidInput("Cannot chunk empty content".into()));
        }

        let max_words = if max_words == 0 { 512 } else { max_words };
        let service = ChunkingService::new();
        let sentences = service.chunk_by_sentences(content);
        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut current_words = 0;

        for sentence in sentences {
            let trimmed = sentence.trim();
            if trimmed.is_empty() {
                continue;
            }

            let sentence_words = service.word_count(trimmed);
            if current_words == 0 {
                if sentence_words > max_words {
                    chunks.push(trimmed.to_string());
                    continue;
                }

                current.push_str(trimmed);
                current_words = sentence_words;
                continue;
            }

            if current_words + sentence_words <= max_words {
                if !current.ends_with(' ') {
                    current.push(' ');
                }
                current.push_str(trimmed);
                current_words += sentence_words;
            } else {
                if !current.trim().is_empty() {
                    chunks.push(current.trim().to_string());
                }
                current.clear();
                current_words = 0;

                if sentence_words > max_words {
                    chunks.push(trimmed.to_string());
                } else {
                    current.push_str(trimmed);
                    current_words = sentence_words;
                }
            }
        }

        if !current.trim().is_empty() {
            chunks.push(current.trim().to_string());
        }

        if chunks.is_empty() {
            return Err(AppError::InvalidState(
                "Chunking produced no non-empty chunks".into(),
            ));
        }

        Ok(chunks
            .into_iter()
            .enumerate()
            .map(|(i, content)| Chunk::new(document_id.clone(), content, i))
            .collect())
    }
}

impl Default for ChunkingStrategy {
    fn default() -> Self {
        ChunkingStrategy::Semantic { max_tokens: 512 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_size_chunking() {
        let content = "Hello, World!";
        let doc_id = DocumentId::new();
        let strategy = ChunkingStrategy::FixedSize { size: 5 };

        let chunks = strategy.chunk(content, &doc_id).unwrap();

        assert!(!chunks.is_empty());
        assert!(chunks.len() >= 2); // "Hello" ", Wor" "ld!"
    }

    #[test]
    fn test_chunking_empty_content() {
        let doc_id = DocumentId::new();
        let strategy = ChunkingStrategy::default();

        let result = strategy.chunk("", &doc_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_chunking_whitespace_only() {
        let doc_id = DocumentId::new();
        let strategy = ChunkingStrategy::default();

        let result = strategy.chunk("   \n\t   ", &doc_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_chunk_has_correct_metadata() {
        let content = "Test content";
        let doc_id = DocumentId::new();
        let strategy = ChunkingStrategy::FixedSize { size: 100 };

        let chunks = strategy.chunk(content, &doc_id).unwrap();

        assert_eq!(chunks.len(), 1);
        let chunk = &chunks[0];
        assert_eq!(chunk.document_id(), &doc_id);
        assert_eq!(chunk.index(), 0);
        assert_eq!(chunk.content(), content);
    }

    #[test]
    fn test_default_strategy() {
        let strategy = ChunkingStrategy::default();
        assert_eq!(strategy, ChunkingStrategy::Semantic { max_tokens: 512 });
    }

    #[test]
    fn test_adapt_for_content_scales_semantic_strategy_for_large_docs() {
        let strategy = ChunkingStrategy::Semantic { max_tokens: 512 };
        let content = "word ".repeat(13_000);

        let adapted = strategy.adapt_for_content(&content);
        assert_eq!(adapted, ChunkingStrategy::Semantic { max_tokens: 1200 });
    }

    #[test]
    fn test_adapt_for_content_keeps_fixed_size_strategy() {
        let strategy = ChunkingStrategy::FixedSize { size: 512 };
        let content = "word ".repeat(30_000);

        let adapted = strategy.adapt_for_content(&content);
        assert_eq!(adapted, strategy);
    }

    #[test]
    fn test_infer_from_existing_chunks_uses_observed_sizes() {
        let doc_id = DocumentId::new();
        let mut c1 = Chunk::new(doc_id.clone(), "alpha beta gamma".to_string(), 0);
        c1.set_token_count(300);
        let mut c2 = Chunk::new(doc_id.clone(), "delta epsilon zeta".to_string(), 1);
        c2.set_token_count(500);
        let mut c3 = Chunk::new(doc_id, "eta theta iota".to_string(), 2);
        c3.set_token_count(700);

        let inferred = ChunkingStrategy::infer_from_existing_chunks(&[c1, c2, c3]);
        assert_eq!(inferred, ChunkingStrategy::Semantic { max_tokens: 840 });
    }

    #[test]
    fn test_zero_size_rejected() {
        let doc_id = DocumentId::new();
        let strategy = ChunkingStrategy::FixedSize { size: 0 };

        let result = strategy.chunk("content", &doc_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_chunking_preserves_content() {
        let content = "The quick brown fox jumps over the lazy dog";
        let doc_id = DocumentId::new();
        let strategy = ChunkingStrategy::FixedSize { size: 10 };

        let chunks = strategy.chunk(content, &doc_id).unwrap();

        // Reconstruct content from chunks
        let reconstructed: String = chunks.iter().map(|c| c.content()).collect();

        // Should preserve all characters (though order might be affected by chunking)
        assert_eq!(reconstructed.chars().count(), content.chars().count());
    }
}
