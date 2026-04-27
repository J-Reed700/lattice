use crate::features::qa::dto::SourceDto;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;
use tracing::warn;

use super::{ChatResponse, ConversationMessage};

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
    message_tokens: usize,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
) -> Result<ChatResponse> {
    conv_service
        .update_message_status(user_message_id, "completed".to_string())
        .await?;

    let response_tokens = llm.count_tokens(&assistant_response);

    let mut metadata_payload = serde_json::Map::new();
    if !sources.is_empty() {
        metadata_payload.insert("sources".to_string(), serde_json::json!(&sources));
    }
    if let Some(verification) = verification_metadata {
        metadata_payload.insert("verification".to_string(), verification);
    }
    let metadata = if metadata_payload.is_empty() {
        None
    } else {
        serde_json::to_string(&serde_json::Value::Object(metadata_payload)).ok()
    };

    let assistant_msg = conv_service
        .add_assistant_message_with_metadata(
            conversation_id,
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
        .update_message_status(user_message_id, "failed".to_string())
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

        let embedding_model = "conversation-memory".to_string();

        for memory in memories {
            if memory.content.trim().is_empty() {
                continue;
            }

            let embedding = match embedding_service.embed_single(&memory.content).await {
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
    embedding
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}
