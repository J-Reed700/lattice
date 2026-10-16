//! Native OpenAI Responses and Anthropic Messages adapters.
//! Explicit selection only; Auto never sends local documents to a cloud provider.
use async_trait::async_trait;
use lattice_core::application::{
    contracts::settings::{LLMProvider, LLMSettingsDto},
    ports::llm_port::{
        CompletionInput, CompletionRequest, CompletionResponse, LLMPort, ThinkingMode, ToolChoice,
    },
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

mod streaming;

pub struct CloudLlm {
    provider: LLMProvider,
    model: String,
    key: String,
    endpoint: String,
    client: reqwest::Client,
    /// Longest silence tolerated inside a streamed response.
    stall_timeout: Duration,
    max_tokens: u32,
    context_window: usize,
}

impl CloudLlm {
    /// Behind the provider's shared scheduler, so every role on one provider
    /// queues in one place.
    pub(crate) fn schedule(
        self,
        schedulers: &crate::features::llm::scheduler::SchedulerRegistry,
    ) -> std::sync::Arc<dyn LLMPort> {
        let scheduler = schedulers.cloud(self.provider_name());
        let output_limit = self.max_tokens;
        std::sync::Arc::new(crate::features::llm::scheduler::ScheduledLlm::new(
            std::sync::Arc::new(self),
            scheduler,
            output_limit,
        ))
    }

    pub fn new(settings: &LLMSettingsDto, key: String) -> Result<Self> {
        if key.trim().is_empty() || settings.model.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "Cloud model and API key are required".into(),
            ));
        }
        let endpoint = match settings.provider {
            LLMProvider::Openai => "https://api.openai.com/v1/responses",
            LLMProvider::Anthropic => "https://api.anthropic.com/v1/messages",
            LLMProvider::Google => {
                "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
            }
            _ => {
                return Err(AppError::InvalidConfig(
                    "Expected an explicit cloud provider".into(),
                ))
            }
        };
        // No client-wide timeout: a long answer is bounded by the request's time
        // budget, and a streamed one additionally by stall detection.
        let client = lattice_core::shared::http::reqwest_client_builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| AppError::InvalidConfig(e.to_string()))?;
        Ok(Self {
            provider: settings.provider,
            model: settings.model.clone(),
            key,
            endpoint: endpoint.into(),
            client,
            stall_timeout: Duration::from_secs(u64::from(settings.timeout_seconds.max(1))),
            max_tokens: settings.max_tokens,
            context_window: settings.context_window as usize,
        })
    }

    fn body(&self, request: &CompletionRequest, stream: bool) -> Result<Value> {
        if self.provider == LLMProvider::Openai {
            let input: Vec<Value> = request.input.iter().map(|item| match item {
                CompletionInput::Native { value } => value.clone(),
                CompletionInput::Message { role, content } => json!({"role":role,"content":content}),
                CompletionInput::ToolCall { id, name, arguments } => json!({"type":"function_call","call_id":id,"name":name,"arguments":arguments.to_string()}),
                CompletionInput::ToolResult { id, output } => json!({"type":"function_call_output","call_id":id,"output":output}),
            }).collect();
            let max_output = request.effective_max_output_tokens(self.max_tokens);
            let mut body = json!({"model":self.model,"input":input,"stream":stream,"store":false,"include":["reasoning.encrypted_content"],"max_output_tokens":max_output});
            if !request.tools.is_empty() {
                set_field(&mut body, "tools", json!(request.tools.iter().map(|t| json!({"type":"function","name":t.name,"description":t.description,"parameters":t.parameters,"strict":false})).collect::<Vec<_>>()))?;
                if let Some(choice) = request.tool_choice {
                    set_field(&mut body, "tool_choice", json!(choice.as_str()))?;
                }
            }
            if let Some(schema) = &request.json_schema {
                set_field(
                    &mut body,
                    "text",
                    json!({"format":{"type":"json_schema","name":"response","schema":schema,"strict":true}}),
                )?;
            }
            // `request.thinking` is deliberately not read here: an OpenAI
            // reasoning model always reasons and a chat model never does, so
            // the effort level below is the only control the API offers.
            let effort = openai_reasoning_effort(&self.model, request.reasoning_effort.as_deref());
            if is_openai_reasoning_model(&self.model)
                && (effort.is_some() || request.include_reasoning)
            {
                let mut reasoning = json!({});
                if let Some(effort) = effort {
                    set_field(&mut reasoning, "effort", json!(effort))?;
                }
                if request.include_reasoning {
                    // Raw reasoning tokens are not exposed by the Responses
                    // API. `auto` asks for the most detailed supported summary.
                    set_field(&mut reasoning, "summary", json!("auto"))?;
                }
                set_field(&mut body, "reasoning", reasoning)?;
            }
            // Reasoning models reject both knobs outright, so an override that
            // would 400 the request is dropped rather than sent: the caller
            // wanted determinism, not a failed call.
            if let Some(sampling) = request
                .sampling
                .filter(|_| !is_openai_reasoning_model(&self.model))
            {
                if let Some(temperature) = sampling.temperature {
                    set_field(&mut body, "temperature", json!(temperature))?;
                }
                if let Some(top_p) = sampling.top_p {
                    set_field(&mut body, "top_p", json!(top_p))?;
                }
            }
            Ok(body)
        } else if self.provider == LLMProvider::Google {
            let mut messages = Vec::new();
            for item in &request.input {
                match item {
                    CompletionInput::Native { value } => messages.push(value.clone()),
                    CompletionInput::Message { role, content } => {
                        messages.push(json!({"role": role, "content": content}));
                    }
                    CompletionInput::ToolCall {
                        id,
                        name,
                        arguments,
                    } => {
                        let call = json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": arguments.to_string(),
                            }
                        });
                        if let Some(last) = messages.last_mut().filter(|m| {
                            field(m, "role") == "assistant" && field(m, "tool_calls").is_array()
                        }) {
                            if let Some(calls) =
                                last.get_mut("tool_calls").and_then(Value::as_array_mut)
                            {
                                calls.push(call);
                            }
                        } else {
                            messages.push(json!({
                                "role": "assistant",
                                "tool_calls": [call],
                            }));
                        }
                    }
                    CompletionInput::ToolResult { id, output } => {
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": id,
                            "content": output,
                        }));
                    }
                }
            }
            let max_output = request.effective_max_output_tokens(self.max_tokens);
            let model = self.model.strip_prefix("models/").unwrap_or(&self.model);
            let mut body = json!({
                "model": model,
                "messages": messages,
                "stream": stream,
                "max_tokens": max_output,
            });
            if stream {
                set_field(&mut body, "stream_options", json!({"include_usage": true}))?;
            }
            if let Some(sampling) = request.sampling {
                if let Some(temperature) = sampling.temperature {
                    set_field(&mut body, "temperature", json!(temperature))?;
                }
                if let Some(top_p) = sampling.top_p {
                    set_field(&mut body, "top_p", json!(top_p))?;
                }
            }
            if let Some(effort) = request.reasoning_effort.as_deref() {
                set_field(&mut body, "reasoning_effort", json!(effort))?;
            }
            if !request.tools.is_empty() {
                set_field(
                    &mut body,
                    "tools",
                    json!(request
                        .tools
                        .iter()
                        .map(|t| json!({
                            "type": "function",
                            "function": {
                                "name": t.name,
                                "description": t.description,
                                "parameters": t.parameters,
                            }
                        }))
                        .collect::<Vec<_>>()),
                )?;
                if let Some(choice) = request.tool_choice {
                    let choice_str = match choice {
                        ToolChoice::Auto => "auto",
                        ToolChoice::Required => "required",
                        ToolChoice::None => "none",
                    };
                    set_field(&mut body, "tool_choice", json!(choice_str))?;
                }
            }
            if let Some(schema) = &request.json_schema {
                set_field(
                    &mut body,
                    "response_format",
                    json!({
                        "type": "json_schema",
                        "json_schema": {
                            "name": "response",
                            "schema": schema,
                            "strict": true
                        }
                    }),
                )?;
            }
            Ok(body)
        } else {
            let mut system = Vec::new();
            let mut messages = Vec::new();
            for item in &request.input {
                match item {
                    CompletionInput::Native { value } => messages.push(value.clone()),
                    CompletionInput::Message { role, content } if role == "system" || role == "developer" => system.push(content.clone()),
                    CompletionInput::Message { role, content } => messages.push(json!({"role":role,"content":content})),
                    CompletionInput::ToolCall { id, name, arguments } => messages.push(json!({"role":"assistant","content":[{"type":"tool_use","id":id,"name":name,"input":arguments}]})),
                    CompletionInput::ToolResult { id, output } => {
                        let result = json!({"type":"tool_result","tool_use_id":id,"content":output});
                        if let Some(last) = messages.last_mut().filter(|m| field(m, "role") == "user" && field(m, "content").is_array()) {
                            if let Some(content) = last.get_mut("content").and_then(Value::as_array_mut) { content.push(result); }
                        } else { messages.push(json!({"role":"user","content":[result]})); }
                    },
                }
            }
            let mut body = json!({"model":self.model,"messages":messages,"max_tokens":request.effective_max_output_tokens(self.max_tokens),"stream":stream});
            if let Some(sampling) = request.sampling {
                if let Some(temperature) = sampling.temperature {
                    set_field(&mut body, "temperature", json!(temperature))?;
                }
                if let Some(top_p) = sampling.top_p {
                    set_field(&mut body, "top_p", json!(top_p))?;
                }
                if let Some(top_k) = sampling.top_k {
                    set_field(&mut body, "top_k", json!(top_k))?;
                }
            }
            if !system.is_empty() {
                set_field(&mut body, "system", json!(system.join("\n\n")))?;
            }
            // Extended thinking is off unless a `thinking` block asks for it.
            // A thinking request asks, and so does a real effort level, which
            // grades it; "none" is expressed by leaving the block out, since
            // requesting adaptive thinking turned "do not think" into its
            // opposite.
            let effort = request.reasoning_effort.as_deref();
            let thinks = request.thinking == ThinkingMode::On
                || effort.is_some_and(|effort| effort != "none");
            if thinks {
                set_field(&mut body, "thinking", json!({"type":"adaptive"}))?;
            }
            if let Some(effort) = effort {
                // Anthropic has no "none" effort of its own, so the answer
                // itself is held at the lowest setting instead.
                let effort = if effort == "none" { "low" } else { effort };
                set_field(&mut body, "output_config", json!({"effort":effort}))?;
            }
            if !request.tools.is_empty() {
                set_field(&mut body, "tools", json!(request.tools.iter().map(|t| json!({"name":t.name,"description":t.description,"input_schema":t.parameters})).collect::<Vec<_>>()))?;
                // Forcing a call is refused while extended thinking is on, so a
                // thinking request that requires one leaves the choice to the model.
                let choice = match request.tool_choice {
                    Some(ToolChoice::Required) if !thinks => Some("any"),
                    Some(ToolChoice::Auto | ToolChoice::Required) => Some("auto"),
                    Some(ToolChoice::None) => Some("none"),
                    None => None,
                };
                if let Some(choice) = choice {
                    set_field(&mut body, "tool_choice", json!({"type": choice}))?;
                }
            }
            if let Some(schema) = &request.json_schema {
                let mut config = field(&body, "output_config").clone();
                if config.is_null() {
                    config = json!({});
                }
                set_field(
                    &mut config,
                    "format",
                    json!({"type":"json_schema","schema":schema}),
                )?;
                set_field(&mut body, "output_config", config)?;
            }
            Ok(body)
        }
    }

    /// All three attempts and their backoffs share one `deadline`, so a retry
    /// never multiplies the caller's time budget. `bound_body` applies what is
    /// left of it to the whole exchange; without it only the wait for response
    /// headers is bounded and the streamed body is left to stall detection.
    async fn send(
        &self,
        body: &Value,
        deadline: Option<Instant>,
        bound_body: bool,
    ) -> Result<reqwest::Response> {
        let mut last_error = None;
        for attempt in 0..3u32 {
            let remaining =
                deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
            if remaining.is_some_and(|remaining| remaining.is_zero()) {
                return Err(last_error.unwrap_or_else(|| self.budget_error()));
            }
            let request = self.client.post(&self.endpoint).json(body);
            let request = match remaining.filter(|_| bound_body) {
                Some(remaining) => request.timeout(remaining),
                None => request,
            };
            let request = match self.provider {
                LLMProvider::Openai => request.bearer_auth(&self.key),
                LLMProvider::Anthropic => request
                    .header("x-api-key", &self.key)
                    .header("anthropic-version", "2023-06-01"),
                LLMProvider::Google => request
                    .bearer_auth(&self.key)
                    .header("x-goog-api-key", &self.key),
                _ => request,
            };
            let send = request.send();
            let response = if let Some(remaining) = remaining.filter(|_| !bound_body) {
                tokio::time::timeout(remaining, send)
                    .await
                    .map_err(|_| self.budget_error())?
            } else {
                send.await
            }
            .map_err(|e| {
                if e.is_timeout() {
                    self.budget_error()
                } else {
                    AppError::Network(format!("Cloud request failed: {e}"))
                }
            })?;
            let status = response.status();
            if status.is_success() {
                return Ok(response);
            }
            // Avoid echoing provider bodies which can contain submitted user text.
            let error = AppError::Network(format!(
                "{} API returned HTTP {}",
                self.provider_name(),
                status
            ));
            if attempt == 2 || !(status.as_u16() == 429 || status.is_server_error()) {
                return Err(error);
            }
            let delay = Duration::from_secs(
                response
                    .headers()
                    .get("retry-after")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(1 << attempt)
                    .min(30),
            );
            // Sleeping past the deadline would report a budget expiry instead of
            // the provider's own reason for refusing.
            if deadline
                .is_some_and(|deadline| deadline.saturating_duration_since(Instant::now()) <= delay)
            {
                return Err(error);
            }
            last_error = Some(error);
            tokio::time::sleep(delay).await;
        }
        Err(AppError::Network("Cloud request retries exhausted".into()))
    }

    /// Exhausting the budget is not a stall, and must not be reported as one.
    fn budget_error(&self) -> AppError {
        AppError::ServiceNotAvailable(format!(
            "{} did not respond within the request's time budget",
            self.provider_name()
        ))
    }
}

#[async_trait]
impl LLMPort for CloudLlm {
    fn supports_tool_calling(&self) -> bool {
        true
    }
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let deadline = request
            .wall_clock_budget()
            .map(|budget| Instant::now() + budget);
        let response = self
            .send(&self.body(request, false)?, deadline, true)
            .await?;
        let value: Value = response
            .json()
            .await
            .map_err(|e| AppError::Network(format!("Invalid cloud response: {e}")))?;
        parse_completion(self.provider, value)
    }
    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.complete_with_reasoning_progress(request, on_text, &|_| Ok(()), &|_| Ok(()))
            .await
    }
    async fn complete_with_reasoning_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
        _on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let deadline = request
            .wall_clock_budget()
            .map(|budget| Instant::now() + budget);
        request
            .within_time_budget(async {
                let response = self
                    .send(&self.body(request, true)?, deadline, false)
                    .await?;
                streaming::drain(
                    self.provider,
                    response.bytes_stream(),
                    self.stall_timeout,
                    on_text,
                    on_reasoning,
                )
                .await
            })
            .await
            .map_err(|_| self.budget_error())?
    }

    fn model_name(&self) -> &str {
        &self.model
    }
    fn provider_name(&self) -> &str {
        match self.provider {
            LLMProvider::Openai => "openai",
            LLMProvider::Anthropic => "anthropic",
            LLMProvider::Google => "google",
            _ => "unknown",
        }
    }
    fn max_context_tokens(&self) -> usize {
        self.context_window
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(!self.key.is_empty())
    }
}

fn parse_completion(provider: LLMProvider, value: Value) -> Result<CompletionResponse> {
    if provider == LLMProvider::Google {
        let mut response = CompletionResponse::default();
        let choices = field(&value, "choices")
            .as_array()
            .ok_or_else(|| AppError::Network("Missing cloud response choices".into()))?;
        let first_choice = choices
            .first()
            .ok_or_else(|| AppError::Network("Empty choices in cloud response".into()))?;
        let message = field(first_choice, "message");
        if let Some(content) = field(message, "content").as_str() {
            response.text.push_str(content);
        }
        if let Some(reasoning) = field(message, "reasoning_content")
            .as_str()
            .or_else(|| field(message, "reasoning").as_str())
            .or_else(|| field(message, "thought").as_str())
            .filter(|t| !t.trim().is_empty())
        {
            response.reasoning = Some(reasoning.to_string());
        }
        if let Some(tool_calls) = field(message, "tool_calls").as_array() {
            for call in tool_calls {
                let id = field(call, "id")
                    .as_str()
                    .ok_or_else(|| AppError::Network("Missing tool call ID".into()))?;
                let function = field(call, "function");
                let name = field(function, "name")
                    .as_str()
                    .ok_or_else(|| AppError::Network("Missing tool name".into()))?;
                let args_val = field(function, "arguments");
                let arguments = if let Some(args_str) = args_val.as_str() {
                    serde_json::from_str(args_str)
                        .map_err(|e| AppError::Network(format!("Invalid tool arguments: {e}")))?
                } else if args_val.is_object() {
                    args_val.clone()
                } else {
                    json!({})
                };
                response.tool_calls.push(CompletionInput::ToolCall {
                    id: id.into(),
                    name: name.into(),
                    arguments,
                });
            }
        }
        let usage = field(&value, "usage");
        response.input_tokens = field(usage, "prompt_tokens").as_u64().unwrap_or(0);
        response.output_tokens = field(usage, "completion_tokens").as_u64().unwrap_or(0);
        let mut finish_reason = field(first_choice, "finish_reason")
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        if finish_reason == "stop" && !response.tool_calls.is_empty() {
            finish_reason = "tool_calls".to_string();
        }
        response.finish_reason = finish_reason;
        response.replay = vec![CompletionInput::Native {
            value: message.clone(),
        }];
        return Ok(response);
    }
    let mut response = CompletionResponse::default();
    let mut reasoning_parts = Vec::new();
    let key = if provider == LLMProvider::Openai {
        "output"
    } else {
        "content"
    };
    let blocks = field(&value, key)
        .as_array()
        .ok_or_else(|| AppError::Network("Missing cloud response content".into()))?;
    for block in blocks {
        match field(block, "type").as_str().unwrap_or("") {
            "message" => {
                if let Some(parts) = field(block, "content").as_array() {
                    for part in parts {
                        if let Some(text) = field(part, "text")
                            .as_str()
                            .or_else(|| field(part, "refusal").as_str())
                        {
                            response.text.push_str(text);
                        }
                    }
                }
            }
            "text" => {
                if let Some(text) = field(block, "text").as_str() {
                    response.text.push_str(text);
                }
            }
            "reasoning" if provider == LLMProvider::Openai => {
                if let Some(summary) = field(block, "summary").as_array() {
                    reasoning_parts.extend(summary.iter().filter_map(|part| {
                        field(part, "text")
                            .as_str()
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                            .map(str::to_owned)
                    }));
                }
            }
            "thinking" if provider == LLMProvider::Anthropic => {
                if let Some(thinking) = field(block, "thinking")
                    .as_str()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                {
                    reasoning_parts.push(thinking.to_owned());
                }
            }
            "function_call" | "tool_use" => {
                let id = field(
                    block,
                    if provider == LLMProvider::Openai {
                        "call_id"
                    } else {
                        "id"
                    },
                )
                .as_str()
                .ok_or_else(|| AppError::Network("Missing tool call ID".into()))?;
                let name = field(block, "name")
                    .as_str()
                    .ok_or_else(|| AppError::Network("Missing tool name".into()))?;
                let arguments = if provider == LLMProvider::Openai {
                    serde_json::from_str(field(block, "arguments").as_str().unwrap_or(""))
                        .map_err(|e| AppError::Network(format!("Invalid tool arguments: {e}")))?
                } else {
                    field(block, "input").clone()
                };
                response.tool_calls.push(CompletionInput::ToolCall {
                    id: id.into(),
                    name: name.into(),
                    arguments,
                });
            }
            _ => {}
        }
    }
    response.input_tokens = field(field(&value, "usage"), "input_tokens")
        .as_u64()
        .unwrap_or(0);
    response.output_tokens = field(field(&value, "usage"), "output_tokens")
        .as_u64()
        .unwrap_or(0);
    response.finish_reason = field(
        &value,
        if provider == LLMProvider::Openai {
            "status"
        } else {
            "stop_reason"
        },
    )
    .as_str()
    .unwrap_or("unknown")
    .into();
    if !reasoning_parts.is_empty() {
        response.reasoning = Some(reasoning_parts.join("\n\n"));
    }
    // OpenAI replays its output items one by one; Anthropic replays the
    // content blocks as the assistant message they were, signed thinking
    // included.
    response.replay = match (provider, field(&value, key)) {
        (LLMProvider::Openai, Value::Array(items)) => items
            .iter()
            .cloned()
            .map(|value| CompletionInput::Native { value })
            .collect(),
        (_, blocks) => vec![CompletionInput::Native {
            value: json!({"role": "assistant", "content": blocks}),
        }],
    };
    Ok(response)
}

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}

fn set_field(value: &mut Value, key: &str, field_value: Value) -> Result<()> {
    value
        .as_object_mut()
        .ok_or_else(|| AppError::InvalidState("Expected a JSON object".into()))?
        .insert(key.into(), field_value);
    Ok(())
}

/// What, if anything, belongs in the Responses API's `reasoning` block.
///
/// The field is not universally accepted: a model without reasoning controls
/// rejects the whole request with a 400, and the retrieval callers swallow that
/// with `.ok()`, so every turn would quietly lose its query rewrites and plans
/// with nothing in the log to say why. Only models that have the control are
/// sent one.
///
/// There is also no "off" switch: the floor is `minimal` on the gpt-5 family and
/// `low` on the o-series, so a request for no reasoning is clamped to whichever
/// the target model understands. A non-reasoning model needs no clamp — it never
/// thinks in the first place.
/// Whether this OpenAI model is a reasoning model, which takes an effort
/// setting in place of sampling knobs and rejects `temperature`/`top_p`.
fn is_openai_reasoning_model(model: &str) -> bool {
    model.starts_with("gpt-5") || model.starts_with("gpt-6") || model.starts_with('o')
}

fn openai_reasoning_effort<'a>(model: &str, effort: Option<&'a str>) -> Option<&'a str> {
    let effort = effort?;
    let floor = if model.starts_with("gpt-5") || model.starts_with("gpt-6") {
        "minimal"
    } else if model.starts_with('o') {
        "low"
    } else {
        return None;
    };
    Some(if effort == "none" { floor } else { effort })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod transport_tests;
