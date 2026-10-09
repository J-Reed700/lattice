//! Remote llama.cpp server adapter. Uses Chat Completions, independently of Ollama.
use crate::application::{
    contracts::settings::{LLMSettingsDto, LlamaCppSettingsDto},
    ports::llm_port::{
        CompletionInput, CompletionRequest, CompletionResponse, LLMPort, ToolDefinition,
    },
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{json, Value};
use std::time::Duration;

mod retry;
/// The bundled sidecar speaks the same stream and needs the same distinction
/// between prompt processing and generation for its first-token allowance.
pub(crate) use retry::ProgressWatch;
pub(crate) mod streaming;
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
        let response = client
            .post(format!("{base_url}/chat/completions"))
            .timeout(probe_timeout)
            .json(
                &json!({"model": model, "messages":[{"role":"user","content":"Reply with OK."}],
                "max_tokens": 2048, "stream": false}),
            )
            .send()
            .await
            .map_err(network_error)?;
        let response = check_status(response).await?;
        parse_completion(response.json().await.map_err(network_error)?)?;
        Ok(models)
    }

    /// What the server can still be asked to generate once this prompt is in
    /// its context.
    ///
    /// `max_tokens` is a *reservation*: llama.cpp holds prompt + n_predict in
    /// one slot, so asking for the whole window on every turn works only while
    /// prompts stay small. A turn carrying an attachment and six web pages ran
    /// 75k prompt tokens into a 163k slot, asked for 131k more on top, and the
    /// server failed the batch — a 500 forty-eight seconds in, after it had
    /// already done the work.
    ///
    /// The prompt is measured with the same rough estimate the budgeter uses,
    /// which reads low on dense text, so the estimate is inflated before it is
    /// subtracted. `MIN_OUTPUT_TOKENS` keeps a nearly-full window answerable
    /// rather than silently truncated to nothing; a prompt that leaves less
    /// room than that is over budget for reasons this clamp cannot fix.
    fn output_room_for(&self, messages: &[Value], requested: u32) -> u32 {
        /// Estimated prompt tokens are scaled by this before being subtracted.
        const ESTIMATE_SAFETY: usize = 3;
        const ESTIMATE_SAFETY_DIVISOR: usize = 2;
        const MIN_OUTPUT_TOKENS: u32 = 1024;

        let window = self.settings.context_window as usize;
        let prompt_chars: usize = messages
            .iter()
            .map(|message| message.to_string().chars().count())
            .sum();
        let prompt_tokens = crate::application::ports::llm_port::tokens_for_chars(
            prompt_chars,
            crate::application::ports::llm_port::DEFAULT_CHARS_PER_TOKEN,
        ) * ESTIMATE_SAFETY
            / ESTIMATE_SAFETY_DIVISOR;
        let room = u32::try_from(window.saturating_sub(prompt_tokens)).unwrap_or(u32::MAX);
        let allowed = requested.min(room).max(MIN_OUTPUT_TOKENS);
        if allowed < requested {
            tracing::debug!(
                requested,
                allowed,
                prompt_tokens,
                window,
                "Capped the llama.cpp output reservation to what the window has left"
            );
        }
        allowed
    }

    fn body(&self, request: &CompletionRequest, stream: bool) -> Result<Value> {
        let messages = chat_messages(&request.input)?;
        let requested_output = request.effective_max_output_tokens(self.settings.max_tokens);
        let max_tokens = self.output_room_for(&messages, requested_output);
        let sampling = request.sampling.unwrap_or_default();
        let mut body = serde_json::Map::from_iter([
            ("model".into(), json!(self.settings.llama_cpp.model)),
            ("messages".into(), json!(messages)),
            ("stream".into(), json!(stream)),
            (
                "temperature".into(),
                json!(sampling.temperature.unwrap_or(self.settings.temperature)),
            ),
            (
                "top_p".into(),
                json!(sampling.top_p.unwrap_or(self.settings.top_p)),
            ),
            (
                "top_k".into(),
                json!(sampling.top_k.unwrap_or(self.settings.top_k)),
            ),
            ("repeat_penalty".into(), json!(self.settings.repeat_penalty)),
            ("max_tokens".into(), json!(max_tokens)),
        ]);
        if stream {
            body.insert("stream_options".into(), json!({"include_usage": true}));
            // Prompt-processing events keep a long or queued prompt exempt from
            // stall detection rather than arming it, so a slow first batch is not
            // mistaken for a stalled server. Servers without support ignore the field.
            body.insert("return_progress".into(), json!(true));
        }
        if let Some(effort) = request
            .reasoning_effort
            .as_deref()
            .filter(|effort| *effort != "none")
        {
            body.insert("reasoning_effort".into(), json!(effort));
            body.insert(
                "chat_template_kwargs".into(),
                json!({"reasoning_effort": effort}),
            );
        }
        if !request.tools.is_empty() {
            body.insert("tools".into(), tool_specs(&request.tools));
            // llama-server lets a reply hold one call unless asked for more,
            // so a round that needs three pages would take three generations.
            body.insert("parallel_tool_calls".into(), json!(true));
        }
        if request.want_logprobs {
            body.insert("logprobs".into(), json!(true));
            body.insert(
                "top_logprobs".into(),
                json!(crate::application::ports::llm_port::FIRST_TOKEN_TOP_LOGPROBS),
            );
        }
        if let Some(schema) = &request.json_schema {
            body.insert(
                "response_format".into(),
                json!({"type":"json_schema","json_schema":{"name":"response","schema":schema}}),
            );
        }
        // The scheduler's slot: a conversation returns to the slot whose KV
        // cache holds its prefix. The retry path may still turn reuse off.
        if let Some(slot) = request.assigned_slot {
            body.insert("id_slot".into(), json!(slot));
            body.insert("cache_prompt".into(), json!(true));
        }
        Ok(Value::Object(body))
    }

    /// The server's root URL, without the `/v1` the chat routes live under.
    fn server_root(&self) -> &str {
        self.base_url.strip_suffix("/v1").unwrap_or(&self.base_url)
    }

    /// This server's shared scheduler, and an exact tokenizer when it answers
    /// like a llama-server.
    pub(crate) async fn schedule(self) -> std::sync::Arc<dyn LLMPort> {
        use crate::features::llm::scheduler::{
            remote_llama_cpp_scheduler, ScheduledLlm, ServerTokenizer,
        };
        let (scheduler, is_llama_server) = remote_llama_cpp_scheduler(
            &self.client,
            self.server_root(),
            self.settings.context_window as usize,
        )
        .await;
        let tokenizer =
            is_llama_server.then(|| ServerTokenizer::new(self.client.clone(), self.server_root()));
        let output_limit = self.settings.max_tokens;
        let scheduled = ScheduledLlm::new(std::sync::Arc::new(self), scheduler, output_limit);
        std::sync::Arc::new(match tokenizer {
            Some(tokenizer) => scheduled.with_tokenizer(tokenizer),
            None => scheduled,
        })
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

/// A typed request's input as OpenAI chat messages.
///
/// Shared with the bundled sidecar, which is the same llama-server behind the
/// same `/v1/chat/completions` route: one translation means a tool round
/// replays identically whichever of the two the model is reached through.
pub(crate) fn chat_messages(input: &[CompletionInput]) -> Result<Vec<Value>> {
    let mut messages: Vec<Value> = Vec::with_capacity(input.len());
    let mut previous_was_call = false;
    for item in input {
        let is_call = matches!(item, CompletionInput::ToolCall { .. });
        match item {
            CompletionInput::Native { value } => messages.push(value.clone()),
            // Chat templates know system, user, assistant and tool; a
            // developer instruction is a system one to every one of them.
            CompletionInput::Message { role, content } if role == "developer" => {
                messages.push(json!({"role":"system","content":content}))
            }
            CompletionInput::Message { role, content } => {
                messages.push(json!({"role":role,"content":content}))
            }
            CompletionInput::ToolCall {
                id,
                name,
                arguments,
            } => {
                let call = json!({"id":id,"type":"function",
                    "function":{"name":name,"arguments":arguments.to_string()}});
                // Calls made in one round are one assistant message. Replayed
                // as one message each, they break the role alternation that
                // strict chat templates enforce.
                match messages
                    .last_mut()
                    .filter(|_| previous_was_call)
                    .and_then(|last| last.get_mut("tool_calls"))
                    .and_then(Value::as_array_mut)
                {
                    Some(calls) => calls.push(call),
                    None => messages
                        .push(json!({"role":"assistant","content":null,"tool_calls":[call]})),
                }
            }
            CompletionInput::ToolResult { id, output } => {
                messages.push(json!({"role":"tool","tool_call_id":id,"content":output}))
            }
        }
        previous_was_call = is_call;
    }
    coalesce_system_messages(messages)
}

/// Tool definitions in the OpenAI `tools` shape llama-server renders through
/// the model's chat template.
pub(crate) fn tool_specs(tools: &[ToolDefinition]) -> Value {
    json!(tools
        .iter()
        .map(|t| json!({"type":"function",
            "function":{"name":t.name,"description":t.description,"parameters":t.parameters}}))
        .collect::<Vec<_>>())
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
        self.complete_reliably(request, &|_| Ok(()), None, Some(&|_| Ok(())))
            .await
    }
    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        // Text-only consumers cannot retract an already delivered draft.
        self.complete_reliably(request, on_text, None, None).await
    }

    async fn complete_with_retry_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.complete_reliably(request, on_text, None, Some(on_retry))
            .await
    }

    async fn complete_with_reasoning_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.complete_reliably(request, on_text, Some(on_reasoning), Some(on_retry))
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
            // A plain text stream has no way to say it was cut short.
            if response.finish_reason == "length" {
                Err(AppError::InvalidState(
                    "llama.cpp reached the output token limit before completing the response".into(),
                ))?;
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
    crate::shared::http::reqwest_client_builder()
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

pub(crate) async fn check_status(mut response: reqwest::Response) -> Result<reqwest::Response> {
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
    if status.as_u16() == 429 {
        Err(AppError::RateLimitExceeded(description))
    } else if status.as_u16() == 408 {
        Err(AppError::Network(description))
    } else if status.is_client_error() {
        Err(AppError::InvalidState(description))
    } else {
        Err(AppError::ServiceNotAvailable(description))
    }
}

/// More calls than any round needs; past this a model is looping, not working.
const MAX_TOOL_CALLS_PER_ROUND: usize = 128;

/// Stands in for the name of a call that gave none, so the replayed history
/// still has a name the chat template can render.
const UNNAMED_TOOL: &str = "unnamed_tool";

/// The arguments of a tool call the model wrote but that cannot be run.
///
/// Kept as a JSON object so the replayed history stays parseable, carrying the
/// model's raw text so the error it is shown quotes what it actually wrote.
pub(crate) fn invalid_tool_arguments(raw: &str, problem: &str) -> Value {
    json!({"invalid": true, "raw": raw, "error": problem})
}

/// Why a call cannot be run, when its arguments are the marker
/// [`invalid_tool_arguments`] wrote.
pub(crate) fn invalid_tool_call_problem(arguments: &Value) -> Option<String> {
    let object = arguments.as_object()?;
    if object.len() != 3 || object.get("invalid") != Some(&Value::Bool(true)) {
        return None;
    }
    let raw = object.get("raw")?.as_str()?;
    let problem = object.get("error")?.as_str()?;
    Some(format!(
        "Tool call not run: {problem}. You wrote: {}. Call the tool again with a complete JSON object of arguments, or answer from what you have.",
        crate::shared::text::safe_truncate(raw, 400)
    ))
}

pub(crate) fn parse_completion(value: Value) -> Result<CompletionResponse> {
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .ok_or_else(|| AppError::Network("llama.cpp returned no completion choices".into()))?;
    let message = choice
        .get("message")
        .filter(|m| m.is_object())
        .ok_or_else(|| AppError::Network("llama.cpp returned no completion message".into()))?;
    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let raw_calls: Vec<&Value> = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_TOOL_CALLS_PER_ROUND)
        .collect();
    let mut calls = Vec::new();
    let mut ids = std::collections::HashSet::new();
    let mut repaired = false;
    for (index, call) in raw_calls.iter().enumerate() {
        // A small model gets a call wrong often enough that failing the whole
        // completion over it throws away a turn the model can recover: it is
        // handed back as a call the tool loop answers with the error, and the
        // model tries again next round.
        let function = call.get("function");
        let name = function
            .and_then(|f| f.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let raw_arguments = function
            .and_then(|f| f.get("arguments"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let id = match call
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty() && !ids.contains(*id))
        {
            Some(id) => id.to_owned(),
            None => {
                repaired = true;
                format!("call_invalid_{index}")
            }
        };
        ids.insert(id.clone());
        // A call that parses whole was finished even when the round then hit
        // its output limit; only the one whose arguments broke off was cut.
        let cut_off = finish_reason == "length" && index + 1 == raw_calls.len();
        let problem = match (name, serde_json::from_str::<Value>(raw_arguments)) {
            (None, _) => Some("the call named no tool".to_owned()),
            (Some(name), Ok(arguments)) if arguments.is_object() => {
                calls.push(CompletionInput::ToolCall {
                    id: id.clone(),
                    name: name.into(),
                    arguments,
                });
                None
            }
            (Some(_), Ok(_)) => Some("arguments must be a JSON object".to_owned()),
            (Some(_), Err(error)) if cut_off => Some(format!(
                "arguments were cut off by the output limit before the JSON closed ({error})"
            )),
            (Some(_), Err(error)) => Some(format!("arguments were not valid JSON: {error}")),
        };
        if let Some(problem) = problem {
            repaired = true;
            calls.push(CompletionInput::ToolCall {
                id,
                name: name.unwrap_or(UNNAMED_TOOL).into(),
                arguments: invalid_tool_arguments(raw_arguments, &problem),
            });
        }
    }
    // The assistant message is replayed as the server returned it, so it has
    // to agree with the calls the tool results will answer: the same ids, and
    // arguments the chat template can parse again.
    let mut provider_output = message.clone();
    let dropped = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .is_some_and(|all| all.len() > raw_calls.len());
    if repaired || dropped {
        if let Some(object) = provider_output.as_object_mut() {
            object.insert(
                "tool_calls".into(),
                Value::Array(
                    calls
                        .iter()
                        .filter_map(|call| match call {
                            CompletionInput::ToolCall {
                                id,
                                name,
                                arguments,
                            } => Some(json!({
                                "id": id, "type": "function",
                                "function": {"name": name, "arguments": arguments.to_string()}
                            })),
                            _ => None,
                        })
                        .collect(),
                ),
            );
        }
    }
    let reasoning = ["reasoning_content", "reasoning"]
        .iter()
        .find_map(|key| message.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned);
    Ok(CompletionResponse {
        text: message
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        reasoning,
        tool_calls: calls,
        finish_reason: finish_reason.into(),
        input_tokens: value
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        output_tokens: value
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        provider_output,
        // Non-streaming servers can include reasoning tokens in this array.
        // Without token/channel alignment, do not treat them as verdict confidence.
        first_token_logprobs: choice
            .get("logprobs")
            .filter(|_| {
                !["reasoning_content", "reasoning"].iter().any(|key| {
                    message
                        .get(key)
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.is_empty())
                })
            })
            .and_then(crate::application::ports::llm_port::first_token_logprobs),
    })
}
