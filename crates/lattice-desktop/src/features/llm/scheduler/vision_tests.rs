//! Pictures in the window check: charged by declared geometry in both the
//! estimate and the exact count, never as their base64, and named in a
//! refusal.

use super::*;
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse, ContentPart, ImagePart,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::models::vision::{VisionGeometry, VisionPreset};
use lattice_core::shared::error::{AppError, Result};
use parking_lot::Mutex;

/// A model with an 8,192-token window that records what reaches it.
#[derive(Default)]
struct Recorder {
    seen: Mutex<Vec<CompletionRequest>>,
}

#[async_trait]
impl LLMPort for Recorder {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.seen.lock().push(request.clone());
        Ok(CompletionResponse::from_text("ok"))
    }
    fn model_name(&self) -> &str {
        "recorder"
    }
    fn max_context_tokens(&self) -> usize {
        8_192
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
    fn vision(&self) -> Option<VisionGeometry> {
        Some(VisionPreset::Qwen25Vl.geometry())
    }
}

fn scheduled(model: Arc<Recorder>) -> ScheduledLlm {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::concurrent(1),
    ));
    ScheduledLlm::new(model, scheduler, 512)
}

/// `text` from the user with one picture of 400 KB of base64 costing `tokens`.
fn with_picture(text: &str, tokens: u32) -> CompletionRequest {
    CompletionRequest {
        input: vec![CompletionInput::Multimodal {
            role: "user".into(),
            parts: vec![
                ContentPart::Image(ImagePart {
                    mime: "image/jpeg".into(),
                    data_base64: "A".repeat(400_000),
                    width: 896,
                    height: 1120,
                    tokens,
                    alt: "photo.jpg".into(),
                }),
                ContentPart::Text { text: text.into() },
            ],
        }],
        max_output_tokens: Some(512),
        ..Default::default()
    }
}

#[tokio::test]
async fn a_pictures_bytes_are_never_counted_as_text() {
    let model = Arc::new(Recorder::default());
    let llm = scheduled(model.clone());
    llm.complete(&with_picture("What is this?", 1_282))
        .await
        .unwrap();
    assert_eq!(model.seen.lock().len(), 1);
}

#[tokio::test]
async fn the_estimate_charges_a_pictures_declared_tokens_and_the_refusal_says_so() {
    let model = Arc::new(Recorder::default());
    let llm = scheduled(model.clone());
    let refused = llm
        .complete(&with_picture("What is this?", 8_200))
        .await
        .unwrap_err();
    assert!(
        matches!(refused, AppError::PromptExceedsWindow { .. }),
        "{refused}"
    );
    let told = refused.to_string();
    assert!(
        told.contains(
            "text estimated; 8200 tokens for 1 picture estimated from the model's declared geometry"
        ),
        "{told}"
    );
    assert!(model.seen.lock().is_empty());
}

#[tokio::test]
async fn the_exact_count_adds_the_pictures_to_the_counted_text() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tokenize"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "tokens": vec![1; 3_000] })),
        )
        .mount(&server)
        .await;
    let model = Arc::new(Recorder::default());
    let llm = scheduled(model.clone())
        .with_tokenizer(ServerTokenizer::new(reqwest::Client::new(), &server.uri()));
    // 6,000 text tokens by the estimate; the server counts 3,000.
    let text = "x".repeat(24_000);
    llm.complete(&with_picture(&text, 4_000)).await.unwrap();
    let refused = llm.complete(&with_picture(&text, 5_200)).await.unwrap_err();
    let told = refused.to_string();
    assert!(told.contains("prompt is 8200 tokens"), "{told}");
    assert!(told.contains("text counted exactly; 5200 tokens"), "{told}");
    assert_eq!(model.seen.lock().len(), 1);
}

#[test]
fn the_scheduler_reports_its_models_vision() {
    let llm = scheduled(Arc::new(Recorder::default()));
    assert_eq!(llm.vision(), Some(VisionPreset::Qwen25Vl.geometry()));
}
