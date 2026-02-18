//! # Embedding Entity
//!
//! Domain entity representing embedding metadata.
//!
//! This is a pure domain entity containing only the business logic and metadata
//! about embeddings. The actual vector data is handled by the infrastructure layer.

use crate::shared::domain_types::ChunkId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Embedding entity (metadata only).
///
/// Represents metadata about a text embedding without storing the actual vector.
/// The vector itself is an infrastructure concern handled by the embedding service
/// and vector store.
///
/// ## Pure Domain Model
///
/// This entity focuses on the business concept of "what is embedded and when",
/// not the technical details of how vectors are stored or computed.
///
/// ## Example
///
/// ```rust,no_run
/// use vault_desktop::domain::entities::embedding::Embedding;
/// use vault_desktop::domain_types::ChunkId;
///
/// use vault_desktop::domain::embedding_constants::{
///     DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_MODEL_NAME
/// };
/// let chunk_id = ChunkId::new();
/// let embedding = Embedding::new(
///     chunk_id.clone(),
///     DEFAULT_EMBEDDING_MODEL_NAME.to_string(),
///     DEFAULT_EMBEDDING_DIM,
/// );
///
/// assert_eq!(embedding.model_name(), DEFAULT_EMBEDDING_MODEL_NAME);
/// assert_eq!(embedding.dimension(), DEFAULT_EMBEDDING_DIM);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Embedding {
    /// ID of the chunk this embedding represents
    chunk_id: ChunkId,

    /// Name of the embedding model used
    model_name: String,

    /// Dimension of the embedding vector
    dimension: usize,

    /// When the embedding was computed
    computed_at: DateTime<Utc>,

    /// Model version (if applicable)
    model_version: Option<String>,
}

impl Embedding {
    /// Create a new embedding metadata.
    ///
    /// # Arguments
    ///
    /// * `chunk_id` - ID of the embedded chunk
    /// * `model_name` - Name of the embedding model (e.g., DEFAULT_EMBEDDING_MODEL_NAME)
    /// * `dimension` - Vector dimension
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::entities::embedding::Embedding;
    /// use vault_desktop::domain_types::ChunkId;
    ///
    /// use vault_desktop::domain::embedding_constants::DEFAULT_EMBEDDING_MODEL_NAME;
    /// let embedding = Embedding::new(
    ///     ChunkId::new(),
    ///     DEFAULT_EMBEDDING_MODEL_NAME.to_string(),
    ///     1536,
    /// );
    /// ```
    pub fn new(chunk_id: ChunkId, model_name: String, dimension: usize) -> Self {
        Self {
            chunk_id,
            model_name,
            dimension,
            computed_at: Utc::now(),
            model_version: None,
        }
    }

    /// Create embedding metadata with model version.
    ///
    /// # Arguments
    ///
    /// * `chunk_id` - ID of the embedded chunk
    /// * `model_name` - Name of the embedding model
    /// * `model_version` - Model version string
    /// * `dimension` - Vector dimension
    pub fn with_version(
        chunk_id: ChunkId,
        model_name: String,
        model_version: String,
        dimension: usize,
    ) -> Self {
        Self {
            chunk_id,
            model_name,
            dimension,
            computed_at: Utc::now(),
            model_version: Some(model_version),
        }
    }

    /// Create embedding metadata with explicit timestamp (for reconstruction).
    ///
    /// This is typically used by repositories when loading from storage.
    pub fn with_timestamp(
        chunk_id: ChunkId,
        model_name: String,
        dimension: usize,
        computed_at: DateTime<Utc>,
    ) -> Self {
        Self {
            chunk_id,
            model_name,
            dimension,
            computed_at,
            model_version: None,
        }
    }

    /// Get the embedding ID (same as chunk ID).
    pub fn id(&self) -> &ChunkId {
        &self.chunk_id
    }

    /// Get the chunk ID.
    pub fn chunk_id(&self) -> &ChunkId {
        &self.chunk_id
    }

    /// Get the model name.
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Get the embedding dimension.
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Get when the embedding was computed.
    pub fn computed_at(&self) -> DateTime<Utc> {
        self.computed_at
    }

    /// Get the model version, if available.
    pub fn model_version(&self) -> Option<&str> {
        self.model_version.as_deref()
    }

    /// Check if this embedding is compatible with a given model.
    ///
    /// Two embeddings are compatible if they use the same model and dimension.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::entities::embedding::Embedding;
    /// use vault_desktop::domain_types::ChunkId;
    ///
    /// let emb1 = Embedding::new(
    ///     ChunkId::new(),
    ///     "model-a".to_string(),
    ///     384,
    /// );
    ///
    /// assert!(emb1.is_compatible_with("model-a", 384));
    /// assert!(!emb1.is_compatible_with("model-b", 384));
    /// assert!(!emb1.is_compatible_with("model-a", 512));
    /// ```
    pub fn is_compatible_with(&self, model_name: &str, dimension: usize) -> bool {
        self.model_name == model_name && self.dimension == dimension
    }

    /// Check if the embedding is stale and needs recomputation.
    ///
    /// Business rule: Embeddings older than the given age threshold are considered stale.
    ///
    /// # Arguments
    ///
    /// * `max_age` - Maximum age in seconds
    pub fn is_stale(&self, max_age_seconds: i64) -> bool {
        let age = Utc::now()
            .signed_duration_since(self.computed_at)
            .num_seconds();
        age > max_age_seconds
    }

    /// Get the age of this embedding in seconds.
    pub fn age_seconds(&self) -> i64 {
        Utc::now()
            .signed_duration_since(self.computed_at)
            .num_seconds()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::embedding_constants::{DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_MODEL_NAME};
    use chrono::Duration;

    #[test]
    fn test_embedding_creation() {
        let chunk_id = ChunkId::new();
        let embedding = Embedding::new(
            chunk_id.clone(),
            DEFAULT_EMBEDDING_MODEL_NAME.to_string(),
            DEFAULT_EMBEDDING_DIM,
        );

        assert_eq!(embedding.chunk_id(), &chunk_id);
        assert_eq!(embedding.model_name(), DEFAULT_EMBEDDING_MODEL_NAME);
        assert_eq!(embedding.dimension(), DEFAULT_EMBEDDING_DIM);
        assert!(embedding.computed_at() <= Utc::now());
    }

    #[test]
    fn test_embedding_with_version() {
        let embedding = Embedding::with_version(
            ChunkId::new(),
            DEFAULT_EMBEDDING_MODEL_NAME.to_string(),
            "v2".to_string(),
            1536,
        );

        assert_eq!(embedding.model_version(), Some("v2"));
    }

    #[test]
    fn test_is_compatible_with() {
        let embedding = Embedding::new(ChunkId::new(), "model-a".to_string(), 384);

        assert!(embedding.is_compatible_with("model-a", 384));
        assert!(!embedding.is_compatible_with("model-b", 384));
        assert!(!embedding.is_compatible_with("model-a", 512));
    }

    #[test]
    fn test_is_stale() {
        // Create an old embedding
        let old_time = Utc::now() - Duration::days(7);
        let old_embedding =
            Embedding::with_timestamp(ChunkId::new(), "model".to_string(), 384, old_time);

        // 1 day = 86400 seconds
        assert!(old_embedding.is_stale(86400)); // Stale after 1 day
        assert!(!old_embedding.is_stale(86400 * 30)); // Not stale if threshold is 30 days

        // Create a fresh embedding
        let fresh_embedding = Embedding::new(ChunkId::new(), "model".to_string(), 384);

        assert!(!fresh_embedding.is_stale(86400)); // Not stale
    }

    #[test]
    fn test_age_seconds() {
        let embedding = Embedding::new(ChunkId::new(), "model".to_string(), 384);

        let age = embedding.age_seconds();
        assert!((0..10).contains(&age)); // Should be very recent (< 10 seconds)
    }

    #[test]
    fn test_serialization() {
        let embedding =
            Embedding::with_version(ChunkId::new(), "model".to_string(), "v1".to_string(), 512);

        let json = serde_json::to_string(&embedding).unwrap();
        let deserialized: Embedding = serde_json::from_str(&json).unwrap();

        assert_eq!(embedding, deserialized);
    }
}
