//! What a provider says a model can take, asked of the provider rather than
//! worked out from the model's name. Anthropic and Google report a window and
//! an output ceiling. OpenAI's model object carries neither, so its models run
//! at what was chosen for them, or at Settings' fallback.

use std::{collections::HashMap, sync::Mutex, time::Duration};

use lattice_core::{
    application::{
        contracts::settings::ModelWindowDto, services::chat_model_resolution::cloud_provider_name,
    },
    domain::models::model_ref::CloudProvider,
    shared::{
        error::{AppError, Result},
        http::reqwest_client_builder,
    },
};
use reqwest::Url;
use serde::Deserialize;

use super::api_base;

/// A model with no report still runs, so a chat waits only this long for one.
const REPORT_TIMEOUT: Duration = Duration::from_secs(8);

/// What each model's provider reported, for as long as the app runs. A lookup
/// that fails is not kept: the next client built asks again.
#[derive(Default)]
pub(crate) struct ReportedLimits {
    known: Mutex<HashMap<(CloudProvider, String), ModelWindowDto>>,
}

impl ReportedLimits {
    pub(crate) async fn of(
        &self,
        provider: CloudProvider,
        model: &str,
        key: &str,
    ) -> ModelWindowDto {
        let id = (provider, model.to_owned());
        if let Some(known) = self.known().get(&id) {
            return *known;
        }
        match report(provider, model, key).await {
            Ok(reported) => {
                self.known().insert(id, reported);
                reported
            }
            Err(error) => {
                tracing::warn!(model, %error, "Running without the provider's limits");
                ModelWindowDto::default()
            }
        }
    }

    fn known(&self) -> std::sync::MutexGuard<'_, HashMap<(CloudProvider, String), ModelWindowDto>> {
        self.known
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// What `provider` reports for `model`.
pub(crate) async fn report(
    provider: CloudProvider,
    model: &str,
    key: &str,
) -> Result<ModelWindowDto> {
    match provider {
        CloudProvider::Openai => Ok(ModelWindowDto::default()),
        CloudProvider::Anthropic => {
            let base = api_base("LATTICE_ANTHROPIC_BASE_URL", "https://api.anthropic.com");
            report_from(provider, &base, model, key).await
        }
        CloudProvider::Google => {
            let base = api_base(
                "LATTICE_GOOGLE_BASE_URL",
                "https://generativelanguage.googleapis.com",
            );
            report_from(provider, &base, model, key).await
        }
    }
}

#[derive(Deserialize)]
struct AnthropicModel {
    max_input_tokens: Option<u32>,
    max_tokens: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleModel {
    input_token_limit: Option<u32>,
    output_token_limit: Option<u32>,
}

async fn report_from(
    provider: CloudProvider,
    base: &str,
    model: &str,
    key: &str,
) -> Result<ModelWindowDto> {
    let label = cloud_provider_name(provider);
    let failed = || AppError::Network(format!("{label} did not report limits for {model}"));
    let client = reqwest_client_builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(REPORT_TIMEOUT)
        .build()
        .map_err(|e| AppError::Network(e.to_string()))?;
    let version = match provider {
        CloudProvider::Google => "v1beta",
        CloudProvider::Openai | CloudProvider::Anthropic => "v1",
    };
    let request = client.get(model_url(base, version, model).ok_or_else(failed)?);
    let request = match provider {
        CloudProvider::Google => request.header("x-goog-api-key", key),
        CloudProvider::Openai | CloudProvider::Anthropic => request
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01"),
    };
    let response = request
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| failed())?;
    let (context_window, max_output_tokens) = match provider {
        CloudProvider::Google => {
            let model: GoogleModel = response.json().await.map_err(|_| failed())?;
            (model.input_token_limit, model.output_token_limit)
        }
        CloudProvider::Openai | CloudProvider::Anthropic => {
            let model: AnthropicModel = response.json().await.map_err(|_| failed())?;
            (model.max_input_tokens, model.max_tokens)
        }
    };
    Ok(ModelWindowDto {
        context_window: context_window.filter(|tokens| *tokens > 0),
        max_output_tokens: max_output_tokens.filter(|tokens| *tokens > 0),
    })
}

/// `model` is typed into Settings, so it is a path segment, never a path.
fn model_url(base: &str, version: &str, model: &str) -> Option<Url> {
    let mut url = Url::parse(base).ok()?;
    url.path_segments_mut()
        .ok()?
        .pop_if_empty()
        .extend([version, "models", model]);
    Some(url)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests;
