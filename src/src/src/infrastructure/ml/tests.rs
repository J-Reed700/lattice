#[cfg(test)]
mod integration_tests {
    use super::super::*;
    use crate::domain::embedding_constants::{
        DEFAULT_EMBEDDING_DIM,
        DEFAULT_EMBEDDING_MODEL_NAME,
    };
    use crate::embeddings::{EmbeddingGenerator, ModelConfig};

    #[test]
    fn test_model_config_dimensions() {
        assert_eq!(ModelConfig::AllMpnetBaseV2.dimensions(), DEFAULT_EMBEDDING_DIM);
        assert_eq!(
            ModelConfig::AllMpnetBaseV2.model_name(),
            DEFAULT_EMBEDDING_MODEL_NAME
        );
    }

    #[test]
    fn test_generator_creation() {
        let gen = EmbeddingGenerator::default();
        assert_eq!(gen.dimensions(), DEFAULT_EMBEDDING_DIM);
        assert_eq!(gen.model_name(), DEFAULT_EMBEDDING_MODEL_NAME);
        assert!(!gen.is_loaded());
    }

    #[test]
    fn test_empty_text_validation() {
        let gen = EmbeddingGenerator::default();

        let result = gen.generate("");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));

        let result = gen.generate("   ");
        assert!(result.is_err());
    }

    #[test]
    fn test_single_embedding_generation() {
        let gen = EmbeddingGenerator::default();
        let result = gen.generate("Hello world");

        assert!(result.is_ok());
        let embedding = result.unwrap();

        assert_eq!(embedding.len(), DEFAULT_EMBEDDING_DIM);

        assert!(embedding.iter().any(|&x| x != 0.0));

        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-5,
            "Norm {} is not close to 1.0",
            norm
        );
    }

    #[test]
    fn test_batch_embedding_generation() {
        let gen = EmbeddingGenerator::default();
        let texts = vec![
            "Hello world".to_string(),
            "Rust is awesome".to_string(),
            "Embedding generation".to_string(),
        ];

        let result = gen.generate_batch(&texts);
        assert!(result.is_ok());

        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 3);

        for embedding in &embeddings {
            assert_eq!(embedding.len(), DEFAULT_EMBEDDING_DIM);

            let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn test_embedding_similarity() {
        let gen = EmbeddingGenerator::default();

        let emb1 = gen.generate("machine learning").unwrap();
        let emb2 = gen.generate("artificial intelligence").unwrap();
        let emb3 = gen.generate("cooking recipes").unwrap();

        // Cosine similarity
        let sim_12: f32 = emb1.iter().zip(&emb2).map(|(a, b)| a * b).sum();
        let sim_13: f32 = emb1.iter().zip(&emb3).map(|(a, b)| a * b).sum();

        // Related terms should have higher similarity
        assert!(
            sim_12 > sim_13,
            "ML and AI should be more similar than ML and cooking: {} vs {}",
            sim_12,
            sim_13
        );
    }

    #[test]
    fn test_empty_batch() {
        let gen = EmbeddingGenerator::default();
        let result = gen.generate_batch(&[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_batch_with_empty_text() {
        let gen = EmbeddingGenerator::default();
        let texts = vec!["Valid text".to_string(), "".to_string()];

        let result = gen.generate_batch(&texts);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_lazy_loading() {
        let gen = EmbeddingGenerator::default();
        assert!(!gen.is_loaded());

        // First call should load the model
        let _ = gen.generate("test");

        // Model should remain loaded for subsequent calls
        // Note: OnceCell ensures model is only loaded once
    }

    #[test]
    fn test_consistency() {
        let gen = EmbeddingGenerator::default();

        // Same text should produce same embedding
        let emb1 = gen.generate("consistent text").unwrap();
        let emb2 = gen.generate("consistent text").unwrap();

        for (a, b) in emb1.iter().zip(&emb2) {
            assert!((a - b).abs() < 1e-6, "Embeddings should be consistent");
        }
    }

    #[test]
    fn test_different_texts() {
        let gen = EmbeddingGenerator::default();

        let emb1 = gen.generate("first text").unwrap();
        let emb2 = gen.generate("second text").unwrap();

        // Different texts should produce different embeddings
        let is_different = emb1.iter().zip(&emb2).any(|(a, b)| (a - b).abs() > 1e-6);

        assert!(
            is_different,
            "Different texts should produce different embeddings"
        );
    }

    #[test]
    fn test_long_text() {
        let gen = EmbeddingGenerator::default();
        let long_text = "word ".repeat(100); // 500 words

        let result = gen.generate(&long_text);
        assert!(result.is_ok());

        let embedding = result.unwrap();
        assert_eq!(embedding.len(), DEFAULT_EMBEDDING_DIM);
    }

    #[test]
    fn test_special_characters() {
        let gen = EmbeddingGenerator::default();
        let texts = vec![
            "Hello, world!".to_string(),
            "Test@123#$%".to_string(),
            "Émojis 😀 🎉".to_string(),
        ];

        let result = gen.generate_batch(&texts);
        assert!(result.is_ok());

        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 3);
    }
}

#[cfg(test)]
mod validator_tests {
    use super::super::validator::*;
    use super::super::ModelConfig;
    use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;

    #[test]
    fn test_dimension_validation_success() {
        let embedding = vec![0.1; DEFAULT_EMBEDDING_DIM];
        let result = validate_embedding_dimension(&embedding, DEFAULT_EMBEDDING_DIM, "test");
        assert!(result.is_ok());
    }

    #[test]
    fn test_dimension_validation_failure() {
        let embedding = vec![0.1; 256];
        let result = validate_embedding_dimension(&embedding, DEFAULT_EMBEDDING_DIM, "test context");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("test context"));
        assert!(err.to_string().contains("256"));
        assert!(err.to_string().contains(&DEFAULT_EMBEDDING_DIM.to_string()));
    }

    #[test]
    fn test_compatibility_validation_success() {
        let config = ModelConfig::AllMpnetBaseV2;
        let result = validate_embedding_compatibility(
            DEFAULT_EMBEDDING_DIM,
            DEFAULT_EMBEDDING_DIM,
            &config
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_compatibility_validation_failure() {
        let config = ModelConfig::AllMpnetBaseV2;
        let result = validate_embedding_compatibility(DEFAULT_EMBEDDING_DIM, 512, &config);

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.to_string();

        assert!(err_msg.contains("CRITICAL"));
        assert!(err_msg.contains("512"));
        assert!(err_msg.contains(&DEFAULT_EMBEDDING_DIM.to_string()));
        assert!(err_msg.contains(DEFAULT_EMBEDDING_MODEL_NAME));
    }
}
