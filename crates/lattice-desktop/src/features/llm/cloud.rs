//! Native OpenAI Responses and Anthropic Messages adapters.
//! Explicit selection only; Auto never sends local documents to a cloud provider.
use crate::features::llm::backend::{ignore_text, ModelBackend, Progress, TextSink};
use crate::features::llm::engine::vision::not_set_up_to_see;
use crate::features::llm::fitting::{Fitted, OutputLimits, Thinking};
use async_trait::async_trait;
use lattice_core::application::{
    contracts::settings::LLMSettingsDto,
    ports::llm_port::{CompletionRequest, CompletionResponse, LLMPort},
};
use lattice_core::domain::models::vision::VisionGeometry;
use lattice_core::shared::error::WithCauses;
use lattice_core::shared::error::{AppError, Result};
use protocol::{cloud_provider, protocols, CompletionProtocol, ProviderApi};
use serde_json::Value;
use std::time::{Duration, Instant};

pub mod catalog;
pub(crate) mod limits;
mod protocol;
mod reply;
mod streaming;
mod timing;

pub struct CloudLlm {
    protocol: &'static dyn CompletionProtocol,
    model: String,
    key: String,
    endpoint: String,
    client: reqwest::Client,
    /// Longest silence tolerated inside a streamed response.
    stall_timeout: Duration,
    /// The most this model can generate in a reply, when someone knows it
    /// (chosen, the role's own, or the provider's report).
    model_ceiling: Option<u32>,
    context_window: usize,
    /// The geometry the model was declared to see with; `None` refuses
    /// pictures.
    vision: Option<VisionGeometry>,
}

impl CloudLlm {
    /// Behind the provider's shared scheduler, so every role on one provider
    /// queues in one place.
    pub(crate) fn schedule(
        self,
        schedulers: &crate::features::llm::scheduler::SchedulerRegistry,
    ) -> std::sync::Arc<dyn LLMPort> {
        let scheduler = schedulers.cloud(self.protocol.name());
        std::sync::Arc::new(crate::features::llm::scheduler::ScheduledLlm::new(
            std::sync::Arc::new(self),
            scheduler,
        ))
    }

    /// This client as a port, behind a scheduler of its own.
    pub fn into_port(self) -> std::sync::Arc<dyn LLMPort> {
        self.schedule(&crate::features::llm::scheduler::SchedulerRegistry::default())
    }

    /// A client for `settings.model`, sending replies of at most
    /// `output_ceiling` tokens. A model whose provider requires a ceiling is
    /// not given one made up.
    pub fn new(
        settings: &LLMSettingsDto,
        key: String,
        output_ceiling: Option<u32>,
    ) -> Result<Self> {
        if key.trim().is_empty() || settings.model.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "Cloud model and API key are required".into(),
            ));
        }
        let protocols = protocols(cloud_provider(settings.provider)?);
        let protocol = protocols.completion;
        let model_ceiling = limits::ceiling_for(protocols.limits, &settings.model, output_ceiling)?;
        let endpoint = format!("{}{}", protocol.api_base(), protocol.completion_path());
        // No client-wide timeout: a long answer is bounded by the request's time
        // budget, and a streamed one additionally by stall detection.
        let client = lattice_core::shared::http::reqwest_client_builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| AppError::InvalidConfig(WithCauses(&e).to_string()))?;
        Ok(Self {
            protocol,
            model: settings.model.clone(),
            key,
            endpoint,
            client,
            stall_timeout: Duration::from_secs(u64::from(settings.timeout_seconds.max(1))),
            model_ceiling,
            context_window: settings.context_window as usize,
            vision: None,
        })
    }

    /// This client with the geometry its model was declared to see with.
    pub fn with_vision(mut self, vision: Option<VisionGeometry>) -> Self {
        self.vision = vision;
        self
    }

    /// This client sending to `endpoint` instead of the provider's public
    /// API: a gateway that speaks the same format, or a stub server in tests.
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    fn body(&self, fitted: &Fitted<'_>, stream: bool) -> Result<Value> {
        if fitted.request().has_images() && self.vision.is_none() {
            return Err(not_set_up_to_see(&self.model));
        }
        self.protocol.body(&self.model, fitted, stream)
    }

    /// All three attempts and their backoffs share one `deadline`, so a retry
    /// never multiplies the caller's time budget. `bound_body` applies what is
    /// left of it to the whole exchange; without it only the wait for response
    /// headers is bounded and the streamed body is left to stall detection.
    async fn post(
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
            let request = self.protocol.sign(request, &self.key);
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
                    AppError::Network(format!("Cloud request failed: {}", WithCauses(&e)))
                }
            })?;
            let status = response.status();
            if status.is_success() {
                return Ok(response);
            }
            // Avoid echoing provider bodies which can contain submitted user text.
            if status == reqwest::StatusCode::BAD_REQUEST {
                let error = status_error(self.protocol, status, &Value::Null);
                let reply = response.json::<Value>().await.unwrap_or(Value::Null);
                let reason = reply
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let refusal = self.protocol.format_refusal(body, reason);
                if self.protocol.out_of_credit(status, &reply) {
                    return Err(credit_balance_error(self.protocol.shown_name()));
                }
                return Err(refusal.unwrap_or(error));
            }
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|h| h.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let error = refusal(self.protocol, response).await;
            let resend =
                matches!(error, AppError::RateLimitExceeded(_)) || status.is_server_error();
            if attempt == 2 || !resend {
                return Err(error);
            }
            let delay = Duration::from_secs(retry_after.unwrap_or(1 << attempt).min(30));
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
            self.protocol.name()
        ))
    }

    /// Sends `body`, and once more with what the provider's fallback makes of
    /// it when the provider refuses it in a way the fallback answers.
    async fn send_with_fallback(
        &self,
        body: Value,
        deadline: Option<Instant>,
        bound_body: bool,
    ) -> Result<reqwest::Response> {
        match self.post(&body, deadline, bound_body).await {
            Err(error) => match self.protocol.resend_after(&error, &body) {
                Some(rewritten) => self.post(&rewritten, deadline, bound_body).await,
                None => Err(error),
            },
            sent => sent,
        }
    }
}

impl CloudLlm {
    /// The whole answer at once.
    async fn complete_whole(&self, fitted: &Fitted<'_>) -> Result<CompletionResponse> {
        let deadline = fitted
            .request()
            .wall_clock_budget()
            .map(|budget| Instant::now() + budget);
        let response = self
            .send_with_fallback(self.body(fitted, false)?, deadline, true)
            .await?;
        let value: Value = response.json().await.map_err(|e| {
            AppError::Network(format!("Invalid cloud response: {}", WithCauses(&e)))
        })?;
        // Without the server's own timings a whole answer at once has no
        // edge between reading and writing, so it is not timed.
        self.protocol.parse_reply(value)
    }

    /// The answer as it is written, text and thinking handed on as they arrive.
    async fn complete_streamed(
        &self,
        fitted: &Fitted<'_>,
        on_text: &TextSink<'_>,
        on_reasoning: &TextSink<'_>,
    ) -> Result<CompletionResponse> {
        let request = fitted.request();
        let deadline = request
            .wall_clock_budget()
            .map(|budget| Instant::now() + budget);
        request
            .within_time_budget(async {
                let sent = Instant::now();
                let response = self
                    .send_with_fallback(self.body(fitted, true)?, deadline, false)
                    .await?;
                streaming::drain(
                    self.protocol,
                    response.bytes_stream(),
                    self.stall_timeout,
                    sent,
                    on_text,
                    on_reasoning,
                )
                .await
            })
            .await
            .map_err(|_| self.budget_error())?
    }
}

#[async_trait]
impl ModelBackend for CloudLlm {
    fn model_name(&self) -> &str {
        &self.model
    }
    fn provider_name(&self) -> &str {
        self.protocol.name()
    }
    fn window(&self) -> usize {
        self.context_window
    }
    fn thinking(&self, request: &CompletionRequest) -> Thinking {
        self.protocol.thinking(&self.model, request)
    }
    fn output_limits(&self) -> OutputLimits {
        OutputLimits::of(self.model_ceiling)
    }
    fn supports_tool_calling(&self) -> bool {
        true
    }
    fn vision(&self) -> Option<VisionGeometry> {
        self.vision
    }
    fn keeps_read_prompts(&self) -> bool {
        false
    }
    fn is_alive(&self) -> bool {
        true
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(!self.key.is_empty())
    }
    async fn send(
        &self,
        fitted: &Fitted<'_>,
        progress: Progress<'_>,
    ) -> Result<CompletionResponse> {
        match progress {
            Progress::None => self.complete_whole(fitted).await,
            Progress::Streamed {
                on_text,
                on_reasoning,
                ..
            } => {
                self.complete_streamed(fitted, on_text, on_reasoning.unwrap_or(&ignore_text))
                    .await
            }
        }
    }
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

#[cfg(test)]
mod tests;

/// A 400 from an account out of credit is a setting to fix.
fn credit_balance_error(shown: &str) -> AppError {
    AppError::InvalidConfig(format!(
        "{shown} refused the request because the account's credit balance is too low \
         (HTTP 400): add credits to the account, or choose another model in Settings → AI"
    ))
}

/// The error for a refused response; only a 429's body is read, as it alone
/// tells an account with nothing left from a busy one. The body is never
/// shown.
async fn refusal(api: &dyn ProviderApi, response: reqwest::Response) -> AppError {
    let status = response.status();
    let reply = if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        response.json::<Value>().await.unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    status_error(api, status, &reply)
}

/// A provider's refusal, typed by what waiting can change: a rejected key or
/// an unknown model is a setting to fix, a rate limit or server error is an
/// outage, and any other refusal would be refused again.
fn status_error(api: &dyn ProviderApi, status: reqwest::StatusCode, reply: &Value) -> AppError {
    let (shown, name) = (api.shown_name(), api.name());
    match status.as_u16() {
        401 => AppError::InvalidConfig(format!(
            "{shown} rejected the saved API key (HTTP {status}): replace it in Settings → AI"
        )),
        403 => AppError::InvalidConfig(format!(
            "{shown} refused the saved API key access to this model (HTTP {status}): check the \
             key's permissions, or choose another model in Settings → AI"
        )),
        404 => AppError::InvalidConfig(format!(
            "{shown} does not offer the configured model (HTTP {status}): choose another in \
             Settings → AI"
        )),
        402 => AppError::InvalidConfig(format!(
            "{shown} needs the account's billing or payment details fixed (HTTP {status}): fix them \
             on the provider's account page, or choose another model in Settings → AI"
        )),
        429 if api.out_of_credit(status, reply) => AppError::InvalidConfig(format!(
            "{shown} says the account has no credit or quota left (HTTP 429): add credit to the \
             account, or choose another model in Settings → AI"
        )),
        429 => AppError::RateLimitExceeded(format!("{name} API returned HTTP {status}")),
        408 => AppError::Network(format!("{name} API returned HTTP {status}")),
        _ if status.is_server_error() => {
            AppError::Network(format!("{name} API returned HTTP {status}"))
        }
        _ => AppError::ModelRejected(format!("{shown} rejected this request (HTTP {status}).")),
    }
}

#[cfg(test)]
mod transport_tests;
