//! A contradiction the check lists is put to the model once more, and only one that read upholds is a finding.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::content_verification::tests::fixture_completion;
use crate::lesson_drafts;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::shared::persistence::Database;
use std::sync::Mutex;

const EMPTY: &str = "The kettle is empty when it boils.";
const FULL: &str = "The kettle is full when it boils.";
const EMPTY_WORDS: &str = "The kettle is empty when it boils";
const FULL_WORDS: &str = "The kettle is full when it boils";
const CASE: &str = "A kettle that is half full when it boils.";

/// The check lists one contradiction of `first` and `second`; every confirming read is answered with `clash`.
struct Sweep {
    plan: String,
    clash: String,
    requests: Mutex<Vec<Value>>,
    schemas: Mutex<Vec<Value>>,
}

impl Sweep {
    fn new(first: &str, second: &str, clash: Value) -> Self {
        let plan = json!({"planned":[],"usedBeforeTaught":[],"contradictions":[{"quote":first,"otherQuote":second}],"unseen":[]});
        Self {
            plan: plan.to_string(),
            clash: clash.to_string(),
            requests: Mutex::new(Vec::new()),
            schemas: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl LLMPort for Sweep {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, user) = crate::lessons::structured_call::task_messages(request);
        if crate::lessons::structured_call::is_draft_request(request) {
            return Ok(fixture_completion(request, Vec::new()));
        }
        if system.starts_with(confirm::SYSTEM) {
            if let Ok(asked) = serde_json::from_str::<Value>(user) {
                self.requests.lock().unwrap().push(asked);
            }
            if let Some(schema) = &request.json_schema {
                self.schemas.lock().unwrap().push(schema.clone());
            }
            return Ok(CompletionResponse::from_text(self.clash.as_str()));
        }
        Ok(fixture_completion(request, vec![self.plan.clone()]))
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

fn kettle() -> Value {
    json!({"blocks":[
        {"kind":"explanation","title":"Heating","body":EMPTY},
        {"kind":"explanation","title":"Pouring","body":FULL}
    ],"questions":[]})
}

fn upheld(case: &str) -> Value {
    json!({"case":case,"contradict":true})
}

fn refused(case: &str) -> Value {
    json!({"case":case,"contradict":false})
}

fn guard(unit: usize, quote: &str) -> Protected {
    Protected {
        unit,
        field: "body".into(),
        quote: quote.into(),
        released: false,
    }
}

async fn found(
    model: &Sweep,
    protected: &[Protected],
    reported: &[&ReviewFinding],
) -> Result<Vec<ReviewFinding>> {
    check(
        model,
        &json!({}),
        &kettle(),
        protected,
        &mut Receipts::default(),
        reported,
    )
    .await
}

fn clashes(found: &[ReviewFinding]) -> Vec<&ReviewFinding> {
    found
        .iter()
        .filter(|finding| finding.note.starts_with("The lesson contradicts itself"))
        .collect()
}

fn told(case: &str) -> String {
    format!("The lesson contradicts itself: Section 1 says \"{EMPTY}\" and Section 2 says \"{FULL}\". Correct the one that is wrong so the lesson says one thing.{case}")
}

#[tokio::test]
async fn a_contradiction_the_confirming_read_refuses_is_no_finding() -> Result<()> {
    let model = Sweep::new(
        EMPTY,
        FULL,
        refused("Both hold for a kettle that boils dry."),
    );
    let result = found(&model, &[], &[]).await?;
    assert!(clashes(&result).is_empty(), "{result:?}");
    assert_eq!(model.requests().len(), 1);
    assert_eq!(model.requests()[0]["task"], confirm::CLASH_TASK);
    Ok(())
}

#[tokio::test]
async fn a_contradiction_the_confirming_read_upholds_is_told_where_the_two_conflict() -> Result<()>
{
    let model = Sweep::new(EMPTY, FULL, upheld(&format!("  {CASE}  ")));
    let result = found(&model, &[], &[]).await?;
    let clash = clashes(&result);
    assert_eq!(clash.len(), 1, "{result:?}");
    assert_eq!(
        clash[0].note,
        told(&format!(" Where they conflict: {CASE}"))
    );
    Ok(())
}

#[tokio::test]
async fn a_contradiction_upheld_with_a_blank_case_keeps_the_plain_description() -> Result<()> {
    let model = Sweep::new(EMPTY, FULL, upheld("  "));
    let result = found(&model, &[], &[]).await?;
    let clash = clashes(&result);
    assert_eq!(clash.len(), 1, "{result:?}");
    assert_eq!(clash[0].note, told(""));
    Ok(())
}

#[tokio::test]
async fn the_confirming_request_gives_both_statements_in_their_parts_and_the_whole_lesson(
) -> Result<()> {
    let model = Sweep::new(
        "the kettle is EMPTY when it boils",
        "The kettle is FULL when it boils",
        upheld(CASE),
    );
    found(&model, &[], &[]).await?;
    let requests = model.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request["task"], confirm::CLASH_TASK);
    assert_eq!(
        request["question"],
        format!("Can the first statement, section-0's words '{EMPTY_WORDS}', and the second, section-1's words '{FULL_WORDS}', both be true? If they can, give in case a situation in which both hold. If they cannot, give in case what one says happens or holds that the other says cannot. In case, call them the first and the second statement.")
    );
    assert_eq!(
        request["statements"],
        json!([
            {"part":"section-0","words":EMPTY_WORDS},
            {"part":"section-1","words":FULL_WORDS}
        ])
    );
    let (sections, questions) = shown_parts(parts_of(&kettle()).iter());
    assert_eq!(request["sections"], json!(sections));
    assert_eq!(request["questions"], json!(questions));
    let schemas = model.schemas.lock().unwrap();
    assert_eq!(schemas.len(), 1);
    assert_eq!(
        schemas[0],
        json!({"type":"object","additionalProperties":false,"required":["case","contradict"],"properties":{"case":{"type":"string"},"contradict":{"type":"boolean"}}})
    );
    Ok(())
}

#[tokio::test]
async fn a_contradiction_with_a_quote_not_in_the_lesson_is_not_put_to_the_model() -> Result<()> {
    for (first, second) in [
        ("The kettle whistles twice.", FULL),
        (EMPTY, "The kettle whistles twice."),
    ] {
        let model = Sweep::new(first, second, upheld(CASE));
        let result = found(&model, &[], &[]).await?;
        assert!(clashes(&result).is_empty(), "{result:?}");
        assert!(model.requests().is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn a_saved_confirming_answer_is_reused() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson_id) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&pool).await?;
    let model = Sweep::new(EMPTY, FULL, upheld(CASE));
    let mut standing = Vec::new();
    for _ in 0..2 {
        let result = lesson_drafts::run(
            &Database::from(pool.clone()),
            &repo,
            &job,
            &lesson_id,
            async { found(&model, &[], &[]).await },
        )
        .await?;
        standing.push(clashes(&result).len());
    }
    assert_eq!(standing, [1, 1]);
    assert_eq!(model.requests().len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_contradiction_with_both_sides_guarded_is_held_at_the_first() -> Result<()> {
    let model = Sweep::new(EMPTY, FULL, upheld(CASE));
    let result = found(&model, &[guard(0, EMPTY), guard(1, FULL)], &[]).await?;
    let clash = clashes(&result);
    assert_eq!(clash.len(), 1, "{result:?}");
    assert_eq!(clash[0].site, Some(FindingSite::Section(0)));
    assert_eq!(clash[0].also, None);
    assert_eq!(clash[0].passage.as_deref(), Some(EMPTY));
    Ok(())
}

#[tokio::test]
async fn a_contradiction_with_one_side_guarded_is_placed_on_the_other() -> Result<()> {
    let model = Sweep::new(EMPTY, FULL, upheld(CASE));
    let result = found(&model, &[guard(0, EMPTY)], &[]).await?;
    let clash = clashes(&result);
    assert_eq!(clash.len(), 1, "{result:?}");
    assert_eq!(clash[0].site, Some(FindingSite::Section(1)));
    assert_eq!(clash[0].also, None);
    assert_eq!(clash[0].passage.as_deref(), Some(FULL));
    let result = found(&model, &[guard(1, FULL)], &[]).await?;
    let clash = clashes(&result);
    assert_eq!(clash[0].site, Some(FindingSite::Section(0)));
    assert_eq!(clash[0].also, None);
    Ok(())
}

#[tokio::test]
async fn a_contradiction_with_both_sides_open_is_about_both() -> Result<()> {
    let model = Sweep::new(EMPTY, FULL, upheld(CASE));
    let result = found(&model, &[], &[]).await?;
    let clash = clashes(&result);
    assert_eq!(clash[0].site, Some(FindingSite::Section(0)));
    assert_eq!(clash[0].also, Some(FindingSite::Section(1)));
    Ok(())
}

#[tokio::test]
async fn a_contradiction_the_reported_ask_lists_is_confirmed_the_same_way() -> Result<()> {
    let report = ReviewFinding::plan(
        PlanCheck::Contradiction,
        "kettle",
        FindingSite::Lesson,
        "The kettle is both empty and full when it boils.",
    );
    let model = Sweep::new(EMPTY, FULL, upheld(CASE));
    let result = found(&model, &[], &[&report]).await?;
    assert_eq!(clashes(&result).len(), 1, "{result:?}");
    assert_eq!(model.requests().len(), 1);
    let model = Sweep::new(EMPTY, FULL, refused(CASE));
    let result = found(&model, &[], &[&report]).await?;
    assert!(clashes(&result).is_empty(), "{result:?}");
    assert_eq!(model.requests().len(), 1);
    Ok(())
}

#[test]
fn the_confirming_read_says_when_two_statements_contradict() {
    assert!(confirm::SYSTEM.contains(concat!(
        " are taught too.\n",
        "Two statements contradict each other when there is a case in which one says something happens or holds and the other says it cannot; statements that can both be true, read with their conditions, do not.",
        "\nCopy every quotation exactly"
    )));
}
