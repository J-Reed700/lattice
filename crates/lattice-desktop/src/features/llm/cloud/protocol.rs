//! A cloud provider's wire protocol, one implementation per provider. The
//! shared flow in `CloudLlm`, the model catalog and the limits lookup run
//! against this trait and never ask which provider they are talking to.

use lattice_core::{
    application::{
        contracts::settings::{LLMProvider, ModelWindowDto},
        ports::llm_port::{CompletionRequest, CompletionResponse, ContentPart},
    },
    domain::models::model_ref::CloudProvider,
    shared::error::{AppError, Result},
};
use reqwest::{Client, RequestBuilder};
use serde_json::Value;

use super::catalog::CloudModelDto;
use super::streaming::Callback;
use crate::features::llm::engine::vision::pictures_outside_a_user_message;

pub(super) mod anthropic;
mod google;
mod listing;
mod openai;

/// The output ceilings a request could be sent with, each worked out only
/// when the protocol asks for it.
pub(super) trait Rooms {
    /// The ceiling a provider that does not require one is sent, if any.
    fn optional(&self) -> Option<u32>;
    /// The ceiling for a reply with no thinking in it.
    fn answer(&self) -> u32;
    /// The ceiling for a reply that is thought through first.
    fn thinking(&self) -> u32;
}

/// One page of a provider's model list.
pub(super) struct ModelPage {
    pub(super) models: Vec<CloudModelDto>,
    /// The cursor of the next page, when there is one.
    pub(super) next: Option<String>,
}

/// Where a provider reports a model's limits and how to read the report.
pub(super) struct LimitsApi {
    /// The API version segment ahead of `models` in the model's URL.
    pub(super) version: &'static str,
    /// The window and output ceiling in a model object; `None` for a reply
    /// that is not one.
    pub(super) parse: fn(Value) -> Option<ModelWindowDto>,
}

/// Decodes one provider's streamed events into its completed response.
pub(super) trait EventDecoder: Send {
    /// One server-sent event. `kind` is the event's `type`, or empty.
    fn event(
        &mut self,
        event: &Value,
        kind: &str,
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<()>;
    /// Whether the terminal event has arrived.
    fn is_complete(&self) -> bool;
    /// Whether a tool call has started streaming its arguments, which counts
    /// as the answer having started, as text does.
    fn has_tool_output(&self) -> bool;
    /// The completed response in the provider's own shape, once the stream
    /// has ended.
    fn finish(self: Box<Self>) -> Result<Option<Value>>;
}

/// Every fact about one provider's API.
pub(super) trait ProviderProtocol: Send + Sync {
    /// The provider's name in logs.
    fn name(&self) -> &'static str;
    /// The provider's name as the user knows it.
    fn shown_name(&self) -> &'static str;
    /// The API root. An evaluation run sends every request through a local
    /// recording proxy named by an environment variable instead.
    fn api_base(&self) -> String;
    /// The completion endpoint's path under the API root.
    fn completion_path(&self) -> &'static str;
    /// `request` signed with `key` for the completion API.
    fn sign(&self, request: RequestBuilder, key: &str) -> RequestBuilder;
    /// `request` signed with `key` for the model list and model objects.
    fn sign_management(&self, request: RequestBuilder, key: &str) -> RequestBuilder;
    /// The request body for `request` on `model`.
    fn body(
        &self,
        model: &str,
        request: &CompletionRequest,
        stream: bool,
        rooms: &dyn Rooms,
    ) -> Result<Value>;
    /// A whole reply, streamed or not, read back into a completion.
    fn parse_reply(&self, value: Value) -> Result<CompletionResponse>;
    /// A fresh decoder for one streamed reply.
    fn event_decoder(&self) -> Box<dyn EventDecoder>;
    /// Whether `model` thinks with no request asking.
    fn thinks_unasked(&self, model: &str) -> bool;
    /// Whether the API refuses a request that sets no output ceiling.
    fn requires_ceiling(&self) -> bool;
    /// The error for a 400 that refused the structured-output format, when
    /// this provider's handling recognises `reason` as that.
    fn format_refusal(&self, _body: &Value, _reason: &str) -> Option<AppError> {
        None
    }
    /// The body to resend after `error` refused `body`, when the provider
    /// has a fallback for it.
    fn resend_after(&self, _error: &AppError, _body: &Value) -> Option<Value> {
        None
    }
    /// The model list's public URL.
    fn catalog_url(&self) -> &'static str;
    /// The request for one page of the model list at `url`.
    fn list_request(
        &self,
        client: &Client,
        url: &str,
        key: &str,
        cursor: Option<&str>,
    ) -> RequestBuilder;
    /// One page of the model list read back.
    fn parse_model_page(&self, value: Value) -> Result<ModelPage>;
    /// Where this provider reports model limits; `None` when it reports none.
    fn limits_api(&self) -> Option<LimitsApi>;
}

static OPENAI: openai::OpenAi = openai::OpenAi;
static ANTHROPIC: anthropic::Anthropic = anthropic::Anthropic;
static GOOGLE: google::Google = google::Google;

/// The protocol of a cloud provider: the only place the cloud code tells
/// providers apart.
pub(super) fn protocol_for(provider: LLMProvider) -> Result<&'static dyn ProviderProtocol> {
    match provider {
        LLMProvider::Openai => Ok(&OPENAI),
        LLMProvider::Anthropic => Ok(&ANTHROPIC),
        LLMProvider::Google => Ok(&GOOGLE),
        _ => Err(AppError::InvalidConfig(
            "Expected an explicit cloud provider".into(),
        )),
    }
}

/// [`protocol_for`], for a provider already known to be a cloud one.
pub(super) fn protocol_for_cloud(provider: CloudProvider) -> &'static dyn ProviderProtocol {
    match provider {
        CloudProvider::Openai => &OPENAI,
        CloudProvider::Anthropic => &ANTHROPIC,
        CloudProvider::Google => &GOOGLE,
    }
}

/// The provider's API root: `variable` names a recording proxy that stands
/// in for the `public` host.
fn api_base(variable: &str, public: &str) -> String {
    std::env::var(variable)
        .ok()
        .map(|base| base.trim().trim_end_matches('/').to_owned())
        .filter(|base| !base.is_empty())
        .unwrap_or_else(|| public.to_owned())
}

/// A user message's parts in a provider's own part format, in the order
/// given. Pictures in any other role are refused.
fn user_parts(
    role: &str,
    parts: &[ContentPart],
    part: impl Fn(&ContentPart) -> Value,
) -> Result<Vec<Value>> {
    if role != "user" {
        return Err(pictures_outside_a_user_message(role));
    }
    Ok(parts.iter().map(part).collect())
}
