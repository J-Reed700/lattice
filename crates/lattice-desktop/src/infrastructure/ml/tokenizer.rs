//! Tokenizer loading shared by the embedding models and the reranker.

use lattice_core::shared::error::{AppError, Result};
use std::path::Path;
use tokenizers::Tokenizer;

/// HF changed BPE merge serialization from "a b" strings to ["a", "b"] pairs.
/// Normalize that representation for the existing tokenizer runtime; reject new
/// semantics the runtime cannot honor instead of silently ignoring them.
pub fn load_tokenizer(path: &Path) -> Result<Tokenizer> {
    let bytes = std::fs::read(path).map_err(|e| AppError::InvalidConfig(e.to_string()))?;
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| AppError::InvalidConfig(format!("Invalid tokenizer: {e}")))?;
    if value
        .pointer("/model/ignore_merges")
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        return Err(AppError::InvalidConfig(
            "This tokenizer requires ignore_merges support".into(),
        ));
    }
    if let Some(merges) = value
        .pointer_mut("/model/merges")
        .and_then(|v| v.as_array_mut())
    {
        for merge in merges {
            if let Some(pair) = merge.as_array() {
                let left = pair.first().and_then(|v| v.as_str());
                let right = pair.get(1).and_then(|v| v.as_str());
                match (pair.len(), left, right) {
                    (2, Some(left), Some(right)) if !left.contains(' ') && !right.contains(' ') => {
                        *merge = serde_json::Value::String(format!("{left} {right}"));
                    }
                    _ => return Err(AppError::InvalidConfig("Unsupported BPE merge pair".into())),
                }
            }
        }
    }
    let normalized =
        serde_json::to_vec(&value).map_err(|e| AppError::InvalidConfig(e.to_string()))?;
    Tokenizer::from_bytes(normalized)
        .map_err(|e| AppError::InvalidConfig(format!("Invalid tokenizer: {e}")))
}

#[cfg(test)]
mod serialization_tests {
    use super::*;
    #[test]
    fn current_pair_merges_encode_identically_to_legacy_merges() {
        use tokenizers::models::bpe::BPE;
        let vocab = [("a".into(), 0), ("b".into(), 1), ("ab".into(), 2)]
            .into_iter()
            .collect();
        let model = BPE::builder()
            .vocab_and_merges(vocab, vec![("a".into(), "b".into())])
            .build()
            .unwrap();
        let original = Tokenizer::new(model);
        let mut json: serde_json::Value =
            serde_json::from_str(&original.to_string(false).unwrap()).unwrap();
        json["model"]["merges"] = serde_json::json!([["a", "b"]]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tokenizer.json");
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        let loaded = load_tokenizer(&path).unwrap();
        for text in ["ab", "aab", "abab"] {
            assert_eq!(
                original.encode(text, false).unwrap().get_ids(),
                loaded.encode(text, false).unwrap().get_ids()
            );
        }
        json["model"]["ignore_merges"] = serde_json::json!(true);
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(load_tokenizer(&path).is_err());
    }
}
