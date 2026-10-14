use super::*;
use crate::application::ports::{embedding_port::EmbeddingTextChunk, EmbeddingPort};

struct LimitedEmbedder;

#[async_trait::async_trait]
impl EmbeddingPort for LimitedEmbedder {
    fn dimension(&self) -> usize {
        2
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
    fn split_text(&self, text: &str, _: &str) -> Result<Vec<EmbeddingTextChunk>> {
        Ok(text
            .split_whitespace()
            .map(|word| EmbeddingTextChunk {
                text: word.into(),
                start: 0,
                end: word.len(),
                token_count: 1,
            })
            .collect())
    }
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        if text.split_whitespace().count() > 1 {
            return Err(AppError::InvalidInput("Model token limit exceeded".into()));
        }
        Ok(if text == "first" {
            vec![1.0, 0.0]
        } else {
            vec![0.0, 1.0]
        })
    }
    async fn embed_batch(&self, _: &[String]) -> Result<Vec<Vec<f32>>> {
        unreachable!()
    }
}

#[tokio::test]
async fn long_memory_embeds_all_passages_within_budget() {
    assert!(LimitedEmbedder.embed_single("first last").await.is_err());
    let vector = embed_memory(&LimitedEmbedder, "first last").await.unwrap();
    let expected = std::f32::consts::FRAC_1_SQRT_2;
    assert!((vector[0] - expected).abs() < 1e-6);
    assert!((vector[1] - expected).abs() < 1e-6);
}

#[tokio::test]
async fn short_memory_retains_original_vector() {
    assert_eq!(
        embed_memory(&LimitedEmbedder, "first").await.unwrap(),
        vec![1.0, 0.0]
    );
}

#[tokio::test]
async fn empty_memory_is_rejected() {
    assert!(embed_memory(&LimitedEmbedder, "").await.is_err());
}
