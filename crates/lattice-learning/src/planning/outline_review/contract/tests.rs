#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::lessons::content_verification::tests::fixture_task;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
use lattice_core::application::services::model_contract::ask::{ask_new, Plain};
use lattice_core::application::services::model_contract::shape::{
    Ended, Found, Listed, Pointers, Written,
};
use lattice_core::application::services::model_contract::shown::Scope;
use lattice_core::application::services::model_contract::test_support::samples::schema_of;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};

const OPEN_ID: &str = "finding-7000001";

fn input(marks: &mut Marks) -> Input {
    Input {
        context: marks.told(json!({"goal":"Run a harbour tide-table office"})),
        open: vec![Finding {
            id: marks.internal(OPEN_ID.to_owned()),
            path: marks.told("/modules/0/lessons/0".to_owned()),
            kind: Told("plan".to_owned()),
            claim: marks.told("Chart the tide before the crane lifts.".to_owned()),
            reason: marks.told("The objective names no measurement.".to_owned()),
            attempts: Told(1),
        }],
        places: vec![
            Place {
                path: marks.told("/modules/0".to_owned()),
            },
            Place {
                path: marks.told("/modules/0/summary".to_owned()),
            },
        ],
    }
}

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn task(sent: &Sent) -> Value {
    let prompt = sent
        .request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    fixture_task(&prompt).unwrap()
}

fn verdict(at: &str, reason: &str) -> Value {
    json!({"entry":{"kind":"verdict","at":at,"status":"resolved","reason":reason}})
}

fn finding(place: &str, reason: &str) -> Value {
    json!({"entry":{"kind":"finding","place":place,"category":"plan","reason":reason}})
}

async fn asked(model: &ScriptedModel) -> Result<Found<Entry>> {
    ask_new::<OutlineReview>(
        model,
        &Plain,
        &mut Scope::for_question(),
        input(&mut Marks::new()),
        |_| None,
        &crate::model_call::answer_options(),
    )
    .await
}

#[test]
fn the_request_holds_no_internal_value_and_every_told_one() {
    let mut marks = Marks::new();
    let input = input(&mut marks);
    holds::<OutlineReview>(&input, &marks);
}

#[test]
fn the_request_shows_open_findings_and_places_by_handle() {
    let input = input(&mut Marks::new());
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let says = OutlineReview::says(&input, &mut showing).unwrap();
    assert!(says.instructions.starts_with(REVIEW_SYSTEM));
    assert!(!says.instructions.contains("by id"));
    let task: Value = serde_json::from_str(&says.task).unwrap();
    assert_eq!(task["openFindings"][0]["id"], "f1");
    assert_eq!(task["openFindings"][0]["path"], "/modules/0/lessons/0");
    assert_eq!(task["allowedPaths"][1]["id"], "p2");
    assert_eq!(task["allowedPaths"][1]["path"], "/modules/0/summary");
    assert!(!says.task.contains(OPEN_ID));
}

#[test]
fn the_schema_points_at_what_is_shown_and_asks_for_no_claim() {
    let schema = schema_of::<OutlineReview>(&input(&mut Marks::new()), Pointers::Enum).unwrap();
    let written = schema.as_json().to_string();
    for named in [
        "\"f1\"",
        "\"p1\"",
        "\"p2\"",
        "\"verdict\"",
        "\"finding\"",
        "\"resolved\"",
        "\"unresolved\"",
        "\"factual\"",
        "\"plan\"",
    ] {
        assert!(written.contains(named), "{named} is not in {written}");
    }
    assert!(!written.contains("\"claim\""), "{written}");
    let caps = crate::content_caps_tests::text_caps(schema.as_json());
    assert!(caps.is_empty(), "{caps:?}");
}

#[tokio::test]
async fn a_complete_answer_is_read_entry_by_entry() {
    let model = ScriptedModel::new().on_request::<OutlineReview>(|sent, _| {
        let task = task(sent);
        let open = task["openFindings"][0]["id"].as_str().unwrap();
        let place = task["allowedPaths"][1]["id"].as_str().unwrap();
        reply(
            json!({"items":[
                verdict(open, "The objective now names the tide."),
                finding(place, "The summary repeats the lesson."),
            ]})
            .to_string(),
        )
    });
    let found = asked(&model).await.unwrap();
    assert!(found.rest.is_none());
    assert_eq!(found.items.len(), 2);
    assert!(matches!(
        &found.items[0],
        Entry::Verdict { at, status: Status::Resolved, .. } if at.reference == OPEN_ID
    ));
    assert!(matches!(
        &found.items[1],
        Entry::Found { place, category: Category::Plan, .. } if place.reference == "/modules/0/summary"
    ));
}

#[tokio::test]
async fn an_entry_pointing_at_nothing_shown_is_left_out_and_the_others_stand() {
    let model = ScriptedModel::new().on_request::<OutlineReview>(|sent, _| {
        let task = task(sent);
        let open = task["openFindings"][0]["id"].as_str().unwrap();
        reply(
            json!({"items":[
                finding("p9", "A place that is not shown."),
                verdict(open, "The objective now names the tide."),
            ]})
            .to_string(),
        )
    });
    let found = asked(&model).await.unwrap();
    assert!(found.rest.is_none());
    assert_eq!(found.items.len(), 1);
    assert!(matches!(&found.items[0], Entry::Verdict { .. }));
}

#[tokio::test]
async fn an_entry_without_a_reason_is_refused_alone() {
    let model = ScriptedModel::new().on_request::<OutlineReview>(|sent, _| {
        let task = task(sent);
        let open = task["openFindings"][0]["id"].as_str().unwrap();
        let place = task["allowedPaths"][0]["id"].as_str().unwrap();
        reply(
            json!({"items":[
                verdict(open, "  "),
                finding(place, "The module title promises a harbour survey."),
            ]})
            .to_string(),
        )
    });
    let found = asked(&model).await.unwrap();
    assert_eq!(found.items.len(), 1);
    assert!(matches!(&found.items[0], Entry::Found { .. }));
}

#[tokio::test]
async fn an_answer_that_is_never_read_fails_the_review() {
    let model = ScriptedModel::new()
        .on_request::<OutlineReview>(|_, _| reply("The outline looks fine.".to_owned()));
    assert!(asked(&model).await.is_err());
}

#[test]
fn an_answer_cut_off_after_an_entry_keeps_that_entry_and_is_not_complete() {
    let input = input(&mut Marks::new());
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    showing.show(&input.open).unwrap();
    showing.show(&input.places).unwrap();
    let whole = json!({"items":[verdict("f1", "The objective now names the tide."), finding("p1", "The module title is vague.")]})
        .to_string();
    let cut = &whole[..whole.len() - 20];
    let found = <New<Entry> as Listed>::read(
        &Written {
            text: cut,
            ended: Ended::CutOff,
        },
        &showing,
    );
    assert_eq!(found.items.len(), 1);
    assert!(found.rest.is_some());
}
