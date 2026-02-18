//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use crate::infrastructure::services::traits::*;
#[cfg(test)]
use crate::shared::error::Result;
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::{Arc, RwLock};

// ============================================================================

#[cfg(test)]
/// Mock embedding service that returns deterministic embeddings
///
/// Useful for testing without loading actual ML models.
/// Returns simple embeddings based on text hash.
pub struct MockEmbeddingService {
    dimension: usize,
}

#[cfg(test)]
impl MockEmbeddingService {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }

    /// Generate a deterministic embedding from text
    ///
    /// Uses a simple hash function to create reproducible embeddings.
    fn hash_to_embedding(&self, text: &str) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        // Generate deterministic embedding from hash
        let mut embedding = vec![0.0; self.dimension];
        for (i, val) in embedding.iter_mut().enumerate() {
            let shifted = hash.wrapping_add(i as u64);
            *val = ((shifted % 10000) as f32) / 10000.0 - 0.5;
        }

        // Normalize to unit vector
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for val in &mut embedding {
                *val /= magnitude;
            }
        }

        embedding
    }
}

#[cfg(test)]
impl Default for MockEmbeddingService {
    fn default() -> Self {
        Self::new(crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM)
    }
}

#[cfg(test)]
#[async_trait]
impl EmbeddingServiceTrait for MockEmbeddingService {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        Ok(self.hash_to_embedding(text))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.hash_to_embedding(t)).collect())
    }

    async fn embed_contextualized_chunks(
        &self,
        chunks: &[crate::infrastructure::indexing::chunker::ContextualizedChunk],
    ) -> Result<Vec<Vec<f32>>> {
        Ok(chunks
            .iter()
            .map(|c| self.hash_to_embedding(&c.contextualized_content))
            .collect())
    }
}
