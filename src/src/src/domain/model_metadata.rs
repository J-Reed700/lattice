//! Model Metadata Domain Logic
//!
//! Parses GGUF model filenames to extract model metadata.
//! Handles common model naming conventions from HuggingFace and Ollama.
//! Also provides ModelFile for multi-file model support (e.g., ONNX models with separate data files).

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Represents a single file in a multi-file model
///
/// Some models (like BGE-M3 ONNX) consist of multiple files:
/// - model.onnx (structure)
/// - model.onnx_data (weights)
///
/// This struct tracks metadata for each file individually.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelFileMetadata {
    /// Filename (e.g., "model.onnx", "model.onnx_data")
    pub filename: String,

    /// Size in bytes
    pub size_bytes: u64,

    /// Optional checksum (SHA256)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,

    /// Download URL for this specific file
    pub url: String,

    /// Whether this file has been downloaded
    #[serde(default)]
    pub downloaded: bool,
}

impl ModelFileMetadata {
    /// Create a new ModelFileMetadata with required fields
    pub fn new(filename: String, url: String, size_bytes: u64) -> Self {
        Self {
            filename,
            size_bytes,
            checksum: None,
            url,
            downloaded: false,
        }
    }

    /// Create a new ModelFileMetadata with checksum
    pub fn with_checksum(filename: String, url: String, size_bytes: u64, checksum: String) -> Self {
        Self {
            filename,
            size_bytes,
            checksum: Some(checksum),
            url,
            downloaded: false,
        }
    }
}

/// Model type classification for inference engines.
///
/// Categorizes models by their primary purpose and capabilities.
/// Used for routing to appropriate inference engines and validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ModelType {
    /// Text embedding models (semantic similarity, search).
    TextEmbeddings,

    /// Vision models (image classification, CLIP).
    Vision,

    /// Reranking models (search result reranking).
    Reranker,

    /// Language models (chat, completion, instruction following).
    LanguageModel,

    /// Speech-to-text models (whisper family) used by audio ingest.
    Transcription,
}

impl ModelType {
    /// Check if this model type is compatible with chat operations.
    pub fn is_chat_compatible(&self) -> bool {
        matches!(self, Self::LanguageModel)
    }

    /// Check if this model type is compatible with embedding operations.
    pub fn is_embedding_compatible(&self) -> bool {
        matches!(self, Self::TextEmbeddings)
    }

    /// Convert to database string representation.
    pub fn to_db_string(&self) -> &'static str {
        match self {
            Self::TextEmbeddings => "text_embeddings",
            Self::Vision => "vision",
            Self::Reranker => "reranker",
            Self::LanguageModel => "language_model",
            // The `models.model_type` CHECK constraint predates this variant and
            // SQLite cannot relax a CHECK without rebuilding the table. `'custom'`
            // is in the allowed set and is written by nothing else, so the mapping
            // is a bijection. Rename to 'transcription' the next time `models` is
            // rebuilt for another reason.
            Self::Transcription => "custom",
        }
    }

    /// Parse from database string representation.
    pub fn from_db_string(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "text_embeddings" => Ok(Self::TextEmbeddings),
            "vision" => Ok(Self::Vision),
            "reranker" => Ok(Self::Reranker),
            "language_model" => Ok(Self::LanguageModel),
            // Legacy compatibility
            "embedding" => Ok(Self::TextEmbeddings),
            "chat" => Ok(Self::LanguageModel),
            // See `to_db_string`: transcription models round-trip through the
            // legacy `custom` slot until the `models` table is rebuilt.
            "custom" | "transcription" => Ok(Self::Transcription),
            _ => Err(format!("Invalid model type: {}", s)),
        }
    }
}

/// Metadata extracted from a model filename
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelMetadata {
    /// Unique identifier derived from filename
    pub model_id: String,

    /// Human-readable display name
    pub model_name: String,
}

impl ModelMetadata {
    /// Parse model metadata from a GGUF filename
    ///
    /// Handles various naming patterns:
    /// - "tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf" -> "TinyLlama 1.1B Chat (Q4_K_M)"
    /// - "llama-3.2-1b-instruct-q4_k_m.gguf" -> "Llama 3.2 1B Instruct (Q4_K_M)"
    /// - "mistral-7b-instruct-v0.2.Q5_K_S.gguf" -> "Mistral 7B Instruct v0.2 (Q5_K_S)"
    ///
    /// # Arguments
    ///
    /// * `filename` - The model filename (e.g., "model.gguf")
    ///
    /// # Returns
    ///
    /// ModelMetadata with extracted model_id and human-readable model_name
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::domain::model_metadata::ModelMetadata;
    ///
    /// let metadata = ModelMetadata::from_filename("tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf");
    /// assert_eq!(metadata.model_id, "tinyllama-1.1b-chat-v1.0");
    /// assert_eq!(metadata.model_name, "TinyLlama 1.1B Chat v1.0 (Q4_K_M)");
    /// ```
    pub fn from_filename(filename: &str) -> Self {
        // Remove .gguf extension
        let base_name = filename.strip_suffix(".gguf").unwrap_or(filename);

        // Split into parts
        let parts: Vec<&str> = base_name.split('.').collect();

        // Extract quantization (last part after final dot, if it looks like quantization)
        let (model_base, quantization) = if parts.len() > 1 {
            let potential_quant = parts.last().unwrap_or(&"");
            // Quantization patterns: Q4_K_M, Q5_K_S, q4_0, etc.
            if potential_quant.to_uppercase().starts_with('Q')
                && (potential_quant.contains('_') || potential_quant.len() <= 4)
            {
                let model_parts = parts.get(..parts.len() - 1).unwrap_or(&[]);
                (model_parts.join("."), Some(*potential_quant))
            } else {
                (base_name.to_string(), None)
            }
        } else {
            (base_name.to_string(), None)
        };

        // Generate model_id (lowercase base without quantization)
        let model_id = model_base.to_lowercase();

        // Generate pretty model_name
        let model_name = Self::generate_pretty_name(&model_base, quantization);

        Self {
            model_id,
            model_name,
        }
    }

    /// Parse metadata from a full file path
    ///
    /// Extracts just the filename and parses it.
    pub fn from_path(path: &Path) -> Self {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown-model.gguf");

        Self::from_filename(filename)
    }

    /// Generate a human-readable pretty name
    ///
    /// Converts "tinyllama-1.1b-chat-v1.0" to "TinyLlama 1.1B Chat v1.0"
    /// Adds quantization suffix: "(Q4_K_M)"
    fn generate_pretty_name(base: &str, quantization: Option<&str>) -> String {
        // Split on hyphens
        let parts: Vec<&str> = base.split('-').collect();

        // Process each part
        let formatted_parts: Vec<String> = parts
            .iter()
            .map(|part| {
                // Check if it's a size indicator (e.g., "1.1b", "7b", "13b")
                if Self::is_size_indicator(part) {
                    Self::format_size(part)
                } else {
                    // Capitalize first letter of each word
                    Self::capitalize_first(part)
                }
            })
            .collect();

        let mut result = formatted_parts.join(" ");

        // Add quantization suffix if present
        if let Some(quant) = quantization {
            result.push_str(&format!(" ({})", quant.to_uppercase()));
        }

        result
    }

    /// Check if a string looks like a model size (e.g., "1.1b", "7b", "13b")
    fn is_size_indicator(s: &str) -> bool {
        let lower = s.to_lowercase();
        // Matches: "1b", "1.1b", "7b", "13b", "70b", etc.
        lower.ends_with('b')
            && lower
                .chars()
                .take(lower.len() - 1)
                .all(|c| c.is_numeric() || c == '.')
    }

    /// Format size indicator with uppercase B (e.g., "1.1b" -> "1.1B")
    fn format_size(s: &str) -> String {
        s.to_uppercase()
    }

    /// Capitalize first letter of a string
    fn capitalize_first(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().chain(chars).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_tinyllama() {
        let meta = ModelMetadata::from_filename("tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf");
        assert_eq!(meta.model_id, "tinyllama-1.1b-chat-v1.0");
        assert_eq!(meta.model_name, "Tinyllama 1.1B Chat V1.0 (Q4_K_M)");
    }

    #[test]
    fn test_parse_llama3() {
        // Note: Quantization separated by dot, not dash
        let meta = ModelMetadata::from_filename("llama-3.2-1b-instruct.q4_k_m.gguf");
        assert_eq!(meta.model_id, "llama-3.2-1b-instruct");
        assert_eq!(meta.model_name, "Llama 3.2 1B Instruct (Q4_K_M)");
    }

    #[test]
    fn test_parse_mistral() {
        let meta = ModelMetadata::from_filename("mistral-7b-instruct-v0.2.Q5_K_S.gguf");
        assert_eq!(meta.model_id, "mistral-7b-instruct-v0.2");
        assert_eq!(meta.model_name, "Mistral 7B Instruct V0.2 (Q5_K_S)");
    }

    #[test]
    fn test_parse_without_quantization() {
        let meta = ModelMetadata::from_filename("phi-2-instruct.gguf");
        assert_eq!(meta.model_id, "phi-2-instruct");
        assert_eq!(meta.model_name, "Phi 2 Instruct");
    }

    #[test]
    fn test_parse_with_underscores() {
        let meta = ModelMetadata::from_filename("llama_3_2_1b_instruct.Q4_0.gguf");
        assert_eq!(meta.model_id, "llama_3_2_1b_instruct");
        // Note: underscores preserved, but we can enhance this later
        assert!(meta.model_name.contains("(Q4_0)"));
    }

    #[test]
    fn test_from_path() {
        let path = PathBuf::from("/models/tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf");
        let meta = ModelMetadata::from_path(&path);
        assert_eq!(meta.model_id, "tinyllama-1.1b-chat-v1.0");
        assert_eq!(meta.model_name, "Tinyllama 1.1B Chat V1.0 (Q4_K_M)");
    }

    #[test]
    fn test_size_indicator_detection() {
        assert!(ModelMetadata::is_size_indicator("1b"));
        assert!(ModelMetadata::is_size_indicator("1.1b"));
        assert!(ModelMetadata::is_size_indicator("7b"));
        assert!(ModelMetadata::is_size_indicator("13b"));
        assert!(!ModelMetadata::is_size_indicator("chat"));
        assert!(!ModelMetadata::is_size_indicator("instruct"));
    }

    #[test]
    fn test_capitalize_first() {
        assert_eq!(ModelMetadata::capitalize_first("hello"), "Hello");
        assert_eq!(ModelMetadata::capitalize_first("HELLO"), "HELLO");
        assert_eq!(ModelMetadata::capitalize_first(""), "");
    }

    #[test]
    fn test_format_size() {
        assert_eq!(ModelMetadata::format_size("1b"), "1B");
        assert_eq!(ModelMetadata::format_size("1.1b"), "1.1B");
        assert_eq!(ModelMetadata::format_size("7b"), "7B");
    }
}
