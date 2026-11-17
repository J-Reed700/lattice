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

fn only(model: ModelRef) -> DefaultChatModel {
    DefaultChatModel(Ok(Candidates {
        models: vec![model],
        auto: false,
    }))
}

fn candidates(models: Vec<ModelRef>, auto: bool) -> Candidates {
    Candidates { models, auto }
}

fn llama_cpp(model: &str) -> ModelRef {
    ModelRef::LlamaCpp {
        model: model.into(),
    }
}

fn no_default() -> DefaultChatModel {
    DefaultChatModel(Err(AppError::AiModelsNotInstalled("nothing".into())))
}

fn explicit(model: ModelRef) -> ChatModelChoice {
    ChatModelChoice::Explicit { model }
}

#[test]
fn a_request_override_wins_over_the_chat_and_the_default() {
    let default = only(local("default"));
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
    let default = only(local("default"));
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
    let default = only(local("default"));
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
        assert_eq!(
            default.0.unwrap(),
            candidates(vec![expected], false),
            "{provider:?}"
        );
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
            if message == "No chat model is selected in Settings → Downloaded"
    ));
}

#[test]
fn auto_without_any_model_names_where_to_choose_one_and_where_to_add_a_server() {
    let mut auto = settings(LLMProvider::Auto);
    auto.model = String::new();
    auto.llama_cpp.model = String::new();
    auto.ollama_url = String::new();
    let error = default_chat_model(&auto, None).0.unwrap_err();
    assert!(matches!(
        error,
        AppError::AiModelsNotInstalled(message)
            if message == "No chat model is selected in Settings → Downloaded, and no llama.cpp or Ollama server is configured in Settings → Chat"
    ));
}

#[test]
fn auto_lists_every_model_the_loader_tries_in_order() {
    let mut auto = settings(LLMProvider::Auto);
    auto.llama_cpp.url = "http://localhost:8080".into();
    auto.ollama_configured = true;
    assert_eq!(
        default_chat_model(&auto, Some("ornith-1.5-9b")).0.unwrap(),
        candidates(
            vec![
                local("ornith-1.5-9b"),
                llama_cpp("qwen3.5-27b"),
                ollama("qwen3:8b")
            ],
            true
        )
    );
    assert_eq!(
        default_chat_model(&auto, None).0.unwrap(),
        candidates(vec![llama_cpp("qwen3.5-27b"), ollama("qwen3:8b")], true)
    );

    let mut no_llama_cpp = auto.clone();
    no_llama_cpp.llama_cpp.model = "  ".into();
    assert_eq!(
        default_chat_model(&no_llama_cpp, None).0.unwrap(),
        candidates(vec![ollama("qwen3:8b")], true)
    );

    let mut nothing = no_llama_cpp;
    nothing.ollama_configured = false;
    assert!(matches!(
        default_chat_model(&nothing, None).0,
        Err(AppError::AiModelsNotInstalled(_))
    ));
}

#[test]
fn auto_skips_llama_cpp_without_a_server_url() {
    let mut auto = settings(LLMProvider::Auto);
    auto.ollama_configured = true;
    for url in ["", "   "] {
        auto.llama_cpp.url = url.into();
        assert_eq!(
            default_chat_model(&auto, None).0.unwrap(),
            candidates(vec![ollama("qwen3:8b")], true),
            "url {url:?}"
        );
    }
}

fn fresh_install(model: &str) -> LLMSettingsDto {
    LLMSettingsDto {
        provider: LLMProvider::Auto,
        model: model.into(),
        ..Default::default()
    }
}

#[test]
fn auto_on_a_fresh_install_has_no_default_chat_model() {
    assert!(default_chat_model(&fresh_install("qwen3:8b"), None)
        .0
        .is_err());
}

#[test]
fn auto_does_not_take_a_leftover_cloud_model_name_for_an_ollama_tag() {
    assert!(default_chat_model(&fresh_install("claude-opus-5-5"), None)
        .0
        .is_err());
}

#[test]
fn auto_uses_ollama_once_its_connection_is_saved() {
    let mut saved = fresh_install("qwen3:8b");
    saved.ollama_configured = true;
    assert_eq!(
        default_chat_model(&saved, None).0.unwrap(),
        candidates(vec![ollama("qwen3:8b")], true)
    );

    let mut elsewhere = fresh_install("qwen3:8b");
    elsewhere.ollama_url = "https://ollama.example.com".into();
    assert_eq!(
        default_chat_model(&elsewhere, None).0.unwrap(),
        candidates(vec![ollama("qwen3:8b")], true)
    );

    let mut default_address = fresh_install("qwen3:8b");
    default_address.ollama_url = " http://localhost:11434/ ".into();
    assert!(default_chat_model(&default_address, None).0.is_err());
}

#[test]
fn a_llama_cpp_model_without_a_server_url_is_no_candidate() {
    let mut auto = fresh_install("qwen3:8b");
    auto.llama_cpp.model = "qwen3.5-27b".into();
    auto.llama_cpp.url = String::new();
    assert_eq!(llama_cpp_connection(&auto), None);
    assert!(default_chat_model(&auto, None).0.is_err());
    auto.llama_cpp.url = "http://localhost:8080".into();
    assert_eq!(llama_cpp_connection(&auto), Some(llama_cpp("qwen3.5-27b")));
}

#[test]
fn explicit_providers_trim_the_model_name() {
    let mut ollama_settings = settings(LLMProvider::Ollama);
    ollama_settings.model = "  qwen3:8b ".into();
    assert_eq!(
        default_chat_model(&ollama_settings, None).0.unwrap(),
        candidates(vec![ollama("qwen3:8b")], false)
    );
    let mut llama = settings(LLMProvider::Llamacpp);
    llama.llama_cpp.model = " qwen3.5-27b ".into();
    assert_eq!(
        default_chat_model(&llama, None).0.unwrap(),
        candidates(vec![llama_cpp("qwen3.5-27b")], false)
    );
}

#[test]
fn explicit_providers_refuse_an_empty_model_name() {
    let mut ollama_settings = settings(LLMProvider::Ollama);
    ollama_settings.model = "  ".into();
    let mut llama = settings(LLMProvider::Llamacpp);
    llama.llama_cpp.model = String::new();
    let mut anthropic = settings(LLMProvider::Anthropic);
    anthropic.model = String::new();
    for (settings, message) in [
        (
            ollama_settings,
            "No Ollama chat model is set in Settings → Chat",
        ),
        (llama, "No llama.cpp model is set in Settings → Chat"),
        (anthropic, "No Anthropic model is set in Settings → Chat"),
    ] {
        let error = default_chat_model(&settings, None).0.unwrap_err();
        assert!(
            matches!(&error, AppError::AiModelsNotInstalled(text) if text == message),
            "{error:?}"
        );
    }
}

fn shared_model_only(provider: LLMProvider) -> LLMSettingsDto {
    LLMSettingsDto {
        provider,
        model: "any-model-id".into(),
        ..Default::default()
    }
}

#[test]
fn the_shared_model_alone_never_establishes_an_ollama_connection() {
    for provider in [
        LLMProvider::Auto,
        LLMProvider::Local,
        LLMProvider::Llamacpp,
        LLMProvider::Openai,
        LLMProvider::Anthropic,
        LLMProvider::Google,
    ] {
        let settings = shared_model_only(provider);
        assert!(!ollama_saved(&settings), "{provider:?}");
        assert_eq!(ollama_connection(&settings), None, "{provider:?}");
        assert_eq!(llama_cpp_connection(&settings), None, "{provider:?}");
    }
}

#[test]
fn a_saved_ollama_endpoint_counts_without_being_selected() {
    let mut settings = shared_model_only(LLMProvider::Auto);
    settings.ollama_url = "https://ollama.example.com".into();
    assert!(ollama_saved(&settings));
    assert_eq!(ollama_connection(&settings), Some(ollama("any-model-id")));
}

#[test]
fn the_default_ollama_address_is_still_the_default_with_spaces_and_a_slash() {
    let mut settings = shared_model_only(LLMProvider::Auto);
    settings.ollama_url = " http://localhost:11434/ ".into();
    assert!(!ollama_saved(&settings));
}

#[test]
fn each_saved_ollama_setting_saves_the_connection() {
    let mut selected = shared_model_only(LLMProvider::Ollama);
    assert!(ollama_saved(&selected));
    selected.ollama_url = "  ".into();
    assert!(!ollama_saved(&selected));

    let mut utility = shared_model_only(LLMProvider::Auto);
    utility.ollama_utility_model = "utility:1b".into();
    assert!(ollama_saved(&utility));

    let mut header = shared_model_only(LLMProvider::Auto);
    header.ollama_auth_header_name = "X-Token".into();
    assert!(ollama_saved(&header));
}

#[test]
fn an_incomplete_saved_ollama_connection_gives_no_connection() {
    let mut no_url = shared_model_only(LLMProvider::Auto);
    no_url.ollama_configured = true;
    no_url.ollama_url = " ".into();
    assert!(!ollama_saved(&no_url));
    assert_eq!(ollama_connection(&no_url), None);

    let mut no_model = shared_model_only(LLMProvider::Auto);
    no_model.ollama_configured = true;
    no_model.model = " ".into();
    assert!(ollama_saved(&no_model));
    assert_eq!(ollama_connection(&no_model), None);
}

fn utility_settings(provider: LLMProvider) -> LLMSettingsDto {
    LLMSettingsDto {
        provider,
        model: "chat:8b".into(),
        ollama_utility_model: "util:1b".into(),
        ..Default::default()
    }
}

fn remote_row() -> Option<UtilityRow<'static>> {
    Some(UtilityRow {
        model_id: "row",
        local: false,
    })
}

fn anthropic(model: &str) -> ModelRef {
    cloud(CloudProvider::Anthropic, model)
}

#[test]
fn a_saved_utility_reference_wins_over_any_row() {
    let mut settings = utility_settings(LLMProvider::Auto);
    settings.utility_model = Some(anthropic("claude-haiku"));
    for row in [
        None,
        remote_row(),
        Some(UtilityRow {
            model_id: "row",
            local: true,
        }),
    ] {
        assert_eq!(
            utility_candidates(&settings, row),
            candidates(vec![anthropic("claude-haiku")], false)
        );
    }
}

#[test]
fn utility_work_has_no_model_without_a_row_or_a_saved_reference() {
    assert_eq!(
        utility_candidates(&utility_settings(LLMProvider::Auto), None),
        candidates(vec![], false)
    );
}

#[test]
fn a_local_utility_row_runs_locally() {
    let row = Some(UtilityRow {
        model_id: "ornith-1.5-9b",
        local: true,
    });
    assert_eq!(
        utility_candidates(&utility_settings(LLMProvider::Ollama), row),
        candidates(vec![local("ornith-1.5-9b")], false)
    );
}

#[test]
fn the_ollama_utility_row_follows_the_provider() {
    for (provider, expected) in [
        (LLMProvider::Local, candidates(vec![], false)),
        (
            LLMProvider::Ollama,
            candidates(vec![ollama("util:1b")], false),
        ),
        (
            LLMProvider::Llamacpp,
            candidates(vec![llama_cpp("util:1b")], false),
        ),
        (
            LLMProvider::Anthropic,
            candidates(vec![anthropic("util:1b")], false),
        ),
    ] {
        assert_eq!(
            utility_candidates(&utility_settings(provider), remote_row()),
            expected,
            "{provider:?}"
        );
    }
}

#[test]
fn under_auto_utility_work_tries_llama_cpp_before_a_saved_ollama() {
    let mut both = utility_settings(LLMProvider::Auto);
    both.llama_cpp.model = "chat-27b".into();
    assert_eq!(
        utility_candidates(&both, remote_row()),
        candidates(vec![llama_cpp("util:1b"), ollama("util:1b")], true)
    );

    let neither = LLMSettingsDto {
        provider: LLMProvider::Auto,
        model: "chat:8b".into(),
        ..Default::default()
    };
    assert!(utility_candidates(&neither, remote_row()).models.is_empty());
}

#[test]
fn the_utility_tag_falls_back_to_the_chat_tag() {
    let mut settings = utility_settings(LLMProvider::Ollama);
    settings.ollama_utility_model = " ".into();
    assert_eq!(ollama_utility_tag(&settings).as_deref(), Some("chat:8b"));
    assert_eq!(
        utility_candidates(&settings, remote_row()),
        candidates(vec![ollama("chat:8b")], false)
    );
    settings.model = String::new();
    assert_eq!(ollama_utility_tag(&settings), None);
    assert_eq!(
        utility_candidates(&settings, remote_row()),
        candidates(vec![], false)
    );
}

#[test]
fn an_unset_learning_model_refuses_instead_of_borrowing_the_chat_default() {
    let error = resolve_learning(&settings(LLMProvider::Ollama)).unwrap_err();
    assert!(matches!(
        error,
        AppError::LearningModelNotSet(message)
            if message == "No Learning model is set. Choose one in Settings → Downloaded."
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
            if message == "No Verifier model is set. Choose one in Settings → Downloaded."
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
