//! What the check does with the answers of its asks.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::confirm::{ConfirmPlanned, ConfirmUse};
use super::scripted::{rows, task_of};
use super::*;
use crate::lessons::structured_call::{is_draft_request, task_messages, FIXTURE_DRAFT};
use crate::review_findings::{FindingSite, PlanCheck, ReviewFinding};
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::services::model_contract::test_support::{
    ScriptedModel as Layered, Sent,
};
use lattice_core::application::services::model_contract::Contract;
use std::sync::atomic::{AtomicUsize, Ordering};

fn context(ideas: &[&str]) -> Value {
    json!({
        "priorKnowledge": "The learner has used a file manager.",
        "currentModule": {"title": "Shell basics"},
        "courseSequence": {"curriculum": [], "coursePlan": [
            {"title": "Looking around", "position": "earlier", "ideas": [], "terms": [], "actions": ["Listing the files in a folder"], "contrast": ""},
            {"title": "Working in the terminal", "position": "this", "ideas": ideas, "terms": [], "actions": [], "contrast": ""}
        ]}
    })
}

fn lesson(bodies: [&str; 3]) -> Value {
    let block = |title: &str, body: &str| {
        json!({"kind": "explanation", "title": title, "body": body, "rubric": [], "sourceIndex": 0, "quote": "An excerpt."})
    };
    json!({"blocks": [
        block("Opening", bodies[0]),
        block("Listing", bodies[1]),
        block("Reading", bodies[2])
    ], "questions": []})
}

/// The name and the words of each sentence a request shows.
fn shown(sent: &Sent) -> Vec<(String, String)> {
    let (_, user) = task_messages(&sent.request);
    rows(&task_of(user))
}

/// The name the request gives the sentence that holds `words`.
fn name_of(sent: &Sent, words: &str) -> String {
    shown(sent)
        .into_iter()
        .find(|(_, said)| said.contains(words))
        .map(|(name, _)| name)
        .unwrap()
}

/// The name and the text of each planned item a request shows.
fn planned_of(sent: &Sent) -> Vec<(String, String)> {
    let (_, user) = task_messages(&sent.request);
    task_of(user)["thisLesson"]["planned"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| {
            (
                item["id"].as_str().unwrap().to_owned(),
                item["text"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn on<C: Contract>(
    model: Layered,
    reply: impl Fn(&Sent) -> String + Send + Sync + 'static,
) -> Layered {
    model.on_request::<C>(move |sent, _| {
        let text = if is_draft_request(&sent.request) {
            FIXTURE_DRAFT.to_owned()
        } else {
            reply(sent)
        };
        Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
    })
}

/// The requests of `C` that are asks, not the prose step before them.
fn asks<C: Contract>(model: &Layered) -> Vec<Sent> {
    model
        .sent()
        .into_iter()
        .filter(|sent| sent.call.id == C::ID && !is_draft_request(&sent.request))
        .collect()
}

async fn run(
    model: &Layered,
    context: &Value,
    candidate: &Value,
    reported: &[&ReviewFinding],
) -> Result<Ruled> {
    check(
        model,
        context,
        candidate,
        &[],
        &mut Receipts::default(),
        reported,
    )
    .await
}

fn nothing_found(_: &Sent) -> String {
    json!({"items": []}).to_string()
}

fn nothing_taught(_: &Sent) -> String {
    json!({"entries": []}).to_string()
}

#[tokio::test]
async fn an_item_taught_with_no_sentence_cited_is_asked_again_alone() -> Result<()> {
    let seen = AtomicUsize::new(0);
    let model = on::<LessonContractCheck>(Layered::new(), move |sent| {
        let first = seen.fetch_add(1, Ordering::SeqCst) == 0;
        let entries: Vec<Value> = planned_of(sent)
            .iter()
            .map(|(id, text)| {
                let cited = if !first {
                    vec![name_of(sent, "second idea is")]
                } else if text.contains("First") {
                    vec![name_of(sent, "first idea is")]
                } else {
                    Vec::new()
                };
                json!({"of": id, "taught": "taught", "units": cited})
            })
            .collect();
        json!({"entries": entries}).to_string()
    });
    let model = on::<LessonContractFind>(model, nothing_found);
    let candidate = lesson([
        "A terminal takes typed commands.",
        "The first idea is explained here.",
        "The second idea is explained here.",
    ]);
    let ruled = run(
        &model,
        &context(&["First idea here", "Second idea here"]),
        &candidate,
        &[],
    )
    .await?;
    assert!(ruled.findings.is_empty(), "{:?}", ruled.findings);
    let sent = asks::<LessonContractCheck>(&model);
    assert_eq!(sent.len(), 2);
    assert_eq!(planned_of(&sent[0]).len(), 2);
    let again = planned_of(&sent[1]);
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].1, "Second idea here");
    Ok(())
}

#[tokio::test]
async fn an_item_the_ask_never_answers_goes_to_the_confirming_read() -> Result<()> {
    let model = on::<LessonContractCheck>(Layered::new(), nothing_taught);
    let model = on::<LessonContractFind>(model, nothing_found);
    let model = on::<ConfirmPlanned>(model, |sent| {
        let cited = name_of(sent, "typed commands");
        json!({"taught": "taught", "units": [cited], "belongs": "none"}).to_string()
    });
    let candidate = lesson([
        "A terminal takes typed commands.",
        "The list shows names.",
        "Read the names.",
    ]);
    let ruled = run(&model, &context(&["Typing commands"]), &candidate, &[]).await?;
    assert!(ruled.findings.is_empty(), "{:?}", ruled.findings);
    assert_eq!(asks::<ConfirmPlanned>(&model).len(), 1);
    Ok(())
}

/// The lesson's opening section uses something, then says what it is.
const USING: &str = "Open the terminal now.";
const TEACHING: &str = "A terminal takes typed commands.";

fn using_the_terminal(sent: &Sent) -> String {
    let uses = name_of(sent, USING);
    json!({"items": [{"finding": {"kind": "usedBeforeTaught", "what": "the terminal", "uses": [uses], "taughtLater": []}}]}).to_string()
}

/// What stands after the confirming read of the use cites `cited`.
async fn standing_after_citing(cited: &'static str) -> Result<Vec<ReviewFinding>> {
    let model = on::<LessonContractCheck>(Layered::new(), nothing_taught);
    let model = on::<LessonContractFind>(model, using_the_terminal);
    let model = on::<ConfirmUse>(model, move |sent| {
        let units = name_of(sent, cited);
        json!({"asked": "yes", "taught": "yes", "units": [units]}).to_string()
    });
    let opening = format!("{USING} {TEACHING}");
    let candidate = lesson([&opening, "The list shows names.", "Read the names."]);
    Ok(run(&model, &context(&[]), &candidate, &[]).await?.findings)
}

#[tokio::test]
async fn a_sentence_after_the_use_does_not_teach_it() -> Result<()> {
    let findings = standing_after_citing(TEACHING).await?;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].site, Some(FindingSite::Section(0)));
    assert_eq!(findings[0].passage.as_deref(), Some(USING));
    Ok(())
}

#[tokio::test]
async fn the_words_of_the_use_itself_teach_it() -> Result<()> {
    let findings = standing_after_citing(USING).await?;
    assert!(findings.is_empty(), "{findings:?}");
    Ok(())
}

#[tokio::test]
async fn a_confirming_read_that_gives_no_answer_fails_the_check() -> Result<()> {
    let model = on::<LessonContractCheck>(Layered::new(), nothing_taught);
    let model = on::<LessonContractFind>(model, using_the_terminal);
    let model = on::<ConfirmUse>(model, |_| "not an answer".to_owned());
    let candidate = lesson([USING, "The list shows names.", "Read the names."]);
    assert!(run(&model, &context(&[]), &candidate, &[]).await.is_err());
    Ok(())
}

fn first_section_report() -> ReviewFinding {
    ReviewFinding::plan(
        PlanCheck::Used,
        "kettle",
        FindingSite::Section(0),
        "The first section uses a kettle it never teaches.",
    )
}

#[tokio::test]
async fn a_report_the_ask_never_rules_on_counts_as_of_another_kind() -> Result<()> {
    let model = on::<LessonContractCheck>(Layered::new(), nothing_taught);
    let model = on::<LessonContractFind>(model, nothing_found);
    let model = on::<LessonContractReports>(model, nothing_taught);
    let candidate = lesson([USING, "The list shows names.", "Read the names."]);
    let report = first_section_report();
    let ruled = run(&model, &context(&[]), &candidate, &[&report]).await?;
    assert_eq!(ruled.outside, [0]);
    Ok(())
}

#[tokio::test]
async fn a_report_the_ask_rules_decided_is_not_outside() -> Result<()> {
    let model = on::<LessonContractCheck>(Layered::new(), nothing_taught);
    let model = on::<LessonContractFind>(model, nothing_found);
    let model = on::<LessonContractReports>(model, |sent| {
        let (_, user) = task_messages(&sent.request);
        let entries: Vec<Value> = task_of(user)["reported"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|report| json!({"of": report["id"], "about": "decided"}))
            .collect();
        json!({"entries": entries}).to_string()
    });
    let candidate = lesson([USING, "The list shows names.", "Read the names."]);
    let report = first_section_report();
    let ruled = run(&model, &context(&[]), &candidate, &[&report]).await?;
    assert!(ruled.outside.is_empty(), "{:?}", ruled.outside);
    Ok(())
}
