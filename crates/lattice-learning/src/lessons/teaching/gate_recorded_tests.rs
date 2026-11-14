//! A defect another check recorded is not repaired a second time from the
//! reviewer's finding of it.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::content_verification::tests::{fixture_completion, fixture_task, CONTRACT_NOTHING};
use crate::lessons::course_generation_tests::{review_reply, ScriptedModel};
use crate::reference_collection::ReferenceCollection;
use crate::review_findings::Held;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use std::collections::VecDeque;
use std::sync::Mutex;

const EXCERPT: &str = "This saved reference describes the exact behavior of the example tool.";
const UNSAVED: &str = "The kettle sings a note that no saved reference holds.";
const FIRST: &str = "The function returns the number 4 for the sum of two and two. ";
const SECOND: &str = "The kettle then rests on the stand once the water is hot. ";
const REVIEWER_ONLY: &str =
    "The explanation promises that the tutor will certify the learner's sum as expert reviewed.";

/// The scripted model, with the lesson's check against its plan answered
/// from `plan` and the evidence gate's requests kept.
struct Routed {
    inner: ScriptedModel,
    plan: Mutex<VecDeque<String>>,
    gate: Mutex<Vec<Value>>,
    plan_asked: Mutex<Vec<Value>>,
    clash_reply: Mutex<String>,
    clash_asked: Mutex<usize>,
}

#[async_trait::async_trait]
impl LLMPort for Routed {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, user) = crate::lessons::structured_call::task_messages(request);
        let drafting = crate::lessons::structured_call::is_draft_request(request);
        if !drafting && system.starts_with(contract::SYSTEM) {
            if let Ok(asked) = serde_json::from_str::<Value>(user) {
                self.plan_asked.lock().unwrap().push(asked);
            }
            let reply = self
                .plan
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| CONTRACT_NOTHING.to_owned());
            return Ok(fixture_completion(request, vec![reply]));
        }
        if !drafting
            && system.starts_with(contract::confirm::SYSTEM)
            && user.contains(contract::confirm::CLASH_TASK)
        {
            *self.clash_asked.lock().unwrap() += 1;
            let reply = self.clash_reply.lock().unwrap().clone();
            return Ok(CompletionResponse::from_text(reply.as_str()));
        }
        if !drafting {
            if let Some(asked) = serde_json::from_str::<Value>(user)
                .ok()
                .filter(|asked| asked.get("proposals").is_some())
            {
                self.gate.lock().unwrap().push(asked);
            }
        }
        self.inner.complete(request).await
    }
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn routed(outputs: Vec<String>, plan: Vec<String>) -> Routed {
    Routed {
        inner: ScriptedModel {
            outputs: Mutex::new(outputs.into()),
            prompts: Mutex::new(Vec::new()),
        },
        plan: Mutex::new(plan.into()),
        gate: Mutex::new(Vec::new()),
        plan_asked: Mutex::new(Vec::new()),
        clash_reply: Mutex::new(
            json!({"case":"The beads cannot be counted once and twice on one day.","contradict":true})
                .to_string(),
        ),
        clash_asked: Mutex::new(0),
    }
}

fn block(title: &str, body: &str, quote: &str) -> Value {
    json!({"kind":"explanation","title":title,"body":body.repeat(12),"rubric":[],"sourceIndex":0,"quote":quote})
}

fn context() -> String {
    json!({"sources":[{"sourceIndex":0,"excerpt":EXCERPT}],
        "courseSequence":{"coursePlan":[{"title":"Boiling","position":"this","ideas":[],"terms":[],"actions":[],"contrast":""}]}})
    .to_string()
}

/// The plan check finds the first section using what it has not taught.
fn used_before_taught() -> String {
    json!({"planned":[],"usedBeforeTaught":[{"what":"the number 4","uses":["returns the number 4"],"taughtLater":""}],"contradictions":[],"unseen":[]}).to_string()
}

/// A review that reports `defect` in the first of `sections` sections.
fn reporting(defect: &str, sections: usize) -> String {
    let checks = (0..sections)
        .map(|index| {
            crate::lessons::course_generation_tests::section_check(
                index,
                (index == 0).then_some(defect),
            )
        })
        .collect();
    review_reply(&[], checks, &[])
}

fn clean(sections: usize) -> String {
    let checks = (0..sections)
        .map(|index| crate::lessons::course_generation_tests::section_check(index, None))
        .collect();
    review_reply(&[], checks, &[])
}

/// A clean review of `sections` sections that finds the reviewer's first finding resolved.
fn resolved(sections: usize) -> String {
    let checks = (0..sections)
        .map(|index| crate::lessons::course_generation_tests::section_check(index, None))
        .collect();
    review_reply(
        &[],
        checks,
        &[crate::lessons::course_generation_tests::verdict(
            "reviewer/1",
            "resolved",
        )],
    )
}

fn gate_answer(left_to: &str) -> String {
    json!({"decisions":[{"issueId":"issue-0","verdict":"actionable","basis":"internal_conflict","evidenceIds":["blocks-0"],"partId":"blocks-0","requirementId":null,"leftTo":left_to,"reason":"The explanation contradicts the sum it states."}]}).to_string()
}

fn gate_not_established(left_to: &str) -> String {
    json!({"decisions":[{"issueId":"issue-0","verdict":"not_established","basis":"internal_conflict","evidenceIds":[],"partId":"lesson","requirementId":null,"leftTo":left_to,"reason":"Nothing in the explanation shows it."}]}).to_string()
}

/// `reply` with the plan check's ruling on each report: whether each is about a kind it decides.
fn about(reply: &str, about: &[bool]) -> String {
    let mut reply: Value = serde_json::from_str(reply).unwrap();
    reply["reports"] = about
        .iter()
        .enumerate()
        .map(|(index, about)| json!({"report":format!("report-{index}"),"about":about}))
        .collect();
    reply.to_string()
}

async fn run(model: &dyn LLMPort, candidate: &Value) -> Result<Reviewed> {
    let sources = vec![crate::dto::LearningSourceDto {
        id: "saved-reference".into(),
        title: "Example reference".into(),
        url: None,
        acquired_at: 0,
        excerpt: EXCERPT.into(),
    }];
    let references = ReferenceCollection::lexical(&sources)?;
    review_material(
        model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        ReviewSchema::Other(&json!({"type":"object"})),
        candidate.to_string(),
        4000,
        &mut LessonSubject::new(&references, &sources, &[], Held::default()),
    )
    .await
}

fn repair_requests(model: &Routed) -> Vec<Value> {
    model
        .inner
        .prompts
        .lock()
        .unwrap()
        .iter()
        .filter_map(|prompt| fixture_task(prompt))
        .filter(|request| request.get("issuesToFix").is_some())
        .collect()
}

#[tokio::test]
async fn a_reviewer_finding_the_plan_check_recorded_is_not_repaired_a_second_time() -> Result<()> {
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 1),
            gate_answer("recorded-0"),
            json!({"edits":[{"path":"/blocks/0/title","before":"Example","after":"Corrected example"}]}).to_string(),
            clean(1),
        ],
        vec![used_before_taught()],
    );
    let candidate = json!({"blocks":[block("Example", FIRST, EXCERPT)],"questions":[]});
    let reviewed = run(&model, &candidate).await?;
    assert!(reviewed.set_aside.is_empty());
    let asked = repair_requests(&model);
    assert_eq!(asked.len(), 1);
    let sent = asked[0]["issuesToFix"].to_string();
    assert!(sent.contains("the number 4"), "{sent}");
    assert!(!sent.contains(REVIEWER_ONLY), "{sent}");
    Ok(())
}

#[tokio::test]
async fn the_gate_is_handed_the_other_checks_findings_and_not_the_reviewers() -> Result<()> {
    let model = routed(
        vec![reporting(REVIEWER_ONLY, 2), gate_answer("none")],
        vec![used_before_taught()],
    );
    let candidate = json!({"blocks":[
        block("Example", FIRST, EXCERPT),
        block("Example", SECOND, UNSAVED),
    ],"questions":[]});
    // The repairs that follow have no scripted answer; the gate's request is what is read.
    let _ = run(&model, &candidate).await;
    let gate = model.gate.lock().unwrap();
    let recorded = gate[0]["recordedFindings"].as_array().unwrap();
    let parts: Vec<_> = recorded
        .iter()
        .map(|finding| finding["part"].as_str().unwrap())
        .collect();
    // The second section's quotation, then the plan check's finding in the first.
    assert_eq!(parts, ["Section 2", "Section 1"]);
    let ids: Vec<_> = recorded
        .iter()
        .map(|finding| finding["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["recorded-0", "recorded-1"]);
    assert!(!gate[0]["recordedFindings"]
        .to_string()
        .contains(REVIEWER_ONLY));
    assert!(gate[0]["proposals"].to_string().contains(REVIEWER_ONLY));
    Ok(())
}

#[tokio::test]
async fn a_reviewer_finding_about_what_the_plan_check_decides_is_left_to_it_when_it_found_nothing(
) -> Result<()> {
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 1),
            gate_answer("check-1"),
            clean(1),
        ],
        vec![
            CONTRACT_NOTHING.to_owned(),
            about(CONTRACT_NOTHING, &[true]),
        ],
    );
    let candidate = json!({"blocks":[block("Example", FIRST, EXCERPT)],"questions":[]});
    let reviewed = run(&model, &candidate).await?;
    assert!(reviewed.set_aside.is_empty());
    assert!(repair_requests(&model).is_empty());
    assert_eq!(model.gate.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn the_gate_is_told_what_each_of_the_three_checks_decides_in_order() -> Result<()> {
    let model = routed(
        vec![reporting(REVIEWER_ONLY, 1), gate_answer("none")],
        Vec::new(),
    );
    let candidate = json!({"blocks":[block("Example", FIRST, EXCERPT)],"questions":[]});
    // The repairs that follow have no scripted answer; the gate's request is what is read.
    let _ = run(&model, &candidate).await;
    let gate = model.gate.lock().unwrap();
    assert_eq!(
        gate[0]["otherChecks"],
        json!([
            {"id":"check-0","decides":crate::question_review::DECIDES},
            {"id":"check-1","decides":contract::DECIDES},
            {"id":"check-2","decides":RULES_DECIDE}
        ])
    );
    Ok(())
}

const ONCE: &str = "The beads are counted once a day.";
const TWICE: &str = "The beads are counted twice on one day.";

/// The plan check lists a contradiction between two sentences of the beads lesson.
fn contradiction() -> String {
    about(
        &json!({"planned":[],"usedBeforeTaught":[],"contradictions":[{"quote":ONCE,"otherQuote":TWICE}],"unseen":[]}).to_string(),
        &[true],
    )
}

fn beads() -> Value {
    json!({"blocks":[
        block("Example", &format!("{ONCE} "), EXCERPT),
        block("Example", &format!("{TWICE} "), EXCERPT),
    ],"questions":[]})
}

fn plan_requests(model: &Routed) -> Vec<Value> {
    model.plan_asked.lock().unwrap().clone()
}

fn repair_edit() -> String {
    json!({"edits":[{"path":"/blocks/0/title","before":"Example","after":"Corrected example"}]})
        .to_string()
}

#[tokio::test]
async fn what_the_review_reported_about_a_contradiction_is_ruled_on_by_the_plan_check() -> Result<()>
{
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 2),
            gate_answer("check-1"),
            repair_edit(),
            clean(1),
        ],
        vec![CONTRACT_NOTHING.to_owned(), contradiction()],
    );
    let reviewed = run(&model, &beads()).await?;
    assert!(reviewed.set_aside.is_empty());
    let asked = repair_requests(&model);
    assert_eq!(asked.len(), 1);
    let id = asked[0]["issuesToFix"][0]["id"].as_str().unwrap();
    assert!(id.starts_with("plan/contradiction"), "{id}");
    Ok(())
}

#[tokio::test]
async fn the_plan_check_is_asked_again_with_what_the_review_reported() -> Result<()> {
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 2),
            gate_answer("check-1"),
            repair_edit(),
            clean(1),
        ],
        vec![CONTRACT_NOTHING.to_owned(), contradiction()],
    );
    run(&model, &beads()).await?;
    let asks = plan_requests(&model);
    assert!(asks.len() >= 2, "{}", asks.len());
    assert!(asks[0].get("reported").is_none());
    let reported = asks[1]["reported"].as_array().unwrap();
    assert_eq!(reported.len(), 1);
    assert_eq!(reported[0]["part"], "Section 1");
    let gate = model.gate.lock().unwrap();
    assert_eq!(reported[0]["defect"], gate[0]["proposals"][0]["proposal"]);
    Ok(())
}

#[tokio::test]
async fn a_finding_left_to_the_question_review_is_not_given_to_the_plan_check() -> Result<()> {
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 2),
            gate_not_established("check-0"),
            clean(2),
        ],
        vec![CONTRACT_NOTHING.to_owned(), contradiction()],
    );
    let reviewed = run(&model, &beads()).await?;
    assert!(reviewed.set_aside.is_empty());
    assert_eq!(plan_requests(&model).len(), 1);
    assert!(repair_requests(&model).is_empty());
    Ok(())
}

#[tokio::test]
async fn a_contradiction_the_reported_ask_lists_is_confirmed_once_before_it_is_repaired(
) -> Result<()> {
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 2),
            gate_answer("check-1"),
            repair_edit(),
            clean(1),
        ],
        vec![CONTRACT_NOTHING.to_owned(), contradiction()],
    );
    run(&model, &beads()).await?;
    assert_eq!(*model.clash_asked.lock().unwrap(), 1);
    assert_eq!(repair_requests(&model).len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_contradiction_the_reported_ask_lists_and_the_read_refuses_repairs_nothing() -> Result<()>
{
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 2),
            gate_answer("check-1"),
            clean(2),
        ],
        vec![CONTRACT_NOTHING.to_owned(), contradiction()],
    );
    *model.clash_reply.lock().unwrap() =
        json!({"case":"Counted once a day, the beads can still be counted twice on one day.","contradict":false})
            .to_string();
    let reviewed = run(&model, &beads()).await?;
    assert!(reviewed.set_aside.is_empty());
    assert_eq!(*model.clash_asked.lock().unwrap(), 1);
    assert!(repair_requests(&model).is_empty());
    Ok(())
}

#[tokio::test]
async fn a_report_the_plan_check_does_not_hold_up_repairs_nothing() -> Result<()> {
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 2),
            gate_answer("check-1"),
            clean(2),
        ],
        vec![
            CONTRACT_NOTHING.to_owned(),
            about(CONTRACT_NOTHING, &[true]),
        ],
    );
    let reviewed = run(&model, &beads()).await?;
    assert!(reviewed.set_aside.is_empty());
    assert!(repair_requests(&model).is_empty());
    Ok(())
}

fn single() -> Value {
    json!({"blocks":[block("Example", FIRST, EXCERPT)],"questions":[]})
}

/// The one repair the gate's finding reached, when it did.
fn reached_repair(model: &Routed) -> bool {
    let asked = repair_requests(model);
    assert!(asked.len() <= 1, "{asked:?}");
    asked
        .first()
        .is_some_and(|request| request["issuesToFix"].to_string().contains(REVIEWER_ONLY))
}

#[tokio::test]
async fn a_report_the_plan_check_says_is_of_another_kind_stands_on_the_gates_verdict() -> Result<()>
{
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 1),
            gate_answer("check-1"),
            repair_edit(),
            resolved(1),
        ],
        vec![
            CONTRACT_NOTHING.to_owned(),
            about(CONTRACT_NOTHING, &[false]),
        ],
    );
    run(&model, &single()).await?;
    assert!(reached_repair(&model));
    Ok(())
}

#[tokio::test]
async fn a_report_the_plan_check_says_is_of_its_kind_and_leaves_out_is_not_repaired() -> Result<()>
{
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 1),
            gate_answer("check-1"),
            clean(1),
        ],
        vec![
            CONTRACT_NOTHING.to_owned(),
            about(CONTRACT_NOTHING, &[true]),
        ],
    );
    let reviewed = run(&model, &single()).await?;
    assert!(reviewed.set_aside.is_empty());
    assert!(!reached_repair(&model));
    Ok(())
}

#[tokio::test]
async fn a_report_the_gate_did_not_establish_is_not_repaired_even_when_it_is_of_another_kind(
) -> Result<()> {
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 1),
            gate_not_established("check-1"),
            clean(1),
        ],
        vec![
            CONTRACT_NOTHING.to_owned(),
            about(CONTRACT_NOTHING, &[false]),
        ],
    );
    let reviewed = run(&model, &single()).await?;
    assert!(reviewed.set_aside.is_empty());
    assert!(!reached_repair(&model));
    Ok(())
}

#[tokio::test]
async fn a_finding_left_to_the_question_review_is_repaired_when_the_gate_established_it(
) -> Result<()> {
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 1),
            gate_answer("check-0"),
            repair_edit(),
            resolved(1),
        ],
        Vec::new(),
    );
    run(&model, &single()).await?;
    assert!(reached_repair(&model));
    Ok(())
}

#[tokio::test]
async fn a_finding_left_to_the_question_review_is_not_repaired_when_the_gate_did_not_establish_it(
) -> Result<()> {
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 1),
            gate_not_established("check-0"),
            clean(1),
        ],
        Vec::new(),
    );
    let reviewed = run(&model, &single()).await?;
    assert!(reviewed.set_aside.is_empty());
    assert!(!reached_repair(&model));
    Ok(())
}

#[tokio::test]
async fn a_finding_left_to_the_rules_is_repaired_when_the_gate_established_it() -> Result<()> {
    let model = routed(
        vec![
            reporting(REVIEWER_ONLY, 1),
            gate_answer("check-2"),
            repair_edit(),
            resolved(1),
        ],
        Vec::new(),
    );
    run(&model, &single()).await?;
    assert!(reached_repair(&model));
    Ok(())
}
