//! What the plan check is given of the review's reports.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::scripted::{ask_of, nothing, task_of, Ask};
use super::*;
use crate::content_verification::tests::fixture_completion;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use std::sync::Mutex;

fn report(site: Option<FindingSite>, note: &str, passage: Option<&str>) -> ReviewFinding {
    let mut finding =
        ReviewFinding::plan(PlanCheck::Contradiction, "beads", FindingSite::Lesson, note);
    finding.site = site;
    finding.passage = passage.map(str::to_owned);
    finding
}

/// The check's model: rules on the reports it is shown as `about` says, position by position, and keeps what it was sent.
struct Asked {
    about: Vec<bool>,
    asks: Mutex<Vec<(Ask, String, Value)>>,
    drafting: Mutex<Vec<String>>,
}

impl Asked {
    fn new(about: Vec<bool>) -> Self {
        Self {
            about,
            asks: Mutex::new(Vec::new()),
            drafting: Mutex::new(Vec::new()),
        }
    }

    /// The system text and the task of each request of the ask `ask`.
    fn of(&self, ask: Ask) -> Vec<(String, Value)> {
        self.asks
            .lock()
            .unwrap()
            .iter()
            .filter(|(kind, _, _)| *kind == ask)
            .map(|(_, system, user)| (system.clone(), user.clone()))
            .collect()
    }
}

#[async_trait::async_trait]
impl LLMPort for Asked {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if crate::lessons::structured_call::is_draft_request(request) {
            let said: Vec<&str> = request
                .input()
                .iter()
                .filter_map(|item| match item {
                    CompletionInput::Message { role, content } if role == "system" => {
                        Some(content.as_str())
                    }
                    _ => None,
                })
                .collect();
            self.drafting.lock().unwrap().push(said.join("\n"));
            return Ok(fixture_completion(request, Vec::new()));
        }
        let (system, user) = crate::lessons::structured_call::task_messages(request);
        let ask = ask_of(system, user).unwrap();
        self.asks
            .lock()
            .unwrap()
            .push((ask, system.to_owned(), task_of(user)));
        let reply = if ask == Ask::Reports {
            let entries: Vec<Value> = task_of(user)["reported"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(at, shown)| {
                    let decided = self.about.get(at).copied().unwrap_or(false);
                    json!({"of": shown["id"], "about": if decided { "decided" } else { "other" }})
                })
                .collect();
            json!({ "entries": entries }).to_string()
        } else {
            nothing(ask, user)
        };
        Ok(CompletionResponse::from_text(reply.as_str()))
    }
    fn model_name(&self) -> &str {
        "scripted-review"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn two() -> (ReviewFinding, ReviewFinding) {
    (
        report(Some(FindingSite::Section(0)), "The kettle is cold.", None),
        report(None, "The rope is cut twice.", Some("cut twice")),
    )
}

async fn ruled(model: &Asked, reported: &[&ReviewFinding]) -> Result<Ruled> {
    check(
        model,
        &json!({}),
        &json!({"blocks":[{"kind":"explanation","title":"Heating","body":"The kettle is full."}],"questions":[]}),
        &[],
        &mut Receipts::default(),
        reported,
    )
    .await
}

#[tokio::test]
async fn without_reports_the_find_ask_has_none_and_its_texts_are_the_plain_ones() -> Result<()> {
    let model = Asked::new(Vec::new());
    let found = ruled(&model, &[]).await?;
    assert!(found.outside.is_empty());
    let finds = model.of(Ask::Find);
    assert_eq!(finds.len(), 1);
    assert_eq!(finds[0].0, FIND_SYSTEM);
    assert!(finds[0].1.get("reported").is_none());
    assert!(model.of(Ask::Reports).is_empty());
    let drafting = model.drafting.lock().unwrap();
    assert!(drafting[0].contains(FIND_BRIEF));
    assert!(!drafting[0].contains("For each report"));
    Ok(())
}

#[tokio::test]
async fn a_report_gives_its_part_its_defect_and_its_words() -> Result<()> {
    let first = report(
        Some(FindingSite::Section(1)),
        "The beads are counted twice.",
        Some("counted twice"),
    );
    let model = Asked::new(vec![true]);
    ruled(&model, &[&first]).await?;
    assert_eq!(
        model.of(Ask::Find)[0].1["reported"],
        json!([{"part":"Section 2","defect":"The beads are counted twice.","words":"counted twice"}])
    );
    Ok(())
}

#[tokio::test]
async fn a_report_without_a_site_or_about_the_whole_lesson_names_no_part_and_one_without_a_passage_no_words(
) -> Result<()> {
    let whole = report(Some(FindingSite::Lesson), "The rope is cut twice.", None);
    let unplaced = report(None, "The shelf is full.", Some("full"));
    let model = Asked::new(vec![true, true]);
    ruled(&model, &[&whole, &unplaced]).await?;
    assert_eq!(
        model.of(Ask::Find)[0].1["reported"],
        json!([
            {"defect":"The rope is cut twice."},
            {"defect":"The shelf is full.","words":"full"}
        ])
    );
    Ok(())
}

#[tokio::test]
async fn the_find_ask_with_reports_is_told_how_to_rule_on_them() -> Result<()> {
    let (first, second) = two();
    let model = Asked::new(vec![true, true]);
    ruled(&model, &[&first, &second]).await?;
    let finds = model.of(Ask::Find);
    assert_eq!(finds[0].0, format!("{FIND_SYSTEM}{REPORTED}"));
    let drafting = model.drafting.lock().unwrap();
    assert!(
        drafting
            .iter()
            .any(|said| said.contains(&format!("{FIND_BRIEF}{REPORTED_FIND_BRIEF}"))),
        "{drafting:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_reports_ask_sends_each_report_under_a_name_of_its_own() -> Result<()> {
    let (first, second) = two();
    let model = Asked::new(vec![true, true]);
    ruled(&model, &[&first, &second]).await?;
    let reports = model.of(Ask::Reports);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].0, REPORTS_SYSTEM);
    let shown = reports[0].1["reported"].as_array().unwrap();
    assert_eq!(shown.len(), 2);
    assert_ne!(shown[0]["id"], shown[1]["id"]);
    assert_eq!(shown[0]["defect"], "The kettle is cold.");
    assert_eq!(shown[1]["words"], "cut twice");
    let drafting = model.drafting.lock().unwrap();
    assert!(
        drafting.iter().any(|said| said.contains(REPORTS_BRIEF)),
        "{drafting:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_reports_about_a_kind_the_checks_do_not_decide_are_the_outside_ones() -> Result<()> {
    let (first, second) = two();
    let model = Asked::new(vec![true, false]);
    assert_eq!(ruled(&model, &[&first, &second]).await?.outside, [1]);
    let model = Asked::new(vec![true, true]);
    assert!(ruled(&model, &[&first, &second]).await?.outside.is_empty());
    Ok(())
}
