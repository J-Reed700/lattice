//! Whether a model a chat names can be used now. Checked when the chat
//! chooses it, so a choice that cannot run is refused with its cause and its
//! fix instead of failing the chat's next turn. The turn still resolves the
//! choice again: a model can disappear after it was chosen.

use lattice_core::application::contracts::settings::LLMSettingsDto;
use lattice_core::domain::models::model_ref::{CloudProvider, ModelRef};
use lattice_core::domain::repositories::downloaded_model_repository::DownloadedModelRepository;
use lattice_core::shared::error::{AppError, Result};

/// `Ok` when `model` is downloaded, or its connection or key is configured.
/// `cloud_key_saved` answers whether a provider (`openai`, `anthropic`,
/// `google`) has an API key, read the way the loader reads it.
pub(crate) async fn check_available(
    model: &ModelRef,
    settings: &LLMSettingsDto,
    downloaded: &dyn DownloadedModelRepository,
    cloud_key_saved: impl FnOnce(&str) -> Result<bool>,
) -> Result<()> {
    match model {
        ModelRef::Local { model_id } => {
            let not_downloaded = |name: &str| {
                AppError::InvalidInput(format!(
                    "{name} is not downloaded any more. Download it in Settings → Models or pick another model"
                ))
            };
            let row = downloaded
                .find_by_model_id(model_id)
                .await?
                .ok_or_else(|| not_downloaded(model_id))?;
            if !row.is_chat_model() {
                return Err(AppError::InvalidInput(format!(
                    "{} is not a chat model. Pick another model",
                    row.model_name()
                )));
            }
            let path = row
                .loadable_path()
                .ok_or_else(|| not_downloaded(row.model_name()))?;
            if !path.exists() {
                return Err(AppError::InvalidInput(format!(
                    "{}'s file is missing from {}. Download it again in Settings → Models or pick another model",
                    row.model_name(),
                    path.display()
                )));
            }
            Ok(())
        }
        ModelRef::Ollama { .. } if settings.ollama_url.trim().is_empty() => {
            Err(AppError::InvalidConfig(
                "No Ollama connection is configured. Add one in Settings → Chat".into(),
            ))
        }
        ModelRef::LlamaCpp { .. } if settings.llama_cpp.url.trim().is_empty() => {
            Err(AppError::InvalidConfig(
                "No llama.cpp connection is configured. Add one in Settings → Chat".into(),
            ))
        }
        ModelRef::Ollama { .. } | ModelRef::LlamaCpp { .. } => Ok(()),
        ModelRef::Cloud { provider, .. } => {
            let (key, label) = match provider {
                CloudProvider::Openai => ("openai", "OpenAI"),
                CloudProvider::Anthropic => ("anthropic", "Anthropic"),
                CloudProvider::Google => ("google", "Google"),
            };
            if cloud_key_saved(key)? {
                Ok(())
            } else {
                Err(AppError::InvalidConfig(format!(
                    "No {label} API key is saved. Add it in Settings → Chat"
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests;
