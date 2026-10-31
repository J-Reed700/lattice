//! The answer is saved and returned before any cited page is read; the
//! capture that follows patches the saved message and tells the window.
use super::*;
use crate::features::conversation::chat::focus::FocusScope;
use crate::features::conversation::chat::persistence::{finalize_successful_turn, FinishedAnswer};
use crate::features::conversation::chat::test_runtime::FakeRuntime;
use crate::features::conversation::chat::turn_record::TurnRecorder;
use crate::features::conversation::chat::{ChatStreamEventDto, ChatTurn};
use crate::features::conversation::{
    repository::ConversationRepository, service::ConversationService, ConversationServiceTrait,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::conversation::MessageRole;
use lattice_core::shared::error::Result;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

const PAGE: &str = "https://example.com/slow-page";

/// A reader whose site never answers.
struct StalledReader {
    asked: Arc<Notify>,
}

#[async_trait::async_trait]
impl crate::features::conversation::chat::ports::PageReader for StalledReader {
    async fn read_page(&self, _url: &str) -> Result<FetchUrlContentOutput> {
        self.asked.notify_one();
        std::future::pending().await
    }
}

/// A reader that answers at once with the page.
struct ReadyReader;

#[async_trait::async_trait]
impl crate::features::conversation::chat::ports::PageReader for ReadyReader {
    async fn read_page(&self, url: &str) -> Result<FetchUrlContentOutput> {
        Ok(FetchUrlContentOutput {
            url: url.to_string(),
            title: Some("The page".into()),
            word_count: 4,
            content: "The whole page text.".into(),
            content_truncated: false,
            fetch_time_ms: 1.0,
            content_type: Some("text/html".into()),
            from_cache: false,
            read_via: Default::default(),
        })
    }
}

struct Saved {
    service: Arc<dyn ConversationServiceTrait>,
    pool: sqlx::SqlitePool,
    conversation_id: String,
    answer: crate::features::conversation::chat::persistence::SavedAnswer,
}

/// Save an answer that cites one search result whose page was never read.
async fn save_answer_citing_an_unread_page(runtime: &FakeRuntime, pool: sqlx::SqlitePool) -> Saved {
    let service: Arc<dyn ConversationServiceTrait> = Arc::new(ConversationService::new(Arc::new(
        ConversationRepository::new(pool.clone()),
    )));
    let conversation_id = service
        .create_conversation("Chat".into(), None)
        .await
        .unwrap()
        .id
        .to_string();
    let question = service
        .add_message_with_status(
            &conversation_id,
            MessageRole::User,
            "question".into(),
            2,
            "pending".into(),
        )
        .await
        .unwrap();
    let source: SourceDto = serde_json::from_value(serde_json::json!({
        "documentId": format!("web:{PAGE}"),
        "chunkId": "web-1",
        "content": "A search snippet.",
        "score": 0.8,
        "path": PAGE,
        "position": null,
        "fileName": "Slow page",
        "filePath": PAGE,
        "mimeType": "text/html",
        "category": "Web Page",
        "fileSizeBytes": 0,
        "modifiedAt": "",
        "citationId": 1
    }))
    .unwrap();
    let llm: Arc<dyn LLMPort> = Arc::new(crate::features::llm::engine::factory::MockLLMPort::new());
    let focus = FocusScope::default();
    let recorder = TurnRecorder::silent(&conversation_id, "request-1");
    let turn = ChatTurn {
        conv_service: &service,
        conversation_id: &conversation_id,
        request_id: "request-1",
        message: "question",
        standalone_question: None,
        llm: &llm,
        highlight_terms: &[],
        focus: &focus,
        recorder: &recorder,
    };
    let answer = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        finalize_successful_turn(
            runtime,
            turn,
            &question.id,
            2,
            0,
            FinishedAnswer {
                response: "The page says so [1].".into(),
                sources: vec![source],
                verification: None,
                memory_usage: None,
                retrieval_trace: None,
                turn_record: None,
                summary_refold: None,
            },
        ),
    )
    .await
    .expect("saving the answer reads no page")
    .unwrap();
    Saved {
        service,
        pool,
        conversation_id,
        answer,
    }
}

/// One migrated database per test, shared by the service and the archive.
async fn migrated_pool() -> sqlx::SqlitePool {
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

async fn metadata_of(pool: &sqlx::SqlitePool, id: &str) -> serde_json::Value {
    let raw: String = sqlx::query_scalar("SELECT metadata FROM conversation_messages WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn capture(
    runtime: &FakeRuntime,
    saved: &Saved,
    events: &Arc<Mutex<Vec<ChatStreamEventDto>>>,
    emitted: &Arc<Notify>,
) -> CitationCapture {
    let sink = Arc::clone(events);
    let emitted = Arc::clone(emitted);
    CitationCapture {
        runtime: runtime.share(),
        conversation_service: Arc::clone(&saved.service),
        conversation_id: saved.conversation_id.clone(),
        request_id: "request-1".into(),
        message_id: saved.answer.message_id.clone(),
        sources: saved.answer.response.sources.clone(),
        pages: saved.answer.uncaptured.clone(),
        emit: Arc::new(move |event| {
            sink.lock().unwrap().push(event);
            emitted.notify_one();
        }),
        background: Arc::clone(&runtime.background),
        then: None,
    }
}

fn only_event(events: &Mutex<Vec<ChatStreamEventDto>>) -> ChatStreamEventDto {
    let events = events.lock().unwrap();
    let [event] = events.as_slice() else {
        panic!("one capture event, got {}", events.len());
    };
    event.clone()
}

/// A site that never answers cannot hold the turn: the answer is saved and
/// returned with the capture marked pending while the read is still open.
/// Shutdown then ends the read, and the window is told so it stops waiting.
#[tokio::test]
async fn the_turn_returns_while_a_cited_page_is_still_being_read() {
    let asked = Arc::new(Notify::new());
    let runtime = FakeRuntime {
        page_reader: Some(Arc::new(StalledReader {
            asked: Arc::clone(&asked),
        })),
        ..Default::default()
    };
    let saved = save_answer_citing_an_unread_page(&runtime, migrated_pool().await).await;
    assert_eq!(
        saved.answer.uncaptured,
        vec![UncapturedPage {
            url: PAGE.into(),
            sources: vec![0],
        }]
    );
    let metadata = metadata_of(&saved.pool, &saved.answer.message_id).await;
    assert_eq!(metadata[CAPTURE_KEY]["pending"], true);
    assert!(metadata["sources"][0].get("webSnapshot").is_none());

    let events = Arc::new(Mutex::new(Vec::new()));
    capture(&runtime, &saved, &events, &Arc::new(Notify::new()))
        .spawn()
        .await;
    asked.notified().await;
    assert!(events.lock().unwrap().is_empty(), "the read is still open");

    runtime.background.close();
    runtime.background.wait().await;
    let event = only_event(&events);
    assert_eq!(event.status.as_deref(), Some("captured"));
    let captured = event.captured.as_ref().unwrap();
    assert_eq!(captured.message_id, saved.answer.message_id);
    assert!(captured.sources[0].web_snapshot.is_none());
    let metadata = metadata_of(&saved.pool, &saved.answer.message_id).await;
    assert_eq!(metadata[CAPTURE_KEY]["pending"], false);
    assert_eq!(metadata[CAPTURE_KEY]["interrupted"], true);
}

/// A page that can be read is archived, attached to the saved message's
/// sources, and sent to the window with them.
#[tokio::test]
async fn a_captured_page_is_archived_attached_and_announced() {
    // The archive and the conversation share one database, so the page is
    // stored beside the conversation it was cited in.
    let pool = migrated_pool().await;
    let runtime = FakeRuntime {
        page_reader: Some(Arc::new(ReadyReader)),
        records: Some(Arc::new(ConversationRepository::new(pool.clone()))),
        ..Default::default()
    };
    let saved = save_answer_citing_an_unread_page(&runtime, pool).await;

    let events = Arc::new(Mutex::new(Vec::new()));
    let emitted = Arc::new(Notify::new());
    capture(&runtime, &saved, &events, &emitted).spawn().await;
    tokio::time::timeout(std::time::Duration::from_secs(5), emitted.notified())
        .await
        .expect("the capture reports");

    let event = only_event(&events);
    let captured = event.captured.as_ref().unwrap();
    let snapshot = captured.sources[0].web_snapshot.as_ref().unwrap();
    assert_eq!(snapshot.text, "The whole page text.");
    let metadata = metadata_of(&saved.pool, &saved.answer.message_id).await;
    assert_eq!(
        metadata["sources"][0]["webSnapshot"]["text"],
        "The whole page text."
    );
    assert_eq!(metadata[CAPTURE_KEY]["pending"], false);
    assert!(metadata[CAPTURE_KEY].get("interrupted").is_none());
}
