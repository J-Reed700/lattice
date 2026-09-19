use crate::domain::embedding_constants::{
    DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME, QWEN3_EMBEDDING_MODEL_CURATED_ID,
    QWEN3_EMBEDDING_MODEL_DISPLAY_NAME,
};
use crate::domain::model_metadata::ModelType;
use std::collections::HashMap;
use std::sync::RwLock;

/// Thread-safe, lazily-initialized cache of known models and their types.
///
/// Provides O(1) lookup for model type classification based on a curated catalog.
/// Uses RwLock for concurrent read access with minimal contention.
pub struct ModelCatalogCache {
    catalog: RwLock<HashMap<String, ModelType>>,
}

impl ModelCatalogCache {
    /// Get the singleton instance of the catalog cache.
    pub fn instance() -> &'static Self {
        static INSTANCE: std::sync::OnceLock<ModelCatalogCache> = std::sync::OnceLock::new();
        INSTANCE.get_or_init(|| {
            let mut catalog = HashMap::new();
            Self::populate_catalog(&mut catalog);
            ModelCatalogCache {
                catalog: RwLock::new(catalog),
            }
        })
    }

    /// Look up a model type by normalized model name.
    ///
    /// Returns `Some(ModelType)` if found in catalog, `None` otherwise.
    pub fn lookup(&self, model_name: &str) -> Option<ModelType> {
        let normalized = model_name.trim().to_lowercase();
        self.catalog.read().ok()?.get(&normalized).copied()
    }

    /// Populate the catalog with known models.
    ///
    /// This is a curated list of popular models with explicit type mappings.
    fn populate_catalog(catalog: &mut HashMap<String, ModelType>) {
        // Text Embeddings - Popular sentence transformers
        catalog.insert("all-minilm-l6-v2".to_string(), ModelType::TextEmbeddings);
        catalog.insert("all-minilm-l12-v2".to_string(), ModelType::TextEmbeddings);
        catalog.insert(
            DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME.to_string(),
            ModelType::TextEmbeddings,
        );
        // Both first-run defaults, by catalog id and display name, so the
        // classifier never has to fall back to substring guessing for either.
        for name in [
            QWEN3_EMBEDDING_MODEL_CURATED_ID,
            QWEN3_EMBEDDING_MODEL_DISPLAY_NAME,
        ] {
            catalog.insert(name.to_lowercase(), ModelType::TextEmbeddings);
        }
        catalog.insert(
            "all-distilroberta-v1".to_string(),
            ModelType::TextEmbeddings,
        );

        // Text Embeddings - BGE models
        catalog.insert("bge-small-en-v1.5".to_string(), ModelType::TextEmbeddings);
        catalog.insert("bge-base-en-v1.5".to_string(), ModelType::TextEmbeddings);
        catalog.insert("bge-large-en-v1.5".to_string(), ModelType::TextEmbeddings);

        // Text Embeddings - E5 models
        catalog.insert("e5-small-v2".to_string(), ModelType::TextEmbeddings);
        catalog.insert("e5-base-v2".to_string(), ModelType::TextEmbeddings);
        catalog.insert("e5-large-v2".to_string(), ModelType::TextEmbeddings);
        catalog.insert(
            "e5-mistral-7b-instruct".to_string(),
            ModelType::TextEmbeddings,
        );

        // Text Embeddings - GTE models
        catalog.insert("gte-small".to_string(), ModelType::TextEmbeddings);
        catalog.insert("gte-base".to_string(), ModelType::TextEmbeddings);
        catalog.insert("gte-large".to_string(), ModelType::TextEmbeddings);

        // Text Embeddings - Nomic
        catalog.insert("nomic-embed-text-v1".to_string(), ModelType::TextEmbeddings);
        catalog.insert(
            "nomic-embed-text-v1.5".to_string(),
            ModelType::TextEmbeddings,
        );

        // Text Embeddings - Instructor models
        catalog.insert("instructor-base".to_string(), ModelType::TextEmbeddings);
        catalog.insert("instructor-large".to_string(), ModelType::TextEmbeddings);
        catalog.insert("instructor-xl".to_string(), ModelType::TextEmbeddings);

        // Vision Models - CLIP variants
        catalog.insert("clip-vit-base-patch32".to_string(), ModelType::Vision);
        catalog.insert("clip-vit-base-patch16".to_string(), ModelType::Vision);
        catalog.insert("clip-vit-large-patch14".to_string(), ModelType::Vision);
        catalog.insert("clip-vit-large-patch14-336".to_string(), ModelType::Vision);

        // Vision Models - Other vision encoders
        catalog.insert("dinov2-small".to_string(), ModelType::Vision);
        catalog.insert("dinov2-base".to_string(), ModelType::Vision);
        catalog.insert("dinov2-large".to_string(), ModelType::Vision);
        catalog.insert("siglip-so400m-patch14-384".to_string(), ModelType::Vision);

        // Reranker Models
        catalog.insert("bge-reranker-base".to_string(), ModelType::Reranker);
        catalog.insert("bge-reranker-large".to_string(), ModelType::Reranker);
        catalog.insert("bge-reranker-v2-m3".to_string(), ModelType::Reranker);

        // Reranker Models - Cross encoders
        catalog.insert(
            "cross-encoder/ms-marco-minilm-l-6-v2".to_string(),
            ModelType::Reranker,
        );
        catalog.insert(
            "cross-encoder/ms-marco-minilm-l-12-v2".to_string(),
            ModelType::Reranker,
        );

        // Language Models - Llama family
        catalog.insert("llama-2-7b".to_string(), ModelType::LanguageModel);
        catalog.insert("llama-2-7b-chat".to_string(), ModelType::LanguageModel);
        catalog.insert("llama-2-13b".to_string(), ModelType::LanguageModel);
        catalog.insert("llama-2-13b-chat".to_string(), ModelType::LanguageModel);
        catalog.insert("llama-3-8b".to_string(), ModelType::LanguageModel);
        catalog.insert("llama-3-8b-instruct".to_string(), ModelType::LanguageModel);

        // Language Models - Mistral family
        catalog.insert("mistral-7b-v0.1".to_string(), ModelType::LanguageModel);
        catalog.insert("mistral-7b-v0.2".to_string(), ModelType::LanguageModel);
        catalog.insert(
            "mistral-7b-instruct-v0.1".to_string(),
            ModelType::LanguageModel,
        );
        catalog.insert(
            "mistral-7b-instruct-v0.2".to_string(),
            ModelType::LanguageModel,
        );
        catalog.insert("mixtral-8x7b-v0.1".to_string(), ModelType::LanguageModel);
        catalog.insert(
            "mixtral-8x7b-instruct-v0.1".to_string(),
            ModelType::LanguageModel,
        );

        // Language Models - Phi family
        catalog.insert("phi-2".to_string(), ModelType::LanguageModel);
        catalog.insert("phi-3-mini".to_string(), ModelType::LanguageModel);
        catalog.insert("phi-3-small".to_string(), ModelType::LanguageModel);

        // Language Models - Gemma family
        catalog.insert("gemma-2b".to_string(), ModelType::LanguageModel);
        catalog.insert("gemma-2b-it".to_string(), ModelType::LanguageModel);
        catalog.insert("gemma-7b".to_string(), ModelType::LanguageModel);
        catalog.insert("gemma-7b-it".to_string(), ModelType::LanguageModel);

        // Language Models - Qwen family
        catalog.insert("qwen-1.5-0.5b".to_string(), ModelType::LanguageModel);
        catalog.insert("qwen-1.5-1.8b".to_string(), ModelType::LanguageModel);
        catalog.insert("qwen-1.5-4b".to_string(), ModelType::LanguageModel);
        catalog.insert("qwen-1.5-7b".to_string(), ModelType::LanguageModel);
        catalog.insert("qwen-1.5-7b-chat".to_string(), ModelType::LanguageModel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_lookup_exact_match() {
        let cache = ModelCatalogCache::instance();

        assert_eq!(
            cache.lookup("all-minilm-l6-v2"),
            Some(ModelType::TextEmbeddings)
        );
        assert_eq!(
            cache.lookup("llama-2-7b-chat"),
            Some(ModelType::LanguageModel)
        );
        assert_eq!(
            cache.lookup("clip-vit-base-patch32"),
            Some(ModelType::Vision)
        );
        assert_eq!(
            cache.lookup("bge-reranker-large"),
            Some(ModelType::Reranker)
        );
    }

    #[test]
    fn test_catalog_lookup_case_insensitive() {
        let cache = ModelCatalogCache::instance();

        assert_eq!(
            cache.lookup("ALL-MiniLM-L6-v2"),
            Some(ModelType::TextEmbeddings)
        );
        assert_eq!(
            cache.lookup("LLAMA-2-7B-CHAT"),
            Some(ModelType::LanguageModel)
        );
    }

    #[test]
    fn test_catalog_lookup_with_whitespace() {
        let cache = ModelCatalogCache::instance();

        assert_eq!(
            cache.lookup("  all-minilm-l6-v2  "),
            Some(ModelType::TextEmbeddings)
        );
    }

    #[test]
    fn test_catalog_lookup_not_found() {
        let cache = ModelCatalogCache::instance();

        assert_eq!(cache.lookup("unknown-model-xyz"), None);
        assert_eq!(cache.lookup(""), None);
    }

    #[test]
    fn test_catalog_contains_popular_embeddings() {
        let cache = ModelCatalogCache::instance();

        let embeddings = vec![
            "all-minilm-l6-v2",
            "bge-large-en-v1.5",
            "e5-large-v2",
            "gte-base",
            "nomic-embed-text-v1.5",
        ];

        for model in embeddings {
            assert_eq!(
                cache.lookup(model),
                Some(ModelType::TextEmbeddings),
                "Missing or wrong type for: {}",
                model
            );
        }
    }

    #[test]
    fn test_catalog_contains_popular_language_models() {
        let cache = ModelCatalogCache::instance();

        let llms = vec![
            "llama-2-7b-chat",
            "mistral-7b-instruct-v0.2",
            "phi-3-mini",
            "gemma-7b-it",
            "qwen-1.5-7b-chat",
        ];

        for model in llms {
            assert_eq!(
                cache.lookup(model),
                Some(ModelType::LanguageModel),
                "Missing or wrong type for: {}",
                model
            );
        }
    }

    #[test]
    fn test_catalog_thread_safety() {
        use std::thread;

        let cache = ModelCatalogCache::instance();

        // Spawn multiple threads doing concurrent lookups
        let handles: Vec<_> = (0..10)
            .map(|_| {
                thread::spawn(|| {
                    let cache = ModelCatalogCache::instance();
                    for _ in 0..100 {
                        let _ = cache.lookup("all-minilm-l6-v2");
                        let _ = cache.lookup("llama-2-7b-chat");
                        let _ = cache.lookup("unknown-model");
                    }
                })
            })
            .collect();

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(
            cache.lookup("all-minilm-l6-v2"),
            Some(ModelType::TextEmbeddings)
        );
    }
}
