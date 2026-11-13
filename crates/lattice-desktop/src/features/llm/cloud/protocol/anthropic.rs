//! Anthropic's Messages API.

use lattice_core::application::{
    contracts::settings::ModelWindowDto,
    ports::llm_port::{
        CompletionInput, CompletionRequest, CompletionResponse, ContentPart, ThinkingMode,
        ToolChoice,
    },
};
use lattice_core::domain::models::model_ref::CloudProvider;
use lattice_core::shared::error::{AppError, Result};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};

use super::listing;
use super::{
    api_base, CatalogProtocol, CompletionProtocol, EventDecoder, LimitsApi, LimitsProtocol,
    ModelPage, ProviderApi,
};
use crate::features::llm::cloud::{field, reply, set_field};
use crate::features::llm::engine::vision::user_parts;
use crate::features::llm::fitting::{Fitted, Thinking};

pub(in crate::features::llm::cloud) mod schema;
mod stream;

/// The API version every Anthropic request names.
const API_VERSION: &str = "2023-06-01";

/// Models per page of the model list.
const LIST_PAGE_SIZE: &str = "100";

pub(super) struct Anthropic;

impl ProviderApi for Anthropic {
    fn provider(&self) -> CloudProvider {
        CloudProvider::Anthropic
    }

    fn api_base(&self) -> String {
        api_base("LATTICE_ANTHROPIC_BASE_URL", "https://api.anthropic.com")
    }

    fn sign_management(&self, request: RequestBuilder, key: &str) -> RequestBuilder {
        self.sign(request, key)
    }

    /// An account out of credit is refused with a 400, not a 429.
    fn out_of_credit(&self, status: StatusCode, reply: &Value) -> bool {
        status == StatusCode::BAD_REQUEST
            && reply
                .pointer("/error/message")
                .and_then(Value::as_str)
                .is_some_and(|message| message.to_ascii_lowercase().contains("credit balance"))
    }

    /// The monthly spend limit is a 429 whose error details carry a code.
    fn spend_limit_reached(&self, status: StatusCode, reply: &Value) -> bool {
        status == StatusCode::TOO_MANY_REQUESTS
            && reply
                .pointer("/error/details/error_code")
                .and_then(Value::as_str)
                == Some("enforced_spend_limit_reached")
    }
}

impl CompletionProtocol for Anthropic {
    fn completion_path(&self) -> &'static str {
        "/v1/messages"
    }

    fn sign(&self, request: RequestBuilder, key: &str) -> RequestBuilder {
        request
            .header("x-api-key", key)
            .header("anthropic-version", API_VERSION)
    }

    fn thinking(&self, model: &str, request: &CompletionRequest) -> Thinking {
        let effort = request.reasoning_effort.as_deref();
        let asked = request.thinking == ThinkingMode::On;
        // A request for no thinking at all turns it off where the model
        // allows that; elsewhere it thinks at the lowest effort.
        let turned_off = effort == Some("none") && !asked && claude_thinking_off(model).is_some();
        let thinks = !turned_off
            && (claude_thinks_unasked(model)
                || asked
                || effort.is_some_and(|effort| effort != "none"));
        if thinks {
            Thinking::On
        } else {
            Thinking::Off
        }
    }

    fn body(&self, model: &str, fitted: &Fitted<'_>, stream: bool) -> Result<Value> {
        let request = fitted.request();
        let (system, messages) = messages(&request.input)?;
        // No sampling knobs are sent: current Claude models reject `temperature`,
        // `top_p` and `top_k` with a 400 (some for any non-default value, all
        // while thinking). Claude thinks unasked from the 5 family on, and "none"
        // is expressed by leaving the thinking block out, since asking for
        // adaptive thinking means think.
        let effort = request.reasoning_effort.as_deref();
        let max_tokens = fitted.ceiling().ok_or_else(|| {
            AppError::InvalidState("Anthropic needs a reply ceiling and none was set".into())
        })?;
        let mut body =
            json!({"model":model,"messages":messages,"max_tokens":max_tokens,"stream":stream});
        if !system.is_empty() {
            set_field(&mut body, "system", json!(system.join("\n\n")))?;
        }
        if !request.stop.is_empty() {
            set_field(&mut body, "stop_sequences", json!(request.stop))?;
        }
        // A block asks for the thinking; on a model that thinks unasked
        // it is the same request, and says how its thinking is shown.
        if fitted.thinking().is_on() {
            // Thinking is not shown unless asked for: summaries are what a
            // caller that displays reasoning gets back.
            let block = if request.include_reasoning {
                json!({"type":"adaptive","display":"summarized"})
            } else {
                json!({"type":"adaptive"})
            };
            set_field(&mut body, "thinking", block)?;
        } else if effort == Some("none") {
            if let Some(off) = claude_thinking_off(model) {
                set_field(&mut body, "thinking", off)?;
            }
        }
        if let Some(effort) = effort {
            // Anthropic has no "none" effort of its own, so the answer
            // itself is held at the lowest setting instead.
            let effort = if effort == "none" { "low" } else { effort };
            set_field(&mut body, "output_config", json!({"effort":effort}))?;
        }
        if !request.tools.is_empty() {
            set_field(&mut body, "tools", json!(request.tools.iter().map(|t| json!({"name":t.name,"description":t.description,"input_schema":t.parameters})).collect::<Vec<_>>()))?;
            // A forced call is never sent: models refuse one while extended thinking
            // is on, and the newest refuse it outright with a 400, so the choice is
            // left to the model.
            let choice = match request.tool_choice {
                Some(ToolChoice::Auto | ToolChoice::Required) => Some("auto"),
                Some(ToolChoice::None) => Some("none"),
                None => None,
            };
            if let Some(choice) = choice {
                set_field(&mut body, "tool_choice", json!({"type": choice}))?;
            }
        }
        if let Some(schema) = &request.json_schema {
            let schema = schema::anthropic_schema(schema);
            let compiles = schema::anthropic_schema_compiles(&schema);
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
            if !compiles {
                body = schema::in_words(body);
            }
        }
        // Requests that share a cache key share a prompt prefix, so Claude is
        // asked to cache it; a one-off request is not, since a cache write costs
        // more than a plain input token.
        if request.cache_key.is_some() {
            body = with_cache_breakpoints(body, continues_an_exchange(&request.input))?;
        }
        Ok(body)
    }

    fn parse_reply(&self, value: Value) -> Result<CompletionResponse> {
        let mut response = CompletionResponse::default();
        let mut reasoning_parts = Vec::new();
        let blocks = field(&value, "content")
            .as_array()
            .ok_or_else(|| AppError::Network("Missing cloud response content".into()))?;
        for block in blocks {
            match field(block, "type").as_str().unwrap_or("") {
                "message" => reply::push_message_text(&mut response.text, block),
                "text" => reply::push_block_text(&mut response.text, block),
                "thinking" => {
                    if let Some(thinking) = field(block, "thinking")
                        .as_str()
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                    {
                        reasoning_parts.push(thinking.to_owned());
                    }
                }
                "tool_use" => {
                    let id = field(block, "id")
                        .as_str()
                        .ok_or_else(|| AppError::Network("Missing tool call ID".into()))?;
                    let name = field(block, "name")
                        .as_str()
                        .ok_or_else(|| AppError::Network("Missing tool name".into()))?;
                    response.tool_calls.push(CompletionInput::ToolCall {
                        id: id.into(),
                        name: name.into(),
                        arguments: field(block, "input").clone(),
                    });
                }
                _ => {}
            }
        }
        // The whole prompt: cache reads and writes are counted apart from
        // `input_tokens`.
        let input_tokens: u64 = [
            "input_tokens",
            "cache_creation_input_tokens",
            "cache_read_input_tokens",
        ]
        .iter()
        .filter_map(|key| field(field(&value, "usage"), key).as_u64())
        .sum();
        reply::finish(
            &mut response,
            &value,
            "stop_reason",
            input_tokens,
            &reasoning_parts,
        );
        if response.finish_reason == "refusal" {
            // Claude's safety classifiers decline with a 200 and name the one
            // that did; the reply itself says nothing worth logging.
            tracing::warn!(
                category = field(field(&value, "stop_details"), "category")
                    .as_str()
                    .unwrap_or("unnamed"),
                "The model declined the request"
            );
        }
        // The content blocks replay as the assistant message they were,
        // signed thinking included.
        response.replay = vec![CompletionInput::Native {
            value: json!({"role": "assistant", "content": field(&value, "content")}),
        }];
        Ok(response)
    }

    fn event_decoder(&self) -> Box<dyn EventDecoder> {
        Box::new(stream::Events::default())
    }

    /// Cache reads are counted apart from `input_tokens`, and cache writes
    /// too, though the backend read those like any others.
    fn prompt_tokens_read(&self, reply: &Value) -> u64 {
        let usage = field(reply, "usage");
        field(usage, "input_tokens").as_u64().unwrap_or(0)
            + field(usage, "cache_creation_input_tokens")
                .as_u64()
                .unwrap_or(0)
    }

    fn format_refusal(&self, body: &Value, reason: &str) -> Option<AppError> {
        schema::log_refusal(body, reason);
        schema::asks_for_format(body)
            .then(|| schema::refusal(reason))
            .flatten()
    }

    /// Claude refuses to compile a schema it finds too large, which cannot be
    /// told from its shape: the schema is asked for in words instead.
    fn resend_after(&self, error: &AppError, body: &Value) -> Option<Value> {
        schema::is_refusal(error).then(|| {
            tracing::info!("Claude refused the response schema; asking for it in words");
            schema::in_words(body.clone())
        })
    }
}

impl CatalogProtocol for Anthropic {
    fn catalog_url(&self) -> &'static str {
        "https://api.anthropic.com/v1/models"
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
            .query(&[("limit", LIST_PAGE_SIZE)]);
        match cursor {
            Some(cursor) => request.query(&[("after_id", cursor)]),
            None => request,
        }
    }

    fn parse_model_page(&self, value: Value) -> Result<ModelPage> {
        let page = listing::api_page(value, self.shown_name())?;
        let next = if page.has_more {
            Some(page.last_id.filter(|id| !id.is_empty()).ok_or_else(|| {
                AppError::Network(format!(
                    "{} returned an invalid model-list cursor",
                    self.shown_name()
                ))
            })?)
        } else {
            None
        };
        let today = listing::today();
        let models = page
            .data
            .into_iter()
            .filter(|model| model.is_offered(&today))
            .map(listing::ApiModel::into_dto)
            .collect();
        Ok(ModelPage { models, next })
    }
}

impl LimitsProtocol for Anthropic {
    /// The Messages API refuses a request without `max_tokens`.
    fn requires_ceiling(&self) -> bool {
        true
    }

    fn limits_api(&self) -> Option<LimitsApi> {
        Some(LimitsApi {
            version: "v1",
            parse: |reply| {
                let model: ReportedModel = serde_json::from_value(reply).ok()?;
                Some(ModelWindowDto {
                    context_window: model.max_input_tokens.filter(|tokens| *tokens > 0),
                    max_output_tokens: model.max_tokens.filter(|tokens| *tokens > 0),
                })
            },
        })
    }
}

#[derive(Deserialize)]
struct ReportedModel {
    max_input_tokens: Option<u32>,
    max_tokens: Option<u32>,
}

/// How a Claude model is asked not to think, where it can be: Sonnet 5.5
/// takes `between_tools` and refuses `disabled`; Haiku 5.5, Opus 5 and
/// Sonnet 5 take `disabled`. Opus 5.5 and Fable cannot turn thinking off.
fn claude_thinking_off(model: &str) -> Option<Value> {
    let mut parts = model.strip_prefix("claude-")?.split('-');
    let family = parts.next()?;
    let version: Vec<&str> = parts
        .take_while(|part| part.len() <= 2 && part.chars().all(|c| c.is_ascii_digit()))
        .collect();
    match (family, version.as_slice()) {
        ("sonnet", ["5", "5"]) => Some(json!({"type": "between_tools"})),
        ("haiku", ["5", "5"]) | ("opus", ["5"]) | ("sonnet", ["5"]) => {
            Some(json!({"type": "disabled"}))
        }
        _ => None,
    }
}

/// Whether this Claude model thinks with no `thinking` block asking: every
/// model from the 5 family on, where omitting the block runs adaptive
/// thinking (and Opus 5.5 refuses to have it turned off). The family is the
/// first number in the id, so `claude-opus-4-8` and `claude-3-5-sonnet` do
/// not and `claude-sonnet-5-5` does.
fn claude_thinks_unasked(model: &str) -> bool {
    model
        .strip_prefix("claude-")
        .and_then(|rest| rest.split('-').find_map(|part| part.parse::<u32>().ok()))
        .is_some_and(|family| family >= 5)
}

/// The system prompt is what every call of one step shares, so it is cached on
/// its own. The whole prompt is cached only when the request continues an
/// exchange, since the next request repeats it and adds to it; a single
/// exchange ends in its own task, and a write of that would never be read.
fn with_cache_breakpoints(mut body: Value, continues: bool) -> Result<Value> {
    if let Some(system) = body
        .get("system")
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        set_field(
            &mut body,
            "system",
            json!([{"type": "text", "text": system, "cache_control": {"type": "ephemeral"}}]),
        )?;
    }
    if continues {
        set_field(&mut body, "cache_control", json!({"type": "ephemeral"}))?;
    }
    Ok(body)
}

fn continues_an_exchange(input: &[CompletionInput]) -> bool {
    input.iter().any(|item| match item {
        CompletionInput::Message { role, .. } | CompletionInput::Multimodal { role, .. } => {
            role == "assistant"
        }
        CompletionInput::ToolCall { .. } | CompletionInput::ToolResult { .. } => true,
        CompletionInput::Native { value } => {
            value.get("role").and_then(Value::as_str) == Some("assistant")
        }
    })
}

/// Anthropic Messages: the system and developer instructions, then the
/// messages. Tool results that follow one another share one user message.
fn messages(input: &[CompletionInput]) -> Result<(Vec<String>, Vec<Value>)> {
    let mut system = Vec::new();
    let mut messages: Vec<Value> = Vec::new();
    for item in input {
        match item {
            CompletionInput::Native { value } => messages.push(value.clone()),
            CompletionInput::Message { role, content } if role == "system" || role == "developer" => {
                system.push(content.clone())
            }
            CompletionInput::Message { role, content } => {
                messages.push(json!({"role":role,"content":content}))
            }
            CompletionInput::Multimodal { role, parts } => {
                let content = user_parts(role, parts, |part| match part {
                    ContentPart::Text { text } => json!({"type":"text","text":text}),
                    ContentPart::Image(image) => json!({
                        "type":"image",
                        "source":{"type":"base64","media_type":image.mime,"data":image.data_base64}
                    }),
                })?;
                messages.push(json!({"role":role,"content":content}))
            }
            CompletionInput::ToolCall {
                id,
                name,
                arguments,
            } => messages.push(
                json!({"role":"assistant","content":[{"type":"tool_use","id":id,"name":name,"input":arguments}]}),
            ),
            CompletionInput::ToolResult { id, output } => {
                let result = json!({"type":"tool_result","tool_use_id":id,"content":output});
                if let Some(last) = messages
                    .last_mut()
                    .filter(|m| field(m, "role") == "user" && field(m, "content").is_array())
                {
                    if let Some(content) = last.get_mut("content").and_then(Value::as_array_mut) {
                        content.push(result);
                    }
                } else {
                    messages.push(json!({"role":"user","content":[result]}));
                }
            }
        }
    }
    Ok((system, messages))
}
