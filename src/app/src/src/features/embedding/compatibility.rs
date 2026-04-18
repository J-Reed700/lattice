//! Embedding model compatibility detection.
//!
//! Used by the HF catalog adapter to mark models as compatible /
//! incompatible with the local Candle inference path. Lets the UI badge
//! and disable models that won't load instead of letting users start
//! a 1GB+ download that will fail at first inference.
//!
//! ## Detection
//!
//! HuggingFace tags include the model architecture (`bert`, `qwen3`,
//! `gemma3`, etc.) for sentence-transformer / feature-extraction models.
//! We map those tags to the BERT-family architectures Candle's
//! `models::bert` loader handles. Decoder-style architectures
//! (Gemma, Qwen, Llama, Mistral) are reserved for a follow-up PR with
//! last-token pooling.

use serde::{Deserialize, Serialize};

/// Whether a model can be loaded by the current Candle embedding path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum EmbeddingCompatibility {
    /// Model is BERT-family — should load with Candle's standard BERT loader.
    Compatible {
        /// The architecture tag detected (`bert`, `xlm_roberta`, etc).
        architecture: String,
    },
    /// Model is recognized but its architecture isn't supported yet.
    /// Surface this so the UI can badge the row and explain *why*.
    Incompatible {
        /// The architecture tag detected (e.g. `qwen3`, `gemma3`, `llama`).
        architecture: String,
        /// Short human-readable reason shown in tooltip.
        reason: String,
    },
    /// We couldn't determine the architecture from tags. Treat as
    /// not-yet-supported (don't pretend to know what we don't).
    Unknown,
}

impl EmbeddingCompatibility {
    /// Is this model loadable via Candle today?
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible { .. })
    }
}

/// BERT-family architecture tags that load via `candle_transformers::models::bert`.
const BERT_FAMILY_TAGS: &[&str] = &[
    "bert",
    "distilbert",
    "xlm_roberta",
    "xlm-roberta",
    "roberta",
    "mpnet",
    "jina_bert",
    "jina_bert_v2",
    "nomic_bert",
    "modernbert",
];

/// Architecture tags we recognize as decoder-style. Surfaced in the
/// "Incompatible" reason so users see a meaningful message rather than a
/// generic failure.
const DECODER_FAMILY_TAGS: &[(&str, &str)] = &[
    ("gemma", "Gemma family — decoder-style, planned for a future release"),
    ("gemma2", "Gemma 2 — decoder-style, planned for a future release"),
    ("gemma3", "Gemma 3 — decoder-style, planned for a future release"),
    ("gemma3_text", "Gemma 3 text — decoder-style, planned for a future release"),
    ("qwen2", "Qwen2 — decoder-style, planned for a future release"),
    ("qwen3", "Qwen3 — decoder-style, planned for a future release"),
    ("llama", "Llama — decoder-style, planned for a future release"),
    ("mistral", "Mistral — decoder-style, planned for a future release"),
];

/// Inspect HF model tags and return a compatibility verdict.
///
/// Tags are case-insensitive; we normalize before comparing.
pub fn detect_from_tags(tags: &[String]) -> EmbeddingCompatibility {
    let normalized: Vec<String> = tags.iter().map(|t| t.to_ascii_lowercase()).collect();

    for tag in &normalized {
        if BERT_FAMILY_TAGS.contains(&tag.as_str()) {
            return EmbeddingCompatibility::Compatible {
                architecture: tag.clone(),
            };
        }
    }

    for tag in &normalized {
        if let Some((_, reason)) = DECODER_FAMILY_TAGS.iter().find(|(name, _)| name == tag) {
            return EmbeddingCompatibility::Incompatible {
                architecture: tag.clone(),
                reason: (*reason).to_string(),
            };
        }
    }

    EmbeddingCompatibility::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(slice: &[&str]) -> Vec<String> {
        slice.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn bert_tag_is_compatible() {
        let result = detect_from_tags(&tags(&["sentence-transformers", "pytorch", "bert"]));
        assert!(matches!(
            result,
            EmbeddingCompatibility::Compatible { ref architecture } if architecture == "bert"
        ));
    }

    #[test]
    fn xlm_roberta_with_dash_is_compatible() {
        // HF uses both forms in the wild
        let result = detect_from_tags(&tags(&["xlm-roberta", "feature-extraction"]));
        assert!(result.is_compatible());
    }

    #[test]
    fn qwen3_is_incompatible_with_reason() {
        let result = detect_from_tags(&tags(&["sentence-transformers", "qwen3", "safetensors"]));
        match result {
            EmbeddingCompatibility::Incompatible { architecture, reason } => {
                assert_eq!(architecture, "qwen3");
                assert!(reason.to_lowercase().contains("decoder"));
            }
            other => panic!("expected Incompatible, got {:?}", other),
        }
    }

    #[test]
    fn no_recognized_arch_is_unknown() {
        let result = detect_from_tags(&tags(&["sentence-transformers", "wav2vec2-bert"]));
        assert!(matches!(result, EmbeddingCompatibility::Unknown));
    }

    #[test]
    fn empty_tags_are_unknown() {
        assert!(matches!(detect_from_tags(&[]), EmbeddingCompatibility::Unknown));
    }
}
