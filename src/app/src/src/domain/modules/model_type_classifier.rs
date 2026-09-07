use crate::domain::model_metadata::ModelType;
use crate::shared::error::AppError;
use std::path::Path;
use tracing::{debug, info, warn};

/// Normalized model identifier (lowercase, trimmed).
///
/// Ensures consistent comparison across different naming conventions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelIdentifier(String);

impl ModelIdentifier {
    /// Create a new model identifier from a string.
    ///
    /// Normalizes the input by converting to lowercase and trimming whitespace.
    pub fn new(model_name: impl AsRef<str>) -> Self {
        let normalized = model_name.as_ref().trim().to_lowercase();
        Self(normalized)
    }

    /// Get the normalized identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if the identifier contains a substring (case-insensitive).
    pub fn contains(&self, substring: &str) -> bool {
        self.0.contains(&substring.to_lowercase())
    }

    /// Check if the identifier starts with a prefix (case-insensitive).
    pub fn starts_with(&self, prefix: &str) -> bool {
        self.0.starts_with(&prefix.to_lowercase())
    }

    /// Check if the identifier ends with a suffix (case-insensitive).
    pub fn ends_with(&self, suffix: &str) -> bool {
        self.0.ends_with(&suffix.to_lowercase())
    }
}

impl From<String> for ModelIdentifier {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for ModelIdentifier {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// Classification result with confidence score and strategy used.
#[derive(Debug, Clone)]
pub struct ModelTypeClassification {
    pub model_type: ModelType,
    pub confidence: f32,
    pub strategy: ClassificationStrategy,
}

impl ModelTypeClassification {
    /// Create a new classification result.
    pub fn new(model_type: ModelType, confidence: f32, strategy: ClassificationStrategy) -> Self {
        Self {
            model_type,
            confidence,
            strategy,
        }
    }

    /// Check if the classification is high confidence (>= 0.8).
    pub fn is_high_confidence(&self) -> bool {
        self.confidence >= 0.8
    }

    /// Check if the classification is medium confidence (>= 0.5).
    pub fn is_medium_confidence(&self) -> bool {
        self.confidence >= 0.5
    }
}

/// Strategy used for model type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassificationStrategy {
    /// Direct catalog lookup (highest confidence).
    CatalogLookup,
    /// Pattern matching on model name.
    PatternMatching,
    /// File extension inference.
    FileExtension,
    /// Fallback to default.
    Fallback,
}

/// Domain service for classifying model types.
///
/// Uses a multi-strategy approach to determine the correct model type:
/// 1. Catalog lookup (explicit mappings)
/// 2. Pattern matching (naming conventions)
/// 3. File extension inference
/// 4. Fallback to text embeddings
pub struct ModelTypeClassifier;

impl ModelTypeClassifier {
    /// Classify a model type using all available strategies.
    ///
    /// Attempts strategies in order of confidence:
    /// 1. Catalog lookup (confidence: 1.0)
    /// 2. Pattern matching (confidence: 0.8)
    /// 3. File extension (confidence: 0.5)
    /// 4. Fallback (confidence: 0.3)
    pub fn classify(
        &self,
        model_name: impl AsRef<str>,
        file_path: Option<&Path>,
    ) -> ModelTypeClassification {
        let identifier = ModelIdentifier::new(model_name.as_ref());

        debug!(
            model_name = identifier.as_str(),
            has_file_path = file_path.is_some(),
            "Classifying model type"
        );

        // Strategy 1: Catalog lookup
        if let Some(classification) = self.classify_by_catalog(&identifier) {
            info!(
                model_name = identifier.as_str(),
                model_type = ?classification.model_type,
                strategy = ?classification.strategy,
                "Model type classified via catalog"
            );
            return classification;
        }

        // Strategy 2: Pattern matching
        if let Some(classification) = self.classify_by_pattern(&identifier) {
            info!(
                model_name = identifier.as_str(),
                model_type = ?classification.model_type,
                strategy = ?classification.strategy,
                "Model type classified via pattern matching"
            );
            return classification;
        }

        // Strategy 3: File extension (if available)
        if let Some(path) = file_path {
            if let Some(classification) = self.classify_by_extension(path) {
                info!(
                    model_name = identifier.as_str(),
                    model_type = ?classification.model_type,
                    strategy = ?classification.strategy,
                    file_path = ?path,
                    "Model type classified via file extension"
                );
                return classification;
            }
        }

        // Strategy 4: Fallback
        warn!(
            model_name = identifier.as_str(),
            "No specific classification found, using fallback (TextEmbeddings)"
        );
        ModelTypeClassification::new(
            ModelType::TextEmbeddings,
            0.3,
            ClassificationStrategy::Fallback,
        )
    }

    /// Attempt to classify by catalog lookup.
    ///
    /// Uses a curated list of known models with explicit type mappings.
    fn classify_by_catalog(&self, identifier: &ModelIdentifier) -> Option<ModelTypeClassification> {
        use crate::features::model_management::catalog_cache::ModelCatalogCache;

        let catalog = ModelCatalogCache::instance();

        if let Some(model_type) = catalog.lookup(identifier.as_str()) {
            return Some(ModelTypeClassification::new(
                model_type,
                1.0,
                ClassificationStrategy::CatalogLookup,
            ));
        }

        None
    }

    /// Attempt to classify by pattern matching.
    ///
    /// Matches common naming conventions and model families.
    fn classify_by_pattern(&self, identifier: &ModelIdentifier) -> Option<ModelTypeClassification> {
        // Transcription patterns. Evaluated first: whisper ships as a `.gguf`
        // file, which the extension fallback would otherwise call a language
        // model, putting it in the chat picker.
        if self.matches_transcription_pattern(identifier) {
            return Some(ModelTypeClassification::new(
                ModelType::Transcription,
                0.8,
                ClassificationStrategy::PatternMatching,
            ));
        }

        // Text embeddings patterns
        if self.matches_text_embeddings_pattern(identifier) {
            return Some(ModelTypeClassification::new(
                ModelType::TextEmbeddings,
                0.8,
                ClassificationStrategy::PatternMatching,
            ));
        }

        // Vision patterns
        if self.matches_vision_pattern(identifier) {
            return Some(ModelTypeClassification::new(
                ModelType::Vision,
                0.8,
                ClassificationStrategy::PatternMatching,
            ));
        }

        // Reranker patterns
        if self.matches_reranker_pattern(identifier) {
            return Some(ModelTypeClassification::new(
                ModelType::Reranker,
                0.8,
                ClassificationStrategy::PatternMatching,
            ));
        }

        // Language model patterns
        if self.matches_language_model_pattern(identifier) {
            return Some(ModelTypeClassification::new(
                ModelType::LanguageModel,
                0.8,
                ClassificationStrategy::PatternMatching,
            ));
        }

        None
    }

    /// Attempt to classify by file extension.
    fn classify_by_extension(&self, path: &Path) -> Option<ModelTypeClassification> {
        let extension = path.extension()?.to_str()?.to_lowercase();

        let model_type = match extension.as_str() {
            "onnx" => ModelType::TextEmbeddings,
            "gguf" => ModelType::LanguageModel,
            "safetensors" => ModelType::TextEmbeddings,
            _ => return None,
        };

        Some(ModelTypeClassification::new(
            model_type,
            0.5,
            ClassificationStrategy::FileExtension,
        ))
    }

    // Pattern matching helpers

    fn matches_transcription_pattern(&self, id: &ModelIdentifier) -> bool {
        id.contains("whisper") || id.contains("transcri") || id.contains("distil-whisper")
    }

    fn matches_text_embeddings_pattern(&self, id: &ModelIdentifier) -> bool {
        id.contains("embed")
            || id.contains("e5-")
            || id.contains("bge-")
            || id.contains("gte-")
            || id.starts_with("all-minilm")
            || id.starts_with("all-mpnet")
            || id.contains("sentence-transformers")
            || id.contains("instructor")
    }

    fn matches_vision_pattern(&self, id: &ModelIdentifier) -> bool {
        id.contains("clip")
            || id.contains("vision")
            || id.contains("vit")
            || id.contains("dino")
            || id.contains("blip")
            || id.contains("siglip")
    }

    fn matches_reranker_pattern(&self, id: &ModelIdentifier) -> bool {
        id.contains("rerank") || id.contains("cross-encoder") || id.contains("colbert")
    }

    fn matches_language_model_pattern(&self, id: &ModelIdentifier) -> bool {
        id.contains("llama")
            || id.contains("mistral")
            || id.contains("gpt")
            || id.contains("phi")
            || id.contains("gemma")
            || id.contains("qwen")
            || id.contains("chat")
            || id.contains("instruct")
            || id.ends_with(".gguf")
    }
}
