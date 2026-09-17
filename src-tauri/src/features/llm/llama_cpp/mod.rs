//! Remote llama.cpp server adapter. Uses Chat Completions, independently of Ollama.
use crate::application::{
    contracts::settings::{LLMSettingsDto, LlamaCppSettingsDto},
    ports::llm_port::{CompletionInput, CompletionRequest, CompletionResponse, LLMPort},
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{json, Value};
use std::time::Duration;

mod retry;
mod streaming;
#[cfg(test)]
mod tests;

pub struct LlamaCppLlm {
    client: reqwest::Client,
    base_url: String,
    settings: LLMSettingsDto,
    /// Longest silence tolerated once a response has started streaming.
    stall_timeout: Duration,
}

impl LlamaCppLlm {
    pub fn new(settings: &LLMSettingsDto) -> Result<Self> {
        let connection = &settings.llama_cpp;
        let base_url = validate_connection(connection)?;
        if connection.model.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "Choose a llama.cpp model in Chat settings".into(),
            ));
        }
        Ok(Self {
            client: http_client(connection)?,
            base_url,
            settings: settings.clone(),
            stall_timeout: Duration::from_secs(u64::from(settings.timeout_seconds.max(1))),
        })
    }

    pub async fn test_connection(connection: &LlamaCppSettingsDto) -> Result<Vec<String>> {
        let base_url = validate_connection(connection)?;
        let client = http_client(connection)?;
        let probe_timeout = Duration::from_secs(30);
        let response = client
            .get(format!("{base_url}/models"))
            .timeout(probe_timeout)
            .send()
            .await
            .map_err(network_error)?;
        let response = check_status(response).await?;
        let value: Value = response.json().await.map_err(network_error)?;
        let models: Vec<String> = value
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| m.get("id").and_then(Value::as_str))
            .filter(|m| !m.trim().is_empty())
            .map(str::to_owned)
            .collect();
        let model = if connection.model.trim().is_empty() {
            models.first()
        } else {
            models.iter().find(|model| *model == &connection.model)
        }
        .ok_or_else(|| {
            AppError::InvalidConfig("The llama.cpp server did not list the selected model".into())
        })?;
        // A model list alone does not prove that the generation route works.
        let response = client.post(format!("{base_url}/chat/completions"))
            .timeout(probe_timeout)
            .json(&json!({"model": model, "messages":[{"role":"user","content":"Reply with OK."}],
                "max_tokens": 32, "stream": false, "chat_template_kwargs":{"enable_thinking":false}}))
            .send().await.map_err(network_error)?;
        let response = check_status(response).await?;
        parse_completion(response.json().await.map_err(network_error)?)?;
        Ok(models)
    }

    fn body(&self, request: &CompletionRequest, stream: bool) -> Result<Value> {
        let mut messages = Vec::new();
        for item in &request.input {
            messages.push(match item {
                CompletionInput::Native { value } => value.clone(),
                CompletionInput::Message { role, content } => json!({"role":role,"content":content}),
                CompletionInput::ToolCall { id, name, arguments } => json!({"role":"assistant","content":null,
                    "tool_calls":[{"id":id,"type":"function","function":{"name":name,"arguments":arguments.to_string()}}]}),
                CompletionInput::ToolResult { id, output } => json!({"role":"tool","tool_call_id":id,"content":output}),
            });
        }
        let messages = coalesce_system_messages(messages)?;
        let mut body = serde_json::Map::from_iter([
            ("model".into(), json!(self.settings.llama_cpp.model)),
            ("messages".into(), json!(messages)),
            ("stream".into(), json!(stream)),
            ("temperature".into(), json!(self.settings.temperature)),
            ("top_p".into(), json!(self.settings.top_p)),
            ("top_k".into(), json!(self.settings.top_k)),
            ("repeat_penalty".into(), json!(self.settings.repeat_penalty)),
            ("max_tokens".into(), json!(self.settings.max_tokens)),
        ]);
        if stream {
            body.insert("stream_options".into(), json!({"include_usage": true}));
            // Prompt-processing events keep a long or queued prompt exempt from
            // stall detection rather than arming it, so a slow first batch is not
            // mistaken for a stalled server. Servers without support ignore the field.
            body.insert("return_progress".into(), json!(true));
        }
        if let Some(effort) = &request.reasoning_effort {
            body.insert("reasoning_effort".into(), json!(effort));
            body.insert(
                "chat_template_kwargs".into(),
                if effort == "none" {
                    json!({"enable_thinking": false})
                } else {
                    json!({"reasoning_effort": effort})
                },
            );
        }
        if !request.tools.is_empty() {
            body.insert(
                "tools".into(),
                json!(request
                    .tools
                    .iter()
                    .map(|t| json!({"type":"function",
                "function":{"name":t.name,"description":t.description,"parameters":t.parameters}}))
                    .collect::<Vec<_>>()),
            );
        }
        if let Some(schema) = &request.json_schema {
            body.insert(
                "response_format".into(),
                json!({"type":"json_schema","json_schema":{"name":"response","schema":schema}}),
            );
        }
        Ok(Value::Object(body))
    }

    fn legacy_request(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
    ) -> CompletionRequest {
        let mut input = crate::application::services::completion_input::from_context(
            &self.settings.prompts.system_prompt,
            context,
            prompt,
        );
        if let Some(images) = images.filter(|images| !images.is_empty()) {
            let mut content = vec![json!({"type":"text","text":prompt})];
            content.extend(images.into_iter().map(|image| {
                let url = if image.starts_with("data:") {
                    image
                } else {
                    format!("data:image/png;base64,{image}")
                };
                json!({"type":"image_url","image_url":{"url":url}})
            }));
            // Replace the final text-only user message, retaining its position.
            input.pop();
            input.push(CompletionInput::Native {
                value: json!({"role":"user","content":content}),
            });
        }
        CompletionRequest {
            input,
            ..Default::default()
        }
    }
}

/// The server's chat template may allow only one system message at index zero.
/// Normalize typed/native requests too, without changing history or tool payloads.
fn coalesce_system_messages(messages: Vec<Value>) -> Result<Vec<Value>> {
    let mut instructions = Vec::new();
    let mut conversation = Vec::new();
    for message in messages {
        if message.get("role").and_then(Value::as_str) == Some("system") {
            let content = message
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AppError::InvalidInput("llama.cpp system instructions must be text".into())
                })?
                .trim();
            if !content.is_empty() && !instructions.iter().any(|existing| existing == content) {
                instructions.push(content.to_owned());
            }
        } else {
            conversation.push(message);
        }
    }
    if !instructions.is_empty() {
        conversation.insert(
            0,
            json!({"role":"system","content":instructions.join("\n\n")}),
        );
    }
    Ok(conversation)
}

#[async_trait]
impl LLMPort for LlamaCppLlm {
    fn supports_typed_completions(&self) -> bool {
        true
    }
    fn supports_tool_calling(&self) -> bool {
        true
    }
    fn provider_name(&self) -> &str {
        "llamacpp"
    }
    fn model_name(&self) -> &str {
        &self.settings.llama_cpp.model
    }
    fn max_context_tokens(&self) -> usize {
        self.settings.context_window as usize
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len().div_ceil(4)
    }
    async fn is_ready(&self) -> Result<bool> {
        let response = self
            .client
            .get(format!("{}/models", self.base_url))
            .timeout(self.stall_timeout)
            .send()
            .await
            .map_err(network_error)?;
        let response = check_status(response).await?;
        let value: Value = response.json().await.map_err(network_error)?;
        Ok(value
            .get("data")
            .and_then(Value::as_array)
            .is_some_and(|models| {
                models
                    .iter()
                    .any(|model| model.get("id").and_then(Value::as_str) == Some(self.model_name()))
            }))
    }
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.complete_reliably(request, &|_| Ok(()), Some(&|_| Ok(())))
            .await
    }
    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        // Text-only consumers cannot retract an already delivered draft.
        self.complete_reliably(request, on_text, None).await
    }

    async fn complete_with_retry_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.complete_reliably(request, on_text, Some(on_retry))
            .await
    }

    async fn generate(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
    ) -> Result<String> {
        let response = self
            .complete(&self.legacy_request(prompt, context, images))
            .await?;
        if response.finish_reason == "length" {
            return Err(AppError::InvalidState(
                "llama.cpp reached the output token limit before completing the response".into(),
            ));
        }
        Ok(response.text)
    }
    async fn generate_streaming(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
    ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin + '_>> {
        let request = self.legacy_request(prompt, context, images);
        let output = async_stream::try_stream! {
            let (sender, mut receiver) = futures::channel::mpsc::unbounded();
            let on_text = |text| sender.unbounded_send(text).map_err(|_| {
                AppError::InvalidState("Generation consumer disconnected".into())
            });
            let completion = self.complete_with_progress(&request, &on_text);
            tokio::pin!(completion);
            let result = loop {
                tokio::select! {
                    biased;
                    Some(text) = receiver.next() => { yield text; }
                    result = &mut completion => break result,
                }
            };
            let response = result?;
            if !response.tool_calls.is_empty() {
                Err(AppError::InvalidState("Unexpected tool call in text generation".into()))?;
            }
            while let Ok(text) = receiver.try_recv() { yield text; }
        };
        Ok(Box::new(Box::pin(output)))
    }
}

pub fn validate_connection(connection: &LlamaCppSettingsDto) -> Result<String> {
    let raw = connection.url.trim().trim_end_matches('/');
    let parsed = url::Url::parse(raw)
        .map_err(|_| AppError::InvalidConfig("Invalid llama.cpp server URL".into()))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AppError::InvalidConfig("Use an HTTP(S) server URL without credentials, query, or fragment; configure authentication in the header fields".into()));
    }
    if connection.auth_header_name.trim().is_empty()
        ^ connection.auth_header_value.trim().is_empty()
    {
        return Err(AppError::InvalidConfig(
            "Set both authentication header fields, or clear both".into(),
        ));
    }
    Ok(if raw.ends_with("/v1") {
        raw.to_owned()
    } else {
        format!("{raw}/v1")
    })
}

/// No client-wide read or total timeout: generations are bounded by their time
/// budget and stall detection, probes by per-request timeouts.
fn http_client(connection: &LlamaCppSettingsDto) -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    if !connection.auth_header_name.trim().is_empty() {
        let name = HeaderName::from_bytes(connection.auth_header_name.trim().as_bytes())
            .map_err(|_| AppError::InvalidConfig("Invalid authentication header name".into()))?;
        let mut value = HeaderValue::from_str(connection.auth_header_value.trim())
            .map_err(|_| AppError::InvalidConfig("Invalid authentication header value".into()))?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    crate::shared::utils::reqwest_client_builder()
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(network_error)
}

fn network_error(error: reqwest::Error) -> AppError {
    // Do not include headers, bodies, or submitted conversation text in diagnostics.
    if error.is_timeout() {
        AppError::Network("llama.cpp request timed out".into())
    } else {
        AppError::Network("llama.cpp request failed; check the server connection".into())
    }
}

async fn check_status(mut response: reqwest::Response) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let mut description = format!(
        "llama.cpp returned HTTP {} from {}",
        status,
        response.url().path()
    );
    // Read a bounded error envelope. Template errors can contain prompt fragments,
    // so expose recognized causes rather than copying arbitrary server bodies.
    let mut bytes = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        if bytes.len() + chunk.len() > 16 * 1024 {
            break;
        }
        bytes.extend_from_slice(&chunk);
    }
    let error = serde_json::from_slice::<Value>(&bytes).ok();
    if let Some(message) = error
        .as_ref()
        .and_then(|e| e.pointer("/error/message"))
        .and_then(Value::as_str)
    {
        if message.contains("System message must be at the beginning") {
            description.push_str(": the model requires a single system message at the beginning");
        } else if message.contains("Unable to generate parser for this template") {
            description.push_str(": the model's chat template rejected the message format");
        }
    }
    if status.is_client_error() {
        Err(AppError::InvalidState(description))
    } else {
        Err(AppError::ServiceNotAvailable(description))
    }
}

fn parse_completion(value: Value) -> Result<CompletionResponse> {
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .ok_or_else(|| AppError::Network("llama.cpp returned no completion choices".into()))?;
    let message = choice
        .get("message")
        .filter(|m| m.is_object())
        .ok_or_else(|| AppError::Network("llama.cpp returned no completion message".into()))?;
    let mut calls = Vec::new();
    let mut ids = std::collections::HashSet::new();
    for call in message
        .get("tool_calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = call.get("id").and_then(Value::as_str);
        let function = call.get("function");
        let name = function.and_then(|f| f.get("name")).and_then(Value::as_str);
        let arguments = function
            .and_then(|f| f.get("arguments"))
            .and_then(Value::as_str);
        let (Some(id), Some(name), Some(arguments)) = (id, name, arguments) else {
            return Err(AppError::Network(
                "llama.cpp returned an invalid tool call".into(),
            ));
        };
        let arguments: Value = serde_json::from_str(arguments).map_err(|_| {
            AppError::InvalidState("llama.cpp returned invalid tool arguments".into())
        })?;
        if id.trim().is_empty()
            || name.trim().is_empty()
            || !ids.insert(id)
            || !arguments.is_object()
            || calls.len() >= 128
            || call.get("type").and_then(Value::as_str) != Some("function")
        {
            return Err(AppError::InvalidState(
                "llama.cpp returned an invalid tool batch".into(),
            ));
        }
        calls.push(CompletionInput::ToolCall {
            id: id.into(),
            name: name.into(),
            arguments,
        });
    }
    Ok(CompletionResponse {
        text: message
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        tool_calls: calls,
        finish_reason: choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        input_tokens: value
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        output_tokens: value
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        provider_output: message.clone(),
    })
}
