use crate::features::qa::dto::SourceDto;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;
use tracing::{debug, warn};

use super::{ChatResponse, ConversationMessage, RetrievalTraceDto, TurnRecordDto};

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
    attachment_names: &[String],
    llm: &Arc<dyn crate::application::ports::LLMPort>,
) -> Result<(String, usize)> {
    let message_tokens = llm.count_tokens(user_message);
    // The files this turn brought into the conversation, stamped on the message
    // so history shows where they entered. Names only: the documents themselves
    // are reachable through the conversation's linked sources.
    let metadata = (!attachment_names.is_empty())
        .then(|| serde_json::json!({ "attachments": attachment_names }).to_string());
    let user_msg = conv_service
        .add_message_with_metadata(
            conversation_id,
            crate::domain::conversation::MessageRole::User,
            user_message.to_string(),
            message_tokens as i64,
            "pending".to_string(),
            metadata,
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
    turn_record: Option<TurnRecordDto>,
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

    let assistant_msg = conv_service
        .complete_turn(
            conversation_id,
            user_message_id,
            assistant_response.clone(),
            response_tokens as i64,
            metadata,
        )
        .await?;

    record_cited_web_sources(container, conversation_id, &sources).await;

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

/// How many of a turn's web citations are kept as conversation context.
///
/// The prompt side already takes only the ten most recent and truncates their
/// excerpts, so a larger number here buys nothing for the next turn. It only
/// governs how fast one long research turn can push earlier rounds out of that
/// window, which is why it is smaller than a full result page.
const CITED_WEB_SOURCES_KEPT_PER_TURN: usize = 8;

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
    container: &Container,
    conversation_id: &str,
    sources: &[SourceDto],
) {
    let repository = crate::features::conversation::repository::ConversationRepository::new(
        container.db_pool().clone(),
    );

    let keepable = cited_web_sources(sources);
    let recorded = keepable.len();
    for source in keepable {
        if let Err(error) = repository
            .add_conversation_web_source(
                conversation_id.to_string(),
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
    for source in sources {
        if keepable.len() >= CITED_WEB_SOURCES_KEPT_PER_TURN {
            break;
        }
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
            excerpt: Some(source.content.trim())
                .filter(|value| !value.is_empty())
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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod cited_web_source_tests {
    use super::*;
    use crate::features::conversation::chat::retrieval::WEB_SOURCE_PREFIX;

    fn web(url: &str, title: &str, excerpt: &str) -> SourceDto {
        SourceDto {
            document_id: format!("{WEB_SOURCE_PREFIX}{url}"),
            chunk_id: "web-result-1".to_string(),
            content: excerpt.to_string(),
            score: 0.5,
            path: Some(url.to_string()),
            position: Some(1),
            file_name: title.to_string(),
            file_path: url.to_string(),
            mime_type: "text/html".to_string(),
            category: "Web Article".to_string(),
            file_size_bytes: 0,
            modified_at: "2026-09-21T00:00:00Z".to_string(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            page_number: None,
            chunk_excerpts: None,
            citation_id: None,
        }
    }

    fn document(id: &str) -> SourceDto {
        SourceDto {
            document_id: id.to_string(),
            chunk_id: format!("{id}-c1"),
            content: "A passage from a file on disk.".to_string(),
            score: 0.9,
            path: Some(format!("/vault/{id}.md")),
            position: Some(1),
            file_name: format!("{id}.md"),
            file_path: format!("/vault/{id}.md"),
            mime_type: "text/markdown".to_string(),
            category: "Markdown".to_string(),
            file_size_bytes: 0,
            modified_at: "2026-09-21T00:00:00Z".to_string(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            page_number: None,
            chunk_excerpts: None,
            citation_id: None,
        }
    }

    #[test]
    fn a_turns_web_citations_are_what_gets_kept() {
        let kept = cited_web_sources(&[
            document("doc-1"),
            web(
                "https://example.com/broccolini",
                "Growing broccolini",
                "Broccolini tolerates light frost.",
            ),
            document("doc-2"),
        ]);

        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(kept[0].url, "https://example.com/broccolini");
        assert_eq!(kept[0].title.as_deref(), Some("Growing broccolini"));
        assert_eq!(
            kept[0].excerpt.as_deref(),
            Some("Broccolini tolerates light frost.")
        );
    }

    /// The store is keyed by URL, so a second row for a page the turn cited
    /// twice would only overwrite the first.
    #[test]
    fn one_page_cited_twice_is_kept_once() {
        let kept = cited_web_sources(&[
            web("https://example.com/a", "A", "first"),
            web("https://EXAMPLE.com/a", "A again", "second"),
        ]);

        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(kept[0].excerpt.as_deref(), Some("first"));
    }

    /// A deep-research turn can cite dozens of pages. The prompt window that
    /// reads these takes ten, so one turn must not fill it on its own and push
    /// every earlier round of the conversation out.
    #[test]
    fn one_turn_cannot_crowd_out_every_earlier_round() {
        let sources: Vec<SourceDto> = (0..40)
            .map(|i| web(&format!("https://example.com/{i}"), "Page", "text"))
            .collect();

        assert_eq!(
            cited_web_sources(&sources).len(),
            CITED_WEB_SOURCES_KEPT_PER_TURN
        );
    }

    /// A result with no title carries its URL as its file name. Stored as a
    /// title, the prompt entry would read `- <url> (<url>)`.
    #[test]
    fn a_url_standing_in_for_a_missing_title_is_not_stored_as_one() {
        let url = "https://example.com/untitled";
        let kept = cited_web_sources(&[web(url, url, "")]);

        assert_eq!(kept[0].title, None, "{kept:?}");
        assert_eq!(kept[0].excerpt, None, "{kept:?}");
    }

    #[test]
    fn a_turn_that_cited_no_web_page_stores_nothing() {
        assert!(cited_web_sources(&[document("doc-1")]).is_empty());
        assert!(cited_web_sources(&[]).is_empty());
    }
}
