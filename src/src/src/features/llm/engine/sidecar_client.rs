//! `SidecarLLMClient` — HTTP client for the bundled `llama-server` sidecar.
//!
//! Implements the `LLMClient` trait by talking to a local llama-server
//! process over loopback HTTP, using the OpenAI-compatible
//! `/v1/chat/completions` endpoint. Owns the `SidecarHandle` so the
//! sidecar process dies with the client.
//!
//! # Architecture
//!
//! ```text
//! LLMClient trait               SidecarLLMClient                llama-server
//! ───────────────────────────► ───────────────────────────► ────────────
//!   generate(...)               POST /v1/chat/completions      Metal/Vulkan/CPU
//!   generate_stream(...)        POST /v1/chat/completions      ggml backend
//!                               (stream=true, SSE)
//!   health_check()              GET  /health
//! ```
//!
//! The SSE wire format is OpenAI-standard: `data: {json}\n\n` lines,
//! terminated by `data: [DONE]`. Each chunk's
//! `choices[0].delta.content` is the next token piece.
//!
//! # Why we don't use llama.cpp's native `/completion`
//!
//! The native endpoint is older, has a different streaming format
//! (newline-delimited JSON, no `data:` prefix, no `[DONE]` sentinel),
//! and changes more often than the OpenAI-compat layer. Standardizing
//! on `/v1/chat/completions` lets us swap llama-server for any
//! OpenAI-compatible backend later (vLLM, mistralrs-server, etc.)
//! without changing this file.

use crate::features::llm::engine::sidecar_manager::SidecarHandle;
use crate::features::llm::engine::traits::{ChatMessage, GenerationConfig, LLMClient};
use crate::features::llm::engine::types::LLMError;
use async_stream::stream;
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tokio_stream::Stream;

/// How long to wait for the first byte of a streamed response. The
/// server has to load the model into VRAM, run the prompt prefill, and
/// emit the first token. On a 7B Q4_K_M GGUF this is sub-second on
/// Apple Silicon but can be 10s+ on cold Windows Vulkan.
const FIRST_TOKEN_TIMEOUT: Duration = Duration::from_secs(60);

/// Per-chunk timeout once a stream is active. If the model goes silent
/// for this long mid-generation, the connection is dead and we should
/// surface an error rather than hang.
const CHUNK_TIMEOUT: Duration = Duration::from_secs(30);

/// Total timeout for non-streaming `generate()` calls. A 1024-token
/// completion at ~30 tok/s is ~30s; doubled for safety on slow hardware.
const NON_STREAM_TIMEOUT: Duration = Duration::from_secs(120);

/// Health check timeout — should be milliseconds; if it isn't, the
/// sidecar is in trouble and we should report unhealthy fast.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);

/// OpenAI-shaped chat completion request. We use `serde_json::Value`
/// for fields llama-server's OpenAI compat layer accepts but our trait
/// doesn't expose, to keep the surface minimal.
#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    /// Model identifier. `SidecarManager` spawns single-model servers
    /// (one `-m <path>` flag at startup), so llama-server serves
    /// whatever weights it loaded regardless of what the request
    /// specifies. We send a meaningful value (the GGUF basename) so
    /// server logs stay readable; switching active models is done by
    /// killing and respawning the sidecar, matching LM Studio's
    /// per-runtime model lifecycle.
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    stream: bool,
    temperature: f32,
    /// Nucleus sampling.
    top_p: f32,
    /// Top-k sampling. llama-server-specific extension to OpenAI shape.
    top_k: i32,
    /// Repetition penalty. llama-server-specific extension.
    repeat_penalty: f32,
    max_tokens: usize,
}

#[derive(Debug, Serialize)]
struct WireMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    choices: Vec<ChatChunkChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChunkChoice {
    delta: ChatDelta,
}

#[derive(Debug, Deserialize, Default)]
struct ChatDelta {
    /// Absent on the first/last chunks; present with the next token
    /// piece on content chunks. Default to "" if missing.
    #[serde(default)]
    content: String,
}

/// LLM client that talks to a bundled llama-server sidecar.
///
/// Owns the `SidecarHandle`; dropping the client kills the process.
pub struct SidecarLLMClient {
    /// Process handle. Wrapped in `Arc` because `SidecarHandle`'s drop
    /// kills the process; if multiple clones existed, dropping one
    /// would orphan the others. We hold exactly one Arc so the process
    /// outlives all clones until the last drop.
    sidecar: Arc<SidecarHandle>,

    /// HTTP client. Reused across calls for connection pooling.
    http: Client,

    /// Display name surfaced via `LLMClient::model_name()`. Typically
    /// the GGUF filename without extension.
    model_name: String,

    /// Mutable generation config for `generation_config_mut()`.
    config: GenerationConfig,
}

/// Incremental parser for the server's SSE frames.
///
/// Extracted from `parse_sse_stream` so the byte handling is testable
/// without constructing a `reqwest::Response`. Two things it must get
/// right, both of which the previous inline version got wrong:
///
/// 1. **UTF-8 across chunk boundaries.** Bytes arrive in network-sized
///    chunks with no regard for character boundaries. Decoding each chunk
///    independently with `from_utf8_lossy` turned every split character
///    into U+FFFD, so non-English replies came back peppered with
///    replacement characters at random offsets.
///
/// 2. **The final frame.** When the server closes without sending
///    `data: [DONE]`, a frame still sitting in the buffer without its
///    trailing `\n\n` was silently dropped — losing the last tokens of
///    the reply.
#[derive(Default)]
pub(crate) struct SseDecoder {
    /// Text decoded so far, awaiting frame termination.
    buffer: String,
    /// Bytes that do not yet form complete characters (a split sequence).
    pending: Vec<u8>,
    /// Whether the server's `[DONE]` sentinel has been seen.
    done: bool,
}

impl SseDecoder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// True once `data: [DONE]` has been observed.
    pub(crate) fn is_done(&self) -> bool {
        self.done
    }

    /// Feed a network chunk; returns any content deltas it completed.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        if self.done {
            return Vec::new();
        }

        self.pending.extend_from_slice(bytes);
        self.decode_pending();
        self.drain_frames()
    }

    /// Flush at clean EOF, emitting any residual frame.
    pub(crate) fn finish(&mut self) -> Vec<String> {
        if self.done {
            return Vec::new();
        }

        if !self.pending.is_empty() {
            // Whatever is left cannot be completed; surface it lossily
            // rather than discarding it.
            let tail = std::mem::take(&mut self.pending);
            self.buffer.push_str(&String::from_utf8_lossy(&tail));
        }

        if !self.buffer.trim().is_empty() && !self.buffer.ends_with("\n\n") {
            self.buffer.push_str("\n\n");
        }

        self.drain_frames()
    }

    /// Move as much of `pending` into `buffer` as forms whole characters.
    fn decode_pending(&mut self) {
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    self.buffer.push_str(text);
                    self.pending.clear();
                    return;
                }
                Err(err) => {
                    let valid = err.valid_up_to();
                    if valid > 0 {
                        if let Some(valid_bytes) = self.pending.get(..valid) {
                            if let Ok(text) = std::str::from_utf8(valid_bytes) {
                                self.buffer.push_str(text);
                            }
                        }
                        self.pending.drain(..valid);
                    }
                    match err.error_len() {
                        // Genuinely invalid bytes: drop them and mark the
                        // damage rather than stalling the stream forever.
                        Some(len) => {
                            let len = len.min(self.pending.len()).max(1);
                            self.pending.drain(..len.min(self.pending.len()));
                            self.buffer.push('\u{FFFD}');
                        }
                        // Truncated tail: wait for the next chunk.
                        None => return,
                    }
                }
            }
        }
    }

    /// Consume every complete `\n\n`-terminated frame in the buffer.
    fn drain_frames(&mut self) -> Vec<String> {
        let mut out = Vec::new();

        while let Some(idx) = self.buffer.find("\n\n") {
            let frame = self.buffer[..idx].to_string();
            self.buffer.drain(..idx + 2);

            for line in frame.lines() {
                let Some(payload) = line.strip_prefix("data:") else {
                    continue;
                };
                let payload = payload.trim();

                if payload == "[DONE]" {
                    self.done = true;
                    return out;
                }

                // Skip empty / malformed payloads — better to drop one bad
                // frame than abort the whole stream.
                match serde_json::from_str::<ChatCompletionChunk>(payload) {
                    Ok(chunk) => {
                        if let Some(choice) = chunk.choices.into_iter().next() {
                            let content = choice.delta.content;
                            if !content.is_empty() {
                                out.push(content);
                            }
                        }
                    }
                    Err(err) => {
                        tracing::warn!(
                            target: "sidecar_client",
                            "Skipping malformed SSE chunk ({err}): {payload}"
                        );
                    }
                }
            }
        }

        out
    }
}

impl SidecarLLMClient {
    /// Create a client wrapping the given sidecar handle.
    pub fn new(
        sidecar: Arc<SidecarHandle>,
        model_name: impl Into<String>,
        config: GenerationConfig,
    ) -> Result<Self, LLMError> {
        let http = Client::builder()
            // Streaming responses can be long. Don't set a global timeout;
            // we apply per-call timeouts via `tokio::time::timeout` instead.
            .pool_idle_timeout(Some(Duration::from_secs(90)))
            .build()
            .map_err(|err| {
                LLMError::Other(format!("Failed to build HTTP client for sidecar: {err}"))
            })?;

        Ok(Self {
            sidecar,
            http,
            model_name: model_name.into(),
            config,
        })
    }

    /// Context window this client's sidecar was launched with.
    pub fn context_size(&self) -> u32 {
        self.sidecar.context_size()
    }

    fn endpoint_for(&self, path: &str) -> String {
        format!("{}{}", self.sidecar.endpoint(), path)
    }

    fn build_messages(system: Option<&str>, prompt: &str) -> Vec<(&'static str, String)> {
        let mut msgs: Vec<(&'static str, String)> = Vec::with_capacity(2);
        if let Some(sys) = system {
            msgs.push(("system", sys.to_string()));
        }
        msgs.push(("user", prompt.to_string()));
        msgs
    }

    fn build_request<'a>(
        &'a self,
        messages: &'a [(&'a str, String)],
        stream: bool,
    ) -> ChatCompletionRequest<'a> {
        ChatCompletionRequest {
            model: "local",
            messages: messages
                .iter()
                .map(|(role, content)| WireMessage {
                    role,
                    content: content.as_str(),
                })
                .collect(),
            stream,
            temperature: self.config.temperature,
            top_p: self.config.top_p,
            top_k: self.config.top_k,
            repeat_penalty: self.config.repeat_penalty,
            max_tokens: self.config.max_tokens,
        }
    }

    async fn post_chat_completion(
        &self,
        messages: &[(&str, String)],
        stream: bool,
    ) -> Result<reqwest::Response, LLMError> {
        let body = self.build_request(messages, stream);
        let request_timeout = if stream {
            FIRST_TOKEN_TIMEOUT
        } else {
            NON_STREAM_TIMEOUT
        };

        let response = timeout(
            request_timeout,
            self.http
                .post(self.endpoint_for("/v1/chat/completions"))
                .json(&body)
                .send(),
        )
        .await
        .map_err(|_| LLMError::Timeout)?
        .map_err(|err| LLMError::Network(format!("HTTP error to sidecar: {err}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(LLMError::GenerationFailed(format!(
                "llama-server returned {status}: {body}"
            )));
        }

        Ok(response)
    }

    async fn extract_completion_text(response: reqwest::Response) -> Result<String, LLMError> {
        let parsed: ChatCompletionResponse = response.json().await.map_err(|err| {
            LLMError::GenerationFailed(format!("Failed to parse sidecar response: {err}"))
        })?;

        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| LLMError::GenerationFailed("Sidecar returned zero choices".to_string()))
    }

    fn normalize_chat_messages(messages: Vec<ChatMessage>) -> Vec<(&'static str, String)> {
        messages
            .into_iter()
            .map(|m| {
                let role: &'static str = match m.role.as_str() {
                    "system" => "system",
                    "assistant" => "assistant",
                    _ => "user",
                };
                (role, m.content)
            })
            .collect()
    }

    /// Drive the SSE stream into a token-string stream. Centralized so
    /// `generate_stream` and `generate_chat_stream` share the parser.
    fn parse_sse_stream(
        response: reqwest::Response,
    ) -> Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send>> {
        Box::pin(stream! {
            let mut bytes = response.bytes_stream();
            let mut decoder = SseDecoder::new();

            loop {
                let next = match timeout(CHUNK_TIMEOUT, bytes.next()).await {
                    Ok(next) => next,
                    Err(_) => {
                        yield Err(LLMError::Timeout);
                        return;
                    }
                };

                match next {
                    Some(Ok(b)) => {
                        for content in decoder.push(&b) {
                            yield Ok(content);
                        }
                        if decoder.is_done() {
                            return;
                        }
                    }
                    Some(Err(err)) => {
                        yield Err(LLMError::Network(format!("SSE byte stream error: {err}")));
                        return;
                    }
                    None => {
                        // Server closed: flush any frame left without its
                        // trailing "\n\n" instead of discarding it.
                        for content in decoder.finish() {
                            yield Ok(content);
                        }
                        return;
                    }
                }
            }
        })
    }
}

#[async_trait]
impl LLMClient for SidecarLLMClient {
    async fn generate(
        &self,
        prompt: &str,
        system: Option<&str>,
        _images: Option<Vec<String>>,
    ) -> Result<String, LLMError> {
        let messages = Self::build_messages(system, prompt);
        let response = self.post_chat_completion(&messages, false).await?;
        Self::extract_completion_text(response).await
    }

    async fn generate_stream(
        &self,
        prompt: &str,
        system: Option<&str>,
        _images: Option<Vec<String>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        let messages = Self::build_messages(system, prompt);
        let response = self.post_chat_completion(&messages, true).await?;
        Ok(Self::parse_sse_stream(response))
    }

    async fn health_check(&self) -> bool {
        let url = self.endpoint_for("/health");
        match timeout(HEALTH_TIMEOUT, self.http.get(url).send()).await {
            Ok(Ok(resp)) => resp.status().is_success(),
            _ => false,
        }
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn generation_config(&self) -> &GenerationConfig {
        &self.config
    }

    fn generation_config_mut(&mut self) -> &mut GenerationConfig {
        &mut self.config
    }

    async fn generate_chat(&self, messages: Vec<ChatMessage>) -> Result<String, LLMError> {
        let pairs = Self::normalize_chat_messages(messages);
        let response = self.post_chat_completion(&pairs, false).await?;
        Self::extract_completion_text(response).await
    }

    async fn generate_chat_stream(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        let pairs = Self::normalize_chat_messages(messages);
        let response = self.post_chat_completion(&pairs, true).await?;
        Ok(Self::parse_sse_stream(response))
    }

    fn supports_chat(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    //! These tests target the request-shaping and SSE-parsing logic.
    //!
    //! The SSE byte handling is exercised through `SseDecoder`, which exists
    //! precisely so this logic can be tested without a live HTTP response.
    //! Lifecycle integration tests (real sidecar process) live in
    //! `tests/sidecar_integration.rs` — they require a llama-server
    //! binary on disk and a tiny GGUF model.
    //!
    //! Note: full end-to-end tests on Windows are blocked by the
    //! esaxx-rs/cxx CRT-mismatch linker error; these tests pass on
    //! macOS and Linux where linking works.

    fn content_frame(text: &str) -> String {
        format!(
            "data: {}\n\n",
            serde_json::json!({ "choices": [ { "delta": { "content": text } } ] })
        )
    }

    /// A multi-byte character split across two network chunks must survive.
    /// Decoding each chunk with `from_utf8_lossy` independently turned the
    /// split character into U+FFFD.
    #[test]
    fn sse_decoder_reassembles_characters_split_across_chunks() {
        let frame = content_frame("你好世界");
        let bytes = frame.as_bytes();

        // Split at every possible offset; none may corrupt the output.
        for split in 1..bytes.len() {
            let mut decoder = SseDecoder::new();
            let mut out = String::new();
            for chunk in decoder.push(&bytes[..split]) {
                out.push_str(&chunk);
            }
            for chunk in decoder.push(&bytes[split..]) {
                out.push_str(&chunk);
            }
            for chunk in decoder.finish() {
                out.push_str(&chunk);
            }

            assert_eq!(
                out, "你好世界",
                "split at byte {} corrupted the stream",
                split
            );
            assert!(
                !out.contains('\u{FFFD}'),
                "split at byte {} produced a replacement character",
                split
            );
        }
    }

    /// Emoji are 4-byte sequences — the widest split window.
    #[test]
    fn sse_decoder_handles_emoji_split_across_chunks() {
        let frame = content_frame("ship it 🚀🔥");
        let bytes = frame.as_bytes();

        for split in 1..bytes.len() {
            let mut decoder = SseDecoder::new();
            let mut out = String::new();
            for chunk in decoder.push(&bytes[..split]) {
                out.push_str(&chunk);
            }
            for chunk in decoder.push(&bytes[split..]) {
                out.push_str(&chunk);
            }
            for chunk in decoder.finish() {
                out.push_str(&chunk);
            }
            assert_eq!(
                out, "ship it 🚀🔥",
                "split at byte {} corrupted output",
                split
            );
        }
    }

    /// A frame left in the buffer when the server closes without `[DONE]`
    /// used to be discarded, losing the last tokens of the reply.
    #[test]
    fn sse_decoder_flushes_the_final_frame_without_done_sentinel() {
        let mut decoder = SseDecoder::new();
        let mut out = String::new();

        for chunk in decoder.push(content_frame("hello ").as_bytes()) {
            out.push_str(&chunk);
        }

        // Final frame arrives with no trailing blank line and no [DONE].
        let tail = format!(
            "data: {}",
            serde_json::json!({ "choices": [ { "delta": { "content": "world" } } ] })
        );
        for chunk in decoder.push(tail.as_bytes()) {
            out.push_str(&chunk);
        }

        assert_eq!(out, "hello ", "unterminated frame must not emit early");

        for chunk in decoder.finish() {
            out.push_str(&chunk);
        }
        assert_eq!(out, "hello world", "final frame must be flushed on EOF");
    }

    #[test]
    fn sse_decoder_stops_at_done_sentinel() {
        let mut decoder = SseDecoder::new();
        let mut out = String::new();

        let stream = format!(
            "{}data: [DONE]\n\n{}",
            content_frame("kept"),
            content_frame("ignored")
        );
        for chunk in decoder.push(stream.as_bytes()) {
            out.push_str(&chunk);
        }

        assert_eq!(out, "kept");
        assert!(decoder.is_done());
        assert!(decoder.finish().is_empty(), "nothing may follow [DONE]");
    }

    #[test]
    fn sse_decoder_skips_malformed_frames_without_aborting() {
        let mut decoder = SseDecoder::new();
        let mut out = String::new();

        let stream = format!(
            "{}data: {{not json}}\n\n{}",
            content_frame("before "),
            content_frame("after")
        );
        for chunk in decoder.push(stream.as_bytes()) {
            out.push_str(&chunk);
        }

        assert_eq!(
            out, "before after",
            "one bad frame must not kill the stream"
        );
    }

    use super::*;

    #[test]
    fn build_request_serializes_to_openai_shape() {
        let cfg = GenerationConfig::default();
        let messages = [
            ("system", "You are helpful.".to_string()),
            ("user", "Hi.".to_string()),
        ];

        let body = ChatCompletionRequest {
            model: "local",
            messages: messages
                .iter()
                .map(|(role, content)| WireMessage { role, content })
                .collect(),
            stream: false,
            temperature: cfg.temperature,
            top_p: cfg.top_p,
            top_k: cfg.top_k,
            repeat_penalty: cfg.repeat_penalty,
            max_tokens: cfg.max_tokens,
        };

        let json = serde_json::to_value(&body).expect("serialize");
        assert_eq!(json["model"], "local");
        assert_eq!(json["stream"], false);
        let messages_json = json["messages"].as_array().expect("messages array");
        assert_eq!(messages_json.len(), 2);
        assert_eq!(messages_json[0]["role"], "system");
        assert_eq!(messages_json[0]["content"], "You are helpful.");
        assert_eq!(messages_json[1]["role"], "user");
        assert_eq!(messages_json[1]["content"], "Hi.");
    }

    #[test]
    fn deserialize_chunk_handles_empty_delta() {
        // First and last SSE chunks often have empty/missing content.
        let payload = r#"{"choices":[{"delta":{}}]}"#;
        let chunk: ChatCompletionChunk = serde_json::from_str(payload).expect("parse");
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content, "");
    }

    #[test]
    fn deserialize_chunk_extracts_content() {
        let payload = r#"{"choices":[{"delta":{"content":"hello"}}]}"#;
        let chunk: ChatCompletionChunk = serde_json::from_str(payload).expect("parse");
        assert_eq!(chunk.choices[0].delta.content, "hello");
    }

    #[test]
    fn deserialize_response_extracts_message() {
        let payload = r#"{
            "choices":[{
                "message":{"role":"assistant","content":"hi there"}
            }]
        }"#;
        let resp: ChatCompletionResponse = serde_json::from_str(payload).expect("parse");
        assert_eq!(resp.choices[0].message.content, "hi there");
    }
}
