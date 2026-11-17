//! Which model a chat turn or a learning job runs on. Settings and the active
//! local row go in, a model and the reason for it come out, so precedence
//! needs no I/O to test.
//!
//! A chat runs on its request override, else its own choice, else the Settings
//! default. Learning never follows a chat: it runs on its own role or refuses.
//! The Verifier, likewise, runs on its own role or refuses. Each role can
//! name a local, server-hosted or cloud model.
//!
//! This module is the one place that decides what a default chat and utility
//! work run on, and whether the llama.cpp and Ollama connections are
//! configured. The model loader and `get_active_models` both read it.

use crate::application::contracts::settings::{LLMProvider, LLMSettingsDto, DEFAULT_OLLAMA_URL};
use crate::application::ports::llm_port::{ModelOrigin, ResolvedModel};
use crate::domain::conversation::ChatModelChoice;
use crate::domain::models::model_ref::{CloudProvider, ModelRef};
use crate::shared::error::{AppError, Result};

/// The models tried, in order. `auto` means Auto chose them, so each gives way
/// to the next when it fails, and a llama.cpp server must list its model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidates {
    pub models: Vec<ModelRef>,
    pub auto: bool,
}

impl Candidates {
    /// Whether `model`'s server must list it: a llama.cpp server Auto chose,
    /// so Auto moves on when the server lacks the model.
    pub fn must_list(&self, model: &ModelRef) -> bool {
        self.auto && matches!(model, ModelRef::LlamaCpp { .. })
    }
}

/// The model the llama.cpp connection serves, when Settings names both its
/// server URL and its model.
pub fn llama_cpp_connection(settings: &LLMSettingsDto) -> Option<ModelRef> {
    let model = settings.llama_cpp.model.trim();
    (!settings.llama_cpp.url.trim().is_empty() && !model.is_empty()).then(|| ModelRef::LlamaCpp {
        model: model.to_owned(),
    })
}

/// Whether Settings holds a saved Ollama connection. A default address is a
/// suggestion, not a saved connection, and `model` can't establish one because
/// it also holds cloud model names.
pub fn ollama_saved(settings: &LLMSettingsDto) -> bool {
    let url = settings.ollama_url.trim().trim_end_matches('/');
    !url.is_empty()
        && (settings.ollama_configured
            || settings.provider == LLMProvider::Ollama
            || url != DEFAULT_OLLAMA_URL
            || !settings.ollama_utility_model.trim().is_empty()
            || !settings.ollama_auth_header_name.trim().is_empty())
}

/// The chat model the Ollama connection serves, when the connection is saved
/// and Settings names a chat tag.
pub fn ollama_connection(settings: &LLMSettingsDto) -> Option<ModelRef> {
    let model = settings.model.trim();
    (ollama_saved(settings) && !model.is_empty()).then(|| ModelRef::Ollama {
        model: model.to_owned(),
    })
}

/// The tag utility work asks the Ollama connection for: its utility tag, else
/// its chat tag.
pub fn ollama_utility_tag(settings: &LLMSettingsDto) -> Option<String> {
    [&settings.ollama_utility_model, &settings.model]
        .into_iter()
        .map(|tag| tag.trim())
        .find(|tag| !tag.is_empty())
        .map(str::to_owned)
}

/// The models a `Default` chat tries right now, or why there are none. `Ok`
/// never holds an empty list.
#[derive(Clone, Debug)]
pub struct DefaultChatModel(pub Result<Candidates>);

/// The Settings default, from the provider and `active_local`, the `model_id`
/// of the active downloaded row. An explicit provider names its one model, or
/// refuses when none is set. `Auto` lists the active row, the configured
/// llama.cpp model and the saved Ollama model, in the order the loader tries
/// them.
pub fn default_chat_model(
    settings: &LLMSettingsDto,
    active_local: Option<&str>,
) -> DefaultChatModel {
    let local = active_local.map(|model_id| ModelRef::Local {
        model_id: model_id.to_owned(),
    });
    let one = |model| {
        Ok(Candidates {
            models: vec![model],
            auto: false,
        })
    };
    let unset = |what: &str| {
        Err(AppError::AiModelsNotInstalled(format!(
            "No {what} is set in Settings → Chat"
        )))
    };
    let name = settings.model.trim();
    let cloud = |provider| {
        if name.is_empty() {
            unset(&format!("{} model", cloud_provider_name(provider)))
        } else {
            one(ModelRef::Cloud {
                provider,
                model: name.to_owned(),
            })
        }
    };
    DefaultChatModel(match settings.provider {
        LLMProvider::Local => match local {
            Some(model) => one(model),
            None => Err(AppError::AiModelsNotInstalled(
                "No chat model is selected in Settings → Downloaded".into(),
            )),
        },
        LLMProvider::Ollama if name.is_empty() => unset("Ollama chat model"),
        LLMProvider::Ollama => one(ModelRef::Ollama {
            model: name.to_owned(),
        }),
        LLMProvider::Llamacpp => match settings.llama_cpp.model.trim() {
            "" => unset("llama.cpp model"),
            model => one(ModelRef::LlamaCpp {
                model: model.to_owned(),
            }),
        },
        LLMProvider::Openai => cloud(CloudProvider::Openai),
        LLMProvider::Anthropic => cloud(CloudProvider::Anthropic),
        LLMProvider::Google => cloud(CloudProvider::Google),
        LLMProvider::Auto => {
            let models: Vec<ModelRef> = [
                local,
                llama_cpp_connection(settings),
                ollama_connection(settings),
            ]
            .into_iter()
            .flatten()
            .collect();
            if models.is_empty() {
                Err(nothing_configured())
            } else {
                Ok(Candidates { models, auto: true })
            }
        }
    })
}

fn nothing_configured() -> AppError {
    AppError::AiModelsNotInstalled(
        "No chat model is selected in Settings → Downloaded, and no llama.cpp or Ollama server is \
         configured in Settings → Chat"
            .into(),
    )
}

/// The row active for the utility role, as far as which models utility work
/// tries is concerned.
pub struct UtilityRow<'a> {
    pub model_id: &'a str,
    /// A downloaded file; otherwise the row is the Ollama connection's.
    pub local: bool,
}

/// The models utility work tries, in order; empty when none is set. A
/// reference saved in Settings comes first and ignores `row`; the row counts
/// only while none is saved: a downloaded row runs locally, and the Ollama
/// connection's row runs on the configured provider, with Auto trying a
/// llama.cpp connection before a saved Ollama.
pub fn utility_candidates(settings: &LLMSettingsDto, row: Option<UtilityRow<'_>>) -> Candidates {
    let one = |model| Candidates {
        models: vec![model],
        auto: false,
    };
    let none = || Candidates {
        models: Vec::new(),
        auto: false,
    };
    if let Some(model) = &settings.utility_model {
        return one(model.clone());
    }
    let Some(row) = row else {
        return none();
    };
    if row.local {
        return one(ModelRef::Local {
            model_id: row.model_id.to_owned(),
        });
    }
    let Some(tag) = ollama_utility_tag(settings) else {
        return none();
    };
    let cloud = |provider| {
        one(ModelRef::Cloud {
            provider,
            model: tag.clone(),
        })
    };
    match settings.provider {
        LLMProvider::Local => none(),
        LLMProvider::Ollama => one(ModelRef::Ollama { model: tag.clone() }),
        LLMProvider::Llamacpp => one(ModelRef::LlamaCpp { model: tag.clone() }),
        LLMProvider::Openai => cloud(CloudProvider::Openai),
        LLMProvider::Anthropic => cloud(CloudProvider::Anthropic),
        LLMProvider::Google => cloud(CloudProvider::Google),
        LLMProvider::Auto => Candidates {
            models: [
                llama_cpp_connection(settings).map(|_| ModelRef::LlamaCpp { model: tag.clone() }),
                ollama_saved(settings).then(|| ModelRef::Ollama { model: tag.clone() }),
            ]
            .into_iter()
            .flatten()
            .collect(),
            auto: true,
        },
    }
}

/// A chat's model: `request_override`, else the chat's own choice, else the
/// first model of the Settings default. An override or an explicit choice is
/// used as named and never falls through to another model.
pub fn resolve(
    request_override: Option<&ModelRef>,
    chat: &ChatModelChoice,
    default: &DefaultChatModel,
) -> Result<ResolvedModel> {
    let (model, origin) = match (request_override, chat) {
        (Some(model), _) => (model.clone(), ModelOrigin::RequestOverride),
        (None, ChatModelChoice::Explicit { model }) => (model.clone(), ModelOrigin::ChatOverride),
        (None, ChatModelChoice::Default) => {
            let first = default.0.clone()?.models.into_iter().next();
            (first.ok_or_else(nothing_configured)?, ModelOrigin::Default)
        }
    };
    Ok(ResolvedModel { model, origin })
}

/// The Learning model, or a refusal when none is set. Neither the chat default
/// nor any chat's choice is consulted.
pub fn resolve_learning(settings: &LLMSettingsDto) -> Result<ResolvedModel> {
    let model = settings.learning_model.clone().ok_or_else(|| {
        AppError::LearningModelNotSet(
            "No Learning model is set. Choose one in Settings → Downloaded.".into(),
        )
    })?;
    Ok(ResolvedModel {
        model,
        origin: ModelOrigin::LearningRole,
    })
}

/// The Verifier model, or a refusal: none is set, or it names a cloud model
/// whose provider has no API key saved. `key_saved` answers that for a
/// provider (the keyring is the caller's I/O). Nothing else is consulted.
pub fn resolve_verifier(
    settings: &LLMSettingsDto,
    key_saved: impl FnOnce(CloudProvider) -> Result<bool>,
) -> Result<ResolvedModel> {
    let model = settings.verifier_model.clone().ok_or_else(|| {
        AppError::InvalidConfig(
            "No Verifier model is set. Choose one in Settings → Downloaded.".into(),
        )
    })?;
    if let ModelRef::Cloud {
        provider,
        model: name,
    } = &model
    {
        if !key_saved(*provider)? {
            return Err(AppError::InvalidConfig(format!(
                "Your Verifier model {}/{name} has no API key saved.",
                cloud_provider_name(*provider)
            )));
        }
    }
    Ok(ResolvedModel {
        model,
        origin: ModelOrigin::VerifierRole,
    })
}

/// A cloud provider as the user knows it: "OpenAI", "Anthropic", "Google".
pub fn cloud_provider_name(provider: CloudProvider) -> &'static str {
    match provider {
        CloudProvider::Openai => "OpenAI",
        CloudProvider::Anthropic => "Anthropic",
        CloudProvider::Google => "Google",
    }
}

/// A cloud provider's stored id, as Settings and the consent column spell it.
pub fn cloud_provider_id(provider: CloudProvider) -> &'static str {
    match provider {
        CloudProvider::Openai => "openai",
        CloudProvider::Anthropic => "anthropic",
        CloudProvider::Google => "google",
    }
}

#[cfg(test)]
mod tests;
