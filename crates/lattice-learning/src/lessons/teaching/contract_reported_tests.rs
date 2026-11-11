//! What the plan check is given of the review's reports.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::content_verification::tests::{fixture_completion, CONTRACT_NOTHING};
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use std::sync::Mutex;

fn prompt() -> Value {
    json!({"task":"Check","sections":[],"questions":[]})
}

fn report(site: Option<FindingSite>, note: &str, passage: Option<&str>) -> ReviewFinding {
    let mut finding =
        ReviewFinding::plan(PlanCheck::Contradiction, "beads", FindingSite::Lesson, note);
    finding.site = site;
    finding.passage = passage.map(str::to_owned);
    finding
}

#[test]
fn without_reports_the_request_and_the_system_text_are_as_they_were() {
    let plain = prompt();
    assert_eq!(with_reported(plain.clone(), &[]), plain);
    assert!(with_reported(plain, &[]).get("reported").is_none());
    assert_eq!(system_for(&[]), SYSTEM);
}

#[test]
fn a_report_gives_its_part_its_defect_and_its_words() {
    let first = report(
        Some(FindingSite::Section(1)),
        "The beads are counted twice.",
        Some("counted twice"),
    );
    let asked = with_reported(prompt(), &[&first]);
    assert_eq!(
        asked["reported"],
        json!([{"part":"Section 2","defect":"The beads are counted twice.","words":"counted twice"}])
    );
}

#[test]
fn a_report_without_a_site_or_about_the_whole_lesson_names_no_part_and_one_without_a_passage_no_words(
) {
    let whole = report(Some(FindingSite::Lesson), "The rope is cut twice.", None);
    let unplaced = report(None, "The shelf is full.", Some("full"));
    let asked = with_reported(prompt(), &[&whole, &unplaced]);
    assert_eq!(
        asked["reported"],
        json!([
            {"defect":"The rope is cut twice."},
            {"defect":"The shelf is full.","words":"full"}
        ])
    );
}

#[test]
fn the_system_text_with_reports_is_the_plain_one_followed_by_the_sentence() {
    let one = report(None, "The kettle is cold.", None);
    let system = system_for(&[&one]);
    assert_eq!(system, format!("{SYSTEM}{REPORTED}"));
}

const NEW_REPORTED: &str = " reported lists defects a review of this lesson described, each with its id. A report is a pointer where to read again, not evidence. Read the parts each one names again. For every report give in reports its id and about: true when the defect it describes is of a kind these four checks decide, whether or not it holds; false when it is of any other kind. Rule on each report whose about is true by the rules above: when it holds, give it in the list it belongs to exactly as any other entry, with the words copied exactly; when it does not hold by these rules, leave it out.";

const REPORT_BRIEF: &str = " For each report, say whether the defect it describes is of a kind these four checks decide and, when it is, whether it holds by their rules.";

/// The check's model: answers every ask with `reply` and keeps what it was sent.
struct Asked {
    reply: String,
    systems: Mutex<Vec<String>>,
    users: Mutex<Vec<Value>>,
    schemas: Mutex<Vec<Value>>,
    drafting: Mutex<Vec<String>>,
}

impl Asked {
    fn new(reply: String) -> Self {
        Self {
            reply,
            systems: Mutex::new(Vec::new()),
            users: Mutex::new(Vec::new()),
            schemas: Mutex::new(Vec::new()),
            drafting: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl LLMPort for Asked {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if crate::lessons::structured_call::is_draft_request(request) {
            let said: Vec<&str> = request
                .input
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
        self.systems.lock().unwrap().push(system.to_owned());
        self.users
            .lock()
            .unwrap()
            .push(serde_json::from_str(user).unwrap());
        self.schemas
            .lock()
            .unwrap()
            .push(request.json_schema.clone().unwrap());
        Ok(CompletionResponse::from_text(self.reply.as_str()))
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

fn replying(reports: Value) -> String {
    json!({"planned":[],"usedBeforeTaught":[],"contradictions":[],"unseen":[],"reports":reports})
        .to_string()
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
async fn a_reported_ask_sends_each_report_with_its_id_and_requires_a_ruling_on_each() -> Result<()>
{
    let (first, second) = two();
    let model = Asked::new(replying(json!([
        {"report":"report-0","about":true},
        {"report":"report-1","about":true}
    ])));
    ruled(&model, &[&first, &second]).await?;
    let users = model.users.lock().unwrap();
    let ids: Vec<_> = users[0]["reported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["report-0", "report-1"]);
    let systems = model.systems.lock().unwrap();
    assert!(systems[0].starts_with(SYSTEM));
    assert!(systems[0].ends_with(NEW_REPORTED));
    let schema = &model.schemas.lock().unwrap()[0];
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("reports")));
    assert_eq!(
        schema["properties"]["reports"],
        json!({"type":"array","minItems":2,"maxItems":2,"items":{"type":"object","additionalProperties":false,"required":["report","about"],"properties":{"report":{"type":"string","enum":["report-0","report-1"]},"about":{"type":"boolean"}}}})
    );
    let drafting = model.drafting.lock().unwrap();
    assert!(
        drafting[0].contains(&format!("{BRIEF}{REPORT_BRIEF}")),
        "{}",
        drafting[0]
    );
    Ok(())
}

#[tokio::test]
async fn a_first_ask_has_no_reports_in_its_schema_and_its_brief_is_as_before() -> Result<()> {
    let model = Asked::new(replying(json!([])));
    let found = ruled(&model, &[]).await?;
    assert!(found.outside.is_empty());
    let schema = &model.schemas.lock().unwrap()[0];
    assert!(schema["properties"].get("reports").is_none());
    assert!(!schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("reports")));
    assert_eq!(*model.systems.lock().unwrap(), [SYSTEM]);
    assert_eq!(brief_for(&[]), BRIEF);
    let drafting = model.drafting.lock().unwrap();
    assert!(drafting[0].contains(BRIEF));
    assert!(!drafting[0].contains("For each report"));
    Ok(())
}

#[test]
fn a_saved_first_ask_answer_serializes_without_reports() {
    let saved: Answer = serde_json::from_str(CONTRACT_NOTHING).unwrap();
    assert!(saved.reports.is_empty());
    assert!(serde_json::to_value(&saved)
        .unwrap()
        .get("reports")
        .is_none());
}

#[tokio::test]
async fn the_reports_about_a_kind_the_checks_do_not_decide_are_the_outside_ones() -> Result<()> {
    let (first, second) = two();
    let model = Asked::new(replying(json!([
        {"report":"report-0","about":true},
        {"report":"report-1","about":false}
    ])));
    assert_eq!(ruled(&model, &[&first, &second]).await?.outside, [1]);
    let model = Asked::new(replying(json!([
        {"report":"report-0","about":true},
        {"report":"report-1","about":true}
    ])));
    assert!(ruled(&model, &[&first, &second]).await?.outside.is_empty());
    Ok(())
}

#[tokio::test]
async fn the_first_entry_for_a_report_counts_and_a_report_with_none_is_outside() -> Result<()> {
    let (first, second) = two();
    let model = Asked::new(replying(json!([
        {"report":"report-0","about":true},
        {"report":"report-0","about":false}
    ])));
    assert_eq!(ruled(&model, &[&first, &second]).await?.outside, [1]);
    Ok(())
}
