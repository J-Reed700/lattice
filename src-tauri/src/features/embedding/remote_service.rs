//! Remote embedding service backed by HTTP APIs.
//!
//! Provides embeddings via a remote API (e.g., OpenAI-compatible endpoint).

use crate::features::embedding::EmbeddingServiceTrait;
use crate::shared::error::{AppError, Result};
use crate::shared::utils::reqwest_client_builder;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const DEFAULT_REMOTE_EMBEDDING_URL: &str = "https://api.openai.com/v1/embeddings";
pub const DEFAULT_REMOTE_EMBEDDING_MODEL: &str = "text-embedding-3-small";

#[derive(Debug, Clone)]
pub struct RemoteEmbeddingService {
    api_key: String,
    api_url: String,
    model: String,
    client: Client,
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    input: Vec<String>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl RemoteEmbeddingService {
    pub fn new(api_key: String, api_url: String, model: String) -> Result<Self> {
        if api_key.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Remote embedding API key cannot be empty".to_string(),
            ));
        }

        let client = reqwest_client_builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("Failed to create HTTP client: {}", e),
            })?;

        Ok(Self {
            api_key,
            api_url,
            model,
            client,
        })
    }

    async fn request_embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let request = EmbeddingRequest {
            input: texts.to_vec(),
            model: self.model.clone(),
        };

        let response = self
            .client
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("Embedding request failed: {}", e),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable response body>".to_string());
            return Err(AppError::EmbeddingFailed {
                reason: format!("Embedding API error ({}): {}", status, body),
            });
        }

        let payload: EmbeddingResponse =
            response
                .json()
                .await
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("Failed to parse embedding response: {}", e),
                })?;

        if payload.data.len() != texts.len() {
            return Err(AppError::EmbeddingFailed {
                reason: format!(
                    "Embedding API returned {} items for {} inputs",
                    payload.data.len(),
                    texts.len()
                ),
            });
        }

        Ok(payload
            .data
            .into_iter()
            .map(|item| item.embedding)
            .collect())
    }
}

#[async_trait]
impl EmbeddingServiceTrait for RemoteEmbeddingService {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.request_embeddings(&[text.to_string()]).await?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| AppError::EmbeddingFailed {
                reason: "Embedding API returned no embedding".to_string(),
            })
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.request_embeddings(texts).await
    }

    async fn embed_contextualized_chunks(
        &self,
        chunks: &[crate::features::indexing::engine::chunker::ContextualizedChunk],
    ) -> Result<Vec<Vec<f32>>> {
        let texts: Vec<String> = chunks
            .iter()
            .map(|chunk| chunk.contextualized_content.clone())
            .collect();
        self.request_embeddings(&texts).await
    }
}
