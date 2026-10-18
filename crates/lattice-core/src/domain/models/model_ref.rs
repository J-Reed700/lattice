//! The identity of a model, independent of Settings and of the role using it.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A model a chat or a role can name.
///
/// Endpoints are connection configuration, not identity: `Ollama` and
/// `LlamaCpp` carry the model only, and the URL and auth header come from
/// Settings when the model is loaded, so moving a server moves every chat
/// that names it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelRef {
    /// A downloaded GGUF row in `models` (`DownloadedModel::model_id`).
    Local { model_id: String },
    /// A tag served by the Ollama endpoint configured in Settings.
    Ollama { model: String },
    /// A model name served by the llama.cpp endpoint configured in Settings.
    LlamaCpp { model: String },
    /// A cloud provider's model; the key comes from the keyring.
    Cloud {
        provider: CloudProvider,
        model: String,
    },
}

/// A cloud provider, each with its own keyring entry and endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum CloudProvider {
    Openai,
    Anthropic,
    Google,
}

impl CloudProvider {
    fn as_str(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
        }
    }
}

/// Stable, log-friendly form: `local:ornith-1.5-9b`, `ollama:qwen3:8b`,
/// `llamacpp:qwen3.5-27b`, `cloud:openai/gpt-x`.
impl fmt::Display for ModelRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local { model_id } => write!(f, "local:{model_id}"),
            Self::Ollama { model } => write!(f, "ollama:{model}"),
            Self::LlamaCpp { model } => write!(f, "llamacpp:{model}"),
            Self::Cloud { provider, model } => write!(f, "cloud:{}/{model}", provider.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn every_kind() -> Vec<(ModelRef, serde_json::Value, &'static str)> {
        vec![
            (
                ModelRef::Local {
                    model_id: "ornith-1.5-9b".into(),
                },
                json!({"kind": "local", "model_id": "ornith-1.5-9b"}),
                "local:ornith-1.5-9b",
            ),
            (
                ModelRef::Ollama {
                    model: "qwen3:8b".into(),
                },
                json!({"kind": "ollama", "model": "qwen3:8b"}),
                "ollama:qwen3:8b",
            ),
            (
                ModelRef::LlamaCpp {
                    model: "qwen3.5-27b".into(),
                },
                json!({"kind": "llama_cpp", "model": "qwen3.5-27b"}),
                "llamacpp:qwen3.5-27b",
            ),
            (
                ModelRef::Cloud {
                    provider: CloudProvider::Openai,
                    model: "gpt-x".into(),
                },
                json!({"kind": "cloud", "provider": "openai", "model": "gpt-x"}),
                "cloud:openai/gpt-x",
            ),
        ]
    }

    #[test]
    fn every_kind_round_trips_through_its_wire_form() {
        for (model, wire, _) in every_kind() {
            assert_eq!(serde_json::to_value(&model).unwrap(), wire);
            assert_eq!(serde_json::from_value::<ModelRef>(wire).unwrap(), model);
        }
    }

    #[test]
    fn every_kind_has_a_stable_log_form() {
        for (model, _, logged) in every_kind() {
            assert_eq!(model.to_string(), logged);
        }
    }

    #[test]
    fn a_reference_that_does_not_name_a_model_is_an_error() {
        for unreadable in [
            json!({"kind": "local"}),
            json!({"kind": "local", "model": "ornith-1.5-9b"}),
            json!({"kind": "remote", "model": "qwen3:8b"}),
            json!({"kind": "cloud", "provider": "mistral", "model": "large"}),
            json!({"model_id": "ornith-1.5-9b"}),
            json!("local:ornith-1.5-9b"),
        ] {
            assert!(
                serde_json::from_value::<ModelRef>(unreadable.clone()).is_err(),
                "{unreadable} was read as a model"
            );
        }
    }
}
