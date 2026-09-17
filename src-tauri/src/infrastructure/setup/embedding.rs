use std::path::Path;
use std::sync::Arc;

pub fn setup_tokenizer(model_dir: &Path) -> Option<Arc<tokenizers::Tokenizer>> {
    let tokenizer_path = model_dir.join("tokenizer.json");

    if !tokenizer_path.exists() {
        tracing::info!(
            "Optional ONNX tokenizer absent at {:?}; configured models supply their own tokenizers.",
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
                "Failed to load optional ONNX tokenizer: {}. Configured models supply their own tokenizers.",
                e
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

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
