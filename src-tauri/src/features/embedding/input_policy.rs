//! One tokenization policy for indexing and inference. Never silently truncates input.
use crate::application::ports::embedding_port::EmbeddingTextChunk;
use crate::shared::error::{AppError, Result};
use std::path::Path;
use tokenizers::Tokenizer;

pub struct InputPolicy {
    tokenizer: Tokenizer,
    pub max_tokens: usize,
}

impl InputPolicy {
    pub fn new(mut tokenizer: Tokenizer, max_tokens: usize) -> Result<Self> {
        if max_tokens == 0 {
            return Err(AppError::InvalidConfig(
                "Embedding token limit must be positive".into(),
            ));
        }
        tokenizer
            .with_truncation(None)
            .map_err(|e| AppError::InvalidConfig(e.to_string()))?;
        tokenizer.with_padding(None);
        Ok(Self {
            tokenizer,
            max_tokens,
        })
    }

    pub fn count(&self, text: &str) -> Result<usize> {
        self.tokenizer
            .encode(text, true)
            .map(|e| e.len())
            .map_err(|e| AppError::InvalidInput(format!("Embedding tokenization: {e}")))
    }

    pub fn validate(&self, text: &str) -> Result<()> {
        let count = self.count(text)?;
        if count > self.max_tokens {
            return Err(AppError::InvalidInput(format!("Embedding input has {count} tokens; model limit is {}. Split the passage before embedding.", self.max_tokens)));
        }
        Ok(())
    }

    /// Splits only at character boundaries; every source byte belongs to a returned slice.
    /// Each candidate is re-tokenized with its prefix and special tokens before acceptance.
    pub fn split(&self, text: &str, prefix: &str) -> Result<Vec<EmbeddingTextChunk>> {
        let mut chunks = Vec::new();
        let mut start = 0;
        while start < text.len() {
            let tail = slice(text, start, text.len())?;
            // Bound tokenization work per chunk, including for multi-megabyte documents.
            let window_end = tail
                .char_indices()
                .take(self.max_tokens.saturating_mul(8).max(1))
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(tail.len());
            let remaining = slice(tail, 0, window_end)?;
            let candidate = format!("{prefix}{remaining}");
            let mut end = start + remaining.len();
            if self.count(&candidate)? > self.max_tokens {
                let boundaries: Vec<usize> = remaining
                    .char_indices()
                    .map(|(i, c)| i + c.len_utf8())
                    .collect();
                // A fitting candidate is always validated, even when BPE counts are non-monotonic.
                let (mut low, mut high, mut fit) = (0, boundaries.len(), 0);
                while low < high {
                    let mid = low + (high - low) / 2;
                    let offset = boundaries.get(mid).copied().unwrap_or(remaining.len());
                    if self.count(&format!("{prefix}{}", slice(remaining, 0, offset)?))?
                        <= self.max_tokens
                    {
                        fit = offset;
                        low = mid + 1;
                    } else {
                        high = mid;
                    }
                }
                if fit == 0 {
                    return Err(AppError::InvalidInput(
                        "Embedding context prefix leaves no room for the passage".into(),
                    ));
                }
                end = start + fit;
            }
            // Prefer a nearby word boundary without dropping the separating whitespace.
            if end < text.len() {
                let candidate = slice(text, start, end)?;
                if let Some((i, c)) = candidate
                    .char_indices()
                    .rev()
                    .find(|(i, c)| *i >= candidate.len() * 3 / 4 && c.is_whitespace())
                {
                    let proposed = start + i + c.len_utf8();
                    if self.count(&format!("{prefix}{}", slice(text, start, proposed)?))?
                        <= self.max_tokens
                    {
                        end = proposed;
                    }
                }
            }
            let slice = slice(text, start, end)?;
            let token_count = self.count(&format!("{prefix}{slice}"))?;
            chunks.push(EmbeddingTextChunk {
                text: slice.to_owned(),
                start,
                end,
                token_count,
            });
            start = end;
        }
        Ok(chunks)
    }
}

fn slice(text: &str, start: usize, end: usize) -> Result<&str> {
    text.get(start..end)
        .ok_or_else(|| AppError::InvalidState("Invalid embedding passage boundary".into()))
}

/// Use the tightest declared model, tokenizer, and sentence-transformer limit.
/// Huge HF tokenizer sentinel values are not real limits.
pub fn model_token_limit(dir: &Path) -> Result<usize> {
    let mut limits = Vec::new();
    for (file, keys) in [
        (
            "config.json",
            &["max_position_embeddings", "n_positions"][..],
        ),
        ("tokenizer_config.json", &["model_max_length"][..]),
        ("sentence_bert_config.json", &["max_seq_length"][..]),
    ] {
        let path = dir.join(file);
        if !path.exists() {
            continue;
        }
        let bytes = std::fs::read(&path)
            .map_err(|e| AppError::InvalidConfig(format!("{}: {e}", path.display())))?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| AppError::InvalidConfig(format!("{}: {e}", path.display())))?;
        for key in keys {
            if let Some(limit) = value.get(key).and_then(|v| v.as_u64()) {
                if limit == 0 {
                    return Err(AppError::InvalidConfig(format!(
                        "{file}: {key} must be positive"
                    )));
                }
                if limit <= 1_048_576 {
                    limits.push(limit as usize);
                }
            }
        }
    }
    Ok(limits.into_iter().min().unwrap_or(512))
}

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
mod tests {
    use super::*;
    fn policy(limit: usize) -> InputPolicy {
        use tokenizers::{models::wordlevel::WordLevel, pre_tokenizers::whitespace::Whitespace};
        let vocab = [("[UNK]".to_owned(), 0), ("word".to_owned(), 1)]
            .into_iter()
            .collect();
        let mut tokenizer = Tokenizer::new(
            WordLevel::builder()
                .vocab(vocab)
                .unk_token("[UNK]".into())
                .build()
                .unwrap(),
        );
        tokenizer.with_pre_tokenizer(Whitespace);
        InputPolicy::new(tokenizer, limit).unwrap()
    }
    #[test]
    fn preserves_tail_unicode_and_prefix_budget() {
        let policy = policy(8);
        let text = format!("{}末尾 🧪", "word ".repeat(800));
        let chunks = policy.split(&text, "document title ").unwrap();
        assert!(chunks.len() > 1);
        assert_eq!(
            chunks.iter().map(|c| c.text.as_str()).collect::<String>(),
            text
        );
        for c in chunks {
            assert_eq!(&text[c.start..c.end], c.text);
            policy
                .validate(&format!("document title {}", c.text))
                .unwrap();
        }
        assert!(policy.validate(&text).is_err());
    }
    #[test]
    fn rejects_exhausted_prefix_and_accepts_empty_document() {
        assert!(policy(1).split("word", "word word ").is_err());
        assert!(policy(1).split("", "").unwrap().is_empty());
    }
    #[test]
    fn respects_sentence_transformer_limit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"max_position_embeddings":512}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("sentence_bert_config.json"),
            r#"{"max_seq_length":256}"#,
        )
        .unwrap();
        assert_eq!(model_token_limit(dir.path()).unwrap(), 256);
    }
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
