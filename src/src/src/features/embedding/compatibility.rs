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

pub use crate::domain::model_management::EmbeddingCompatibility;

/// Architecture tags that the local CandleEmbeddingService can actually
/// load today. Each entry corresponds to a wired `ModelVariant` case in
/// `candle_service.rs` — promote here only after wiring the loader and
/// forward dispatch, otherwise the UI lies and downloads will fail at
/// model-load time.
const SUPPORTED_TAGS: &[&str] = &[
    "bert",
    "distilbert",
    "xlm-roberta",
    "xlm_roberta",
    "roberta", // RoBERTa repos commonly tag this; XLMRobertaModel handles them
    "jina_bert",
    "jina_bert_v2",
    "nomic_bert",
    "modernbert",
    "qwen3",
];

/// Architecture tags we recognize but can't run yet. The reason is shown
/// in the UI tooltip so users understand what's blocking the model.
const KNOWN_INCOMPATIBLE_TAGS: &[(&str, &str)] = &[
    // BERT-family variants without a Candle loader.
    (
        "mpnet",
        "MPNet loader not yet wired (no Candle module today; planned)",
    ),
    // Decoder-style architectures need last-token pooling and a different runner.
    (
        "gemma",
        "Gemma family — decoder-style, planned for a future release",
    ),
    (
        "gemma2",
        "Gemma 2 — decoder-style, planned for a future release",
    ),
    (
        "gemma3",
        "Gemma 3 — decoder-style, planned for a future release",
    ),
    (
        "gemma3_text",
        "Gemma 3 text — decoder-style, planned for a future release",
    ),
    (
        "qwen2",
        "Qwen2 — decoder-style, planned for a future release",
    ),
    (
        "llama",
        "Llama — decoder-style, planned for a future release",
    ),
    (
        "mistral",
        "Mistral — decoder-style, planned for a future release",
    ),
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
    fn xlm_roberta_is_compatible() {
        let result = detect_from_tags(&tags(&["xlm-roberta", "feature-extraction"]));
        assert!(
            result.is_compatible(),
            "expected Compatible, got {:?}",
            result
        );
    }

    #[test]
    fn nomic_bert_is_compatible() {
        let result = detect_from_tags(&tags(&["sentence-transformers", "nomic_bert"]));
        assert!(
            result.is_compatible(),
            "expected Compatible, got {:?}",
            result
        );
    }

    #[test]
    fn modernbert_is_compatible() {
        let result = detect_from_tags(&tags(&["sentence-transformers", "modernbert"]));
        assert!(
            result.is_compatible(),
            "expected Compatible, got {:?}",
            result
        );
    }

    #[test]
    fn distilbert_is_compatible() {
        let result = detect_from_tags(&tags(&["sentence-transformers", "distilbert"]));
        assert!(
            result.is_compatible(),
            "expected Compatible, got {:?}",
            result
        );
    }

    #[test]
    fn mpnet_remains_incompatible() {
        // mpnet has no Candle loader available — should still be Incompatible.
        let result = detect_from_tags(&tags(&["sentence-transformers", "mpnet"]));
        match result {
            EmbeddingCompatibility::Incompatible { architecture, .. } => {
                assert_eq!(architecture, "mpnet");
            }
            other => panic!("expected Incompatible for mpnet, got {:?}", other),
        }
    }

    #[test]
    fn qwen3_is_supported() {
        let result = detect_from_tags(&tags(&["sentence-transformers", "qwen3", "safetensors"]));
        assert!(matches!(result, EmbeddingCompatibility::Compatible { .. }));
    }

    #[test]
    fn no_recognized_arch_is_unknown() {
        let result = detect_from_tags(&tags(&["sentence-transformers", "wav2vec2-bert"]));
        assert!(matches!(result, EmbeddingCompatibility::Unknown));
    }

    #[test]
    fn empty_tags_are_unknown() {
        assert!(matches!(
            detect_from_tags(&[]),
            EmbeddingCompatibility::Unknown
        ));
    }

    #[test]
    fn specific_incompatible_arch_overrides_generic_bert_inheritance() {
        // HF inheritance tagging: an MPNet checkpoint commonly carries both
        // "mpnet" AND "bert" tags (sentence-transformers exports inherit
        // the bert tag). The specific incompatible arch must win over the
        // generic compatible one — otherwise we'd falsely promote and the
        // BertModel loader would crash at load (MPNet has no Candle loader).
        let result = detect_from_tags(&tags(&["sentence-transformers", "mpnet", "bert"]));
        match result {
            EmbeddingCompatibility::Incompatible { architecture, .. } => {
                assert_eq!(architecture, "mpnet");
            }
            other => panic!("expected Incompatible (mpnet wins), got {:?}", other),
        }
    }
}
