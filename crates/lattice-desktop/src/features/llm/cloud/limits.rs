//! What a provider says a model can take, asked of the provider rather than
//! worked out from the model's name. Anthropic and Google report a window and
//! an output ceiling. OpenAI's model object carries neither, so its models run
//! at what was chosen for them, or at Settings' fallback.

use std::{collections::HashMap, sync::Mutex, time::Duration};

use lattice_core::{
    application::contracts::settings::ModelWindowDto,
    domain::models::model_ref::CloudProvider,
    shared::{
        error::{AppError, Result},
        http::reqwest_client_builder,
    },
};
use reqwest::Url;

use super::protocol::{protocols, LimitsProtocol};
use lattice_core::shared::error::WithCauses;

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
    ) -> Result<ModelWindowDto> {
        let id = (provider, model.to_owned());
        if let Some(known) = self.known().get(&id) {
            return Ok(*known);
        }
        let reported = report(provider, model, key).await?;
        self.known().insert(id, reported);
        Ok(reported)
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
    let protocol = protocols(provider).limits;
    if protocol.limits_api().is_none() {
        return Ok(ModelWindowDto::default());
    }
    report_from(provider, &protocol.api_base(), model, key).await
}

async fn report_from(
    provider: CloudProvider,
    base: &str,
    model: &str,
    key: &str,
) -> Result<ModelWindowDto> {
    let protocol = protocols(provider).limits;
    let label = protocol.shown_name();
    let failed = || AppError::Network(format!("{label} did not report limits for {model}"));
    let api = protocol.limits_api().ok_or_else(failed)?;
    let client = reqwest_client_builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(REPORT_TIMEOUT)
        .build()
        .map_err(|e| AppError::Network(WithCauses(&e).to_string()))?;
    let url = model_url(base, api.version, model).ok_or_else(failed)?;
    let response = protocol
        .sign_management(client.get(url), key)
        .send()
        .await
        .map_err(|_| failed())?;
    let status = response.status();
    if !status.is_success() {
        return Err(super::refusal(protocol, response).await);
    }
    let reply: serde_json::Value = response.json().await.map_err(|_| failed())?;
    (api.parse)(reply).ok_or_else(failed)
}

/// The reply ceiling a client of `model` is built with: a known one, or none
/// where the provider's API takes a request without one.
pub(crate) fn ceiling_to_send(
    provider: CloudProvider,
    model: &str,
    known: Option<u32>,
) -> Result<Option<u32>> {
    ceiling_for(protocols(provider).limits, model, known)
}

/// [`ceiling_to_send`] for a provider whose protocol is already resolved.
pub(super) fn ceiling_for(
    protocol: &dyn LimitsProtocol,
    model: &str,
    known: Option<u32>,
) -> Result<Option<u32>> {
    if known.is_some() || !protocol.requires_ceiling() {
        return Ok(known);
    }
    Err(AppError::InvalidConfig(format!(
        "{} needs a reply limit for {model} and reports none: set Max output for the model \
         under Configure in Settings → Downloaded, or choose another model in Settings → AI",
        protocol.shown_name()
    )))
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
mod tests;
