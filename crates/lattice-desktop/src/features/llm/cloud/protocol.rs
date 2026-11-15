//! A cloud provider's wire protocol, one implementation per provider and one
//! trait per concern. The shared flow in `CloudLlm`, the model catalog and
//! the limits lookup each run against the concern they use and never ask
//! which provider they are talking to.

use lattice_core::{
    application::{
        contracts::settings::{LLMProvider, ModelWindowDto},
        ports::llm_port::{CompletionRequest, CompletionResponse},
        services::chat_model_resolution::{cloud_provider_id, cloud_provider_name},
    },
    domain::models::model_ref::CloudProvider,
    shared::error::{AppError, Result},
};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::Value;

use super::catalog::CloudModelDto;
use super::streaming::Callback;
use crate::features::llm::fitting::{Fitted, Thinking};

pub(super) mod anthropic;
mod google;
mod listing;
mod openai;

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
    /// Whether an event of this `kind` says the stream failed.
    fn is_failure(&self, kind: &str) -> bool;
    /// Whether a tool call has started streaming its arguments, which counts
    /// as the answer having started, as text does.
    fn has_tool_output(&self) -> bool;
    /// Whether the model has begun its answer: an output of any kind has
    /// started, shown or not (thinking it does not stream, text, a tool call).
    fn answer_begun(&self) -> bool;
    /// The completed response in the provider's own shape, once the stream
    /// has ended.
    fn finish(self: Box<Self>) -> Result<Option<Value>>;
}

/// What the shared flow, the catalog and the limits lookup all need to know
/// about a provider's API.
pub(super) trait ProviderApi: Send + Sync {
    fn provider(&self) -> CloudProvider;
    /// The API root. An evaluation run sends every request through a local
    /// recording proxy named by an environment variable instead.
    fn api_base(&self) -> String;
    /// `request` signed with `key` for the model list and model objects.
    fn sign_management(&self, request: RequestBuilder, key: &str) -> RequestBuilder;
    /// Whether a refusal with `status` and body `reply` (Null when the body
    /// was not read or not JSON) says the account has no credit or quota left.
    fn out_of_credit(&self, status: StatusCode, reply: &Value) -> bool;
    /// Whether a refusal with `status` and body `reply` says the account
    /// reached a spend limit it set, which no wait lifts before it resets.
    /// A provider with no such limit answers false.
    fn spend_limit_reached(&self, _status: StatusCode, _reply: &Value) -> bool {
        false
    }
    /// The provider's name in logs.
    fn name(&self) -> &'static str {
        cloud_provider_id(self.provider())
    }
    /// The provider's name as the user knows it.
    fn shown_name(&self) -> &'static str {
        cloud_provider_name(self.provider())
    }
}

/// Sending a completion and reading its reply.
pub(super) trait CompletionProtocol: ProviderApi {
    /// The completion endpoint's path under the API root.
    fn completion_path(&self) -> &'static str;
    /// `request` signed with `key` for the completion API.
    fn sign(&self, request: RequestBuilder, key: &str) -> RequestBuilder;
    /// Whether `model` thinks through `request` before answering: the one
    /// answer the body, the reply ceiling and the port's reasoning flag read.
    fn thinking(&self, model: &str, request: &CompletionRequest) -> Thinking;
    /// The request body for the fitted request on `model`.
    fn body(&self, model: &str, fitted: &Fitted<'_>, stream: bool) -> Result<Value>;
    /// A whole reply, streamed or not, read back into a completion.
    fn parse_reply(&self, value: Value) -> Result<CompletionResponse>;
    /// A fresh decoder for one streamed reply.
    fn event_decoder(&self) -> Box<dyn EventDecoder>;
    /// The prompt tokens a completed reply says the backend read, cached ones
    /// excluded.
    fn prompt_tokens_read(&self, reply: &Value) -> u64;
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
}

/// Listing a provider's models.
pub(super) trait CatalogProtocol: ProviderApi {
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
}

/// Asking a provider what a model can take.
pub(super) trait LimitsProtocol: ProviderApi {
    /// Whether the API refuses a request that sets no output ceiling.
    fn requires_ceiling(&self) -> bool;
    /// Where this provider reports model limits; `None` when it reports none.
    fn limits_api(&self) -> Option<LimitsApi>;
}

/// One provider's protocol, by concern.
pub(super) struct Protocols {
    pub(super) completion: &'static dyn CompletionProtocol,
    pub(super) catalog: &'static dyn CatalogProtocol,
    pub(super) limits: &'static dyn LimitsProtocol,
}

/// The protocols of a cloud provider: the only place the cloud code tells
/// providers apart.
pub(super) fn protocols(provider: CloudProvider) -> Protocols {
    match provider {
        CloudProvider::Openai => Protocols {
            completion: &openai::OpenAi,
            catalog: &openai::OpenAi,
            limits: &openai::OpenAi,
        },
        CloudProvider::Anthropic => Protocols {
            completion: &anthropic::Anthropic,
            catalog: &anthropic::Anthropic,
            limits: &anthropic::Anthropic,
        },
        CloudProvider::Google => Protocols {
            completion: &google::Google,
            catalog: &google::Google,
            limits: &google::Google,
        },
    }
}

/// `provider` as a cloud provider; any other is not one.
pub(super) fn cloud_provider(provider: LLMProvider) -> Result<CloudProvider> {
    match provider {
        LLMProvider::Openai => Ok(CloudProvider::Openai),
        LLMProvider::Anthropic => Ok(CloudProvider::Anthropic),
        LLMProvider::Google => Ok(CloudProvider::Google),
        _ => Err(AppError::InvalidConfig(
            "Expected an explicit cloud provider".into(),
        )),
    }
}

/// Whether a refusal's body says the account has no quota left, by its
/// error's type or code. Both OpenAI and Google answer in this shape.
fn insufficient_quota(reply: &Value) -> bool {
    ["/error/type", "/error/code"]
        .iter()
        .any(|field| reply.pointer(field).and_then(Value::as_str) == Some("insufficient_quota"))
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
