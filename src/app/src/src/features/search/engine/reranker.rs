use crate::shared::error::{AppError, Result, ResultExt};
use ndarray::Array2;
use ort::{
    execution_providers::CPUExecutionProvider,
    inputs,
    session::{builder::GraphOptimizationLevel, Session},
    value::Value,
};
use parking_lot::Mutex;
use std::path::Path;
use std::sync::Arc;
use tokenizers::Tokenizer;

#[derive(Debug, Clone)]
pub struct RerankResult {
    pub index: usize,
    pub score: f32,
}

#[derive(Debug)]
pub struct RerankerService {
    session: Arc<Mutex<Session>>,
    tokenizer: Arc<Tokenizer>,
    max_length: usize,
}

impl RerankerService {
    pub async fn new(model_path: impl AsRef<Path>) -> Result<Self> {
        let session = Session::builder()?
            .with_execution_providers([CPUExecutionProvider::default().build()])?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(model_path.as_ref())?;

        let model_dir = model_path
            .as_ref()
            .parent()
            .context("Failed to get model directory")?;
        let tokenizer_path = model_dir.join("tokenizer.json");

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| AppError::Other(format!("Failed to load tokenizer: {}", e)))?;

        tracing::info!(
            "RerankerService initialized with model: {:?}",
            model_path.as_ref()
        );

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            tokenizer: Arc::new(tokenizer),
            max_length: 512,
        })
    }

    pub async fn rerank(
        &self,
        query: &str,
        documents: Vec<String>,
        top_k: usize,
    ) -> Result<Vec<RerankResult>> {
        if documents.is_empty() {
            return Ok(vec![]);
        }

        let session = Arc::clone(&self.session);
        let tokenizer = Arc::clone(&self.tokenizer);
        let query = query.to_string();
        let max_length = self.max_length;

        let result = tokio::task::spawn_blocking(move || {
            Self::rerank_sync(&session, &tokenizer, &query, documents, top_k, max_length)
        })
        .await
        .map_err(|e| AppError::Other(format!("Reranking task failed: {}", e)))??;

        Ok(result)
    }

    fn rerank_sync(
        session: &Arc<Mutex<Session>>,
        tokenizer: &Tokenizer,
        query: &str,
        documents: Vec<String>,
        top_k: usize,
        max_length: usize,
    ) -> Result<Vec<RerankResult>> {
        let pairs: Vec<String> = documents
            .iter()
            .map(|doc| format!("{} [SEP] {}", query, doc))
            .collect();

        let scores = Self::score_batch_sync(session, tokenizer, &pairs, max_length)?;

        let mut ranked: Vec<RerankResult> = scores
            .into_iter()
            .enumerate()
            .map(|(idx, score)| RerankResult { index: idx, score })
            .collect();

        ranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        ranked.truncate(top_k);

        Ok(ranked)
    }

    fn score_batch_sync(
        session: &Arc<Mutex<Session>>,
        tokenizer: &Tokenizer,
        texts: &[String],
        max_length: usize,
    ) -> Result<Vec<f32>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let batch_size = 32;
        let mut all_scores = Vec::new();

        for batch_start in (0..texts.len()).step_by(batch_size) {
            let batch_end = (batch_start + batch_size).min(texts.len());
            let batch = texts.get(batch_start..batch_end).ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "Batch slice out of bounds: {}..{} (texts len: {})",
                    batch_start,
                    batch_end,
                    texts.len()
                ))
            })?;

            let batch_scores = Self::score_batch_internal(session, tokenizer, batch, max_length)?;
            all_scores.extend(batch_scores);
        }

        Ok(all_scores)
    }

    fn score_batch_internal(
        session: &Arc<Mutex<Session>>,
        tokenizer: &Tokenizer,
        texts: &[String],
        max_length: usize,
    ) -> Result<Vec<f32>> {
        let encodings = tokenizer.encode_batch(texts.to_vec(), true).map_err(|e| {
            AppError::TokenizationError {
                reason: format!("Tokenization failed: {}", e),
            }
        })?;

        let batch_size = encodings.len();
        let mut input_ids_batch = Vec::new();
        let mut attention_mask_batch = Vec::new();

        for encoding in encodings {
            let tokens = encoding.get_ids();
            let attention_mask = encoding.get_attention_mask();

            let mut input_ids = tokens.to_vec();
            let mut attn_mask = attention_mask.to_vec();

            if input_ids.len() > max_length {
                input_ids.truncate(max_length);
                attn_mask.truncate(max_length);
            } else {
                let padding = max_length - input_ids.len();
                input_ids.extend(vec![0; padding]);
                attn_mask.extend(vec![0; padding]);
            }

            let input_ids_i64: Vec<i64> = input_ids.iter().map(|&x| x as i64).collect();
            let attn_mask_i64: Vec<i64> = attn_mask.iter().map(|&x| x as i64).collect();

            input_ids_batch.extend(input_ids_i64);
            attention_mask_batch.extend(attn_mask_i64);
        }

        let input_ids_array = Array2::from_shape_vec((batch_size, max_length), input_ids_batch)?;

        let attn_mask_array =
            Array2::from_shape_vec((batch_size, max_length), attention_mask_batch)?;

        // Convert to dynamic dimension arrays for ORT
        let input_ids_dyn = input_ids_array.into_dyn();
        let attn_mask_dyn = attn_mask_array.into_dyn();

        // Create Values for ONNX Runtime 2.0 API
        // Convert to (shape, vec) format required by ORT 2.0
        let input_ids_shape = input_ids_dyn.shape().to_vec();
        let input_ids_vec = input_ids_dyn.into_raw_vec();
        let input_ids_value = Value::from_array((input_ids_shape.as_slice(), input_ids_vec))?;

        let attn_mask_shape = attn_mask_dyn.shape().to_vec();
        let attn_mask_vec = attn_mask_dyn.into_raw_vec();
        let attn_mask_value = Value::from_array((attn_mask_shape.as_slice(), attn_mask_vec))?;

        let mut guard = session.lock();
        let outputs = guard.run(inputs![
            "input_ids" => input_ids_value,
            "attention_mask" => attn_mask_value,
        ])?;

        let output_value = outputs
            .iter()
            .find(|(name, _)| *name == "logits")
            .or_else(|| outputs.iter().next())
            .map(|(_, v)| v)
            .context("ONNX model produced no output tensors")?;

        let output_tensor = output_value.try_extract_array::<f32>()?;

        // Validate shape
        let shape = output_tensor.shape();
        if shape.len() < 2 {
            return Err(AppError::Other(format!(
                "Expected 2D tensor, got shape: {:?}",
                shape
            )));
        }

        let scores: Vec<f32> = (0..batch_size)
            .map(|i| {
                // Access tensor values manually instead of using into_dimensionality()
                let idx1 = output_tensor
                    .get([i, 1])
                    .copied()
                    .unwrap_or_else(|| output_tensor.get([i, 0]).copied().unwrap_or(0.0));
                Self::sigmoid(idx1)
            })
            .collect();

        Ok(scores)
    }

    fn sigmoid(x: f32) -> f32 {
        1.0 / (1.0 + (-x).exp())
    }

    pub fn with_max_length(mut self, max_length: usize) -> Self {
        self.max_length = max_length;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_documents() -> Vec<String> {
        vec![
            "The quick brown fox jumps over the lazy dog".to_string(),
            "Machine learning is a subset of artificial intelligence".to_string(),
            "Rust is a systems programming language".to_string(),
            "Neural networks are inspired by biological neurons".to_string(),
            "The cat sat on the mat".to_string(),
        ]
    }

    #[test]
    fn test_rerank_result_ordering() {
        let mut results = [
            RerankResult {
                index: 0,
                score: 0.5,
            },
            RerankResult {
                index: 1,
                score: 0.9,
            },
            RerankResult {
                index: 2,
                score: 0.3,
            },
        ];

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        assert_eq!(results[0].index, 1);
        assert_eq!(results[1].index, 0);
        assert_eq!(results[2].index, 2);
    }

    #[test]
    fn test_sigmoid() {
        assert!((RerankerService::sigmoid(0.0) - 0.5).abs() < 0.001);
        assert!(RerankerService::sigmoid(10.0) > 0.99);
        assert!(RerankerService::sigmoid(-10.0) < 0.01);
    }
}
