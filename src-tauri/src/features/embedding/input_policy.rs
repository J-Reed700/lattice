//! One tokenization policy for indexing and inference. Never silently truncates input.
use crate::application::ports::embedding_port::EmbeddingTextChunk;
use crate::shared::error::{AppError, Result};
use std::path::Path;
use tokenizers::Tokenizer;

/// Passage tokens one retrieval chunk aims for, before its context prefix.
///
/// Chunk size is a retrieval decision, not a property of whichever model
/// happens to be active. A 2048-token vector is a blurry average of several
/// topics, and the whole chunk is what later fills the answer prompt, so the
/// window is the wrong budget to spend. Models with a smaller window stay
/// window-bound: the effective limit is the smaller of the two.
pub const RETRIEVAL_CHUNK_TARGET_TOKENS: usize = 480;

/// A final piece smaller than this percentage of the passage budget is merged
/// back into the chunk before it, when the model window still has room.
const TRAILING_FRAGMENT_PERCENT: usize = 15;

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
    ///
    /// The budget is [`RETRIEVAL_CHUNK_TARGET_TOKENS`] of passage on top of the
    /// prefix, or the model window when that is smaller.
    pub fn split(&self, text: &str, prefix: &str) -> Result<Vec<EmbeddingTextChunk>> {
        let prefix_tokens = self.count(prefix)?;
        let limit = self
            .max_tokens
            .min(RETRIEVAL_CHUNK_TARGET_TOKENS.saturating_add(prefix_tokens));
        let mut chunks = Vec::new();
        let mut start = 0;
        while start < text.len() {
            let tail = slice(text, start, text.len())?;
            // Bound tokenization work per chunk, including for multi-megabyte documents.
            let window_end = tail
                .char_indices()
                .take(limit.saturating_mul(8).max(1))
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(tail.len());
            let remaining = slice(tail, 0, window_end)?;
            let candidate = format!("{prefix}{remaining}");
            let mut end = start + remaining.len();
            if self.count(&candidate)? > limit {
                let boundaries: Vec<usize> = remaining
                    .char_indices()
                    .map(|(i, c)| i + c.len_utf8())
                    .collect();
                // A fitting candidate is always validated, even when BPE counts are non-monotonic.
                let (mut low, mut high, mut fit) = (0, boundaries.len(), 0);
                while low < high {
                    let mid = low + (high - low) / 2;
                    let offset = boundaries.get(mid).copied().unwrap_or(remaining.len());
                    if self.count(&format!("{prefix}{}", slice(remaining, 0, offset)?))? <= limit {
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
            // Prefer a boundary a reader would recognize over an arbitrary token
            // edge, checking each class against the real tokenizer before it is
            // accepted so the budget still holds.
            if end < text.len() {
                let fitted = slice(text, start, end)?;
                for proposed in cut_candidates(fitted) {
                    if proposed == 0 || proposed >= fitted.len() {
                        continue;
                    }
                    if self.count(&format!("{prefix}{}", slice(fitted, 0, proposed)?))? <= limit {
                        end = start + proposed;
                        break;
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
            // Zero overlap, deliberately. Chunk spans stay disjoint and ordered
            // so citation offsets resolve, the context prefix already carries
            // document identity, and the chat path expands a hit with its
            // neighbouring chunks. Overlap would only duplicate tokens in the
            // index and in the answer prompt.
            start = end;
        }
        self.merge_trailing_fragment(&mut chunks, prefix, prefix_tokens, limit)?;
        Ok(chunks)
    }

    /// A stray tail — one closing line, a signature — is not a retrievable
    /// passage on its own, so give it back to the chunk it belongs with when
    /// the model window has room for both.
    fn merge_trailing_fragment(
        &self,
        chunks: &mut Vec<EmbeddingTextChunk>,
        prefix: &str,
        prefix_tokens: usize,
        limit: usize,
    ) -> Result<()> {
        let Some(previous) = chunks.len().checked_sub(2) else {
            return Ok(());
        };
        let tail = &chunks[previous + 1];
        let passage_budget = limit.saturating_sub(prefix_tokens);
        if tail.token_count.saturating_sub(prefix_tokens) * 100
            >= passage_budget * TRAILING_FRAGMENT_PERCENT
        {
            return Ok(());
        }
        let merged = format!("{}{}", chunks[previous].text, tail.text);
        let token_count = self.count(&format!("{prefix}{merged}"))?;
        if token_count > self.max_tokens {
            return Ok(());
        }
        let end = tail.end;
        chunks.truncate(previous + 1);
        let last = &mut chunks[previous];
        last.text = merged;
        last.end = end;
        last.token_count = token_count;
        Ok(())
    }
}

/// Byte offsets inside `fitted` where a chunk could end, best first: a
/// paragraph break, a sentence end, then whitespace.
///
/// Paragraph and sentence cuts must leave at least half the budget behind, so
/// one stray blank line near the start cannot halve every chunk; when they do
/// not, the next class answers instead. None of them lands inside a fenced code
/// block or a markdown table, where a cut would orphan a closing fence or a
/// table row. Whitespace keeps the tighter three-quarter floor it always had,
/// because in a script that does not space its words the nearest space can be
/// arbitrarily far back and a plain character boundary is the better cut.
fn cut_candidates(fitted: &str) -> Vec<usize> {
    let floor = fitted.len() / 2;
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0;
    for line in fitted.split_inclusive('\n') {
        lines.push((offset, line));
        offset += line.len();
    }
    // Per line: (inside a code fence, is a table row). The ``` delimiters count
    // as fenced themselves, so a cut never separates one from its block.
    let mut in_fence = false;
    let flags: Vec<(bool, bool)> = lines
        .iter()
        .map(|(_, line)| {
            let trimmed = line.trim_start();
            let delimiter = trimmed.starts_with("```");
            if delimiter {
                in_fence = !in_fence;
            }
            (in_fence || delimiter, trimmed.starts_with('|'))
        })
        .collect();

    let (mut paragraph, mut sentence) = (None, None);
    for (index, (start, line)) in lines.iter().enumerate() {
        let (fenced, table) = flags[index];
        let joined = index > 0 && ((fenced && flags[index - 1].0) || (table && flags[index - 1].1));
        if !joined
            && *start >= floor
            && index > 0
            && lines[index - 1].1.trim().is_empty()
            && !line.trim().is_empty()
        {
            paragraph = Some(*start);
        }
        if fenced || table {
            continue;
        }
        for (i, ch) in line.char_indices() {
            // The ideographic stops end a sentence on their own; the shared
            // ASCII ones also end abbreviations and decimals, so they need
            // whitespace or the end of the passage behind them.
            let terminal = match ch {
                '。' | '！' | '？' => true,
                '.' | '!' | '?' | '…' => false,
                _ => continue,
            };
            let after = start + i + ch.len_utf8();
            let run: usize = fitted[after..]
                .chars()
                .take_while(|c| c.is_whitespace())
                .map(char::len_utf8)
                .sum();
            if !terminal && run == 0 && after != fitted.len() {
                continue;
            }
            if after + run >= floor {
                sentence = Some(after + run);
            }
        }
    }
    // The separating whitespace stays with the chunk before it.
    let whitespace = fitted
        .char_indices()
        .rev()
        .find(|(i, c)| *i >= fitted.len() * 3 / 4 && c.is_whitespace())
        .map(|(i, c)| i + c.len_utf8());
    [paragraph, sentence, whitespace]
        .into_iter()
        .flatten()
        .collect()
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
    /// Every chunk fits the policy, the spans tile the source exactly, and the
    /// texts concatenate back to it. This is the contract the whole pipeline
    /// rests on: citation offsets resolve against it.
    fn assert_tiles(policy: &InputPolicy, text: &str, prefix: &str, chunks: &[EmbeddingTextChunk]) {
        assert_eq!(
            chunks.iter().map(|c| c.text.as_str()).collect::<String>(),
            text
        );
        let mut expected = 0;
        for chunk in chunks {
            assert_eq!(chunk.start, expected);
            assert_eq!(&text[chunk.start..chunk.end], chunk.text);
            assert!(chunk.token_count <= policy.max_tokens);
            policy
                .validate(&format!("{prefix}{}", chunk.text))
                .expect("every chunk fits the model window");
            expected = chunk.end;
        }
        assert_eq!(expected, text.len());
    }

    #[test]
    fn the_retrieval_target_caps_chunks_below_a_large_model_window() {
        let policy = policy(4096);
        let text = "word ".repeat(2000);
        let chunks = policy.split(&text, "").unwrap();
        assert!(
            chunks.len() >= 4,
            "a 2000-token passage must not become one vector: {} chunks",
            chunks.len()
        );
        assert_tiles(&policy, &text, "", &chunks);
        for chunk in &chunks {
            assert!(chunk.token_count <= RETRIEVAL_CHUNK_TARGET_TOKENS);
        }
        // The prefix gets its own budget on top of the target, so a long
        // prefix shortens nothing until the model window is the binding limit.
        let prefixed = policy.split(&text, "heading collection filename ").unwrap();
        assert_eq!(prefixed.len(), chunks.len());
    }

    #[test]
    fn a_passage_under_the_target_stays_one_chunk() {
        let policy = policy(4096);
        let text = "word ".repeat(100);
        let chunks = policy.split(&text, "title ").unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, text);
    }

    #[test]
    fn a_small_model_window_still_binds() {
        let policy = policy(24);
        let text = "word ".repeat(200);
        let chunks = policy.split(&text, "title ").unwrap();
        assert!(chunks.len() > 8);
        assert_tiles(&policy, &text, "title ", &chunks);
    }

    #[test]
    fn a_tiny_trailing_fragment_joins_the_chunk_before_it() {
        let policy = policy(4096);
        // Just past one target: the remainder is a couple of tokens, which is
        // not a passage anybody would want to retrieve on its own.
        let text = "word ".repeat(482);
        let chunks = policy.split(&text, "").unwrap();
        assert_eq!(chunks.len(), 1, "the stray tail merged back");
        assert_tiles(&policy, &text, "", &chunks);
        assert!(chunks[0].token_count > RETRIEVAL_CHUNK_TARGET_TOKENS);

        // A tail worth keeping is kept.
        let text = "word ".repeat(900);
        let chunks = policy.split(&text, "").unwrap();
        assert_eq!(chunks.len(), 2);
        assert!(chunks[1].token_count * 100 >= RETRIEVAL_CHUNK_TARGET_TOKENS * 15);
    }

    #[test]
    fn a_merge_that_would_overflow_the_window_is_refused() {
        let policy = policy(20);
        let text = "word ".repeat(41);
        let chunks = policy.split(&text, "").unwrap();
        assert!(chunks.len() > 2);
        assert_tiles(&policy, &text, "", &chunks);
    }

    #[test]
    fn a_paragraph_break_wins_over_a_word_boundary() {
        let paragraph = "aaaa aaaa\n\nbbbb bbbb";
        assert_eq!(cut_candidates(paragraph).first(), Some(&11));
        let policy = policy(20);
        let text = format!("{}\n\n{}", "aa ".repeat(11).trim(), "bb ".repeat(11).trim());
        let chunks = policy.split(&text, "").unwrap();
        assert!(chunks[0].text.ends_with("\n\n"));
        assert!(chunks[1].text.starts_with("bb"));
        assert_tiles(&policy, &text, "", &chunks);
    }

    #[test]
    fn a_break_in_the_first_half_falls_through_to_the_next_class() {
        // The only blank line is 3 bytes in; cutting there would waste the
        // budget, so the whitespace class answers instead.
        let candidates = cut_candidates("a\n\nbbbb bbbb bbbb bbbb");
        assert!(!candidates.contains(&3));
        assert!(candidates.iter().all(|cut| *cut >= 11));
    }

    #[test]
    fn a_sentence_end_wins_when_there_is_no_paragraph_break() {
        // Ordinary prose: the stop needs whitespace behind it, so "3.5" and
        // "e.g." inside a line are not sentence ends.
        assert_eq!(
            cut_candidates("Alpha beta gamma. Delta epsilon").first(),
            Some(&18)
        );
        assert!(
            cut_candidates("Alpha beta 3.5 gamma delta epsilon").is_empty()
                || !cut_candidates("Alpha beta 3.5 gamma delta epsilon").contains(&13)
        );
        // The ideographic stops end a sentence with nothing behind them.
        let cjk = "これは最初のとても長い文章です。次の文です";
        let stop = cjk.find('。').unwrap() + '。'.len_utf8();
        assert!(cut_candidates(cjk).contains(&stop));
    }

    #[test]
    fn code_fences_and_tables_are_never_cut_open() {
        let fenced = "intro text here\n\n```\ncode one\n\ncode two\n```\n\ntail text";
        let inside = fenced.find("code two").unwrap();
        assert!(!cut_candidates(fenced).contains(&inside));
        // The blank line after the closing fence is still a fine place to cut.
        assert!(cut_candidates(fenced).contains(&fenced.find("tail text").unwrap()));

        let table = "| a | b |\n| - | - |\n| 1 | 2 |\n| 3 | 4 |\n";
        let row = table.find("| 3 ").unwrap();
        assert!(!cut_candidates(table).contains(&row));
        // A stop inside a table cell is not a sentence end either: a cut may
        // land before the table or after it, never between its rows.
        let prose_table = "Lead in line.\n| x | End. |\n| y | More. |\n";
        let table_start = prose_table.find("| x").unwrap();
        assert!(cut_candidates(prose_table)
            .iter()
            .all(|cut| *cut <= table_start || *cut == prose_table.len()));
    }

    #[test]
    fn unspaced_and_non_latin_scripts_split_on_character_boundaries() {
        let policy = policy(12);
        for text in [
            "これは日本語のテキストです。句読点で区切ります。最後に絵文字🧪があります。".repeat(8),
            "Это кириллический текст. Он состоит из нескольких предложений. Конец.".repeat(8),
            "한국어텍스트입니다".repeat(40),
        ] {
            let chunks = policy.split(&text, "doc ").unwrap();
            assert!(!chunks.is_empty());
            assert_tiles(&policy, &text, "doc ", &chunks);
        }
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
