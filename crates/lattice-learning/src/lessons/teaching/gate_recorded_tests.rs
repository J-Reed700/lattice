//! A defect another check recorded is not repaired a second time from the
//! reviewer's finding of it.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::content_verification::tests::{fixture_completion, CONTRACT_NOTHING};
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
const REVIEWER_ONLY: &str = "The explanation leans on an unstated premise about the sum.";

/// The scripted model, with the lesson's check against its plan answered
/// from `plan` and the evidence gate's requests kept.
struct Routed {
    inner: ScriptedModel,
    plan: Mutex<VecDeque<String>>,
    gate: Mutex<Vec<Value>>,
}

#[async_trait::async_trait]
impl LLMPort for Routed {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, user) = crate::lessons::structured_call::task_messages(request);
        let drafting = crate::lessons::structured_call::is_draft_request(request);
        if !drafting && system.starts_with(contract::SYSTEM) {
            let reply = self
                .plan
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| CONTRACT_NOTHING.to_owned());
            return Ok(fixture_completion(request, vec![reply]));
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

fn gate_answer(recorded_as: &str) -> String {
    json!({"decisions":[{"issueId":"issue-0","verdict":"actionable","basis":"internal_conflict","evidenceIds":["blocks-0"],"partId":"blocks-0","requirementId":null,"recordedAs":recorded_as,"reason":"The explanation contradicts the sum it states."}]}).to_string()
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
        &mut LessonSubject::new(&references, &[], Held::default()),
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
        .filter_map(|prompt| serde_json::from_str::<Value>(prompt.rsplit("\n\n").next()?).ok())
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
async fn the_gate_is_told_the_checks_that_read_the_candidate_beside_the_reviewer() -> Result<()> {
    let model = routed(
        vec![reporting(REVIEWER_ONLY, 1), gate_answer("none")],
        Vec::new(),
    );
    let candidate = json!({"blocks":[block("Example", FIRST, EXCERPT)],"questions":[]});
    // The repairs that follow have no scripted answer; the gate's request is what is read.
    let _ = run(&model, &candidate).await;
    let gate = model.gate.lock().unwrap();
    let ids: Vec<_> = gate[0]["otherChecks"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|check| check["id"].clone())
        .collect();
    assert_eq!(ids, [json!("check-0"), json!("check-1"), json!("check-2")]);
    Ok(())
}
