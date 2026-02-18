#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! # Mock Implementations
// Test code - allow common test patterns
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//!
//! Mock implementations of external services for testing.
//!
//! ## Purpose
//!
//! Provides deterministic, fast, in-memory implementations of:
//! - Embedding generation
//! - LLM services
//! - External APIs
//!
//! ## Design Principles
//!
//! - **Fast**: No actual model loading or network calls
//! - **Deterministic**: Same inputs produce same outputs
//! - **Configurable**: Easy to simulate different scenarios
//!
//! ## Examples
//!
//! ```rust
//! let embedder = MockEmbedder::new(384);
//! let embedding = embedder.embed_text("test").await?;
//! assert_eq!(embedding.len(), 384);
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use vault::error::{AppError, Result};

// ============================================================================
// MockEmbedder
// ============================================================================

/// Mock embedding service for testing.
///
/// Generates deterministic embeddings without requiring actual models.
/// Uses simple hashing to create consistent embeddings for the same input.
///
/// # Examples
///
/// ```rust
/// let embedder = MockEmbedder::new(384);
/// let emb1 = embedder.embed_text("hello").await?;
/// let emb2 = embedder.embed_text("hello").await?;
/// assert_eq!(emb1, emb2); // Deterministic
/// ```
#[derive(Debug)]
pub struct MockEmbedder {
    /// Embedding dimension
    dimensions: usize,

    /// Cache of previously generated embeddings
    cache: Arc<Mutex<HashMap<String, Vec<f32>>>>,

    /// Whether to use deterministic or random embeddings
    deterministic: bool,
}

impl MockEmbedder {
    /// Create a new mock embedder.
    ///
    /// # Arguments
    ///
    /// * `dimensions` - Size of embedding vectors
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions,
            cache: Arc::new(Mutex::new(HashMap::new())),
            deterministic: true,
        }
    }

    /// Create a mock embedder with random (non-deterministic) embeddings.
    pub fn new_random(dimensions: usize) -> Self {
        Self {
            dimensions,
            cache: Arc::new(Mutex::new(HashMap::new())),
            deterministic: false,
        }
    }

    /// Embed a single text string.
    ///
    /// # Arguments
    ///
    /// * `text` - Text to embed
    ///
    /// # Returns
    ///
    /// Embedding vector of configured dimensions
    pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        // Check cache first
        let mut cache = self.cache.lock().await;

        if let Some(cached) = cache.get(text) {
            return Ok(cached.clone());
        }

        // Generate embedding
        let embedding = if self.deterministic {
            self.generate_deterministic_embedding(text)
        } else {
            self.generate_random_embedding()
        };

        // Cache it
        cache.insert(text.to_string(), embedding.clone());

        Ok(embedding)
    }

    /// Embed a batch of texts.
    ///
    /// # Arguments
    ///
    /// * `texts` - Texts to embed
    ///
    /// # Returns
    ///
    /// Vector of embeddings
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut embeddings = Vec::new();

        for text in texts {
            embeddings.push(self.embed_text(text).await?);
        }

        Ok(embeddings)
    }

    /// Get embedding dimensions.
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Clear the embedding cache.
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.lock().await;
        cache.clear();
    }

    /// Generate a deterministic embedding based on text hash.
    fn generate_deterministic_embedding(&self, text: &str) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        // Use hash to seed a simple PRNG
        let mut seed = hash;
        let mut embedding = Vec::with_capacity(self.dimensions);

        for _ in 0..self.dimensions {
            // Simple LCG (Linear Congruential Generator)
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let value = ((seed / 65536) % 32768) as f32 / 32768.0;
            // Normalize to [-1, 1]
            embedding.push(value * 2.0 - 1.0);
        }

        // Normalize to unit vector
        normalize_vector(&mut embedding);

        embedding
    }

    /// Generate a random embedding.
    fn generate_random_embedding(&self) -> Vec<f32> {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let mut embedding: Vec<f32> = (0..self.dimensions)
            .map(|_| rng.gen_range(-1.0..1.0))
            .collect();

        // Normalize to unit vector
        normalize_vector(&mut embedding);

        embedding
    }
}

/// Normalize a vector to unit length.
fn normalize_vector(vec: &mut [f32]) {
    let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude > 0.0 {
        for val in vec.iter_mut() {
            *val /= magnitude;
        }
    }
}

// ============================================================================
// MockLLMClient
// ============================================================================

/// Mock LLM client for testing.
///
/// Returns predefined responses without making actual API calls.
#[derive(Debug)]
pub struct MockLLMClient {
    responses: Arc<Mutex<HashMap<String, String>>>,
    default_response: String,
}

impl MockLLMClient {
    /// Create a new mock LLM client.
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
            default_response: "This is a mock LLM response.".to_string(),
        }
    }

    /// Set a response for a specific prompt.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The prompt to match
    /// * `response` - The response to return
    pub async fn set_response(&self, prompt: &str, response: &str) {
        let mut responses = self.responses.lock().await;
        responses.insert(prompt.to_string(), response.to_string());
    }

    /// Generate a response for a prompt.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The prompt to generate a response for
    ///
    /// # Returns
    ///
    /// Mock response text
    pub async fn generate(&self, prompt: &str) -> Result<String> {
        let responses = self.responses.lock().await;

        Ok(responses
            .get(prompt)
            .cloned()
            .unwrap_or_else(|| self.default_response.clone()))
    }

    /// Generate a streaming response (returns full response).
    pub async fn generate_stream(&self, prompt: &str) -> Result<Vec<String>> {
        let response = self.generate(prompt).await?;

        // Split into chunks to simulate streaming
        let chunks: Vec<String> = response
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        Ok(chunks)
    }
}

impl Default for MockLLMClient {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// MockSearchIndex
// ============================================================================

/// Mock search index for testing.
///
/// Provides simple in-memory search without actual vector indexing.
#[derive(Debug)]
pub struct MockSearchIndex {
    documents: Arc<Mutex<HashMap<String, (String, Vec<f32>)>>>, // id -> (content, embedding)
}

impl MockSearchIndex {
    /// Create a new mock search index.
    pub fn new() -> Self {
        Self {
            documents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add a document to the index.
    ///
    /// # Arguments
    ///
    /// * `id` - Document ID
    /// * `content` - Document content
    /// * `embedding` - Document embedding
    pub async fn add_document(&self, id: &str, content: &str, embedding: Vec<f32>) {
        let mut docs = self.documents.lock().await;
        docs.insert(id.to_string(), (content.to_string(), embedding));
    }

    /// Search for documents.
    ///
    /// # Arguments
    ///
    /// * `query_embedding` - Query embedding vector
    /// * `limit` - Maximum number of results
    ///
    /// # Returns
    ///
    /// Vector of (document_id, similarity_score) tuples
    pub async fn search(&self, query_embedding: &[f32], limit: usize) -> Vec<(String, f32)> {
        let docs = self.documents.lock().await;

        let mut results: Vec<(String, f32)> = docs
            .iter()
            .map(|(id, (_content, emb))| {
                let similarity = cosine_similarity(query_embedding, emb);
                (id.clone(), similarity)
            })
            .collect();

        // Sort by similarity (descending)
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top-k
        results.truncate(limit);

        results
    }

    /// Get document count.
    pub async fn document_count(&self) -> usize {
        let docs = self.documents.lock().await;
        docs.len()
    }

    /// Clear the index.
    pub async fn clear(&self) {
        let mut docs = self.documents.lock().await;
        docs.clear();
    }
}

impl Default for MockSearchIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();

    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a mock embedding with specific characteristics.
pub fn create_mock_embedding(dimensions: usize, seed: u64) -> Vec<f32> {
    let mut seed_val = seed;
    let mut embedding = Vec::with_capacity(dimensions);

    for _ in 0..dimensions {
        seed_val = seed_val.wrapping_mul(1103515245).wrapping_add(12345);
        let value = ((seed_val / 65536) % 32768) as f32 / 32768.0;
        embedding.push(value * 2.0 - 1.0);
    }

    normalize_vector(&mut embedding);
    embedding
}

/// Create similar embeddings for testing similarity search.
pub fn create_similar_embeddings(base: &[f32], count: usize, variance: f32) -> Vec<Vec<f32>> {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    (0..count)
        .map(|_| {
            let mut emb: Vec<f32> = base
                .iter()
                .map(|&x| x + rng.gen_range(-variance..variance))
                .collect();

            normalize_vector(&mut emb);
            emb
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_embedder_deterministic() {
        let embedder = MockEmbedder::new(384);

        let emb1 = embedder.embed_text("test").await.unwrap();
        let emb2 = embedder.embed_text("test").await.unwrap();

        assert_eq!(emb1.len(), 384);
        assert_eq!(emb1, emb2);
    }

    #[tokio::test]
    async fn test_mock_embedder_different_texts() {
        let embedder = MockEmbedder::new(384);

        let emb1 = embedder.embed_text("hello").await.unwrap();
        let emb2 = embedder.embed_text("world").await.unwrap();

        assert_ne!(emb1, emb2);
    }

    #[tokio::test]
    async fn test_mock_embedder_batch() {
        let embedder = MockEmbedder::new(384);

        let texts = vec!["one".to_string(), "two".to_string(), "three".to_string()];
        let embeddings = embedder.embed_batch(&texts).await.unwrap();

        assert_eq!(embeddings.len(), 3);
        assert_eq!(embeddings[0].len(), 384);
    }

    #[tokio::test]
    async fn test_mock_llm_client() {
        let client = MockLLMClient::new();

        client
            .set_response("test prompt", "test response")
            .await;

        let response = client.generate("test prompt").await.unwrap();
        assert_eq!(response, "test response");

        let default = client.generate("unknown prompt").await.unwrap();
        assert_eq!(default, "This is a mock LLM response.");
    }

    #[tokio::test]
    async fn test_mock_search_index() {
        let index = MockSearchIndex::new();
        let embedder = MockEmbedder::new(384);

        // Add documents
        let emb1 = embedder.embed_text("machine learning").await.unwrap();
        let emb2 = embedder.embed_text("deep learning").await.unwrap();
        let emb3 = embedder.embed_text("rust programming").await.unwrap();

        index.add_document("doc1", "machine learning", emb1.clone()).await;
        index.add_document("doc2", "deep learning", emb2).await;
        index.add_document("doc3", "rust programming", emb3).await;

        assert_eq!(index.document_count().await, 3);

        // Search
        let results = index.search(&emb1, 2).await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "doc1"); // Most similar is itself
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];

        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.0001);
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.0001);
    }

    #[test]
    fn test_normalize_vector() {
        let mut vec = vec![3.0, 4.0];
        normalize_vector(&mut vec);

        let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_create_similar_embeddings() {
        let base = vec![1.0, 0.0, 0.0, 0.0];
        let similar = create_similar_embeddings(&base, 3, 0.1);

        assert_eq!(similar.len(), 3);

        for emb in similar {
            assert_eq!(emb.len(), 4);
            let sim = cosine_similarity(&base, &emb);
            assert!(sim > 0.8); // Should be similar
        }
    }
}
