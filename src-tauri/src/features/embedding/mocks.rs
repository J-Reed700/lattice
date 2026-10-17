//! Mock implementations for testing
//!
//! This module provides mock implementations of the embedding port.

#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use lattice_core::application::ports::EmbeddingPort;
#[cfg(test)]
use lattice_core::shared::error::Result;

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
        Self::new(lattice_core::domain::models::embedding_defaults::DEFAULT_EMBEDDING_DIM)
    }
}

#[cfg(test)]
#[async_trait]
impl EmbeddingPort for MockEmbeddingService {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        Ok(self.hash_to_embedding(text))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.hash_to_embedding(t)).collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}
