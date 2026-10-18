//! The exact request body a picture becomes on each provider's wire, recorded
//! by a stub server on loopback: llama.cpp (and any OpenAI-compatible
//! server), Ollama, OpenAI Responses, Anthropic Messages and Gemini's
//! OpenAI-compatible chat. A model nobody declared able to see sends nothing,
//! and its text fallback carries the caption line instead of the picture.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use lattice::features::llm::cloud::CloudLlm;
use lattice::features::llm::engine::ollama_client::OllamaClient;
use lattice::features::llm::llama_cpp::LlamaCppLlm;
use lattice_core::application::contracts::settings::{
    LLMProvider, LLMSettingsDto, LlamaCppSettingsDto,
};
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, ImagePart, ToolDefinition,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::completion_input::{
    attach_images_to_last_user, strip_images,
};
use lattice_core::domain::models::vision::{VisionGeometry, VisionPreset};
use serde_json::{json, Value};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

fn picture(geometry: VisionGeometry) -> ImagePart {
    let fitted = geometry.plan(1280, 960);
    ImagePart {
        mime: "image/png".into(),
        data_base64: PNG_BASE64.into(),
        width: fitted.width,
        height: fitted.height,
        tokens: fitted.tokens,
        alt: "chart.png".into(),
    }
}

/// A system prompt, then the user's question with one picture attached the
/// way the chat attaches it, and one tool on offer.
fn request(geometry: VisionGeometry) -> CompletionRequest {
    let mut input = vec![
        CompletionInput::Message {
            role: "system".into(),
            content: "Answer from the picture.".into(),
        },
        CompletionInput::Message {
            role: "user".into(),
            content: "What does the chart show?".into(),
        },
    ];
    assert!(attach_images_to_last_user(
        &mut input,
        vec![picture(geometry)]
    ));
    CompletionRequest {
        input,
        tools: vec![ToolDefinition {
            name: "search".into(),
            description: "Search the library".into(),
            parameters: json!({"type":"object","properties":{"q":{"type":"string"}}}),
        }],
        time_budget: Some(Duration::from_secs(30)),
        ..Default::default()
    }
}

/// The first body the stub server received. The reply is not what these
/// tests check, so an adapter that retries a reply it cannot read sends the
/// same body again.
async fn received_body(server: &MockServer) -> Value {
    let requests = server.received_requests().await.unwrap_or_default();
    let first = requests.first().expect("a request reached the server");
    serde_json::from_slice(&first.body).unwrap()
}

fn llama_cpp(server: &MockServer, vision: Option<VisionGeometry>) -> LlamaCppLlm {
    let settings = LLMSettingsDto {
        llama_cpp: LlamaCppSettingsDto {
            url: server.uri(),
            model: "qwen2.5-vl".into(),
            ..Default::default()
        },
        context_window: 32_768,
        max_tokens: 4_096,
        ..Default::default()
    };
    LlamaCppLlm::new(&settings).unwrap().with_vision(vision)
}

async fn chat_completions_stub() -> MockServer {
    let server = MockServer::start().await;
    let stream = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"A rising line.\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn llama_cpp_sends_the_picture_as_an_image_url_part_before_the_text() {
    let server = chat_completions_stub().await;
    let geometry = VisionPreset::Qwen25Vl.geometry();
    let request = request(geometry);
    let _ = llama_cpp(&server, Some(geometry)).complete(&request).await;
    let body = received_body(&server).await;
    let fitted = geometry.plan(1280, 960);
    assert_eq!(
        body["messages"],
        json!([
            {"role":"system","content":"Answer from the picture."},
            {"role":"user","content":[
                {"type":"image_url","image_url":{"url":format!("data:image/png;base64,{PNG_BASE64}")}},
                {"type":"text","text":"What does the chart show?"}
            ]}
        ])
    );
    assert_eq!(body["tools"][0]["function"]["name"], "search");
    // The picture is charged by its declared tokens, not its bytes: the answer
    // keeps the full configured ceiling the window still has room for.
    assert!(fitted.tokens > 0);
    assert_eq!(body["max_tokens"], 4_096);
}

#[tokio::test]
async fn a_model_nobody_declared_able_to_see_sends_nothing_and_its_fallback_carries_the_caption() {
    let server = chat_completions_stub().await;
    let request = request(VisionPreset::Qwen25Vl.geometry());
    let client = llama_cpp(&server, None);
    let refused = client.complete(&request).await.unwrap_err();
    assert!(
        refused.to_string().contains("is not set up to see images"),
        "{refused}"
    );
    assert!(server
        .received_requests()
        .await
        .unwrap_or_default()
        .is_empty());

    let fallback = CompletionRequest {
        input: strip_images(&request.input),
        ..request
    };
    let _ = client.complete(&fallback).await;
    let body = received_body(&server).await;
    let fitted = VisionPreset::Qwen25Vl.geometry().plan(1280, 960);
    assert_eq!(
        body["messages"][1],
        json!({"role":"user","content":format!(
            "What does the chart show?\n[Picture: chart.png ({}×{})]",
            fitted.width, fitted.height
        )})
    );
}

#[tokio::test]
async fn ollama_sends_the_picture_in_the_messages_images_list() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "{\"model\":\"llava:7b\",\"message\":{\"role\":\"assistant\",\"content\":\"A rising line.\"},\"done\":true}\n",
        ))
        .mount(&server)
        .await;
    let geometry = VisionPreset::GemmaSiglip.geometry();
    let client = OllamaClient::with_model_and_timeouts(
        server.uri(),
        "llava:7b",
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .unwrap()
    .with_vision(Some(geometry));
    let _ = client.complete(&request(geometry)).await;
    let body = received_body(&server).await;
    assert_eq!(
        body["messages"][1],
        json!({
            "role":"user",
            "content":"What does the chart show?",
            "images":[PNG_BASE64]
        })
    );
}

/// A cloud client for `provider` pointed at the stub server's `route`.
async fn cloud(provider: LLMProvider, route: &str, reply: Value) -> (MockServer, CloudLlm) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_json(reply))
        .mount(&server)
        .await;
    let settings = LLMSettingsDto {
        provider,
        model: "vision-model".into(),
        ..Default::default()
    };
    let client = CloudLlm::new(&settings, "test-key".into())
        .unwrap()
        .with_endpoint(format!("{}{route}", server.uri()))
        .with_vision(Some(VisionPreset::OpenAi.geometry()));
    (server, client)
}

#[tokio::test]
async fn openai_responses_take_an_input_image_before_the_input_text() {
    let (server, client) = cloud(
        LLMProvider::Openai,
        "/responses",
        json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"A rising line."}]}]}),
    )
    .await;
    let _ = client
        .complete(&request(VisionPreset::OpenAi.geometry()))
        .await;
    let body = received_body(&server).await;
    assert_eq!(
        body["input"][1],
        json!({"role":"user","content":[
            {"type":"input_image","image_url":format!("data:image/png;base64,{PNG_BASE64}"),"detail":"auto"},
            {"type":"input_text","text":"What does the chart show?"}
        ]})
    );
}

#[tokio::test]
async fn anthropic_takes_a_base64_image_block_before_the_text() {
    let (server, client) = cloud(
        LLMProvider::Anthropic,
        "/messages",
        json!({"content":[{"type":"text","text":"A rising line."}],"stop_reason":"end_turn"}),
    )
    .await;
    let _ = client
        .complete(&request(VisionPreset::Anthropic.geometry()))
        .await;
    let body = received_body(&server).await;
    assert_eq!(body["system"], "Answer from the picture.");
    assert_eq!(
        body["messages"][0],
        json!({"role":"user","content":[
            {"type":"image","source":{"type":"base64","media_type":"image/png","data":PNG_BASE64}},
            {"type":"text","text":"What does the chart show?"}
        ]})
    );
}

#[tokio::test]
async fn gemini_takes_an_image_url_part_as_llama_cpp_does() {
    let (server, client) = cloud(
        LLMProvider::Google,
        "/chat/completions",
        json!({"choices":[{"message":{"role":"assistant","content":"A rising line."},"finish_reason":"stop"}]}),
    )
    .await;
    let _ = client
        .complete(&request(VisionPreset::Gemini.geometry()))
        .await;
    let body = received_body(&server).await;
    assert_eq!(
        body["messages"][1],
        json!({"role":"user","content":[
            {"type":"image_url","image_url":{"url":format!("data:image/png;base64,{PNG_BASE64}")}},
            {"type":"text","text":"What does the chart show?"}
        ]})
    );
}
