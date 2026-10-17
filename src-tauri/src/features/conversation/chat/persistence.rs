use crate::features::conversation::chat::ports::{ChatRuntime, MemoryVector};
use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::shared::error::{AppError, Result};
use std::sync::Arc;
use tracing::{debug, warn};

use super::{ChatResponse, ChatTurn, ConversationMessage, RetrievalTraceDto, TurnRecordDto};

#[derive(Debug, Clone)]
struct MemoryToIndex {
    message_id: String,
    role: String,
    content: String,
}

pub(super) async fn persist_user_message_pending(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    request_id: &str,
    user_message: &str,
    attachment_names: &[String],
    attachment_document_ids: &[String],
    llm: &Arc<dyn lattice_core::application::ports::LLMPort>,
) -> Result<(String, usize)> {
    let message_tokens = llm.count_tokens(user_message);
    // The files this turn brought into the conversation, stamped on the message
    // so history shows where they entered. The names draw the chips; the ids
    // are what a regenerate re-reads, so the second run of a turn sees the same
    // files the first one did.
    let mut payload = serde_json::Map::new();
    payload.insert("requestId".to_string(), serde_json::json!(request_id));
    if !attachment_names.is_empty() {
        payload.insert(
            "attachments".to_string(),
            serde_json::json!(attachment_names),
        );
    }
    if !attachment_document_ids.is_empty() {
        payload.insert(
            "attachmentDocumentIds".to_string(),
            serde_json::json!(attachment_document_ids),
        );
    }
    let metadata = Some(serde_json::Value::Object(payload).to_string());
    let user_msg = conv_service
        .add_message_with_metadata(
            conversation_id,
            lattice_core::domain::conversation::MessageRole::User,
            user_message.to_string(),
            message_tokens as i64,
            "pending".to_string(),
            metadata,
        )
        .await?;

    Ok((user_msg.id, message_tokens))
}

/// The answer as it is saved: its text, the sources it cites, and what the
/// message's metadata carries beside them.
pub(super) struct FinishedAnswer {
    pub(super) response: String,
    pub(super) sources: Vec<SourceDto>,
    pub(super) verification: Option<serde_json::Value>,
    pub(super) memory_usage: Option<serde_json::Value>,
    pub(super) retrieval_trace: Option<RetrievalTraceDto>,
    pub(super) turn_record: Option<TurnRecordDto>,
    /// The turn's plan asking for the working summary to be refolded, passed
    /// on to consolidation.
    pub(super) summary_refold: Option<lattice_core::domain::conversation::memory::SummaryRefold>,
}

/// Save `answer` against the question saved as `user_message_id`, and return
/// the turn's response with the answer's message id.
pub(super) async fn finalize_successful_turn(
    container: &dyn ChatRuntime,
    turn: ChatTurn<'_>,
    user_message_id: &str,
    message_tokens: usize,
    context_len: usize,
    answer: FinishedAnswer,
) -> Result<(ChatResponse, String)> {
    let ChatTurn {
        conv_service,
        conversation_id,
        message: user_message,
        llm,
        ..
    } = turn;
    let FinishedAnswer {
        response: assistant_response,
        mut sources,
        verification: verification_metadata,
        memory_usage,
        retrieval_trace,
        turn_record,
        summary_refold,
    } = answer;
    super::source_snapshots::attach_cited_web_snapshots(
        container,
        conversation_id,
        &assistant_response,
        &mut sources,
    )
    .await;
    let response_tokens = llm.count_tokens(&assistant_response);

    let mut metadata_payload = serde_json::Map::new();
    if !sources.is_empty() {
        metadata_payload.insert("sources".to_string(), serde_json::json!(&sources));
    }
    if let Some(memory) = memory_usage {
        metadata_payload.insert("memory".into(), memory);
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
    // What the turn did, in the same shape the reader watched it happen. Absent
    // on every message written before the record existed, and on a turn that
    // recorded nothing at all; absent is "no record", never an empty timeline.
    if let Some(record) = turn_record {
        metadata_payload.insert("turn".to_string(), serde_json::json!(record));
    }
    let metadata = if metadata_payload.is_empty() {
        None
    } else {
        serde_json::to_string(&serde_json::Value::Object(metadata_payload)).ok()
    };

    let assistant_msg = commit_turn(
        conv_service,
        conversation_id,
        user_message_id,
        assistant_response.clone(),
        response_tokens as i64,
        metadata,
    )
    .await?;

    record_cited_web_sources(container, conversation_id, &sources).await;
    container.consolidate_after_turn(conversation_id.to_string(), summary_refold);

    spawn_memory_indexing(
        container.share(),
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

    let messages = turn_transcript(
        conv_service
            .get_conversation(conversation_id)
            .await
            .and_then(|aggregate| {
                aggregate.ok_or_else(|| {
                    AppError::NotFound(format!("Conversation {conversation_id} not found"))
                })
            }),
        user_message_id,
        user_message,
        message_tokens,
        &assistant_msg,
    );

    Ok((
        ChatResponse {
            conversation_id: conversation_id.to_string(),
            message: assistant_response,
            messages,
            context_used: context_len,
            sources,
            timing_metrics: None,
        },
        assistant_msg.id,
    ))
}

/// Commit the answer and settle the question in one step, or mark the question
/// failed.
///
/// A commit that fails (a busy database, or a turn truncated or deleted while
/// it generated) would otherwise leave the question `pending` for good, and a
/// pending question renders as still in flight.
async fn commit_turn(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    user_message_id: &str,
    content: String,
    tokens: i64,
    metadata: Option<String>,
) -> Result<lattice_core::domain::conversation::ConversationMessage> {
    match conv_service
        .complete_turn(conversation_id, user_message_id, content, tokens, metadata)
        .await
    {
        Ok(message) => Ok(message),
        Err(error) => {
            mark_user_message_failed(conv_service, user_message_id).await;
            Err(error)
        }
    }
}

fn message_dto(
    message: &lattice_core::domain::conversation::ConversationMessage,
) -> ConversationMessage {
    ConversationMessage {
        id: message.id.clone(),
        conversation_id: message.conversation_id.as_str().to_string(),
        tokens: message.tokens,
        role: match message.role {
            lattice_core::domain::conversation::MessageRole::User => "user".to_string(),
            lattice_core::domain::conversation::MessageRole::Assistant => "assistant".to_string(),
            lattice_core::domain::conversation::MessageRole::System => "system".to_string(),
        },
        content: message.content.clone(),
        status: message.status.clone(),
        created_at: message.created_at.to_rfc3339(),
        metadata: message.metadata.clone(),
    }
}

/// The conversation as it stands after the commit.
///
/// The answer is saved by the time this is read, so a failed read is not a
/// failed turn: reporting it as one would show an error for an answer the
/// user will find on reload, and would skip the grounding check. What the turn
/// itself wrote is known without the read, so that is returned instead.
fn turn_transcript(
    read: Result<lattice_core::domain::conversation::ConversationAggregate>,
    user_message_id: &str,
    user_message: &str,
    message_tokens: usize,
    answer: &lattice_core::domain::conversation::ConversationMessage,
) -> Vec<ConversationMessage> {
    match read {
        Ok(aggregate) => aggregate.messages().iter().map(message_dto).collect(),
        Err(error) => {
            warn!(%error, "Answer saved but the conversation could not be re-read; returning this turn only");
            let answer = message_dto(answer);
            let question = ConversationMessage {
                id: user_message_id.to_string(),
                role: "user".to_string(),
                content: user_message.to_string(),
                tokens: message_tokens as i64,
                status: "completed".to_string(),
                metadata: None,
                ..answer.clone()
            };
            vec![question, answer]
        }
    }
}

/// Keep the pages a turn cited as context for the turns that follow.
///
/// Without this, web research ends with the turn that did it: the citations
/// survive in the answer's own metadata and in the sources panel, but the next
/// turn assembles its context from `conversation_web_sources`, which nothing
/// was writing. So a chat could show thirty-seven sources in its sidebar and
/// still enter the next turn with no grounded context at all, and say — truth-
/// fully, from where it stood — that it had nothing to go on.
///
/// Failure here is logged and dropped. The turn is already finished and
/// answered; losing tomorrow's context must not retract today's answer.
async fn record_cited_web_sources(
    container: &dyn ChatRuntime,
    conversation_id: &str,
    sources: &[SourceDto],
) {
    let repository = container.chat_records();

    let keepable = cited_web_sources(sources);
    let recorded = keepable.len();
    for source in keepable {
        if let Err(error) = repository
            .add_cited_web_source(
                conversation_id,
                source.url,
                source.title,
                source.excerpt,
                Some(source.score),
            )
            .await
        {
            warn!(
                conversation_id = conversation_id,
                error = %error,
                "Failed to keep a cited web source as conversation context"
            );
            return;
        }
    }

    if recorded > 0 {
        debug!(
            conversation_id = conversation_id,
            recorded, "Kept this turn's web citations as conversation context"
        );
    }
}

/// One web page a turn cited, in the shape the conversation keeps it.
#[derive(Debug, PartialEq)]
struct CitedWebSource {
    url: String,
    title: Option<String>,
    excerpt: Option<String>,
    score: f32,
}

/// Pick the web pages worth carrying forward out of a turn's citations.
///
/// Document sources are left alone: they are already reachable by searching the
/// vault, and a copy here would compete with the passage that has a citation
/// number. Two citations of one URL become one row, because the store is keyed
/// by URL and the second would only overwrite the first.
fn cited_web_sources(sources: &[SourceDto]) -> Vec<CitedWebSource> {
    use crate::features::conversation::chat::retrieval::WEB_SOURCE_PREFIX;

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut keepable: Vec<CitedWebSource> = Vec::new();
    // Every cited page is kept: a source the answer cited is evidence the next
    // turn may need, and what the next prompt has room for is decided where
    // that prompt is assembled, not by a count here.
    for source in sources {
        // `file_path` holds the page URL for a web source; `document_id` is the
        // same URL behind a prefix and is what marks it as one.
        if !source.document_id.starts_with(WEB_SOURCE_PREFIX) {
            continue;
        }
        let url = source.file_path.trim();
        if url.is_empty() || !seen.insert(url.to_ascii_lowercase()) {
            continue;
        }
        keepable.push(CitedWebSource {
            url: url.to_string(),
            // A result with no title carries its own URL as the file name;
            // storing that as a title would render the link twice.
            title: Some(source.file_name.trim())
                .filter(|value| !value.is_empty() && *value != url)
                .map(ToOwned::to_owned),
            // The pointer the sources panel shows, not the page: a page the
            // turn read is archived whole beside this row (`content`), and
            // storing it again here doubled it on disk and had every later
            // turn load it to list a link. A search result's pointer is its
            // snippet.
            excerpt: source
                .excerpt
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or_else(|| Some(source.content.trim()).filter(|value| !value.is_empty()))
                .map(ToOwned::to_owned),
            score: source.score,
        });
    }
    keepable
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
    container: Arc<dyn ChatRuntime>,
    conversation_id: String,
    memories: Vec<MemoryToIndex>,
) {
    let background = container.background();
    let cancel = background.token();
    background.spawn(async move {
        let loaded = tokio::select! {
            biased;
            _ = cancel.cancelled() => return,
            result = container.get_or_load_embedding() => result,
        };
        let embedding_service = match loaded {
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

            let embedded = tokio::select! {
                biased;
                _ = cancel.cancelled() => return,
                result = embed_memory(embedding_service.as_ref(), &memory.content) => result,
            };
            let embedding = match embedded {
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

            let insert_result = container
                .chat_records()
                .persist_memory_vector(MemoryVector {
                    conversation_id: &conversation_id,
                    vector_id: &memory_id,
                    message_id: &memory.message_id,
                    role: &memory.role,
                    content: &memory.content,
                    embedding: embedding_blob,
                    dimension,
                    embedding_model: &embedding_model,
                    created_at: &created_at,
                })
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
    lattice_core::shared::persistence::embedding_encoding::encode_embedding(embedding)
}

/// Keep the per-message vector while respecting the active model's token budget.
/// Every passage contributes; long answers are never silently truncated.
async fn embed_memory(
    service: &dyn lattice_core::application::ports::EmbeddingPort,
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
mod memory_embedding_tests;

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod cited_web_source_tests;

/// Explicit user memory notes use the same embedding writer as chat turns.
pub(crate) fn index_memory_note(
    container: Arc<dyn ChatRuntime>,
    conversation_id: String,
    message_id: String,
    content: String,
) {
    spawn_memory_indexing(
        container,
        conversation_id,
        vec![MemoryToIndex {
            message_id,
            role: "user".into(),
            content,
        }],
    );
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod turn_commit_tests;
