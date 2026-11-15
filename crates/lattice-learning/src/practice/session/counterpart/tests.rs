#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::session::fake_kind::{
    fake_kind, letters_exercise, letters_observation, FakeKindBuilder, ScriptedModel,
};
use crate::practice::session::policy::{policy_for, Cue};
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::domain::learning::practice::work::{PartRef, Setting};
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

fn plain() -> FakeKindBuilder {
    fake_kind(&["a", "b", "c"])
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

fn reply(content: &str, parts: &[&str]) -> String {
    serde_json::json!({
        "content": content,
        "parts": parts,
        "clarification": {"kind": "none"},
    })
    .to_string()
}

fn clarifying(content: &str, clarification: serde_json::Value) -> String {
    serde_json::json!({
        "content": content,
        "parts": [],
        "clarification": clarification,
    })
    .to_string()
}

fn properties_of(model: &ScriptedModel) -> Vec<String> {
    let requests = model.requests();
    let schema = requests[0].schema().unwrap().as_json().clone();
    schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

#[tokio::test]
async fn the_schema_holds_content_parts_and_clarification_and_speak_only_on_a_checkpoint() {
    let kind = plain().build();
    let ordinary = reply("Look again.", &[]);
    let (result, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&ordinary]).await;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(properties_of(&model), ["content", "parts", "clarification"]);

    let spoken = serde_json::json!({
        "content": "Why this one?",
        "parts": [],
        "clarification": {"kind": "none"},
        "speak": "true",
    });
    let (result, model) = ask(
        &kind,
        Setting::Watched,
        &Cue::Checkpoint,
        &[&spoken.to_string()],
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        properties_of(&model),
        ["content", "parts", "clarification", "speak"]
    );
}

#[tokio::test]
async fn silence_is_none_on_a_checkpoint() {
    let kind = plain().build();
    let silent = serde_json::json!({
        "content": "",
        "parts": [],
        "clarification": {"kind": "none"},
        "speak": "false",
    })
    .to_string();
    let (result, model) = ask(&kind, Setting::Watched, &Cue::Checkpoint, &[&silent]).await;
    assert_eq!(result.unwrap(), None);
    assert_eq!(model.requests().len(), 1);
}

#[tokio::test]
async fn a_blank_reply_is_corrected_and_then_refused() {
    let kind = plain().build();
    let blank = reply("  \n ", &[]);
    let (result, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&blank, &blank]).await;
    assert_eq!(unusable(result), "The reply is blank. Say something.");
    assert_eq!(model.requests().len(), 2);
}

#[tokio::test]
async fn a_reply_that_cannot_be_read_is_asked_for_again() {
    let kind = plain().build();
    let good = reply("Look at the middle one.", &["l2"]);
    let (result, model) = ask(
        &kind,
        Setting::Coached,
        &Cue::Reply,
        &["no json here", &good],
    )
    .await;
    assert_eq!(
        result.unwrap().map(|reply| reply.parts),
        Some(vec![PartRef::new("1")])
    );
    assert_eq!(model.requests().len(), 2);
}

#[tokio::test]
async fn a_handle_that_names_no_part_costs_only_that_pointer() {
    let kind = plain().build();
    let pointing = reply("Compare these.", &["l9", "l1"]);
    let (result, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&pointing]).await;
    assert_eq!(
        result.unwrap(),
        Some(Reply {
            content: "Compare these.".to_owned(),
            parts: vec![PartRef::new("0")],
        })
    );
    assert_eq!(model.requests().len(), 1);
}

#[tokio::test]
async fn a_good_reply_returns_the_parts_it_points_at_in_order() {
    let kind = plain().build();
    let good = reply("Compare the last with the first.", &["l3", "l1"]);
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
async fn the_request_shows_each_part_under_its_handle() {
    let kind = plain().build();
    let good = reply("Look again.", &[]);
    let (_, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&good]).await;
    let text = &model.texts()[0];
    for row in ["[l1] slot 0", "[l2] slot 1", "[l3] slot 2"] {
        assert!(text.contains(row), "{row}: {text}");
    }
}

#[tokio::test]
async fn prompt_snapshot_practice_counterpart_reply() {
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let kind = plain().build();
    let good = reply("Compare the last with the first.", &["l3", "l1"]);
    let (result, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&good]).await;
    assert!(result.is_ok(), "{result:?}");
    snapshot::assert_requests(
        env!("CARGO_MANIFEST_DIR"),
        "practice-counterpart-reply",
        &model.requests(),
    );
}

#[tokio::test]
async fn the_kinds_word_refusal_reaches_the_second_request() {
    let kind = plain().refusing_word("forbidden").build();
    let bad = reply("That is a forbidden move.", &[]);
    let good = reply("That move needs another look.", &[]);
    let reason = "The reply says \"forbidden\". Never say it.";

    let (result, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&bad, &good]).await;
    assert!(result.unwrap().is_some());
    let texts = model.texts();
    assert!(!texts[0].contains(reason));
    assert!(texts[1].contains(reason), "{}", texts[1]);
    assert!(texts[1].contains("That is a forbidden move."));

    let (result, _) = ask(&kind, Setting::Coached, &Cue::Reply, &[&bad, &bad]).await;
    assert_eq!(unusable(result), reason);
}

#[tokio::test]
async fn a_reply_that_solves_the_exercise_is_asked_again_once_with_the_reason() {
    let kind = plain().solving_with("SOLVED").build();
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
    let kind = plain().refusing_word("forbidden").build();
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
async fn a_clarification_the_reply_points_at_is_answered_in_the_apps_words() {
    let kind = plain()
        .answering("May the tide table be empty?", "Yes, and its total is 0.")
        .build();
    let pointing = clarifying(
        "Yes, it may.",
        serde_json::json!({"kind": "from", "which": "k1"}),
    );
    let (result, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&pointing]).await;
    assert_eq!(
        result.unwrap().map(|reply| reply.content),
        Some("Yes, it may.\n\nYes, and its total is 0.".to_owned())
    );
    assert_eq!(model.requests().len(), 1);
    let text = &model.texts()[0];
    assert!(
        text.contains(
            "[k1] asked \"May the tide table be empty?\", answered \"Yes, and its total is 0.\""
        ),
        "{text}"
    );
}

#[tokio::test]
async fn your_call_adds_the_kinds_own_line() {
    let kind = plain()
        .otherwise_adding("Your call: state your assumption.")
        .build();
    let pointing = clarifying("It is up to you.", serde_json::json!({"kind": "your_call"}));
    let (result, _) = ask(&kind, Setting::Coached, &Cue::Reply, &[&pointing]).await;
    assert_eq!(
        result.unwrap().map(|reply| reply.content),
        Some("It is up to you.\n\nYour call: state your assumption.".to_owned())
    );
}

#[tokio::test]
async fn a_clarification_that_names_nothing_shown_is_asked_for_again() {
    let kind = plain().answering("Is it sorted?", "Yes.").build();
    let bad = clarifying("Yes.", serde_json::json!({"kind": "from", "which": "k9"}));
    let good = clarifying("Yes.", serde_json::json!({"kind": "from", "which": "k1"}));
    let (result, model) = ask(&kind, Setting::Coached, &Cue::Reply, &[&bad, &good]).await;
    assert_eq!(
        result.unwrap().map(|reply| reply.content),
        Some("Yes.\n\nYes.".to_owned())
    );
    assert!(
        model.texts()[1].contains("not shown"),
        "{}",
        model.texts()[1]
    );
}

fn fixture(marks: &mut Marks, earlier: bool) -> Input {
    Input {
        system: marks.told("You are the coach in a practice session.".to_owned()),
        observed: marks.told("Task: Build. Draft: a, x, c.".to_owned()),
        places: vec![
            Place {
                part: marks.reference(PartRef::new("slot-key-0001")),
                name: marks.told("slot number one".to_owned()),
            },
            Place {
                part: marks.reference(PartRef::new("slot-key-0002")),
                name: marks.told("slot number two".to_owned()),
            },
        ],
        facts: vec![Fact {
            at: marks.reference(700_001_usize),
            question: marks.told("May the tide table be empty?".to_owned()),
            answer: marks.told("Yes, and its total is 0.".to_owned()),
        }],
        earlier: earlier.then(|| Earlier {
            reply: marks.told("That is a forbidden move.".to_owned()),
            why: marks.told("The reply says forbidden.".to_owned()),
        }),
    }
}

#[test]
fn the_request_tells_what_it_must_and_writes_no_internal_value() {
    for earlier in [false, true] {
        let mut marks = Marks::new();
        let input = fixture(&mut marks, earlier);
        holds::<CounterpartReply<Ordinary>>(&input, &marks);
        let mut marks = Marks::new();
        let input = fixture(&mut marks, earlier);
        holds::<CounterpartReply<Checkpointed>>(&input, &marks);
    }
}
