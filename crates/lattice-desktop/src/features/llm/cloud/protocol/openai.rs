//! OpenAI's Responses API.

use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse, ContentPart,
};
use lattice_core::domain::models::model_ref::CloudProvider;
use lattice_core::shared::error::{AppError, Result};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::{json, Value};

use super::listing::{self, is_text_model};
use super::{
    api_base, insufficient_quota, CatalogProtocol, CompletionProtocol, EventDecoder, LimitsApi,
    LimitsProtocol, ModelPage, ProviderApi,
};
use crate::features::llm::cloud::{field, reply, set_field};
use crate::features::llm::engine::vision::user_parts;
use crate::features::llm::fitting::{Fitted, Thinking};

mod stream;

pub(super) struct OpenAi;

impl ProviderApi for OpenAi {
    fn provider(&self) -> CloudProvider {
        CloudProvider::Openai
    }

    fn api_base(&self) -> String {
        api_base("LATTICE_OPENAI_BASE_URL", "https://api.openai.com")
    }

    fn sign_management(&self, request: RequestBuilder, key: &str) -> RequestBuilder {
        request.bearer_auth(key)
    }

    fn out_of_credit(&self, status: StatusCode, reply: &Value) -> bool {
        status == StatusCode::TOO_MANY_REQUESTS && insufficient_quota(reply)
    }
}

impl CompletionProtocol for OpenAi {
    fn completion_path(&self) -> &'static str {
        "/v1/responses"
    }

    fn sign(&self, request: RequestBuilder, key: &str) -> RequestBuilder {
        request.bearer_auth(key)
    }

    fn thinking(&self, model: &str, _request: &CompletionRequest) -> Thinking {
        if is_reasoning_model(model) {
            Thinking::On
        } else {
            Thinking::Off
        }
    }

    fn body(&self, model: &str, fitted: &Fitted<'_>, stream: bool) -> Result<Value> {
        let request = fitted.request();
        let reasons = fitted.thinking().is_on();
        let input = input_items(&request.input)?;
        let mut body = json!({"model":model,"input":input,"stream":stream,"store":false,"include":["reasoning.encrypted_content"]});
        if let Some(max_output) = fitted.ceiling() {
            set_field(&mut body, "max_output_tokens", json!(max_output))?;
        }
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
        // The Responses API takes no stop sequences, so `request.stop` is
        // not sent; the output ceiling above still bounds the answer.
        // `request.thinking` is deliberately not read here: an OpenAI
        // reasoning model always reasons and a chat model never does, so
        // the effort level below is the only control the API offers.
        let effort = reasoning_effort(model, request.reasoning_effort.as_deref());
        if reasons && (effort.is_some() || request.include_reasoning) {
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
        if let Some(sampling) = request.sampling.filter(|_| !reasons) {
            if let Some(temperature) = sampling.temperature {
                set_field(&mut body, "temperature", json!(temperature))?;
            }
            if let Some(top_p) = sampling.top_p {
                set_field(&mut body, "top_p", json!(top_p))?;
            }
        }
        Ok(body)
    }

    fn parse_reply(&self, value: Value) -> Result<CompletionResponse> {
        let mut response = CompletionResponse::default();
        let mut reasoning_parts = Vec::new();
        let blocks = field(&value, "output")
            .as_array()
            .ok_or_else(|| AppError::Network("Missing cloud response content".into()))?;
        for block in blocks {
            match field(block, "type").as_str().unwrap_or("") {
                "message" => reply::push_message_text(&mut response.text, block),
                "text" => reply::push_block_text(&mut response.text, block),
                "reasoning" => {
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
                "function_call" => {
                    let id = field(block, "call_id")
                        .as_str()
                        .ok_or_else(|| AppError::Network("Missing tool call ID".into()))?;
                    let name = field(block, "name")
                        .as_str()
                        .ok_or_else(|| AppError::Network("Missing tool name".into()))?;
                    response.tool_calls.push(CompletionInput::ToolCall {
                        id: id.into(),
                        name: name.into(),
                        arguments: reply::call_arguments(
                            field(block, "arguments").as_str().unwrap_or(""),
                        ),
                    });
                }
                _ => {}
            }
        }
        let input_tokens = field(field(&value, "usage"), "input_tokens")
            .as_u64()
            .unwrap_or(0);
        reply::finish(
            &mut response,
            &value,
            "status",
            input_tokens,
            &reasoning_parts,
        );
        // OpenAI replays its output items one by one.
        response.replay = blocks
            .iter()
            .cloned()
            .map(|value| CompletionInput::Native { value })
            .collect();
        Ok(response)
    }

    fn event_decoder(&self) -> Box<dyn EventDecoder> {
        Box::new(stream::Events::default())
    }

    /// The Responses API counts the cached tokens within `input_tokens`.
    fn prompt_tokens_read(&self, reply: &Value) -> u64 {
        let usage = field(reply, "usage");
        field(usage, "input_tokens")
            .as_u64()
            .unwrap_or(0)
            .saturating_sub(
                field(field(usage, "input_tokens_details"), "cached_tokens")
                    .as_u64()
                    .unwrap_or(0),
            )
    }
}

impl CatalogProtocol for OpenAi {
    fn catalog_url(&self) -> &'static str {
        "https://api.openai.com/v1/models"
    }

    fn list_request(
        &self,
        client: &Client,
        url: &str,
        key: &str,
        _cursor: Option<&str>,
    ) -> RequestBuilder {
        self.sign_management(client.get(url), key)
    }

    fn parse_model_page(&self, value: Value) -> Result<ModelPage> {
        let mut page = listing::api_page(value, self.shown_name())?;
        page.data
            .sort_by(|a, b| b.created.cmp(&a.created).then_with(|| a.id.cmp(&b.id)));
        let today = listing::today();
        let models = page
            .data
            .into_iter()
            .filter(|model| is_chat_model(&model.id))
            .filter(|model| model.is_offered(&today))
            .map(listing::ApiModel::into_dto)
            .collect();
        Ok(ModelPage { models, next: None })
    }
}

impl LimitsProtocol for OpenAi {
    fn requires_ceiling(&self) -> bool {
        false
    }

    /// OpenAI's model object carries neither a window nor an output ceiling.
    fn limits_api(&self) -> Option<LimitsApi> {
        None
    }
}

/// Whether this OpenAI model is a reasoning model, which takes an effort
/// setting in place of sampling knobs and rejects `temperature`/`top_p`.
fn is_reasoning_model(model: &str) -> bool {
    let model = base_model(model);
    model.starts_with("gpt-5") || model.starts_with("gpt-6") || model.starts_with('o')
}

/// A fine-tune answers as the model it was tuned from: `ft:<base>:<org>:...`.
fn base_model(id: &str) -> &str {
    id.strip_prefix("ft:")
        .and_then(|id| id.split(':').next())
        .unwrap_or(id)
}

/// What, if anything, belongs in the Responses API's `reasoning` block. Only
/// models with reasoning controls get one: others reject the whole request
/// with a 400. There is no "off": a request for none is clamped to `minimal`
/// (gpt-5) or `low` (o-series).
fn reasoning_effort<'a>(model: &str, effort: Option<&'a str>) -> Option<&'a str> {
    let effort = effort?;
    let model = base_model(model);
    let floor = if model.starts_with("gpt-5") || model.starts_with("gpt-6") {
        "minimal"
    } else if model.starts_with('o') {
        "low"
    } else {
        return None;
    };
    Some(if effort == "none" { floor } else { effort })
}

// OpenAI's model list has no endpoint capabilities. Recognize chat families,
// including fine tunes, and exclude specialized audio/image/search endpoints.
fn is_chat_model(id: &str) -> bool {
    let base = base_model(id);
    (base.starts_with("gpt-")
        || base.starts_with("chatgpt-")
        || base.starts_with("chat-")
        || base
            .strip_prefix('o')
            .is_some_and(|suffix| suffix.starts_with(|c: char| c.is_ascii_digit())))
        && is_text_model(base)
}

/// OpenAI Responses `input` items.
fn input_items(input: &[CompletionInput]) -> Result<Vec<Value>> {
    input
        .iter()
        .map(|item| {
            Ok(match item {
                CompletionInput::Native { value } => value.clone(),
                CompletionInput::Message { role, content } => {
                    json!({"role":role,"content":content})
                }
                CompletionInput::Multimodal { role, parts } => {
                    let content = user_parts(role, parts, |part| match part {
                        ContentPart::Text { text } => json!({"type":"input_text","text":text}),
                        ContentPart::Image(image) => json!({
                            "type":"input_image",
                            "image_url":image.data_uri(),
                            "detail":"auto"
                        }),
                    })?;
                    json!({"role":role,"content":content})
                }
                CompletionInput::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    json!({"type":"function_call","call_id":id,"name":name,"arguments":arguments.to_string()})
                }
                CompletionInput::ToolResult { id, output } => {
                    json!({"type":"function_call_output","call_id":id,"output":output})
                }
            })
        })
        .collect()
}
