//! A turn's model: resolved once from the request, the chat's own choice
//! and the Settings default, pinned, and announced before the first token.
use super::prepare::pin_model;
use super::*;
use crate::features::conversation::chat::test_runtime::{default_model, FakeRuntime, Scripted};
use crate::features::conversation::ConversationServiceTrait;
use lattice_core::application::ports::llm_port::{ModelOrigin, ModelRequirements};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::conversation::ChatModelChoice;
use lattice_core::domain::models::model_ref::ModelRef;
use std::sync::Mutex;

fn local(model_id: &str) -> ModelRef {
    ModelRef::Local {
        model_id: model_id.into(),
    }
}

/// Settings default A; B and C downloaded beside it.
fn runtime() -> (FakeRuntime, [Arc<Scripted>; 3]) {
    let [a, b, c] = [(); 3].map(|_| Scripted::new([]));
    let runtime = FakeRuntime {
        llm: Some(a.clone()),
        models: vec![
            (local("model-b"), b.clone() as Arc<dyn LLMPort>),
            (local("model-c"), c.clone() as Arc<dyn LLMPort>),
        ],
        ..FakeRuntime::default()
    };
    (runtime, [a, b, c])
}

async fn chat(runtime: &FakeRuntime, choice: ChatModelChoice) -> String {
    let id = runtime
        .conversations
        .create_conversation("Chat".into(), None)
        .await
        .unwrap()
        .id
        .to_string();
    runtime.conversations.set_model_choice(&id, choice).unwrap();
    id
}

fn explicit(model_id: &str) -> ChatModelChoice {
    ChatModelChoice::Explicit {
        model: local(model_id),
    }
}

fn recording_sink() -> (ChatEventSink, Arc<Mutex<Vec<ChatStreamEventDto>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let sink: ChatEventSink = Arc::new(move |event| {
        captured.lock().unwrap().push(event);
        Ok(())
    });
    (sink, events)
}

fn same(left: &Arc<dyn LLMPort>, right: &Arc<Scripted>) -> bool {
    std::ptr::addr_eq(Arc::as_ptr(left), Arc::as_ptr(right))
}

#[tokio::test]
async fn a_chat_that_chose_a_model_runs_on_it_while_the_default_is_another() {
    let (runtime, [a, b, _]) = runtime();
    let overridden = chat(&runtime, explicit("model-b")).await;
    let following = chat(&runtime, ChatModelChoice::Default).await;
    let (sink, events) = recording_sink();

    let (pin, model) = pin_model(
        &runtime,
        &overridden,
        "turn-1",
        None,
        ModelRequirements::default(),
        &sink,
    )
    .await
    .unwrap();
    assert!(same(&pin.llm, &b));
    assert_eq!(model.model, Some(local("model-b")));
    assert_eq!(model.origin, Some(ModelOrigin::ChatOverride));
    assert_eq!(model.provider.as_deref(), Some(b.provider_name()));
    assert_eq!(model.context_tokens, Some(32_768));

    let (pin, model) = pin_model(
        &runtime,
        &following,
        "turn-2",
        None,
        ModelRequirements::default(),
        &sink,
    )
    .await
    .unwrap();
    assert!(same(&pin.llm, &a));
    assert_eq!(model.model, Some(default_model()));
    assert_eq!(model.origin, Some(ModelOrigin::Default));

    // Announced once per turn, before anything else is streamed.
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].conversation_id, overridden);
    assert_eq!(events[0].request_id, "turn-1");
    assert_eq!(
        events[0].model.as_ref().and_then(|model| model.origin),
        Some(ModelOrigin::ChatOverride)
    );
    assert!(events[0].content.is_none() && !events[0].done);
}

#[tokio::test]
async fn a_new_settings_default_moves_default_chats_and_never_an_overridden_one() {
    let (runtime, [_, b, c]) = runtime();
    let overridden = chat(&runtime, explicit("model-b")).await;
    let following = chat(&runtime, ChatModelChoice::Default).await;
    // Settings now names C as the default chat model.
    let changed = FakeRuntime {
        llm: Some(c.clone()),
        ..runtime.clone()
    };
    let (sink, _) = recording_sink();

    let (pin, _) = pin_model(
        &changed,
        &following,
        "turn-1",
        None,
        ModelRequirements::default(),
        &sink,
    )
    .await
    .unwrap();
    assert!(same(&pin.llm, &c));
    let (pin, _) = pin_model(
        &changed,
        &overridden,
        "turn-2",
        None,
        ModelRequirements::default(),
        &sink,
    )
    .await
    .unwrap();
    assert!(same(&pin.llm, &b));
}

#[tokio::test]
async fn trying_another_model_answers_with_it_and_persists_nothing() {
    let (runtime, [_, _, c]) = runtime();
    let overridden = chat(&runtime, explicit("model-b")).await;
    let (sink, _) = recording_sink();

    let (pin, model) = pin_model(
        &runtime,
        &overridden,
        "turn-1",
        Some(&local("model-c")),
        ModelRequirements::default(),
        &sink,
    )
    .await
    .unwrap();

    assert!(same(&pin.llm, &c));
    assert_eq!(model.origin, Some(ModelOrigin::RequestOverride));
    assert_eq!(
        runtime.conversations.model_choice(&overridden).unwrap(),
        explicit("model-b")
    );
}

#[tokio::test]
async fn a_chosen_model_that_is_gone_fails_the_turn_and_never_falls_back() {
    let (runtime, _) = runtime();
    let gone = chat(&runtime, explicit("deleted-model")).await;
    let (sink, events) = recording_sink();

    let error = pin_model(
        &runtime,
        &gone,
        "turn-1",
        None,
        ModelRequirements::default(),
        &sink,
    )
    .await
    .err()
    .expect("the turn must not run on another model");

    assert!(
        matches!(error, AppError::AiModelsNotInstalled(_)),
        "{error:?}"
    );
    assert!(events.lock().unwrap().is_empty(), "nothing was announced");
}
