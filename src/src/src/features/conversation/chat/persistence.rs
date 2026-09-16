use crate::features::qa::dto::SourceDto;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;
use tracing::warn;

use super::{ChatResponse, ConversationMessage, RetrievalTraceDto};

#[derive(Debug, Clone)]
struct MemoryToIndex {
    message_id: String,
    role: String,
    content: String,
}

pub(super) async fn persist_user_message_pending(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    user_message: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
) -> Result<(String, usize)> {
    let message_tokens = llm.count_tokens(user_message);
    let user_msg = conv_service
        .add_message_with_status(
            conversation_id,
            crate::domain::conversation::MessageRole::User,
            user_message.to_string(),
            message_tokens as i64,
            "pending".to_string(),
        )
        .await?;

    Ok((user_msg.id, message_tokens))
}

// Turn finalization deliberately keeps its persisted inputs explicit at this boundary.
#[allow(clippy::too_many_arguments)]
pub(super) async fn finalize_successful_turn(
    container: &Container,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    user_message_id: &str,
    user_message: &str,
    assistant_response: String,
    context_len: usize,
    sources: Vec<SourceDto>,
    verification_metadata: Option<serde_json::Value>,
    retrieval_trace: Option<RetrievalTraceDto>,
    message_tokens: usize,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
) -> Result<ChatResponse> {
    let response_tokens = llm.count_tokens(&assistant_response);

    let mut metadata_payload = serde_json::Map::new();
    if !sources.is_empty() {
        metadata_payload.insert("sources".to_string(), serde_json::json!(&sources));
    }
    if let Some(verification) = verification_metadata {
        metadata_payload.insert("verification".to_string(), verification);
    }
    // Persisted so the "Searched N documents" line survives a reload. Absent
    // when retrieval never ran; older messages have no key at all, which the
    // frontend must read as "no trace", never as zeros.
    if let Some(trace) = retrieval_trace {
        metadata_payload.insert("retrieval".to_string(), serde_json::json!(trace));
    }
    let metadata = if metadata_payload.is_empty() {
        None
    } else {
        serde_json::to_string(&serde_json::Value::Object(metadata_payload)).ok()
    };

    let assistant_msg = conv_service
        .complete_turn(
            conversation_id,
            user_message_id,
            assistant_response.clone(),
            response_tokens as i64,
            metadata,
        )
        .await?;

    spawn_memory_indexing(
        container.clone(),
        conversation_id.to_string(),
        vec![
            MemoryToIndex {
                message_id: user_message_id.to_string(),
                role: "user".to_string(),
                content: user_message.to_string(),
            },
            MemoryToIndex {
                message_id: assistant_msg.id.clone(),
                role: "assistant".to_string(),
                content: assistant_response.clone(),
            },
        ],
    );

    let aggregate = conv_service
        .get_conversation(conversation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Conversation {} not found", conversation_id)))?;

    let messages: Vec<ConversationMessage> = aggregate
        .messages()
        .iter()
        .map(|msg| ConversationMessage {
            id: msg.id.clone(),
            conversation_id: msg.conversation_id.as_str().to_string(),
            tokens: msg.tokens,
            role: match msg.role {
                crate::domain::conversation::MessageRole::User => "user".to_string(),
                crate::domain::conversation::MessageRole::Assistant => "assistant".to_string(),
                crate::domain::conversation::MessageRole::System => "system".to_string(),
            },
            content: msg.content.clone(),
            status: msg.status.clone(),
            created_at: msg.created_at.to_rfc3339(),
            metadata: msg.metadata.clone(),
        })
        .collect();

    let logger = crate::infrastructure::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::infrastructure::audit::AuditAction::QuestionAnswered,
        conversation_id,
        "context_messages" => context_len.to_string().as_str(),
        "message_tokens" => message_tokens.to_string().as_str(),
        "response_tokens" => response_tokens.to_string().as_str()
    )
    .await
    .ok();

    Ok(ChatResponse {
        conversation_id: conversation_id.to_string(),
        message: assistant_response,
        messages,
        context_used: context_len,
        sources,
        timing_metrics: None,
    })
}

pub(super) async fn mark_user_message_failed(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    user_message_id: &str,
) {
    conv_service
        .fail_pending_turn(user_message_id)
        .await
        .map_err(|e| warn!("Failed to update message status to 'failed': {}", e))
        .ok();
}

fn spawn_memory_indexing(
    container: Container,
    conversation_id: String,
    memories: Vec<MemoryToIndex>,
) {
    tokio::spawn(async move {
        let embedding_service = match container.get_or_load_embedding().await {
            Ok(service) => service,
            Err(e) => {
                warn!(
                    conversation_id = conversation_id.as_str(),
                    error = %e,
                    "Skipping memory indexing (embedding service unavailable)"
                );
                return;
            }
        };

        let embedding_model = embedding_service.model_identity();

        for memory in memories {
            if memory.content.trim().is_empty() {
                continue;
            }

            let embedding = match embed_memory(embedding_service.as_ref(), &memory.content).await {
                Ok(value) => value,
                Err(e) => {
                    warn!(
                        conversation_id = conversation_id.as_str(),
                        message_id = memory.message_id.as_str(),
                        error = %e,
                        "Failed to embed conversation memory"
                    );
                    continue;
                }
            };

            let embedding_blob = embedding_to_blob(&embedding);
            let dimension = embedding.len() as i64;
            let memory_id = uuid::Uuid::new_v4().to_string();
            let created_at = chrono::Utc::now().to_rfc3339();

            let insert_result = sqlx::query(
                r#"
                INSERT INTO conversation_memory_vectors (
                    id, conversation_id, message_id, role, content,
                    embedding, dimension, embedding_model, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(message_id) DO UPDATE SET
                    role = excluded.role,
                    content = excluded.content,
                    embedding = excluded.embedding,
                    dimension = excluded.dimension,
                    embedding_model = excluded.embedding_model,
                    created_at = excluded.created_at
                "#,
            )
            .bind(memory_id)
            .bind(&conversation_id)
            .bind(&memory.message_id)
            .bind(&memory.role)
            .bind(&memory.content)
            .bind(embedding_blob)
            .bind(dimension)
            .bind(&embedding_model)
            .bind(created_at)
            .execute(container.db_pool())
            .await;

            if let Err(e) = insert_result {
                warn!(
                    conversation_id = conversation_id.as_str(),
                    message_id = memory.message_id.as_str(),
                    error = %e,
                    "Failed to persist conversation memory vector"
                );
            }
        }
    });
}

fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    crate::features::embedding::encoding::encode_embedding(embedding)
}

/// Keep the per-message vector while respecting the active model's token budget.
/// Every passage contributes; long answers are never silently truncated.
async fn embed_memory(
    service: &dyn crate::application::ports::EmbeddingPort,
    content: &str,
) -> Result<Vec<f32>> {
    let chunks = service.split_text(content, "")?;
    if chunks.is_empty() {
        return Err(AppError::InvalidInput(
            "Cannot embed empty conversation memory".into(),
        ));
    }
    let mut sum = vec![0.0_f64; service.dimension()];
    // Process one passage at a time to bound inference memory for long answers.
    for chunk in &chunks {
        let vector = service.embed_single(&chunk.text).await?;
        if vector.len() != sum.len() || vector.iter().any(|value| !value.is_finite()) {
            return Err(AppError::InvalidInput(
                "Invalid conversation memory embedding".into(),
            ));
        }
        if chunks.len() == 1 {
            return Ok(vector);
        }
        let weight = chunk.token_count.max(1) as f64;
        for (total, value) in sum.iter_mut().zip(vector) {
            *total += value as f64 * weight;
        }
    }
    let norm = sum.iter().map(|value| value * value).sum::<f64>().sqrt();
    if norm == 0.0 || !norm.is_finite() {
        return Err(AppError::InvalidInput(
            "Invalid conversation memory embedding norm".into(),
        ));
    }
    Ok(sum.into_iter().map(|value| (value / norm) as f32).collect())
}

#[cfg(test)]
mod memory_embedding_tests {
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
}
