//! Native OpenAI Responses and Anthropic Messages adapters.
//! Explicit selection only; Auto never sends local documents to a cloud provider.
use crate::application::{
    contracts::settings::{LLMProvider, LLMSettingsDto},
    ports::llm_port::{CompletionInput, CompletionRequest, CompletionResponse, LLMPort},
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
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
    pub(crate) fn schedule(self) -> std::sync::Arc<dyn LLMPort> {
        let scheduler = crate::features::llm::scheduler::cloud_scheduler(self.provider_name());
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
            _ => {
                return Err(AppError::InvalidConfig(
                    "Expected an explicit cloud provider".into(),
                ))
            }
        };
        // No client-wide timeout: a long answer is bounded by the request's time
        // budget, and a streamed one additionally by stall detection.
        let client = crate::shared::http::reqwest_client_builder()
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
            }
            if let Some(schema) = &request.json_schema {
                set_field(
                    &mut body,
                    "text",
                    json!({"format":{"type":"json_schema","name":"response","schema":schema,"strict":true}}),
                )?;
            }
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
            if !request.tools.is_empty() {
                set_field(&mut body, "tools", json!(request.tools.iter().map(|t| json!({"name":t.name,"description":t.description,"input_schema":t.parameters})).collect::<Vec<_>>()))?;
            }
            if let Some(effort) = &request.reasoning_effort {
                // Extended thinking is off unless a `thinking` block asks for it,
                // so "none" is expressed by leaving that block out — requesting
                // adaptive thinking turned "do not think" into its opposite.
                // Anthropic has no "none" effort of its own, so the answer itself
                // is held at the lowest setting instead.
                if effort == "none" {
                    set_field(&mut body, "output_config", json!({"effort":"low"}))?;
                } else {
                    set_field(&mut body, "thinking", json!({"type":"adaptive"}))?;
                    set_field(&mut body, "output_config", json!({"effort":effort}))?;
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
            let request = if self.provider == LLMProvider::Openai {
                request.bearer_auth(&self.key)
            } else {
                request
                    .header("x-api-key", &self.key)
                    .header("anthropic-version", "2023-06-01")
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
        if self.provider == LLMProvider::Openai {
            "openai"
        } else {
            "anthropic"
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
mod tests {
    use super::*;
    #[test]
    fn typed_tool_calls_keep_ids_and_usage() {
        let result = parse_completion(LLMProvider::Openai, json!({"status":"completed","output":[{"type":"function_call","call_id":"call_123","name":"search","arguments":"{\"q\":\"rust\"}"}],"usage":{"input_tokens":12,"output_tokens":3}})).unwrap();
        assert!(
            matches!(&result.tool_calls[0], CompletionInput::ToolCall { id, .. } if id == "call_123")
        );
        assert_eq!(result.input_tokens, 12);
    }

    #[test]
    fn openai_reasoning_summary_is_kept_separate_from_answer_text() {
        let result = parse_completion(
            LLMProvider::Openai,
            json!({
                "status":"completed",
                "output":[
                    {"type":"reasoning","summary":[{"type":"summary_text","text":"Compared the available evidence."}]},
                    {"type":"message","content":[{"type":"output_text","text":"The answer."}]}
                ]
            }),
        )
        .unwrap();
        assert_eq!(result.text, "The answer.");
        assert_eq!(
            result.reasoning.as_deref(),
            Some("Compared the available evidence.")
        );
    }
    #[test]
    fn provider_wire_formats_preserve_tool_results() {
        let mut settings = LLMSettingsDto::default();
        let request = CompletionRequest {
            input: vec![CompletionInput::ToolResult {
                id: "abc".into(),
                output: "found".into(),
            }],
            ..Default::default()
        };
        settings.provider = LLMProvider::Openai;
        let openai = CloudLlm::new(&settings, "test".into()).unwrap();
        assert_eq!(
            openai.body(&request, false).unwrap()["input"][0]["call_id"],
            "abc"
        );
        settings.provider = LLMProvider::Anthropic;
        let anthropic = CloudLlm::new(&settings, "test".into()).unwrap();
        assert_eq!(
            anthropic.body(&request, false).unwrap()["messages"][0]["content"][0]["tool_use_id"],
            "abc"
        );
    }

    /// A turn is replayed by appending the response's own replay items: the
    /// next request then answers calls the provider recognises, signed
    /// thinking included, with no knowledge of which provider it is.
    #[test]
    fn each_provider_replays_its_own_turn_ahead_of_the_tool_results() {
        let continue_with = |llm: &CloudLlm, response: CompletionResponse| {
            let mut input = vec![CompletionInput::Message {
                role: "user".into(),
                content: "find".into(),
            }];
            input.extend(response.replay);
            input.push(CompletionInput::ToolResult {
                id: "call_7".into(),
                output: "found".into(),
            });
            llm.body(
                &CompletionRequest {
                    input,
                    ..Default::default()
                },
                false,
            )
            .unwrap()
        };

        let mut settings = LLMSettingsDto {
            provider: LLMProvider::Openai,
            ..Default::default()
        };
        let openai = CloudLlm::new(&settings, "test".into()).unwrap();
        let reply = parse_completion(
            LLMProvider::Openai,
            json!({"status":"completed","output":[
                {"type":"reasoning","encrypted_content":"opaque","summary":[]},
                {"type":"function_call","call_id":"call_7","name":"search","arguments":"{}"}
            ]}),
        )
        .unwrap();
        let body = continue_with(&openai, reply);
        assert_eq!(body["input"][1]["encrypted_content"], "opaque");
        assert_eq!(body["input"][2]["call_id"], "call_7");
        assert_eq!(body["input"][3]["type"], "function_call_output");

        settings.provider = LLMProvider::Anthropic;
        let anthropic = CloudLlm::new(&settings, "test".into()).unwrap();
        let reply = parse_completion(
            LLMProvider::Anthropic,
            json!({"stop_reason":"tool_use","content":[
                {"type":"thinking","thinking":"Search first.","signature":"signed"},
                {"type":"tool_use","id":"call_7","name":"search","input":{}}
            ]}),
        )
        .unwrap();
        let body = continue_with(&anthropic, reply);
        assert_eq!(body["messages"][1]["role"], "assistant");
        assert_eq!(body["messages"][1]["content"][0]["signature"], "signed");
        assert_eq!(body["messages"][1]["content"][1]["id"], "call_7");
        assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "call_7");
    }

    fn anthropic_body(effort: &str) -> Value {
        let settings = LLMSettingsDto {
            provider: LLMProvider::Anthropic,
            ..Default::default()
        };
        let anthropic = CloudLlm::new(&settings, "test".into()).unwrap();
        anthropic
            .body(
                &CompletionRequest {
                    reasoning_effort: Some(effort.into()),
                    ..Default::default()
                },
                false,
            )
            .unwrap()
    }

    /// A `thinking` block is what switches extended thinking on, so asking for
    /// no reasoning must not send one.
    #[test]
    fn anthropic_omits_the_thinking_block_when_no_reasoning_is_asked_for() {
        let body = anthropic_body("none");
        assert!(body.get("thinking").is_none(), "{body}");
        assert_eq!(body["output_config"]["effort"], "low");
    }

    #[test]
    fn anthropic_still_enables_thinking_for_a_real_effort() {
        let body = anthropic_body("medium");
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["output_config"]["effort"], "medium");
    }

    fn openai_body(model: &str, effort: Option<&str>) -> Value {
        let settings = LLMSettingsDto {
            provider: LLMProvider::Openai,
            model: model.into(),
            ..Default::default()
        };
        let openai = CloudLlm::new(&settings, "test".into()).unwrap();
        openai
            .body(
                &CompletionRequest {
                    reasoning_effort: effort.map(str::to_owned),
                    ..Default::default()
                },
                false,
            )
            .unwrap()
    }

    /// A `reasoning` block on a model without reasoning controls is a 400, and
    /// the retrieval callers discard errors — so the field must never be sent
    /// speculatively.
    #[test]
    fn openai_sends_no_reasoning_block_to_a_model_that_has_no_reasoning() {
        let body = openai_body("gpt-4o-mini", Some("none"));
        assert!(body.get("reasoning").is_none(), "{body}");
    }

    #[test]
    fn openai_clamps_no_reasoning_to_each_family_floor() {
        assert_eq!(
            openai_body("gpt-5.1", Some("none"))["reasoning"]["effort"],
            "minimal"
        );
        assert_eq!(
            openai_body("o3", Some("none"))["reasoning"]["effort"],
            "low"
        );
    }

    #[test]
    fn openai_forwards_a_real_effort_unchanged() {
        assert_eq!(
            openai_body("gpt-5.1", Some("high"))["reasoning"]["effort"],
            "high"
        );
        assert!(openai_body("gpt-5.1", None).get("reasoning").is_none());
    }

    #[test]
    fn openai_requests_a_displayable_summary_only_for_reasoning_models() {
        let body = |model: &str| {
            let settings = LLMSettingsDto {
                provider: LLMProvider::Openai,
                model: model.into(),
                ..Default::default()
            };
            CloudLlm::new(&settings, "test".into())
                .unwrap()
                .body(
                    &CompletionRequest {
                        include_reasoning: true,
                        ..Default::default()
                    },
                    false,
                )
                .unwrap()
        };
        assert_eq!(body("gpt-5.1")["reasoning"]["summary"], "auto");
        assert_eq!(body("gpt-6-sol")["reasoning"]["summary"], "auto");
        assert!(body("gpt-4o-mini").get("reasoning").is_none());
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };
    async fn client(server: &MockServer) -> CloudLlm {
        let settings = LLMSettingsDto {
            provider: LLMProvider::Openai,
            model: "test-model".into(),
            ..Default::default()
        };
        let mut client = CloudLlm::new(&settings, "test-secret".into()).unwrap();
        client.endpoint = format!("{}/responses", server.uri());
        client
    }
    #[tokio::test]
    async fn interactive_completion_can_disable_the_wall_clock_budget() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(150)).set_body_json(json!({
                "status":"completed", "output":[{"type":"message","content":[{"type":"output_text","text":"Completed"}]}]
            }))).mount(&server).await;
        let client = client(&server).await;
        let request = CompletionRequest {
            time_budget: Some(Duration::from_millis(10)),
            ..Default::default()
        };
        assert!(client.complete(&request).await.is_err());
        let response = client
            .complete(&CompletionRequest {
                no_time_limit: true,
                ..request
            })
            .await
            .unwrap();
        assert_eq!(response.text, "Completed");
    }

    #[tokio::test]
    async fn streams_text_and_requires_terminal_event() {
        let server = MockServer::start().await;
        let completed = json!({"type":"response.completed","response":{"status":"completed","output":[
            {"type":"message","content":[{"type":"output_text","text":"héllo"}]}
        ]}});
        Mock::given(method("POST")).and(path("/responses")).and(header("authorization", "Bearer test-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!("data: {{\"type\":\"response.output_text.delta\",\"delta\":\"héllo\"}}\n\ndata: {completed}\n\n")))
            .mount(&server).await;
        let client = client(&server).await;
        let shown = std::sync::Mutex::new(Vec::new());
        let on_text = |text: String| {
            shown.lock().unwrap().push(text);
            Ok(())
        };
        let response = client
            .complete_with_progress(&CompletionRequest::default(), &on_text)
            .await
            .unwrap();
        assert_eq!(response.text, "héllo");
        assert_eq!(*shown.lock().unwrap(), vec!["héllo".to_string()]);
        server.reset().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n",
            ))
            .mount(&server)
            .await;
        assert!(client
            .complete_with_progress(&CompletionRequest::default(), &|_| Ok(()))
            .await
            .is_err());
    }
    #[tokio::test]
    async fn typed_reasoning_completion_requests_a_stream_and_keeps_its_native_result() {
        let server = MockServer::start().await;
        let final_response = json!({"status":"completed","output":[
            {"type":"reasoning","summary":[{"type":"summary_text","text":"Checked evidence."}],"encrypted_content":"opaque"},
            {"type":"message","content":[{"type":"output_text","text":"Answer"}]}
        ],"usage":{"input_tokens":5,"output_tokens":8}});
        let body = [
            json!({"type":"response.reasoning_summary_text.delta","item_id":"r1","summary_index":0,"delta":"Checked evidence."}),
            json!({"type":"response.output_text.delta","delta":"Answer"}),
            json!({"type":"response.completed","response":final_response}),
        ].iter().map(|event| format!("data: {event}\n\n")).collect::<String>();
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;
        let client = client(&server).await;
        let reasoning = std::sync::Mutex::new(String::new());
        let response = client
            .complete_with_reasoning_progress(
                &CompletionRequest {
                    include_reasoning: true,
                    ..Default::default()
                },
                &|_| Ok(()),
                &|delta| {
                    reasoning.lock().unwrap().push_str(&delta);
                    Ok(())
                },
                &|_| Ok(()),
            )
            .await
            .unwrap();
        assert_eq!(*reasoning.lock().unwrap(), "Checked evidence.");
        assert_eq!(replayed(&response), final_response["output"]);
        assert_eq!(response.text, "Answer");
        let requests = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["stream"], true);
    }
    #[tokio::test]
    async fn typed_tool_calls_keep_native_call_ids_and_replay_them() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status":"completed", "output":[{"type":"function_call", "call_id":"call_7", "name":"search", "arguments":"{}"}], "usage":{"input_tokens":4,"output_tokens":2}
            }))).mount(&server).await;
        let client = client(&server).await;
        let request = CompletionRequest {
            input: vec![
                CompletionInput::Message {
                    role: "system".into(),
                    content: "Search first.".into(),
                },
                CompletionInput::Message {
                    role: "user".into(),
                    content: "find".into(),
                },
            ],
            tools: vec![crate::application::ports::ToolDefinition {
                name: "search".into(),
                description: "Search documents".into(),
                parameters: json!({"type":"object","properties":{}}),
            }],
            ..Default::default()
        };
        let response = client.complete(&request).await.unwrap();
        assert!(matches!(
            response.tool_calls.first(),
            Some(CompletionInput::ToolCall { id, .. }) if id == "call_7"
        ));
        assert_eq!(replayed(&response)[0]["call_id"], "call_7");
        let requests = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&requests.first().unwrap().body).unwrap();
        assert_eq!(
            body.pointer("/input/0/role").and_then(Value::as_str),
            Some("system")
        );
        assert_eq!(
            body.pointer("/tools/0/name").and_then(Value::as_str),
            Some("search")
        );
    }

    #[tokio::test]
    async fn authentication_failure_is_not_retried_or_echoed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("sensitive-provider-body"))
            .expect(1)
            .mount(&server)
            .await;
        let client = client(&server).await;
        let error = client
            .complete(&CompletionRequest::default())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("401"));
        assert!(!error.to_string().contains("sensitive-provider-body"));
    }
    #[tokio::test]
    async fn dropping_a_completion_cancels_the_request_without_a_retry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_secs(5))
                    .set_body_string("data: {\"type\":\"response.completed\"}\n\n"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = client(&server).await;
        let abandoned = tokio::time::timeout(
            Duration::from_millis(100),
            client.complete_with_progress(&CompletionRequest::default(), &|_| Ok(())),
        )
        .await;
        assert!(abandoned.is_err());
        // No detached generation or retry task survives a dropped completion.
        server.verify().await;
    }

    /// What the response replays, as the provider's own JSON.
    fn replayed(response: &CompletionResponse) -> Value {
        Value::Array(
            response
                .replay
                .iter()
                .map(|item| match item {
                    CompletionInput::Native { value } => value.clone(),
                    other => panic!("cloud replay is native, not {other:?}"),
                })
                .collect(),
        )
    }
}
