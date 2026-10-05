use crate::features::conversation::chat::ports::ChatRuntime;
use crate::features::qa::dto::SourceDto;
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

#[allow(clippy::too_many_arguments)]
pub(super) async fn persist_user_message_pending(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    request_id: &str,
    user_message: &str,
    attachment_names: &[String],
    attachment_document_ids: &[String],
    llm: &Arc<dyn crate::application::ports::LLMPort>,
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
    container: &dyn ChatRuntime,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    user_message_id: &str,
    user_message: &str,
    assistant_response: String,
    context_len: usize,
    mut sources: Vec<SourceDto>,
    verification_metadata: Option<serde_json::Value>,
    memory_usage: Option<serde_json::Value>,
    retrieval_trace: Option<RetrievalTraceDto>,
    turn_record: Option<TurnRecordDto>,
    message_tokens: usize,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
) -> Result<(ChatResponse, String)> {
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
    container.consolidate_after_turn(conversation_id.to_string());

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
) -> Result<crate::domain::conversation::ConversationMessage> {
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

fn message_dto(message: &crate::domain::conversation::ConversationMessage) -> ConversationMessage {
    ConversationMessage {
        id: message.id.clone(),
        conversation_id: message.conversation_id.as_str().to_string(),
        tokens: message.tokens,
        role: match message.role {
            crate::domain::conversation::MessageRole::User => "user".to_string(),
            crate::domain::conversation::MessageRole::Assistant => "assistant".to_string(),
            crate::domain::conversation::MessageRole::System => "system".to_string(),
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
    read: Result<crate::domain::conversation::ConversationAggregate>,
    user_message_id: &str,
    user_message: &str,
    message_tokens: usize,
    answer: &crate::domain::conversation::ConversationMessage,
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
    container: &dyn ChatRuntime,
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
    container: Arc<dyn ChatRuntime>,
    conversation_id: String,
    memories: Vec<MemoryToIndex>,
) {
    let cancel = crate::shared::runtime::background::cancellation_token();
    crate::shared::runtime::background::spawn(async move {
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

            let insert_result =
                crate::features::conversation::repository::ConversationRepository::new(
                    container.db_pool().clone(),
                )
                .persist_memory_vector(
                    &conversation_id,
                    &memory_id,
                    &memory.message_id,
                    &memory.role,
                    &memory.content,
                    embedding_blob,
                    dimension,
                    &embedding_model,
                    &created_at,
                )
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

            web_snapshot: None,
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

            web_snapshot: None,
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
mod turn_commit_tests {
    use super::*;
    use crate::domain::conversation::MessageRole;
    use crate::features::conversation::{
        repository::ConversationRepository, service::ConversationService, ConversationServiceTrait,
    };

    async fn service_with_pending_question() -> (
        Arc<dyn ConversationServiceTrait>,
        sqlx::SqlitePool,
        String,
        String,
    ) {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let service: Arc<dyn ConversationServiceTrait> = Arc::new(ConversationService::new(
            Arc::new(ConversationRepository::new(pool.clone())),
        ));
        let conversation = service
            .create_conversation("Chat".into(), "model".into(), None)
            .await
            .unwrap();
        let id = conversation.id.to_string();
        let user = service
            .add_message_with_status(
                &id,
                MessageRole::User,
                "question".into(),
                2,
                "pending".into(),
            )
            .await
            .unwrap();
        (service, pool, id, user.id)
    }

    async fn status_of(pool: &sqlx::SqlitePool, id: &str) -> String {
        sqlx::query_scalar("SELECT status FROM conversation_messages WHERE id=?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn failed_question_retains_request_identity_and_attachments() {
        let (service, pool, id, _) = service_with_pending_question().await;
        let llm: Arc<dyn crate::application::ports::LLMPort> =
            Arc::new(crate::features::llm::engine::factory::MockLLMPort::new());
        let (user_id, _) = persist_user_message_pending(
            &service,
            &id,
            "request-one",
            "same question",
            &["notes.pdf".into()],
            &["document-one".into()],
            &llm,
        )
        .await
        .unwrap();
        mark_user_message_failed(&service, &user_id).await;
        let raw: String =
            sqlx::query_scalar("SELECT metadata FROM conversation_messages WHERE id=?")
                .bind(&user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let metadata: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(metadata["requestId"], "request-one");
        assert_eq!(metadata["attachments"], serde_json::json!(["notes.pdf"]));
        assert_eq!(
            metadata["attachmentDocumentIds"],
            serde_json::json!(["document-one"])
        );
        assert_eq!(status_of(&pool, &user_id).await, "failed");
    }

    #[tokio::test]
    async fn a_commit_that_fails_leaves_the_question_failed_not_pending() {
        let (service, pool, id, user_id) = service_with_pending_question().await;
        sqlx::query("CREATE TRIGGER fail_assistant BEFORE INSERT ON conversation_messages WHEN NEW.role='assistant' BEGIN SELECT RAISE(ABORT, 'injected failure'); END")
            .execute(&pool).await.unwrap();

        let result = commit_turn(&service, &id, &user_id, "answer".into(), 3, None).await;

        assert!(result.is_err());
        assert_eq!(status_of(&pool, &user_id).await, "failed");
    }

    #[tokio::test]
    async fn a_committed_turn_is_not_undone_by_a_failed_re_read() {
        let (service, pool, id, user_id) = service_with_pending_question().await;
        let answer = commit_turn(&service, &id, &user_id, "answer".into(), 3, None)
            .await
            .unwrap();

        let messages = turn_transcript(
            Err(AppError::Database("database is locked".into())),
            &user_id,
            "question",
            2,
            &answer,
        );

        assert_eq!(status_of(&pool, &user_id).await, "completed");
        let roles: Vec<_> = messages.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, ["user", "assistant"]);
        assert_eq!(messages[1].id, answer.id);
        assert_eq!(messages[1].content, "answer");
        assert_eq!(messages[0].status, "completed");
    }
}
