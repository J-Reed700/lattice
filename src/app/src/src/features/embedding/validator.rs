use super::generator::ModelConfig;
use crate::shared::error::{AppError, Result};

/// Error raised when embedding dimensions don't match between systems
#[derive(Debug, thiserror::Error)]
#[error("Embedding dimension mismatch: {message}")]
pub struct DimensionMismatchError {
    pub message: String,
}

impl From<DimensionMismatchError> for AppError {
    fn from(err: DimensionMismatchError) -> Self {
        AppError::EmbeddingFailed {
            reason: err.message,
        }
    }
}

/// Validate that an embedding has the correct dimension
///
/// # Arguments
/// * `embedding` - The embedding vector to validate
/// * `expected_dim` - Expected dimension
/// * `context` - Additional context for error message
///
/// # Errors
/// Returns error if dimension doesn't match
pub fn validate_embedding_dimension(
    embedding: &[f32],
    expected_dim: usize,
    context: &str,
) -> Result<()> {
    let actual_dim = embedding.len();

    if actual_dim != expected_dim {
        let error_msg = if context.is_empty() {
            format!(
                "Embedding dimension mismatch. Expected {} dimensions, but got {} dimensions.",
                expected_dim, actual_dim
            )
        } else {
            format!(
                "Embedding dimension mismatch: {}. Expected {} dimensions, but got {} dimensions.",
                context, expected_dim, actual_dim
            )
        };

        return Err(DimensionMismatchError { message: error_msg }.into());
    }

    Ok(())
}

/// Validate that desktop and backend embedding dimensions match
///
/// # Arguments
/// * `desktop_dim` - Embedding dimension from desktop system
/// * `backend_dim` - Embedding dimension from backend system
///
/// # Errors
/// Returns error if dimensions don't match
pub fn validate_embedding_compatibility(
    desktop_dim: usize,
    backend_dim: usize,
    model_config: &ModelConfig,
) -> Result<()> {
    if desktop_dim != backend_dim {
        let error_msg = format!(
            "CRITICAL: Embedding dimension mismatch detected!\n\
            Desktop uses {}-dimensional embeddings.\n\
            Backend uses {}-dimensional embeddings.\n\
            Sync operations will crash until this is fixed.\n\
            Both systems must use the same embedding model and dimension.\n\
            Current expected: {} dimensions with model {}",
            desktop_dim,
            backend_dim,
            model_config.dimensions(),
            model_config.model_name()
        );

        return Err(DimensionMismatchError { message: error_msg }.into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;

    #[test]
    fn test_validate_embedding_dimension_success() {
        let embedding = vec![0.1, 0.2, 0.3];
        let result = validate_embedding_dimension(&embedding, 3, "test");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_embedding_dimension_failure() {
        let embedding = vec![0.1, 0.2, 0.3];
        let result = validate_embedding_dimension(&embedding, 5, "test context");
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("test context"));
        assert!(err_msg.contains("Expected 5"));
        assert!(err_msg.contains("got 3"));
    }

    #[test]
    fn test_validate_compatibility_success() {
        let config = ModelConfig::AllMpnetBaseV2;
        let result =
            validate_embedding_compatibility(DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_DIM, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_compatibility_failure() {
        let config = ModelConfig::AllMpnetBaseV2;
        let result = validate_embedding_compatibility(DEFAULT_EMBEDDING_DIM, 384, &config);
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("CRITICAL"));
        assert!(err_msg.contains("384"));
        assert!(err_msg.contains(&DEFAULT_EMBEDDING_DIM.to_string()));
    }
}
