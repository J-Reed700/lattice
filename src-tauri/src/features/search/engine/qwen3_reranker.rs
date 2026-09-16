//! Qwen3 causal-language-model reranker.
//!
//! Qwen3-Reranker does not expose a sequence-classification head. It formats
//! each query/document pair as a chat prompt and compares the next-token logits
//! for `no` and `yes`. This implementation follows the model publisher's prompt
//! and probability calculation while using Candle's local Qwen3 runtime.

use std::path::Path;
use std::sync::Arc;

use candle_core::safetensors::MmapedSafetensors;
use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::qwen3::{Config, Model};
use parking_lot::Mutex;
use tokenizers::Tokenizer;

use crate::shared::error::{AppError, Result, ResultExt};

use super::reranker::{best_reranker_device, resolve_model_dir, RerankResult, Reranker};

const DEFAULT_MAX_LENGTH: usize = 2048;
const DEFAULT_RETRIEVAL_INSTRUCTION: &str =
    "Given a search query, retrieve relevant passages from the user's knowledge base that best answer or satisfy the query";
const PROMPT_PREFIX: &str = "<|im_start|>system\nJudge whether the Document meets the requirements based on the Query and the Instruct provided. Note that the answer can only be \"yes\" or \"no\".<|im_end|>\n<|im_start|>user\n";
const PROMPT_SUFFIX: &str = "<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n";

/// Local Qwen3 reranker using the publisher's `yes` versus `no` scoring rule.
pub struct Qwen3RerankerService {
    inner: Arc<Mutex<Qwen3Inner>>,
    tokenizer: Arc<Tokenizer>,
    prefix_tokens: Arc<[u32]>,
    suffix_tokens: Arc<[u32]>,
    instruction: Arc<str>,
    max_length: usize,
    model_max_length: usize,
}

struct Qwen3Inner {
    /// The base model returns hidden states without materializing all 151k
    /// vocabulary logits. The decision head below contains only the two rows
    /// needed for exact `no`/`yes` scoring.
    model: Model,
    decision_head: Tensor,
    device: Device,
}

impl std::fmt::Debug for Qwen3RerankerService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Qwen3RerankerService")
            .field("max_length", &self.max_length)
            .field("model_max_length", &self.model_max_length)
            .field("instruction", &self.instruction)
            .finish_non_exhaustive()
    }
}

impl Qwen3RerankerService {
    /// Load a Qwen3ForCausalLM reranker checkpoint from a directory or its
    /// `model.safetensors` path.
    pub async fn new(model_path: impl AsRef<Path>) -> Result<Self> {
        let resolved = resolve_model_dir(model_path.as_ref())?;
        let tokenizer_path = resolved.dir.join("tokenizer.json");
        let config_path = resolved.dir.join("config.json");

        if !tokenizer_path.is_file() {
            return Err(AppError::Other(format!(
                "Qwen3 reranker tokenizer.json not found at {}",
                tokenizer_path.display()
            )));
        }

        // Use the same compatibility loader as Qwen3 embeddings. Recent
        // Hugging Face tokenizers serialize BPE merges as token pairs, while
        // the Rust runtime currently expects the equivalent `"left right"`
        // form.
        let mut tokenizer =
            crate::features::embedding::input_policy::load_tokenizer(&tokenizer_path)?;
        tokenizer.with_padding(None);
        tokenizer
            .with_truncation(None)
            .map_err(|error| AppError::TokenizationError {
                reason: format!("Failed to disable Qwen3 tokenizer truncation: {error}"),
            })?;

        // Qwen's published implementation resolves the literal tokenizer
        // vocabulary entries rather than tokenizing sentences containing them.
        let false_token_id = tokenizer.token_to_id("no").ok_or_else(|| {
            AppError::InvalidConfig("Qwen3 reranker tokenizer has no `no` token".to_string())
        })?;
        let true_token_id = tokenizer.token_to_id("yes").ok_or_else(|| {
            AppError::InvalidConfig("Qwen3 reranker tokenizer has no `yes` token".to_string())
        })?;
        if false_token_id == true_token_id {
            return Err(AppError::InvalidConfig(
                "Qwen3 reranker decision tokens resolve to the same vocabulary id".to_string(),
            ));
        }

        let prefix_tokens = encode_without_special_tokens(&tokenizer, PROMPT_PREFIX)?;
        let suffix_tokens = encode_without_special_tokens(&tokenizer, PROMPT_SUFFIX)?;

        let config_json = std::fs::read_to_string(&config_path)
            .context("Failed to read Qwen3 reranker config.json")?;
        let config: Config = serde_json::from_str(&config_json).map_err(|error| {
            AppError::InvalidConfig(format!("Invalid Qwen3 reranker config.json: {error}"))
        })?;
        let model_max_length = config.max_position_embeddings;
        let weights_path = resolved.weights.clone();

        let inner = tokio::task::spawn_blocking(move || -> Result<Qwen3Inner> {
            let device = best_reranker_device();
            #[allow(unsafe_code)]
            let variable_builder = unsafe {
                VarBuilder::from_mmaped_safetensors(&[&weights_path], DType::F32, &device)
            }
            .map_err(|error| {
                AppError::Other(format!("Failed to mmap Qwen3 reranker weights: {error}"))
            })?;

            let model = Model::new(&config, variable_builder.clone()).map_err(|error| {
                AppError::Other(format!("Failed to build Qwen3 reranker model: {error}"))
            })?;

            // Qwen3-Reranker-0.6B ties the LM head to the input embeddings.
            // Selecting the two relevant rows is mathematically identical to
            // computing the full vocabulary projection and then indexing it,
            // while avoiding roughly 151k unused logits for every candidate.
            let head_name = if config.tie_word_embeddings {
                "model.embed_tokens.weight"
            } else {
                "lm_head.weight"
            };
            let decision_head = load_decision_head(
                &weights_path,
                head_name,
                config.vocab_size,
                config.hidden_size,
                false_token_id,
                true_token_id,
            )?;

            Ok(Qwen3Inner {
                model,
                decision_head,
                device,
            })
        })
        .await
        .map_err(|error| {
            AppError::Other(format!("Qwen3 reranker load task panicked: {error}"))
        })??;

        let max_length = DEFAULT_MAX_LENGTH.min(model_max_length);
        ensure_prompt_budget(max_length, &prefix_tokens, &suffix_tokens)?;

        tracing::info!(
            path = %resolved.weights.display(),
            max_length,
            false_token_id,
            true_token_id,
            "Qwen3 causal reranker initialized"
        );

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
            tokenizer: Arc::new(tokenizer),
            prefix_tokens: prefix_tokens.into(),
            suffix_tokens: suffix_tokens.into(),
            instruction: DEFAULT_RETRIEVAL_INSTRUCTION.into(),
            max_length,
            model_max_length,
        })
    }

    pub fn with_max_length(mut self, max_length: usize) -> Result<Self> {
        if max_length > self.model_max_length {
            return Err(AppError::InvalidConfig(format!(
                "Qwen3 reranker max length {max_length} exceeds the model limit {}",
                self.model_max_length
            )));
        }
        ensure_prompt_budget(max_length, &self.prefix_tokens, &self.suffix_tokens)?;
        self.max_length = max_length;
        Ok(self)
    }

    /// Override the retrieval instruction for a domain-specific corpus.
    pub fn with_instruction(mut self, instruction: impl Into<String>) -> Result<Self> {
        let instruction = instruction.into();
        if instruction.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "Qwen3 reranker instruction cannot be empty".to_string(),
            ));
        }
        self.instruction = instruction.into();
        Ok(self)
    }

    pub async fn rerank(
        &self,
        query: &str,
        documents: Vec<String>,
        top_k: usize,
    ) -> Result<Vec<RerankResult>> {
        if documents.is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }

        let tokenizer = Arc::clone(&self.tokenizer);
        let inner = Arc::clone(&self.inner);
        let prefix_tokens = Arc::clone(&self.prefix_tokens);
        let suffix_tokens = Arc::clone(&self.suffix_tokens);
        let instruction = Arc::clone(&self.instruction);
        let query = query.to_string();
        let max_length = self.max_length;

        tokio::task::spawn_blocking(move || {
            let prompt_tokens = encode_prompts(
                &tokenizer,
                &prefix_tokens,
                &suffix_tokens,
                &instruction,
                &query,
                &documents,
                max_length,
            )?;
            let scores = score_prompts(&inner, &prompt_tokens)?;
            if scores.len() != documents.len() {
                return Err(AppError::InvalidState(format!(
                    "Qwen3 reranker returned {} scores for {} documents",
                    scores.len(),
                    documents.len()
                )));
            }

            let mut ranked: Vec<RerankResult> = scores
                .into_iter()
                .enumerate()
                .map(|(index, score)| RerankResult { index, score })
                .collect();
            ranked.sort_by(|left, right| {
                right
                    .score
                    .total_cmp(&left.score)
                    .then(left.index.cmp(&right.index))
            });
            ranked.truncate(top_k.min(ranked.len()));
            Ok(ranked)
        })
        .await
        .map_err(|error| AppError::Other(format!("Qwen3 reranking task failed: {error}")))?
    }
}

#[async_trait::async_trait]
impl Reranker for Qwen3RerankerService {
    fn is_available(&self) -> bool {
        true
    }

    async fn rerank(
        &self,
        query: &str,
        documents: Vec<String>,
        top_k: usize,
    ) -> Result<Vec<RerankResult>> {
        Qwen3RerankerService::rerank(self, query, documents, top_k).await
    }
}

/// Load two rows directly from the memory-mapped safetensors payload. Loading
/// the entire tied embedding matrix on CPU just to keep `no` and `yes` would
/// add roughly 600 MB to initialization peak memory for the 0.6B checkpoint.
fn load_decision_head(
    weights_path: &Path,
    tensor_name: &str,
    vocabulary_size: usize,
    hidden_size: usize,
    false_token_id: u32,
    true_token_id: u32,
) -> Result<Tensor> {
    #[allow(unsafe_code)]
    let tensors = unsafe { MmapedSafetensors::new(weights_path) }.map_err(|error| {
        AppError::Other(format!("Failed to mmap Qwen3 decision weights: {error}"))
    })?;
    let view = tensors.get(tensor_name).map_err(|error| {
        AppError::Other(format!(
            "Failed to find Qwen3 decision tensor `{tensor_name}`: {error}"
        ))
    })?;
    if view.shape() != [vocabulary_size, hidden_size] {
        return Err(AppError::InvalidConfig(format!(
            "Qwen3 decision tensor `{tensor_name}` has shape {:?}; expected [{vocabulary_size}, {hidden_size}]",
            view.shape()
        )));
    }
    let dtype = DType::try_from(view.dtype()).map_err(|error| {
        AppError::InvalidConfig(format!("Unsupported Qwen3 decision tensor type: {error}"))
    })?;

    let mut rows = Vec::with_capacity(2);
    for (label, token_id) in [("no", false_token_id), ("yes", true_token_id)] {
        // `Dtype::bitsize` reports the element width in bits; a row occupies whole bytes.
        let row_size = hidden_size
            .checked_mul(view.dtype().bitsize())
            .map(|row_bits| row_bits / 8)
            .ok_or_else(|| AppError::InvalidConfig("Qwen3 decision row is too large".into()))?;
        let start = (token_id as usize).checked_mul(row_size).ok_or_else(|| {
            AppError::InvalidConfig(format!("Qwen3 `{label}` decision offset overflowed"))
        })?;
        let end = start.checked_add(row_size).ok_or_else(|| {
            AppError::InvalidConfig(format!("Qwen3 `{label}` decision offset overflowed"))
        })?;
        let bytes = view.data().get(start..end).ok_or_else(|| {
            AppError::InvalidConfig(format!(
                "Qwen3 `{label}` decision token {token_id} is outside the vocabulary"
            ))
        })?;
        let row = Tensor::from_raw_buffer(bytes, dtype, &[hidden_size], &Device::Cpu)
            .and_then(|tensor| tensor.to_dtype(DType::F32))
            .map_err(|error| {
                AppError::Other(format!(
                    "Failed to materialize Qwen3 `{label}` decision weight: {error}"
                ))
            })?;
        rows.push(row);
    }

    Tensor::stack(&rows, 0)
        .and_then(|tensor| tensor.contiguous())
        .map_err(|error| AppError::Other(format!("Failed to build Qwen3 decision head: {error}")))
}

fn ensure_prompt_budget(max_length: usize, prefix: &[u32], suffix: &[u32]) -> Result<()> {
    let overhead = prefix.len().saturating_add(suffix.len());
    if max_length <= overhead {
        return Err(AppError::InvalidConfig(format!(
            "Qwen3 reranker max length {max_length} must exceed its {overhead}-token prompt overhead"
        )));
    }
    Ok(())
}

fn encode_without_special_tokens(tokenizer: &Tokenizer, text: &str) -> Result<Vec<u32>> {
    tokenizer
        .encode(text, false)
        .map(|encoding| encoding.get_ids().to_vec())
        .map_err(|error| AppError::TokenizationError {
            reason: format!("Qwen3 reranker tokenization failed: {error}"),
        })
}

fn encode_prompts(
    tokenizer: &Tokenizer,
    prefix_tokens: &[u32],
    suffix_tokens: &[u32],
    instruction: &str,
    query: &str,
    documents: &[String],
    max_length: usize,
) -> Result<Vec<Vec<u32>>> {
    ensure_prompt_budget(max_length, prefix_tokens, suffix_tokens)?;
    let body_budget = max_length - prefix_tokens.len() - suffix_tokens.len();

    documents
        .iter()
        .map(|document| {
            let body =
                format!("<Instruct>: {instruction}\n<Query>: {query}\n<Document>: {document}");
            let mut body_tokens = encode_without_special_tokens(tokenizer, &body)?;
            body_tokens.truncate(body_budget);

            let mut prompt =
                Vec::with_capacity(prefix_tokens.len() + body_tokens.len() + suffix_tokens.len());
            prompt.extend_from_slice(prefix_tokens);
            prompt.extend(body_tokens);
            prompt.extend_from_slice(suffix_tokens);
            Ok(prompt)
        })
        .collect()
}

fn score_prompts(inner: &Arc<Mutex<Qwen3Inner>>, prompts: &[Vec<u32>]) -> Result<Vec<f32>> {
    let Some(first_prompt) = prompts.first() else {
        return Ok(Vec::new());
    };
    if prompts.iter().any(Vec::is_empty) {
        return Err(AppError::InvalidState(
            "Qwen3 reranker produced an empty prompt".to_string(),
        ));
    }

    // Query, instruction, and chat framing are identical for every candidate.
    // Prefill their longest common token prefix once, then clone the resulting
    // KV cache. Every candidate still gets an exact independent causal pass.
    let common_prefix_length = longest_common_prefix(prompts).min(
        prompts
            .iter()
            .map(Vec::len)
            .min()
            .unwrap_or(1)
            .saturating_sub(1),
    );

    let guard = inner.lock();
    let device = guard.device.clone();
    let mut prefixed_model = guard.model.clone();
    if common_prefix_length > 0 {
        let input = Tensor::from_slice(
            first_prompt.get(..common_prefix_length).ok_or_else(|| {
                AppError::InvalidState("Qwen3 common prefix exceeds the first prompt".into())
            })?,
            (1, common_prefix_length),
            &device,
        )
        .map_err(|error| {
            AppError::Other(format!("Failed to build Qwen3 prefix tensor: {error}"))
        })?;
        prefixed_model.forward(&input, 0).map_err(|error| {
            AppError::Other(format!("Qwen3 prefix forward pass failed: {error}"))
        })?;
    }

    let transposed_head = guard.decision_head.t().map_err(|error| {
        AppError::Other(format!("Failed to transpose Qwen3 decision head: {error}"))
    })?;
    let mut scores = Vec::with_capacity(prompts.len());
    for prompt in prompts {
        let remainder = prompt.get(common_prefix_length..).ok_or_else(|| {
            AppError::InvalidState("Qwen3 common prefix exceeds a candidate prompt".into())
        })?;
        if remainder.is_empty() {
            return Err(AppError::InvalidState(
                "Qwen3 reranker prompt has no scoring token".to_string(),
            ));
        }

        let input =
            Tensor::from_slice(remainder, (1, remainder.len()), &device).map_err(|error| {
                AppError::Other(format!("Failed to build Qwen3 input tensor: {error}"))
            })?;
        let mut model = prefixed_model.clone();
        let hidden = model
            .forward(&input, common_prefix_length)
            .map_err(|error| AppError::Other(format!("Qwen3 forward pass failed: {error}")))?;
        let last_hidden = hidden
            .i((0, remainder.len() - 1, ..))
            .and_then(|tensor| tensor.unsqueeze(0))
            .and_then(|tensor| tensor.to_device(&Device::Cpu))
            .map_err(|error| {
                AppError::Other(format!("Failed to select Qwen3 scoring state: {error}"))
            })?;
        let logits = last_hidden
            .matmul(&transposed_head)
            .and_then(|tensor| tensor.squeeze(0))
            .and_then(|tensor| tensor.to_dtype(DType::F32))
            .and_then(|tensor| tensor.to_device(&Device::Cpu))
            .and_then(|tensor| tensor.to_vec1::<f32>())
            .map_err(|error| {
                AppError::Other(format!(
                    "Failed to calculate Qwen3 decision logits: {error}"
                ))
            })?;
        let [no, yes] = logits.as_slice() else {
            return Err(AppError::InvalidState(format!(
                "Qwen3 reranker produced {} decision logits instead of 2",
                logits.len()
            )));
        };
        scores.push(probability_yes(*no, *yes));
    }

    Ok(scores)
}

fn longest_common_prefix(rows: &[Vec<u32>]) -> usize {
    let Some(first) = rows.first() else {
        return 0;
    };
    let max_common = rows.iter().map(Vec::len).min().unwrap_or(0);
    (0..max_common)
        .take_while(|&index| {
            rows.iter()
                .skip(1)
                .all(|row| row.get(index) == first.get(index))
        })
        .count()
}

#[inline]
fn probability_yes(no_logit: f32, yes_logit: f32) -> f32 {
    let difference = yes_logit - no_logit;
    if difference >= 0.0 {
        1.0 / (1.0 + (-difference).exp())
    } else {
        let exp = difference.exp();
        exp / (1.0 + exp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probability_matches_binary_softmax() {
        assert!((probability_yes(0.0, 0.0) - 0.5).abs() < f32::EPSILON);
        assert!(probability_yes(-4.0, 4.0) > 0.999);
        assert!(probability_yes(4.0, -4.0) < 0.001);
    }

    #[test]
    fn common_prefix_stops_at_first_difference() {
        let rows = vec![vec![1, 2, 3, 4], vec![1, 2, 8], vec![1, 2, 3]];
        assert_eq!(longest_common_prefix(&rows), 2);
        assert_eq!(longest_common_prefix(&[]), 0);
    }

    #[test]
    fn prompt_budget_requires_body_room() {
        assert!(ensure_prompt_budget(5, &[1, 2], &[3, 4]).is_ok());
        assert!(ensure_prompt_budget(4, &[1, 2], &[3, 4]).is_err());
    }
}
