//! Which model a chat turn or a learning job runs on. Settings and the active
//! local row go in, a model and the reason for it come out, so precedence
//! needs no I/O to test.
//!
//! A chat runs on its request override, else its own choice, else the Settings
//! default. Learning never follows a chat: it runs on its own role or refuses.

use crate::application::contracts::settings::{LLMProvider, LLMSettingsDto};
use crate::application::ports::llm_port::{ModelOrigin, ResolvedModel};
use crate::domain::conversation::ChatModelChoice;
use crate::domain::models::model_ref::{CloudProvider, ModelRef};
use crate::shared::error::{AppError, Result};

/// The model a `Default` chat runs on right now, or why there is none.
#[derive(Clone, Debug)]
pub struct DefaultChatModel(pub Result<ModelRef>);

/// The Settings default, from the provider and `active_local`, the `model_id`
/// of the active downloaded row. `Auto` names the first of the active row, a
/// configured llama.cpp model and the Ollama model, in the order the loader
/// tries them.
pub fn default_chat_model(
    settings: &LLMSettingsDto,
    active_local: Option<&str>,
) -> DefaultChatModel {
    let local = active_local.map(|model_id| ModelRef::Local {
        model_id: model_id.to_owned(),
    });
    let cloud = |provider| ModelRef::Cloud {
        provider,
        model: settings.model.clone(),
    };
    DefaultChatModel(match settings.provider {
        LLMProvider::Local => local.ok_or_else(|| {
            AppError::AiModelsNotInstalled("No chat model is selected in Settings → Models".into())
        }),
        LLMProvider::Ollama => Ok(ModelRef::Ollama {
            model: settings.model.clone(),
        }),
        LLMProvider::Llamacpp => Ok(ModelRef::LlamaCpp {
            model: settings.llama_cpp.model.clone(),
        }),
        LLMProvider::Openai => Ok(cloud(CloudProvider::Openai)),
        LLMProvider::Anthropic => Ok(cloud(CloudProvider::Anthropic)),
        LLMProvider::Google => Ok(cloud(CloudProvider::Google)),
        LLMProvider::Auto => local
            .or_else(|| {
                (!settings.llama_cpp.model.trim().is_empty()).then(|| ModelRef::LlamaCpp {
                    model: settings.llama_cpp.model.clone(),
                })
            })
            .or_else(|| {
                (!settings.ollama_url.is_empty() && !settings.model.is_empty()).then(|| {
                    ModelRef::Ollama {
                        model: settings.model.clone(),
                    }
                })
            })
            .ok_or_else(|| {
                AppError::AiModelsNotInstalled(
                    "No chat model is selected in Settings → Models, and no llama.cpp or Ollama \
                     server is configured in Settings → Chat"
                        .into(),
                )
            }),
    })
}

/// A chat's model: `request_override`, else the chat's own choice, else the
/// Settings default. An override or an explicit choice is used as named and
/// never falls through to another model.
pub fn resolve(
    request_override: Option<&ModelRef>,
    chat: &ChatModelChoice,
    default: &DefaultChatModel,
) -> Result<ResolvedModel> {
    let (model, origin) = match (request_override, chat) {
        (Some(model), _) => (model.clone(), ModelOrigin::RequestOverride),
        (None, ChatModelChoice::Explicit { model }) => (model.clone(), ModelOrigin::ChatOverride),
        (None, ChatModelChoice::Default) => (default.0.clone()?, ModelOrigin::Default),
    };
    Ok(ResolvedModel { model, origin })
}

/// The Learning model, or a refusal when none is set. Neither the chat default
/// nor any chat's choice is consulted.
pub fn resolve_learning(settings: &LLMSettingsDto) -> Result<ResolvedModel> {
    let model = settings.learning_model.clone().ok_or_else(|| {
        AppError::InvalidConfig(
            "No Learning model is set. Choose one in Settings → Models under Learning.".into(),
        )
    })?;
    Ok(ResolvedModel {
        model,
        origin: ModelOrigin::LearningRole,
    })
}

#[cfg(test)]
mod tests;
