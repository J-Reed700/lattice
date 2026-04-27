use crate::services;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub async fn setup_embedding_service(
    model_dir: &Path,
) -> Option<Arc<crate::features::embedding::service::EmbeddingService>> {
    tracing::info!("Checking model files availability...");
    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    if !model_path.exists() || !tokenizer_path.exists() {
        tracing::info!(
            "AI model files not found at {:?}. App will start in degraded mode.",
            model_dir
        );
        tracing::info!(
            "AI-dependent features will be unavailable until models are downloaded from Settings → Models."
        );
        return None;
    }

    tracing::info!("Model files found, initializing embedding service...");
    match crate::features::embedding::service::EmbeddingService::new(&model_path) {
        Ok(embedder) => {
            tracing::info!("Embedding service initialized successfully");
            Some(Arc::new(embedder))
        }
        Err(e) => {
            tracing::warn!(
                "Failed to initialize embedding service: {}. App will start in degraded mode.",
                e
            );
            tracing::info!(
                "AI-dependent features will be unavailable. Try re-downloading models from Settings → Models."
            );
            None
        }
    }
}

pub fn setup_tokenizer(model_dir: &Path) -> Option<Arc<tokenizers::Tokenizer>> {
    let tokenizer_path = model_dir.join("tokenizer.json");

    if !tokenizer_path.exists() {
        tracing::info!(
            "Tokenizer file not found at {:?}. App will start in degraded mode.",
            tokenizer_path
        );
        return None;
    }

    match tokenizers::Tokenizer::from_file(&tokenizer_path) {
        Ok(tokenizer) => {
            tracing::info!("Tokenizer initialized successfully");
            Some(Arc::new(tokenizer))
        }
        Err(e) => {
            tracing::warn!(
                "Failed to load tokenizer: {}. App will start in degraded mode.",
                e
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_error_messages() {
        let model_dir = PathBuf::from("/test/models");
        let err = format!(
            "Required AI model files not found at {:?}.\n\n\
             The application requires embedding models to function.\n\n\
             Suggested actions:\n\
             - Download models using the initialize_models command\n\
             - Check if models were deleted or moved\n\
             - Ensure sufficient disk space (~500MB required)\n\n\
             Missing files:\n\
             - model.onnx: Missing\n\
             - tokenizer.json: Missing",
            model_dir
        );

        assert!(err.contains("Required AI model files not found"));
        assert!(err.contains("initialize_models command"));
        assert!(err.contains("/test/models"));
    }

    #[test]
    fn test_tokenizer_error_messages() {
        let tokenizer_path = PathBuf::from("/test/tokenizer.json");
        let err = format!(
            "Required tokenizer file not found at {:?}.

\
             The application requires the tokenizer to function.

\
             Suggested actions:
\
             - Download models using the initialize_models command
\
             - Check if tokenizer.json was deleted or moved
\
             - Ensure sufficient disk space",
            tokenizer_path
        );

        assert!(err.contains("Required tokenizer file not found"));
        assert!(err.contains("/test/tokenizer.json"));
    }
}
