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

/// Architecture tags that the local CandleEmbeddingService can actually
/// load today. Currently just standard BERT — other BERT-family variants
/// (Nomic uses RoPE, Jina v2 uses ALiBi, DistilBert lacks token_type_ids,
/// MPNet uses relative position, ModernBERT uses RoPE+GeGLU) need their
/// own Candle loader before they can be marked Compatible. Marking them
/// Compatible here would lie to the UI: download succeeds, then loading
/// crashes or produces silent garbage.
const SUPPORTED_TAGS: &[&str] = &["bert"];

/// Architecture tags we recognize but can't run yet. The reason is shown
/// in the UI tooltip so users understand what's blocking the model.
const KNOWN_INCOMPATIBLE_TAGS: &[(&str, &str)] = &[
    // BERT-family variants that need their own Candle module.
    ("distilbert", "DistilBert loader not yet wired (no token_type_ids; planned)"),
    ("xlm_roberta", "XLM-RoBERTa loader not yet wired (planned)"),
    ("xlm-roberta", "XLM-RoBERTa loader not yet wired (planned)"),
    ("roberta", "RoBERTa loader not yet wired (planned)"),
    ("mpnet", "MPNet loader not yet wired (relative-position attention; planned)"),
    ("jina_bert", "Jina v2 loader not yet wired (ALiBi attention; planned)"),
    ("jina_bert_v2", "Jina v2 loader not yet wired (ALiBi attention; planned)"),
    ("nomic_bert", "Nomic loader not yet wired (RoPE + SwiGLU; planned)"),
    ("modernbert", "ModernBERT loader not yet wired (RoPE + GeGLU; planned)"),
    // Decoder-style architectures need last-token pooling and a different runner.
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

    // Scan incompatibles FIRST. HF often auto-tags inheritance — a DistilBERT
    // checkpoint may carry both "distilbert" and "bert" tags. The specific
    // architecture (distilbert) is the source of truth; the generic "bert"
    // tag would otherwise falsely promote it to Compatible and the loader
    // would crash at runtime.
    for tag in &normalized {
        if let Some((_, reason)) = KNOWN_INCOMPATIBLE_TAGS.iter().find(|(name, _)| name == tag) {
            return EmbeddingCompatibility::Incompatible {
                architecture: tag.clone(),
                reason: (*reason).to_string(),
            };
        }
    }

    for tag in &normalized {
        if SUPPORTED_TAGS.contains(&tag.as_str()) {
            return EmbeddingCompatibility::Compatible {
                architecture: tag.clone(),
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
    fn xlm_roberta_is_incompatible_today() {
        // XLM-RoBERTa is recognized but its loader isn't wired yet — should
        // surface as Incompatible with a clear reason, not falsely Compatible.
        let result = detect_from_tags(&tags(&["xlm-roberta", "feature-extraction"]));
        match result {
            EmbeddingCompatibility::Incompatible { architecture, reason } => {
                assert_eq!(architecture, "xlm-roberta");
                assert!(reason.to_lowercase().contains("not yet wired"));
            }
            other => panic!("expected Incompatible for xlm-roberta, got {:?}", other),
        }
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

    #[test]
    fn specific_arch_overrides_generic_bert_inheritance() {
        // HF inheritance tagging: a DistilBERT checkpoint may carry both
        // "distilbert" AND "bert" tags. The specific arch wins — otherwise
        // we'd promote it to Compatible and the BertModel loader would
        // crash at load (DistilBERT lacks token_type_ids etc.).
        let result = detect_from_tags(&tags(&["sentence-transformers", "distilbert", "bert"]));
        match result {
            EmbeddingCompatibility::Incompatible { architecture, .. } => {
                assert_eq!(architecture, "distilbert");
            }
            other => panic!("expected Incompatible (distilbert wins), got {:?}", other),
        }
    }
}
