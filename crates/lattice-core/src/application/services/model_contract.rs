//! The one contract of a model call: its identity, the single place it is
//! attached to the request, and the single rule for a reply that cannot be
//! used (one larger send for a cut-off, one correction, then unusable).

use crate::application::ports::llm_port::{
    CacheHold, CompletionInput, CompletionRequest, CompletionResponse, Finish, LLMPort, Pace,
    RequestLoad,
};
use crate::domain::models::vision::VisionGeometry;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod tests;

/// What the correction turn says before the reason the reply was refused.
pub const CORRECTION_LEAD: &str = "That answer could not be used:";

const CUT_OFF: &str = "The model response was cut off before the learning material was complete. Try again with a smaller outline or lesson.";

const DECLINED: &str = "The model declined to write this learning material. Try again, or choose another model in Settings → AI.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ContractId(&'static str);

impl ContractId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Step {
    Answer,
    Draft,
}

/// A model call's identity and the typed input its prompt is built from.
#[derive(Debug, Clone, PartialEq)]
pub struct ContractCall {
    pub id: ContractId,
    pub step: Step,
    pub input: serde_json::Value,
}

pub trait Contract {
    const ID: ContractId;
    type Input: Serialize + DeserializeOwned;
}

/// The input of a contract whose prompt is built only from text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptInput {
    pub system: String,
    pub prompt: String,
}

impl ContractCall {
    pub fn of<C: Contract>(input: &C::Input) -> Result<Self> {
        let input = serde_json::to_value(input).map_err(|error| {
            AppError::InternalError(format!(
                "The input of the {} call could not be recorded: {error}",
                C::ID.as_str()
            ))
        })?;
        Ok(Self {
            id: C::ID,
            step: Step::Answer,
            input,
        })
    }

    pub fn draft(&self) -> Self {
        Self {
            step: Step::Draft,
            ..self.clone()
        }
    }

    pub fn digest(&self) -> String {
        let bytes = serde_json::to_vec(&(self.step, &self.input)).unwrap_or_default();
        hex::encode(Sha256::digest(bytes))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn input_as<C: Contract>(&self) -> Option<C::Input> {
        (self.id == C::ID)
            .then(|| serde_json::from_value(self.input.clone()).ok())
            .flatten()
    }
}

struct Tagged<'a> {
    llm: &'a dyn LLMPort,
    call: &'a ContractCall,
}

/// `llm` with `call` attached to every request sent through it.
pub fn tagged<'a>(llm: &'a dyn LLMPort, call: &'a ContractCall) -> impl LLMPort + 'a {
    Tagged { llm, call }
}

impl Tagged<'_> {
    fn attach(&self, request: &CompletionRequest) -> CompletionRequest {
        CompletionRequest {
            contract: Some(self.call.clone()),
            ..request.clone()
        }
    }
}

#[async_trait]
impl LLMPort for Tagged<'_> {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.llm.complete(&self.attach(request)).await
    }

    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.llm
            .complete_with_progress(&self.attach(request), on_text)
            .await
    }

    async fn complete_with_retry_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.llm
            .complete_with_retry_progress(&self.attach(request), on_text, on_retry)
            .await
    }

    async fn complete_with_reasoning_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.llm
            .complete_with_reasoning_progress(
                &self.attach(request),
                on_text,
                on_reasoning,
                on_retry,
            )
            .await
    }

    fn model_name(&self) -> &str {
        self.llm.model_name()
    }

    fn max_context_tokens(&self) -> usize {
        self.llm.max_context_tokens()
    }

    fn chars_per_token(&self) -> f64 {
        self.llm.chars_per_token()
    }

    fn count_tokens(&self, text: &str) -> usize {
        self.llm.count_tokens(text)
    }

    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        self.llm.count_tokens_exact(text).await
    }

    fn counts_tokens_exactly(&self) -> bool {
        self.llm.counts_tokens_exactly()
    }

    async fn is_ready(&self) -> Result<bool> {
        self.llm.is_ready().await
    }

    fn supports_tool_calling(&self) -> bool {
        self.llm.supports_tool_calling()
    }

    fn vision(&self) -> Option<VisionGeometry> {
        self.llm.vision()
    }

    fn thinks_before_answering(&self) -> bool {
        self.llm.thinks_before_answering()
    }

    fn provider_name(&self) -> &str {
        self.llm.provider_name()
    }

    fn is_alive(&self) -> bool {
        self.llm.is_alive()
    }

    fn pace(&self) -> Option<Pace> {
        self.llm.pace()
    }

    fn requests(&self) -> Option<RequestLoad> {
        self.llm.requests()
    }

    fn hold_cache(&self, key: &str) -> Option<CacheHold> {
        self.llm.hold_cache(key)
    }

    fn keeps_read_prompts(&self) -> bool {
        self.llm.keeps_read_prompts()
    }
}

/// How one call's requests are sent and asked again.
#[async_trait]
pub trait Transport: Sync {
    type Request: Send + Sync;

    async fn send(&self, llm: &dyn LLMPort, request: &Self::Request) -> Result<CompletionResponse>;

    /// The request that asks again after `previous`, quoting `refusal`. Err when it cannot be sent.
    async fn correction(
        &self,
        llm: &dyn LLMPort,
        request: &Self::Request,
        previous: &CompletionResponse,
        refusal: &str,
    ) -> Result<Self::Request>;

    /// The same request with a larger output allowance, or None.
    async fn larger(&self, llm: &dyn LLMPort, request: &Self::Request) -> Option<Self::Request>;
}

/// A request sent as it is, asked again with the refused reply in the history.
pub struct Plain {
    pub request: CompletionRequest,
}

#[async_trait]
impl Transport for Plain {
    type Request = CompletionRequest;

    async fn send(
        &self,
        llm: &dyn LLMPort,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse> {
        llm.complete(request).await
    }

    async fn correction(
        &self,
        _llm: &dyn LLMPort,
        request: &CompletionRequest,
        previous: &CompletionResponse,
        refusal: &str,
    ) -> Result<CompletionRequest> {
        let mut next = request.clone();
        next.input.push(CompletionInput::Message {
            role: "assistant".into(),
            content: previous.text.clone(),
        });
        next.input.push(CompletionInput::Message {
            role: "user".into(),
            content: format!("{CORRECTION_LEAD} {refusal}"),
        });
        Ok(next)
    }

    async fn larger(
        &self,
        _llm: &dyn LLMPort,
        _request: &CompletionRequest,
    ) -> Option<CompletionRequest> {
        None
    }
}

/// An error's own text, without its variant's prefix.
pub fn refusal(error: &AppError) -> String {
    match error {
        AppError::InvalidInput(reason)
        | AppError::InvalidData(reason)
        | AppError::ValidationFailed(reason)
        | AppError::ModelAnswerUnusable(reason)
        | AppError::Json {
            message: reason, ..
        } => reason.clone(),
        other => other.to_string(),
    }
}

/// The reply to a request: sent once, and once more with a larger allowance when cut off.
async fn complete_reply<T: Transport>(
    llm: &dyn LLMPort,
    transport: &T,
    request: T::Request,
) -> Result<(T::Request, CompletionResponse)> {
    let response = transport.send(llm, &request).await?;
    let larger = match response.finish() {
        Finish::CutOff => transport.larger(llm, &request).await,
        _ => None,
    };
    let (request, response) = match larger {
        Some(larger) => {
            let response = transport.send(llm, &larger).await?;
            (larger, response)
        }
        None => (request, response),
    };
    match response.finish() {
        Finish::Complete => Ok((request, response)),
        Finish::Declined => Err(AppError::ModelAnswerUnusable(DECLINED.into())),
        Finish::CutOff | Finish::Filtered => Err(AppError::ModelAnswerUnusable(CUT_OFF.into())),
    }
}

/// The accepted answer to `request`: one correction quoting why a reply was refused, then unusable.
pub async fn answer<T: Transport, A>(
    llm: &dyn LLMPort,
    call: &ContractCall,
    transport: &T,
    request: T::Request,
    accept: impl Fn(&str) -> Result<A> + Send + Sync,
) -> Result<A> {
    let llm = tagged(llm, call);
    let (request, response) = complete_reply(&llm, transport, request).await?;
    let error = match accept(&response.text) {
        Ok(answer) => return Ok(answer),
        Err(error) => error,
    };
    let correction = transport
        .correction(llm, &request, &response, &refusal(&error))
        .await?;
    let (_, response) = complete_reply(&llm, transport, correction).await?;
    accept(&response.text).map_err(|error| AppError::ModelAnswerUnusable(refusal(&error)))
}
