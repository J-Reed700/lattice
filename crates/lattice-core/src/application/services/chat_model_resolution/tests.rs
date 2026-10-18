//! Tests for chat precedence, each provider's default and the Learning role.

#![cfg(test)]

use super::*;

fn local(model_id: &str) -> ModelRef {
    ModelRef::Local {
        model_id: model_id.into(),
    }
}

fn ollama(model: &str) -> ModelRef {
    ModelRef::Ollama {
        model: model.into(),
    }
}

fn settings(provider: LLMProvider) -> LLMSettingsDto {
    let mut settings = LLMSettingsDto {
        provider,
        model: "qwen3:8b".into(),
        ..Default::default()
    };
    settings.llama_cpp.model = "qwen3.5-27b".into();
    settings
}

fn no_default() -> DefaultChatModel {
    DefaultChatModel(Err(AppError::AiModelsNotInstalled("nothing".into())))
}

fn explicit(model: ModelRef) -> ChatModelChoice {
    ChatModelChoice::Explicit { model }
}

#[test]
fn a_request_override_wins_over_the_chat_and_the_default() {
    let default = DefaultChatModel(Ok(local("default")));
    for default in [default, no_default()] {
        let resolved =
            resolve(Some(&local("override")), &explicit(local("chat")), &default).unwrap();
        assert_eq!(
            resolved,
            ResolvedModel {
                model: local("override"),
                origin: ModelOrigin::RequestOverride
            }
        );
    }
}

#[test]
fn a_chat_choice_wins_over_the_default_and_never_falls_through() {
    let default = DefaultChatModel(Ok(local("default")));
    for default in [default, no_default()] {
        let resolved = resolve(None, &explicit(ollama("chat")), &default).unwrap();
        assert_eq!(
            resolved,
            ResolvedModel {
                model: ollama("chat"),
                origin: ModelOrigin::ChatOverride
            }
        );
    }
}

#[test]
fn a_default_chat_follows_the_settings_default_and_its_failure() {
    let default = DefaultChatModel(Ok(local("default")));
    assert_eq!(
        resolve(None, &ChatModelChoice::Default, &default).unwrap(),
        ResolvedModel {
            model: local("default"),
            origin: ModelOrigin::Default
        }
    );
    let error = resolve(None, &ChatModelChoice::Default, &no_default()).unwrap_err();
    assert!(matches!(error, AppError::AiModelsNotInstalled(message) if message == "nothing"));
}

#[test]
fn each_explicit_provider_names_its_own_model_whatever_row_is_active() {
    let cloud = |provider| ModelRef::Cloud {
        provider,
        model: "qwen3:8b".into(),
    };
    for (provider, expected) in [
        (LLMProvider::Local, local("ornith-1.5-9b")),
        (LLMProvider::Ollama, ollama("qwen3:8b")),
        (
            LLMProvider::Llamacpp,
            ModelRef::LlamaCpp {
                model: "qwen3.5-27b".into(),
            },
        ),
        (LLMProvider::Openai, cloud(CloudProvider::Openai)),
        (LLMProvider::Anthropic, cloud(CloudProvider::Anthropic)),
        (LLMProvider::Google, cloud(CloudProvider::Google)),
    ] {
        let default = default_chat_model(&settings(provider), Some("ornith-1.5-9b"));
        assert_eq!(default.0.unwrap(), expected, "{provider:?}");
    }
}

#[test]
fn local_without_an_active_row_has_no_default() {
    let error = default_chat_model(&settings(LLMProvider::Local), None)
        .0
        .unwrap_err();
    assert!(matches!(
        error,
        AppError::AiModelsNotInstalled(message)
            if message == "No chat model is selected in Settings → Models"
    ));
}

#[test]
fn auto_names_the_first_model_the_loader_tries() {
    let auto = settings(LLMProvider::Auto);
    assert_eq!(
        default_chat_model(&auto, Some("ornith-1.5-9b")).0.unwrap(),
        local("ornith-1.5-9b")
    );
    assert_eq!(
        default_chat_model(&auto, None).0.unwrap(),
        ModelRef::LlamaCpp {
            model: "qwen3.5-27b".into()
        }
    );

    let mut no_llama_cpp = auto.clone();
    no_llama_cpp.llama_cpp.model = "  ".into();
    assert_eq!(
        default_chat_model(&no_llama_cpp, None).0.unwrap(),
        ollama("qwen3:8b")
    );

    let mut nothing = no_llama_cpp;
    nothing.ollama_url.clear();
    assert!(matches!(
        default_chat_model(&nothing, None).0,
        Err(AppError::AiModelsNotInstalled(_))
    ));
}

#[test]
fn an_unset_learning_model_refuses_instead_of_borrowing_the_chat_default() {
    let error = resolve_learning(&settings(LLMProvider::Ollama)).unwrap_err();
    assert!(matches!(
        error,
        AppError::InvalidConfig(message)
            if message == "No Learning model is set. Choose one in Settings → Models under Learning."
    ));
}

#[test]
fn a_set_learning_model_is_used_as_named() {
    let mut settings = settings(LLMProvider::Ollama);
    settings.learning_model = Some(local("qwen3.5-27b"));
    assert_eq!(
        resolve_learning(&settings).unwrap(),
        ResolvedModel {
            model: local("qwen3.5-27b"),
            origin: ModelOrigin::LearningRole
        }
    );
}

fn cloud(provider: CloudProvider, model: &str) -> ModelRef {
    ModelRef::Cloud {
        provider,
        model: model.into(),
    }
}

#[test]
fn an_unset_verifier_model_refuses_instead_of_borrowing_another_role() {
    let mut settings = settings(LLMProvider::Ollama);
    settings.learning_model = Some(local("qwen3.5-27b"));
    let error = resolve_verifier(&settings, |_| Ok(true)).unwrap_err();
    assert!(matches!(
        error,
        AppError::InvalidConfig(message)
            if message == "No Verifier model is set. Choose one in Settings → Models under Verifier."
    ));
}

#[test]
fn a_cloud_verifier_with_no_saved_key_refuses_naming_the_model() {
    let mut settings = settings(LLMProvider::Local);
    settings.verifier_model = Some(cloud(CloudProvider::Anthropic, "claude-x"));
    let error = resolve_verifier(&settings, |provider| {
        assert_eq!(provider, CloudProvider::Anthropic);
        Ok(false)
    })
    .unwrap_err();
    assert!(matches!(
        error,
        AppError::InvalidConfig(message)
            if message == "Your Verifier model Anthropic/claude-x has no API key saved."
    ));
}

#[test]
fn a_keyring_failure_is_reported_not_taken_for_a_missing_key() {
    let mut settings = settings(LLMProvider::Local);
    settings.verifier_model = Some(cloud(CloudProvider::Openai, "gpt-x"));
    let error = resolve_verifier(&settings, |_| {
        Err(AppError::InternalError("keyring locked".into()))
    })
    .unwrap_err();
    assert!(matches!(error, AppError::InternalError(message) if message == "keyring locked"));
}

#[test]
fn a_verifier_is_used_as_named_whatever_its_kind() {
    for model in [
        cloud(CloudProvider::Openai, "gpt-x"),
        local("qwen3.5-27b"),
        ollama("qwen3:32b"),
        ModelRef::LlamaCpp {
            model: "remote-27b".into(),
        },
    ] {
        let mut settings = settings(LLMProvider::Local);
        settings.verifier_model = Some(model.clone());
        let key_checked = std::cell::Cell::new(false);
        let resolved = resolve_verifier(&settings, |_| {
            key_checked.set(true);
            Ok(true)
        })
        .unwrap();
        assert_eq!(
            resolved,
            ResolvedModel {
                model: model.clone(),
                origin: ModelOrigin::VerifierRole
            }
        );
        assert_eq!(
            key_checked.get(),
            matches!(model, ModelRef::Cloud { .. }),
            "only a cloud model's key is looked up"
        );
    }
}
