#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::session::fake_kind::{
    fake_kind, letters_exercise, letters_observation, FakeKindBuilder, ScriptedModel,
};
use crate::practice::session::policy::{policy_for, Cue};
use lattice_core::domain::learning::practice::work::{PartRef, Setting};
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

fn slotted() -> FakeKindBuilder {
    fake_kind(&["a", "b", "c"]).with_reply_slot()
}

async fn ask(
    kind: &Kind,
    setting: Setting,
    cue: &Cue,
    answers: &[&str],
) -> (Result<Option<Reply>>, ScriptedModel) {
    let model = ScriptedModel::new(answers.iter().copied());
    let observation = letters_observation(setting);
    let policy = policy_for(setting, kind, observation.form);
    let result = respond(
        &model,
        kind,
        &letters_exercise(),
        policy.as_ref(),
        cue,
        &observation,
        CancellationToken::new(),
    )
    .await;
    (result, model)
}

fn unusable(result: Result<Option<Reply>>) -> String {
    match result {
        Err(AppError::ModelAnswerUnusable(why)) => why,
        other => format!("not unusable: {other:?}"),
    }
}

fn reply(content: &str, slot: &[u32]) -> String {
    serde_json::json!({"content": content, "slot": slot}).to_string()
}

fn schema_of(model: &ScriptedModel) -> String {
    let requests = model.requests();
    let schema = requests[0].json_schema.clone().unwrap();
    serde_json::to_string(&schema).unwrap()
}

#[tokio::test]
async fn the_schema_holds_content_then_the_kinds_fields_then_speak_only_on_a_checkpoint() {
    let kind = slotted().build();
    let plain = reply("Look again.", &[]);
    let (result, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&plain]).await;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        schema_of(&model),
        r#"{"type":"object","additionalProperties":false,"required":["content","slot"],"properties":{"content":{"type":"string"},"slot":{"type":"array","items":{"type":"integer"}}}}"#
    );

    let spoken = serde_json::json!({"content": "Why this one?", "slot": [], "speak": true});
    let (result, model) = ask(
        &kind,
        Setting::Watched,
        &Cue::Checkpoint,
        &[&spoken.to_string()],
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        schema_of(&model),
        r#"{"type":"object","additionalProperties":false,"required":["content","slot","speak"],"properties":{"content":{"type":"string"},"slot":{"type":"array","items":{"type":"integer"}},"speak":{"type":"boolean"}}}"#
    );
}

#[tokio::test]
async fn a_kind_without_fields_has_content_alone() {
    let kind = fake_kind(&["a", "b", "c"]).build();
    let (result, model) = ask(
        &kind,
        Setting::Coached,
        &Cue::Reply,
        &[r#"{"content":"Look again."}"#],
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        schema_of(&model),
        r#"{"type":"object","additionalProperties":false,"required":["content"],"properties":{"content":{"type":"string"}}}"#
    );
}

#[tokio::test]
async fn silence_is_none_on_a_checkpoint_and_refused_on_any_other_cue() {
    let kind = slotted().build();
    let silent = serde_json::json!({"content": "", "slot": [], "speak": false}).to_string();
    let (result, model) = ask(&kind, Setting::Watched, &Cue::Checkpoint, &[&silent]).await;
    assert_eq!(result.unwrap(), None);
    assert_eq!(model.requests().len(), 1);

    for cue in [Cue::Reply, Cue::Opening, Cue::Submitted, Cue::Idle] {
        let (result, _) = ask(&kind, Setting::Watched, &cue, &[&silent, &silent]).await;
        assert_eq!(
            unusable(result),
            "Only a checkpoint may stay silent. Answer the learner.",
            "{cue:?}"
        );
    }
}

#[tokio::test]
async fn a_blank_reply_is_refused() {
    let kind = slotted().build();
    let blank = reply("  \n ", &[]);
    let (result, _) = ask(&kind, Setting::Coached, &Cue::Reply, &[&blank, &blank]).await;
    assert_eq!(unusable(result), "The reply is blank. Say something.");
}

#[tokio::test]
async fn an_internal_id_is_refused_by_name() {
    let kind = slotted().build();
    for id in ["case-1", "miss-1", "session-1", "turn-7"] {
        let bad = reply(&format!("Look at {id} again."), &[]);
        let (result, _) = ask(&kind, Setting::Coached, &Cue::Reply, &[&bad, &bad]).await;
        assert_eq!(
            unusable(result),
            format!("The reply contains the internal id {id}. Never write an id; describe a case by what it is about."),
            "{id}"
        );
    }
}

#[tokio::test]
async fn a_part_the_piece_does_not_hold_is_refused_with_the_kinds_reason() {
    let kind = slotted().build();
    let bad = reply("Look at the fourth one.", &[3]);
    let good = reply("Look at the middle one.", &[1]);
    let reason = "Slot 3 is not in the piece. Name only a slot the piece has.";

    let (result, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&bad, &good]).await;
    assert!(result.unwrap().is_some());
    let texts = model.texts();
    assert_eq!(texts.len(), 2);
    assert!(!texts[0].contains(reason));
    assert!(texts[1].contains(reason), "{}", texts[1]);

    let (result, _) = ask(&kind, Setting::Coached, &Cue::Reply, &[&bad, &bad]).await;
    assert_eq!(unusable(result), reason);
}

#[tokio::test]
async fn a_good_reply_returns_the_kinds_parts_in_order() {
    let kind = slotted().build();
    let good = reply("Compare the last with the first.", &[2, 0]);
    let (result, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&good]).await;
    assert_eq!(
        result.unwrap(),
        Some(Reply {
            content: "Compare the last with the first.".to_owned(),
            parts: vec![PartRef::new("2"), PartRef::new("0")],
        })
    );
    assert_eq!(model.requests().len(), 1);
}

#[tokio::test]
async fn the_kinds_word_refusal_reaches_the_second_request() {
    let kind = slotted().refusing_word("forbidden").build();
    let bad = reply("That is a forbidden move.", &[]);
    let good = reply("That move needs another look.", &[]);
    let reason = "The reply says \"forbidden\". Never say it.";

    let (result, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&bad, &good]).await;
    assert!(result.unwrap().is_some());
    assert!(model.texts()[1].contains(reason));

    let (result, _) = ask(&kind, Setting::Coached, &Cue::Reply, &[&bad, &bad]).await;
    assert_eq!(unusable(result), reason);
}

#[tokio::test]
async fn a_reply_that_solves_the_exercise_is_asked_again_once_with_the_reason() {
    let kind = slotted().solving_with("SOLVED").build();
    let bad = reply("Write SOLVED here.", &[]);
    let good = reply("Describe the step in words.", &[]);
    let reason = "The reply solves the exercise. Describe the step instead.";

    let (result, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&bad, &good]).await;
    assert_eq!(
        result.unwrap().map(|reply| reply.content),
        Some("Describe the step in words.".to_owned())
    );
    let texts = model.texts();
    assert_eq!(texts.len(), 2);
    assert!(texts[1].contains(reason), "{}", texts[1]);

    let (result, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&bad, &bad]).await;
    assert_eq!(unusable(result), reason);
    assert_eq!(model.requests().len(), 2);
}

#[tokio::test]
async fn an_interviewers_two_questions_are_refused_after_the_kinds_checks() {
    let kind = slotted().refusing_word("forbidden").build();
    let two = reply("What is the cost? And why that structure?", &[]);
    let (result, _) = ask(&kind, Setting::Mock, &Cue::Reply, &[&two, &two]).await;
    assert!(
        unusable(result).starts_with("An interviewer asks one question at a time."),
        "two questions"
    );

    let (result, _) = ask(&kind, Setting::Coached, &Cue::Reply, &[&two]).await;
    assert!(result.unwrap().is_some());

    let both = reply("Is that forbidden? And why?", &[]);
    let (result, _) = ask(&kind, Setting::Mock, &Cue::Reply, &[&both, &both]).await;
    assert_eq!(
        unusable(result),
        "The reply says \"forbidden\". Never say it."
    );
}

#[tokio::test]
async fn the_checks_run_in_order_parts_then_ids_then_words() {
    let kind = slotted().refusing_word("forbidden").build();

    let part_and_id = reply("Look at case-1 in the fourth one.", &[3]);
    let (result, _) = ask(
        &kind,
        Setting::Coached,
        &Cue::Reply,
        &[&part_and_id, &part_and_id],
    )
    .await;
    assert_eq!(
        unusable(result),
        "Slot 3 is not in the piece. Name only a slot the piece has."
    );

    let id_and_word = reply("The forbidden case-1 again.", &[]);
    let (result, _) = ask(
        &kind,
        Setting::Coached,
        &Cue::Reply,
        &[&id_and_word, &id_and_word],
    )
    .await;
    assert_eq!(
        unusable(result),
        "The reply contains the internal id case-1. Never write an id; describe a case by what it is about."
    );
}

#[test]
fn fields_are_read_back_as_a_struct() {
    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Shape {
        slot: Vec<u32>,
        #[serde(default)]
        note: Option<String>,
    }
    let fields = serde_json::json!({"slot": [1, 2]});
    let fields = fields.as_object().unwrap();
    assert_eq!(
        fields_as::<Shape>(fields).unwrap(),
        Shape {
            slot: vec![1, 2],
            note: None
        }
    );
}
