use std::sync::{Arc, Mutex};
use std::time::Duration;

use lattice_core::application::contracts::images::NO_PICTURE_NOTE;
use lattice_core::application::contracts::settings::ToolOutputSettingsDto;
use lattice_core::application::contracts::tools::{FunctionCall, FunctionResult};
use lattice_core::application::ports::llm_port::{CompletionResponse, ContentPart};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::features::conversation::chat::focus::FocusScope;
use crate::features::conversation::chat::ports::ChatStorage;
use crate::features::conversation::chat::test_runtime::{
    test_picture, FakeChatImages, FakeRuntime,
};
use crate::features::conversation::chat::tool_loop::clock::TurnClock;
use crate::features::conversation::chat::tool_loop::images;
use crate::features::conversation::chat::turn_record::TurnRecorder;
use crate::features::conversation::chat::{ChatEventSink, ChatStreamEventDto};
use crate::features::conversation::ConversationServiceTrait;

const CONVERSATION: &str = "pick-conversation";
const GRID_28: VisionGeometry = VisionGeometry::CONSERVATIVE_DEFAULT;

/// A model that answers every request with `reply`, or fails when there is
/// none, and keeps what it was sent.
struct Judge {
    reply: Option<String>,
    vision: Option<VisionGeometry>,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl Judge {
    fn new(reply: Option<&str>, vision: Option<VisionGeometry>) -> Arc<Self> {
        Arc::new(Self {
            reply: reply.map(str::to_string),
            vision,
            requests: Mutex::default(),
        })
    }

    fn requests(&self) -> Vec<CompletionRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl LLMPort for Judge {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.requests.lock().unwrap().push(request.clone());
        match &self.reply {
            Some(reply) => Ok(CompletionResponse::from_text(reply.clone())),
            None => Err(AppError::Other("the server refused the request".into())),
        }
    }
    fn model_name(&self) -> &str {
        "judge"
    }
    fn max_context_tokens(&self) -> usize {
        32_768
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
    fn vision(&self) -> Option<VisionGeometry> {
        self.vision
    }
}

struct Called {
    result: FunctionResult,
    streamed: Vec<ChatImageDto>,
    images: Arc<FakeChatImages>,
}

impl Called {
    fn refs(&self) -> Vec<String> {
        self.result.data.as_ref().unwrap()["images"]
            .as_array()
            .unwrap()
            .iter()
            .map(|image| image["ref"].as_str().unwrap().to_string())
            .collect()
    }

    fn note(&self) -> Option<String> {
        self.result.data.as_ref().unwrap()["note"]
            .as_str()
            .map(str::to_string)
    }
}

/// One `find_image` call that stores `img-1` and `img-2`, answered by `llm`.
async fn find_herons(llm: Arc<dyn LLMPort>) -> Called {
    let pictures = vec![test_picture("img-1"), test_picture("img-2")];
    let images = FakeChatImages::with(pictures.clone());
    *images.staged.lock().unwrap() = pictures;
    let runtime = FakeRuntime {
        images: images.clone(),
        ..FakeRuntime::default()
    };
    let conv_service: Arc<dyn ConversationServiceTrait> = runtime.conversations.clone();
    let events: Arc<Mutex<Vec<ChatStreamEventDto>>> = Arc::default();
    let captured = Arc::clone(&events);
    let sink: ChatEventSink = Arc::new(move |event| {
        captured.lock().unwrap().push(event);
        Ok(())
    });
    let settings = ToolOutputSettingsDto::default();
    let focus = FocusScope::default();
    let recorder = TurnRecorder::silent(CONVERSATION, "pick-request");
    let cancel = CancellationToken::new();
    let clock = TurnClock::new(Duration::from_secs(600), Duration::ZERO);
    let scope = CallScope {
        container: &runtime,
        conv_service: &conv_service,
        conv_id: CONVERSATION,
        request_id: "pick-request",
        sink: &sink,
        tools: None,
        highlight_terms: &[],
        tool_output_settings: &settings,
        focus: &focus,
        source: None,
        recorder: &recorder,
        history: runtime.chat_records(),
        cancel: &cancel,
        clock: &clock,
        window_chars: 100_000,
        tool_schema_chars: 0,
        llm: &llm,
        question: "What does a grey heron look like?",
    };
    let result = images::execute(
        &scope,
        FunctionCall::new("call-1", "find_image", json!({ "query": "grey heron" })),
    )
    .await
    .unwrap();
    let streamed = events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| event.images.clone())
        .flatten()
        .collect();
    Called {
        result,
        streamed,
        images,
    }
}

#[tokio::test]
async fn the_pick_keeps_only_the_candidates_the_model_chose() {
    let judge = Judge::new(Some(r#"{"keep":["img-2"]}"#), Some(GRID_28));
    let called = find_herons(judge.clone()).await;

    assert_eq!(called.refs(), ["img-2"]);
    assert_eq!(called.streamed, [test_picture("img-2")]);
    assert_eq!(
        called.note().as_deref(),
        Some("2 candidates were shown to you and 1 kept.")
    );

    let requests = judge.requests();
    assert_eq!(requests.len(), 1, "one side request for the pick");
    let pick = &requests[0];
    assert!(matches!(
        &pick.input[0],
        CompletionInput::Message { role, content } if role == "system" && content == PICK_SYSTEM
    ));
    let CompletionInput::Multimodal { parts, .. } = &pick.input[1] else {
        panic!(
            "the candidates ride in the user message: {:?}",
            pick.input[1]
        );
    };
    assert!(matches!(&parts[0], ContentPart::Image(image) if image.alt == "img-1"));
    assert!(matches!(&parts[1], ContentPart::Image(image) if image.alt == "img-2"));
    assert!(matches!(
        &parts[2],
        ContentPart::Text { text } if text.contains("Subject: grey heron")
            && text.contains("What does a grey heron look like?")
    ));
    assert_eq!(
        pick.cache_key.as_deref(),
        Some("pick-conversation:image-pick")
    );
    assert!(pick.tools.is_empty());
    assert_eq!(
        pick.json_schema.as_ref().unwrap()["properties"]["keep"]["items"]["enum"],
        json!(["img-1", "img-2"])
    );

    // Two candidates share one picture's worth of the geometry.
    let per = GRID_28.min_tokens().max(GRID_28.max_tokens() / 2);
    let prepared: Vec<String> = called
        .images
        .calls()
        .into_iter()
        .filter(|call| call.starts_with("prepared"))
        .collect();
    assert_eq!(
        prepared,
        [
            format!("prepared {} {per}", test_picture("img-1").id),
            format!("prepared {} {per}", test_picture("img-2").id),
        ]
    );
}

#[tokio::test]
async fn keeping_none_answers_with_the_no_picture_note() {
    let judge = Judge::new(Some(r#"{"keep":[]}"#), Some(GRID_28));
    let called = find_herons(judge).await;

    assert!(called.refs().is_empty());
    assert!(called.streamed.is_empty(), "nothing is sent to the window");
    assert_eq!(called.note().as_deref(), Some(NO_PICTURE_NOTE));
}

#[tokio::test]
async fn a_pick_that_fails_keeps_every_candidate() {
    for reply in [None, Some("I would keep the second one.")] {
        let judge = Judge::new(reply, Some(GRID_28));
        let called = find_herons(judge.clone()).await;

        assert_eq!(judge.requests().len(), 1);
        assert_eq!(called.refs(), ["img-1", "img-2"]);
        assert_eq!(called.streamed.len(), 2);
        assert_eq!(called.note(), None, "the store's own note stands");
    }
}

#[tokio::test]
async fn a_model_that_cannot_see_is_not_asked() {
    let judge = Judge::new(Some(r#"{"keep":[]}"#), None);
    let called = find_herons(judge.clone()).await;

    assert!(judge.requests().is_empty());
    assert_eq!(called.refs(), ["img-1", "img-2"]);
    assert!(!called
        .images
        .calls()
        .iter()
        .any(|call| call.starts_with("prepared")));
}

#[test]
fn a_verdict_is_read_from_the_first_json_object_in_the_reply() {
    let keep = parse_keep("```json\n{\"keep\": [\"img-1\", \"img-3\"]}\n```").unwrap();
    assert_eq!(
        keep,
        HashSet::from(["img-1".to_string(), "img-3".to_string()])
    );
    assert_eq!(parse_keep("{\"drop\": []}"), None);
    assert_eq!(parse_keep("no verdict"), None);
}
