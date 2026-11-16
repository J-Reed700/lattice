use super::*;
use crate::features::conversation::{
    repository::ConversationRepository, service::ConversationService, ConversationServiceTrait,
};
use lattice_core::application::contracts::replies::ReplyPieceDto;
use lattice_core::domain::conversation::MessageRole;
use lattice_core::shared::persistence::Database;

async fn service_with_pending_question() -> (
    Arc<dyn ConversationServiceTrait>,
    sqlx::SqlitePool,
    String,
    String,
) {
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let service: Arc<dyn ConversationServiceTrait> = Arc::new(ConversationService::new(Arc::new(
        ConversationRepository::new(Database::new(pool.clone())),
    )));
    let conversation = service
        .create_conversation("Chat".into(), None)
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
    let llm: Arc<dyn lattice_core::application::ports::LLMPort> =
        Arc::new(crate::features::llm::engine::factory::MockLLMPort::new());
    let runtime = crate::features::conversation::chat::test_runtime::FakeRuntime::default();
    let (user_id, _) = persist_user_message_pending(
        &runtime,
        &service,
        &id,
        "request-one",
        "same question",
        QuestionAttachments {
            names: &["notes.pdf".into()],
            document_ids: &["document-one".into()],
            image_ids: &[],
            pictures_seen: true,
        },
        &llm,
    )
    .await
    .unwrap();
    mark_user_message_failed(&service, &user_id).await;
    let raw: String = sqlx::query_scalar("SELECT metadata FROM conversation_messages WHERE id=?")
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
    let references = Some(r#"{"references":[]}"#.to_owned());
    let answer = commit_turn(&service, &id, &user_id, "answer".into(), 3, references)
        .await
        .unwrap();

    let messages = turn_transcript(
        Err(AppError::Database("database is locked".into())),
        &user_id,
        "question",
        2,
        &answer,
        &ReplyText {
            text: "answer".into(),
            ..Default::default()
        },
    );

    assert_eq!(status_of(&pool, &user_id).await, "completed");
    let roles: Vec<_> = messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, ["user", "assistant"]);
    assert_eq!(messages[1].id, answer.id);
    assert_eq!(messages[1].content, "answer");
    assert_eq!(messages[0].status, "completed");
    assert_eq!(messages[0].reply, None);
    assert_eq!(
        messages[1].reply,
        Some(vec![ReplyPieceDto::Text {
            text: "answer".into()
        }])
    );
}

/// The turn's response replaces the messages the frontend holds, so an earlier
/// answer saved before references comes back in it as reading it gives it.
#[tokio::test]
async fn a_finished_turn_hands_back_an_earlier_answer_saved_before_references_with_its_references()
{
    use crate::features::conversation::chat::focus::FocusScope;
    use crate::features::conversation::chat::test_runtime::FakeRuntime;
    use crate::features::conversation::chat::turn_record::TurnRecorder;

    let (service, _pool, _, _) = service_with_pending_question().await;
    let id = service
        .create_conversation("Earlier".into(), None)
        .await
        .unwrap()
        .id
        .to_string();
    service
        .add_message_with_status(
            &id,
            MessageRole::User,
            "Why is the valley colder?".into(),
            2,
            "completed".into(),
        )
        .await
        .unwrap();
    let sources = serde_json::json!({ "sources": [{
        "documentId": "doc-weather", "chunkId": "doc-weather-chunk", "content": "Cold air sinks.",
        "score": 0.5, "fileName": "weather.txt", "filePath": "/notes/weather.txt",
        "mimeType": "text/plain", "category": "Text File", "fileSizeBytes": 10,
        "modifiedAt": "2025-01-01T00:00:00Z", "citationId": 1,
    }] });
    let earlier = service
        .add_message_with_metadata(
            &id,
            MessageRole::Assistant,
            "Cold air sinks [1].".into(),
            4,
            "completed".into(),
            Some(sources.to_string()),
        )
        .await
        .unwrap();
    let question = service
        .add_message_with_status(
            &id,
            MessageRole::User,
            "And at noon?".into(),
            2,
            "pending".into(),
        )
        .await
        .unwrap();

    let runtime = FakeRuntime::default();
    let llm: Arc<dyn lattice_core::application::ports::LLMPort> =
        Arc::new(crate::features::llm::engine::factory::MockLLMPort::new());
    let focus = FocusScope::default();
    let recorder = TurnRecorder::silent(&id, "request-noon");
    let saved = finalize_successful_turn(
        &runtime,
        ChatTurn {
            conv_service: &service,
            conversation_id: &id,
            request_id: "request-noon",
            message: "And at noon?",
            standalone_question: None,
            llm: &llm,
            highlight_terms: &[],
            focus: &focus,
            recorder: &recorder,
            names: &std::sync::Arc::new(
                lattice_core::application::services::model_contract::kept::TurnNames::unstored(),
            ),
        },
        &question.id,
        2,
        0,
        FinishedAnswer {
            reply: ReplyText {
                text: "It warms.".into(),
                ..Default::default()
            },
            sources: Vec::new(),
            verification: None,
            memory_usage: None,
            retrieval_trace: None,
            turn_record: None,
            summary_refold: None,
        },
    )
    .await
    .unwrap();

    let handed_back = saved
        .response
        .messages
        .iter()
        .find(|message| message.id == earlier.id)
        .expect("the earlier answer is in the transcript");
    let Some(
        [ReplyPieceDto::Text { text }, ReplyPieceDto::Reference { reference }, ReplyPieceDto::Text { text: end }],
    ) = handed_back.reply.as_deref()
    else {
        panic!("the earlier answer's pieces: {:?}", handed_back.reply);
    };
    assert_eq!(text, "Cold air sinks ");
    assert_eq!(reference.kind, "source");
    assert_eq!(end, ".");
}

/// The pictures the answer shows are attached to its message and written to
/// `metadata.images`, in the database and in the transcript the turn returns;
/// the ones it stored and did not use are released with the request.
#[tokio::test]
async fn the_pictures_an_answer_shows_are_attached_and_recorded() {
    use crate::features::conversation::chat::focus::FocusScope;
    use crate::features::conversation::chat::test_runtime::{
        test_picture, FakeChatImages, FakeRuntime,
    };
    use crate::features::conversation::chat::turn_record::TurnRecorder;
    use lattice_core::application::contracts::images::FindImageInput;
    use lattice_core::application::ports::{ChatImagesPort, ImageTurnScope};

    let (service, pool, id, user_id) = service_with_pending_question().await;
    let images = FakeChatImages::with(vec![test_picture("img-1"), test_picture("img-2")]);
    let runtime = FakeRuntime {
        images: images.clone(),
        ..FakeRuntime::default()
    };
    let scope = ImageTurnScope {
        conversation_id: id.clone(),
        request_id: "request-pictures".into(),
    };
    let input = FindImageInput {
        query: "Saturn V launch".into(),
        from_page_url: None,
        limit: 3,
    };
    images.find_and_store(&scope, input, None).await.unwrap();

    let llm: Arc<dyn lattice_core::application::ports::LLMPort> =
        Arc::new(crate::features::llm::engine::factory::MockLLMPort::new());
    let focus = FocusScope::default();
    let recorder = TurnRecorder::silent(&id, "request-pictures");
    let saved = finalize_successful_turn(
        &runtime,
        ChatTurn {
            conv_service: &service,
            conversation_id: &id,
            request_id: "request-pictures",
            message: "question",
            standalone_question: None,
            llm: &llm,
            highlight_terms: &[],
            focus: &focus,
            recorder: &recorder,
            names: &std::sync::Arc::new(
                lattice_core::application::services::model_contract::kept::TurnNames::unstored(),
            ),
        },
        &user_id,
        2,
        0,
        FinishedAnswer {
            reply: ReplyText {
                text: "It launched in 1969.\n\n![Saturn V lifting off](img-1)".into(),
                ..Default::default()
            },
            sources: Vec::new(),
            verification: None,
            memory_usage: None,
            retrieval_trace: None,
            turn_record: None,
            summary_refold: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(
        images.calls().last().map(String::as_str),
        Some(format!("attach request-pictures {} img-1", saved.message_id).as_str())
    );
    let raw: String = sqlx::query_scalar("SELECT metadata FROM conversation_messages WHERE id=?")
        .bind(&saved.message_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let stored = metadata["images"].as_array().expect("metadata.images");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0]["ref"], "img-1");
    assert_eq!(stored[0]["fileName"], test_picture("img-1").file_name);

    let answer = saved
        .response
        .messages
        .iter()
        .find(|message| message.id == saved.message_id)
        .expect("the answer is in the transcript");
    let returned: serde_json::Value =
        serde_json::from_str(answer.metadata.as_deref().unwrap()).unwrap();
    assert_eq!(returned["images"][0]["ref"], "img-1");
    // The content is kept as the model wrote it.
    assert!(answer.content.ends_with("![Saturn V lifting off](img-1)"));
}

/// A question's staged pictures are claimed by the saved message and written
/// to `metadata.images`, with whether the model was shown them.
#[tokio::test]
async fn a_question_claims_its_staged_pictures() {
    use crate::features::conversation::chat::test_runtime::{
        test_attachment, FakeChatImages, FakeRuntime,
    };

    let (service, pool, id, _) = service_with_pending_question().await;
    let images = FakeChatImages::staged(vec![test_attachment(1, 800, 600)]);
    let runtime = FakeRuntime {
        images: images.clone(),
        ..FakeRuntime::default()
    };
    let llm: Arc<dyn lattice_core::application::ports::LLMPort> =
        Arc::new(crate::features::llm::engine::factory::MockLLMPort::new());
    let picture = test_attachment(1, 800, 600);
    let (user_id, _) = persist_user_message_pending(
        &runtime,
        &service,
        &id,
        "request-seen",
        "what is this?",
        QuestionAttachments {
            names: &[],
            document_ids: &[],
            image_ids: std::slice::from_ref(&picture.id),
            pictures_seen: false,
        },
        &llm,
    )
    .await
    .unwrap();

    assert_eq!(
        images.calls(),
        vec![format!("claim {} {user_id} request-seen", picture.id)]
    );
    let raw: String = sqlx::query_scalar("SELECT metadata FROM conversation_messages WHERE id=?")
        .bind(&user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(metadata["images"][0]["id"], picture.id.as_str());
    assert_eq!(metadata["images"][0]["originalName"], "shot-1.png");
    assert_eq!(metadata["picturesSeen"], false);
}
