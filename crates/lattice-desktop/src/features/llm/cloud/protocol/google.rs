//! Google's Gemini API through its OpenAI-compatible chat completions.

use lattice_core::application::{
    contracts::settings::ModelWindowDto,
    ports::llm_port::{CompletionInput, CompletionRequest, CompletionResponse, ToolChoice},
};
use lattice_core::shared::error::{AppError, Result};
use reqwest::{Client, RequestBuilder};
use serde::Deserialize;
use serde_json::{json, Value};

use super::listing::{invalid_list, is_text_model};
use super::{api_base, EventDecoder, LimitsApi, ModelPage, ProviderProtocol};
use crate::features::llm::cloud::catalog::CloudModelDto;
use crate::features::llm::cloud::reply::call_arguments;
use crate::features::llm::cloud::{field, set_field, timing};
use crate::features::llm::fitting::{Fitted, Thinking};
use crate::features::llm::llama_cpp::openai_content_parts;

mod stream;

/// Models per page of the model list.
const LIST_PAGE_SIZE: &str = "1000";

pub(super) struct Google;

impl ProviderProtocol for Google {
    fn name(&self) -> &'static str {
        "google"
    }

    fn shown_name(&self) -> &'static str {
        "Google"
    }

    fn api_base(&self) -> String {
        api_base(
            "LATTICE_GOOGLE_BASE_URL",
            "https://generativelanguage.googleapis.com",
        )
    }

    fn completion_path(&self) -> &'static str {
        "/v1beta/openai/chat/completions"
    }

    fn sign(&self, request: RequestBuilder, key: &str) -> RequestBuilder {
        request.bearer_auth(key).header("x-goog-api-key", key)
    }

    /// The native API takes the key as a header of its own, and refuses a
    /// bearer token that is not OAuth.
    fn sign_management(&self, request: RequestBuilder, key: &str) -> RequestBuilder {
        request.header("x-goog-api-key", key)
    }

    /// Google thinks unless told not to. With no effort set nothing says
    /// whether the model does, so the ceiling is the model's and the port does
    /// not claim it reasons first.
    fn thinking(&self, _model: &str, request: &CompletionRequest) -> Thinking {
        match request.reasoning_effort.as_deref() {
            Some("none") => Thinking::Off,
            Some(_) => Thinking::On,
            None => Thinking::Unknown,
        }
    }

    fn body(&self, model: &str, fitted: &Fitted<'_>, stream: bool) -> Result<Value> {
        let request = fitted.request();
        let messages = chat_messages(&request.input)?;
        let model = model.strip_prefix("models/").unwrap_or(model);
        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": stream,
        });
        if let Some(max_output) = fitted.ceiling() {
            set_field(&mut body, "max_tokens", json!(max_output))?;
        }
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
        if !request.stop.is_empty() {
            set_field(&mut body, "stop", json!(request.stop))?;
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
    }

    fn parse_reply(&self, value: Value) -> Result<CompletionResponse> {
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
                    call_arguments(args_str)
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
        response.timing = timing::reported(&value);
        Ok(response)
    }

    fn event_decoder(&self) -> Box<dyn EventDecoder> {
        Box::new(stream::Events::default())
    }

    fn requires_ceiling(&self) -> bool {
        false
    }

    fn catalog_url(&self) -> &'static str {
        "https://generativelanguage.googleapis.com/v1beta/models"
    }

    fn list_request(
        &self,
        client: &Client,
        url: &str,
        key: &str,
        cursor: Option<&str>,
    ) -> RequestBuilder {
        let request = self
            .sign_management(client.get(url), key)
            .query(&[("pageSize", LIST_PAGE_SIZE)]);
        match cursor {
            Some(cursor) => request.query(&[("pageToken", cursor)]),
            None => request,
        }
    }

    fn parse_model_page(&self, value: Value) -> Result<ModelPage> {
        let page: Page =
            serde_json::from_value(value).map_err(|_| invalid_list(self.shown_name()))?;
        let models = page
            .models
            .into_iter()
            .filter(|model| {
                model
                    .supported_generation_methods
                    .iter()
                    .any(|method| method == "generateContent")
            })
            .filter(|model| is_text_model(&model.name))
            .map(|model| {
                let id = model
                    .name
                    .strip_prefix("models/")
                    .unwrap_or(&model.name)
                    .to_owned();
                CloudModelDto {
                    name: model
                        .display_name
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| id.clone()),
                    id,
                }
            })
            .collect();
        Ok(ModelPage {
            models,
            next: page.next_page_token,
        })
    }

    fn limits_api(&self) -> Option<LimitsApi> {
        Some(LimitsApi {
            version: "v1beta",
            parse: |reply| {
                let model: ReportedModel = serde_json::from_value(reply).ok()?;
                Some(ModelWindowDto {
                    context_window: model.input_token_limit.filter(|tokens| *tokens > 0),
                    max_output_tokens: model.output_token_limit.filter(|tokens| *tokens > 0),
                })
            },
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Model {
    name: String,
    display_name: Option<String>,
    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    #[serde(default)]
    models: Vec<Model>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportedModel {
    input_token_limit: Option<u32>,
    output_token_limit: Option<u32>,
}

/// Gemini's OpenAI-compatible Chat Completions messages. Calls made in one
/// round are one assistant message.
fn chat_messages(input: &[CompletionInput]) -> Result<Vec<Value>> {
    let mut messages: Vec<Value> = Vec::new();
    for item in input {
        match item {
            CompletionInput::Native { value } => messages.push(value.clone()),
            CompletionInput::Message { role, content } => {
                messages.push(json!({"role": role, "content": content}));
            }
            CompletionInput::Multimodal { role, parts } => {
                messages.push(json!({"role": role, "content": openai_content_parts(role, parts)?}));
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
                    if let Some(calls) = last.get_mut("tool_calls").and_then(Value::as_array_mut) {
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
    Ok(messages)
}
