//! A lesson's teaching review publishes with what the repairs could not
//! settle waiting for the user.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::lesson_drafts;
use crate::lessons::course_generation_tests::{ScriptedModel, Windowed};
use crate::reference_collection::ReferenceCollection;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

const EXCERPT: &str = "This saved reference describes the exact behavior of the example tool.";

fn lesson(quote: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Example","body":"The function returns the number 4 for the sum of two and two. ".repeat(12),"rubric":[],"sourceIndex":0,"quote":quote}],"questions":[]})
}

fn context() -> String {
    json!({"sources":[{"sourceIndex":0,"excerpt":EXCERPT}]}).to_string()
}

fn review(defect: Option<&str>) -> String {
    let finding =
        defect.unwrap_or("The complete explanation was reviewed and no teaching defect was found.");
    json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![json!({"index":0,"passageId":"section-0-passage-0","finding":finding,"hasDefect":defect.is_some()})])}).to_string()
}

fn actionable() -> String {
    json!({"decisions":[{"issueId":"issue-0","verdict":"actionable","basis":"internal_conflict","evidenceIds":["blocks-0"],"partId":"blocks-0","requirementId":null,"reason":"The example's printed result contradicts the value the code returns."}]}).to_string()
}

fn retitle(after: &str) -> String {
    json!({"edits":[{"path":"/blocks/0/title","before":"Example","after":after}]}).to_string()
}

fn references() -> Vec<crate::dto::LearningSourceDto> {
    vec![crate::dto::LearningSourceDto {
        id: "saved-reference".into(),
        title: "Example reference".into(),
        url: None,
        acquired_at: 0,
        excerpt: EXCERPT.into(),
    }]
}

async fn run(model: &dyn LLMPort, candidate: Value) -> Result<Reviewed> {
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    review_material(
        model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        &json!({"type":"object"}),
        candidate.to_string(),
        4000,
        ReviewContext {
            outline: None,
            protected: &[],
            lesson_references: Some(&references),
            held: BTreeSet::new(),
        },
    )
    .await
}

/// The teaching review after a claim repair, `protected` the sentences the
/// lesson's live claims stand on.
async fn review_protecting(
    model: &ScriptedModel,
    candidate: &Value,
    protected: &[crate::lessons::content_verification::Protected],
) -> Result<Reviewed> {
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    review_material(
        model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        &json!({"type":"object"}),
        candidate.to_string(),
        4000,
        ReviewContext {
            outline: None,
            protected,
            lesson_references: Some(&references),
            held: BTreeSet::new(),
        },
    )
    .await
}

fn model(outputs: Vec<String>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(outputs.into()),
        prompts: Mutex::new(Vec::new()),
    }
}

#[tokio::test]
async fn a_defect_one_repair_did_not_fix_publishes_and_waits_for_the_user() -> Result<()> {
    // The failure that ended a two-hour job: the recheck still found the
    // defect, reworded. Nothing was fixed, so the review stops, and the
    // lesson publishes with the finding set aside.
    let model = model(vec![
        review(Some(
            "The example shows \"result\": \"4\" although the code returns the number 4.",
        )),
        actionable(),
        retitle("Corrected example"),
        review(Some(
            "Section 1 still prints the result as the string \"4\" instead of the number.",
        )),
        actionable(),
    ]);
    let reviewed = run(&model, lesson(EXCERPT)).await?;
    let candidate: Value = serde_json::from_str(&reviewed.candidate)?;
    assert_eq!(candidate["blocks"][0]["title"], "Corrected example");
    assert_eq!(reviewed.set_aside.len(), 1);
    let waiting = &reviewed.set_aside[0];
    assert_eq!(waiting.finding.key().id(), "teaching:section-0");
    assert_eq!(waiting.repairs, 1);
    assert!(waiting.finding.note.contains("instead of the number"));
    assert!(waiting.finding.passage.is_some());
    assert_eq!(model.prompts.lock().unwrap().len(), 5);
    Ok(())
}

#[tokio::test]
async fn the_reviewer_is_told_to_name_a_part_by_its_label_or_title() -> Result<()> {
    let model = model(vec![review(None)]);
    run(&model, lesson(EXCERPT)).await?;
    let prompts = model.prompts.lock().unwrap();
    assert!(prompts[0].contains("by its label"));
    assert!(prompts[0].contains("never by its section-N key or its index"));
    Ok(())
}

#[tokio::test]
async fn a_pass_that_fixes_the_defect_publishes_with_nothing_waiting() -> Result<()> {
    let model = model(vec![
        review(Some(
            "The example shows \"result\": \"4\" although the code returns the number 4.",
        )),
        actionable(),
        retitle("Corrected example"),
        review(None),
    ]);
    let reviewed = run(&model, lesson(EXCERPT)).await?;
    assert!(reviewed.set_aside.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_lessons_own_repair_stays_in_the_review_phase() -> Result<()> {
    let model = model(vec![
        review(Some(
            "The example shows \"result\": \"4\" although the code returns the number 4.",
        )),
        actionable(),
        retitle("Corrected example"),
        review(None),
    ]);
    let (reviewed, phases, stages) =
        crate::lesson_progress::phased(run(&model, lesson(EXCERPT))).await;
    reviewed?;
    assert_eq!(phases, vec![crate::lesson_progress::Phase::Review]);
    assert!(stages
        .iter()
        .any(|stage| stage == "Correcting the lesson from review findings"));
    Ok(())
}

#[tokio::test]
async fn a_quiz_batch_reviewed_inside_a_lesson_job_changes_no_phase() -> Result<()> {
    let issue = json!({"issues":["The exercise answer contradicts its stated condition. Correct the comparison."]}).to_string();
    let patch = json!({"edits":[{"path":"/wrong","before":true,"after":false}]}).to_string();
    let model = model(vec![issue, patch, json!({"issues":[]}).to_string()]);
    let (reviewed, phases, stages) = crate::lesson_progress::phased(async {
        crate::lesson_progress::phase(
            crate::lesson_progress::Phase::Evidence,
            "Verifying factual claims",
        );
        review_and_repair(
            &model,
            "Author quiz questions",
            "Require a correct comparison",
            &json!({"type":"object"}),
            json!({"wrong":true}).to_string(),
            2000,
        )
        .await
    })
    .await;
    reviewed?;
    assert_eq!(phases, vec![crate::lesson_progress::Phase::Evidence]);
    let quiz: Vec<_> = stages
        .iter()
        .filter(|stage| stage.contains("quiz"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        quiz,
        [
            "Checking the quiz questions' answer keys",
            "Reviewing the quiz questions",
            "Correcting the quiz questions from review findings",
            "Checking the quiz questions' answer keys",
            "Rechecking the corrected quiz questions",
        ]
    );
    assert!(!stages.iter().any(|stage| stage.contains("lesson")));
    Ok(())
}

#[tokio::test]
async fn a_revised_quotation_that_still_matches_no_reference_waits_instead_of_failing() -> Result<()>
{
    let model = model(vec![
        review(None),
        retitle("Example, revised"),
        review(None),
    ]);
    let reviewed = run(
        &model,
        lesson("An imaginary handbook says this tool behaves in a different way entirely."),
    )
    .await?;
    let keys: Vec<_> = reviewed
        .set_aside
        .iter()
        .map(|waiting| waiting.finding.key().id())
        .collect();
    assert_eq!(keys, ["quotation:section-0"]);
    // The repair was asked to fix the quotation, with the checks it failed.
    let prompts = model.prompts.lock().unwrap();
    assert!(prompts[1].contains("sourceQuoteChecks"));
    assert!(prompts[1].contains("does not match its saved reference"));
    Ok(())
}

#[tokio::test]
async fn a_repair_whose_answer_cannot_be_used_leaves_the_lesson_unchanged_and_waiting() -> Result<()>
{
    let unusable =
        json!({"edits":[{"path":"/blocks/0/title","before":"Missing text","after":"X"}]})
            .to_string();
    let model = model(vec![
        review(Some(
            "The example shows \"result\": \"4\" although the code returns the number 4.",
        )),
        actionable(),
        unusable.clone(),
        unusable,
    ]);
    let original = lesson(EXCERPT);
    let reviewed = run(&model, original.clone()).await?;
    assert_eq!(
        serde_json::from_str::<Value>(&reviewed.candidate)?,
        original
    );
    assert_eq!(reviewed.set_aside.len(), 1);
    assert_eq!(
        reviewed.set_aside[0].finding.key().id(),
        "teaching:section-0"
    );
    Ok(())
}

/// Whether `request` asks the model to repair the lesson.
fn asks_for_repair(request: &CompletionRequest) -> bool {
    request.input.iter().any(|item| {
        matches!(
            item,
            CompletionInput::Message { content, .. } if content.contains("issuesToFix")
        )
    })
}

/// The scripted review, with every teaching repair request longer than the
/// model's window.
struct RepairTooLong(ScriptedModel);

#[async_trait::async_trait]
impl lattice_core::application::ports::llm_port::LLMPort for RepairTooLong {
    async fn complete(
        &self,
        request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        if asks_for_repair(request) {
            return Err(AppError::PromptExceedsWindow {
                prompt_tokens: 251_588,
                window: 131_072,
                counted_exactly: true,
                message: "over the window".into(),
            });
        }
        self.0.complete(request).await
    }
    fn model_name(&self) -> &str {
        self.0.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.0.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.0.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// A repair request too long for the window was never sent: the lesson
/// publishes unchanged with its finding waiting, instead of failing a job
/// that had already written and checked the lesson.
#[tokio::test]
async fn a_repair_too_long_for_the_window_leaves_the_lesson_waiting() -> Result<()> {
    let model = RepairTooLong(model(vec![
        review(Some(
            "The example shows \"result\": \"4\" although the code returns the number 4.",
        )),
        actionable(),
    ]));
    let original = lesson(EXCERPT);
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    let reviewed = review_material(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        &json!({"type":"object"}),
        original.to_string(),
        4000,
        ReviewContext {
            outline: None,
            protected: &[],
            lesson_references: Some(&references),
            held: BTreeSet::new(),
        },
    )
    .await?;
    assert_eq!(
        serde_json::from_str::<Value>(&reviewed.candidate)?,
        original
    );
    assert_eq!(reviewed.set_aside.len(), 1);
    Ok(())
}

/// The repair carries the writer's prompt beside the lesson and a whole
/// lesson's answer room: on a 9B's 32k window the first repair of a
/// PostgreSQL lesson did not fit, so every teaching finding waited for the
/// user and none was repaired (2026-11-06). Six excerpts of 2,000 tokens
/// here, the lesson citing the fifth, in a window that holds the repair
/// beside some.
#[tokio::test]
async fn a_repair_too_big_for_its_window_keeps_the_excerpts_that_fit_the_cited_first() -> Result<()>
{
    let excerpts: Vec<_> = (0..6)
        .map(|index| json!({"sourceIndex":index,"title":format!("Source {index}"),"url":null,"excerpt":"x ".repeat(4000)}))
        .collect();
    let mut candidate = lesson("x x x x x x x x x x x x x x");
    candidate["blocks"][0]["sourceIndex"] = json!(4);
    let scripted = model(vec![
        review(Some(
            "The example shows \"result\": \"4\" although the code returns the number 4.",
        )),
        actionable(),
        retitle("Corrected example"),
        review(None),
    ]);
    let window = Windowed(&scripted, 13_000);
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    let reviewed = review_material(
        &window,
        LESSON_REPAIR_SYSTEM,
        &json!({"sources":excerpts}).to_string(),
        &json!({"type":"object"}),
        candidate.to_string(),
        4000,
        ReviewContext {
            outline: None,
            protected: &[],
            lesson_references: Some(&references),
            held: BTreeSet::new(),
        },
    )
    .await?;
    assert!(reviewed.set_aside.is_empty());
    assert_eq!(
        serde_json::from_str::<Value>(&reviewed.candidate)?["blocks"][0]["title"],
        "Corrected example"
    );
    let prompts = scripted.prompts.lock().unwrap();
    let repair = prompts
        .iter()
        .find(|prompt| prompt.contains("issuesToFix"))
        .unwrap();
    let sent: Value = serde_json::from_str(repair.rsplit("\n\n").next().unwrap())?;
    let kept: Vec<_> = sent["originalRequirements"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|excerpt| excerpt["sourceIndex"].as_u64().unwrap())
        .collect();
    assert!(kept.contains(&4));
    assert!(!kept.contains(&5));
    Ok(())
}

#[tokio::test]
async fn a_finding_the_user_already_holds_is_not_repaired_again() -> Result<()> {
    let model = model(vec![
        review(Some(
            "The example shows \"result\": \"4\" although the code returns the number 4.",
        )),
        actionable(),
    ]);
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    let reviewed = review_material(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        &json!({"type":"object"}),
        lesson(EXCERPT).to_string(),
        4000,
        ReviewContext {
            outline: None,
            protected: &[],
            lesson_references: Some(&references),
            held: BTreeSet::from([FindingKey {
                kind: FindingKind::Teaching,
                site: FindingSite::Section(0),
            }]),
        },
    )
    .await?;
    assert_eq!(reviewed.set_aside.len(), 1);
    // Two calls: the review and its evidence check; no repair.
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn outlines_and_assessments_still_fail_closed_after_one_repair() {
    let model = model(vec![
        review(Some(
            "The example shows \"result\": \"4\" although the code returns the number 4.",
        )),
        retitle("Corrected example"),
        review(Some(
            "Section 1 still prints the result as the string \"4\" instead of the number.",
        )),
    ]);
    assert!(review_and_repair(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        &json!({"type":"object"}),
        lesson(EXCERPT).to_string(),
        4000,
    )
    .await
    .is_err());
}

#[tokio::test]
async fn a_teaching_finding_on_a_protected_sentence_waits_for_the_user_and_the_repair_leaves_it(
) -> Result<()> {
    // After a claim repair, the sentence a verified claim stands on is
    // protected: the reviewer's finding about it is held, not repaired.
    let model = model(vec![
        review(Some(
            "The example shows \"result\": \"4\" although the code returns the number 4.",
        )),
        actionable(),
    ]);
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    let candidate = lesson(EXCERPT);
    let sentence = "The function returns the number 4 for the sum of two and two.";
    let protected = [crate::lessons::content_verification::Protected {
        unit: 0,
        field: "body".into(),
        quote: sentence.into(),
    }];
    let reviewed = review_material(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        &json!({"type":"object"}),
        candidate.to_string(),
        4000,
        ReviewContext {
            outline: None,
            protected: &protected,
            lesson_references: Some(&references),
            held: BTreeSet::new(),
        },
    )
    .await?;
    assert_eq!(reviewed.set_aside.len(), 1);
    assert_eq!(
        reviewed.set_aside[0].finding.key().id(),
        "teaching:section-0"
    );
    // The review and its evidence check; no repair was asked for.
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    assert_eq!(after["blocks"][0]["body"], candidate["blocks"][0]["body"]);
    Ok(())
}

#[tokio::test]
async fn a_finding_on_a_sentence_no_claim_stands_on_is_repaired_inside_a_paragraph_with_verified_claims(
) -> Result<()> {
    // One paragraph, one inventory passage, two verified claims located to
    // their sentences; the teaching finding is about a sentence no claim
    // stands on, in the review's second passage (claim sources design §18.5).
    use crate::lessons::content_verification::Protected;
    const V1: &str = "Mitochondria produce most of a cell's ATP through oxidative phosphorylation.";
    const V2: &str = "Their inner membrane folds into cristae that enlarge its surface.";
    let body = format!(
        "{V1} {V2} They carry their own circular DNA, passed down from the mother, and divide on their own whenever the cell needs more energy.\nFor example, a muscle cell that works harder grows fewer mitochondria over time.\nTry it yourself: find an electron micrograph of a muscle cell, count the mitochondria you can see in one part of it, then count them in a micrograph of a skin cell taken at the same scale, and write down which cell holds more of them and why you think it does."
    );
    let candidate = json!({"blocks":[{"kind":"explanation","title":"Example","body":body,"rubric":[],"sourceIndex":0,"quote":EXCERPT}],"questions":[]});
    let defect = || {
        json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![json!({"index":0,"passageId":"section-0-passage-1","finding":"The example says a harder-working muscle cell grows fewer mitochondria, which contradicts the section's point that cells divide them when they need more energy.","hasDefect":true})])}).to_string()
    };
    let fix = json!({"edits":[{"path":"/blocks/0/body","before":"grows fewer mitochondria","after":"grows more mitochondria"}]}).to_string();
    let sentence = |quote: &str| Protected {
        unit: 0,
        field: "body".into(),
        quote: quote.into(),
    };

    // Held when the claims were located by their passage: it holds the
    // finding's sentence too.
    let held = model(vec![defect(), actionable()]);
    let outcome = review_protecting(&held, &candidate, &[sentence(&body)]).await?;
    assert_eq!(outcome.set_aside.len(), 1);
    assert_eq!(
        held.prompts.lock().unwrap().len(),
        2,
        "no repair was asked for"
    );

    // Repaired now: the claims protect their own sentences only.
    let repaired = model(vec![defect(), actionable(), fix, review(None)]);
    let outcome = review_protecting(&repaired, &candidate, &[sentence(V1), sentence(V2)]).await?;
    assert!(outcome.set_aside.is_empty(), "{:?}", outcome.set_aside);
    let after: Value = serde_json::from_str(&outcome.candidate)?;
    let after = after["blocks"][0]["body"].as_str().unwrap();
    assert_eq!(after, body.replace("grows fewer", "grows more"));
    assert!(
        after.starts_with(&format!("{V1} {V2} ")),
        "the claims' sentences are byte-identical"
    );
    Ok(())
}

#[tokio::test]
async fn a_lesson_whose_sentences_stand_on_checked_claims_is_never_written_again_whole_for_its_shape(
) -> Result<()> {
    // After cuts a lesson lacks some of what its writer was asked for. A
    // rewrite of the whole lesson changes the sentences its claims stand on
    // and is always refused: four of them, fourteen minutes each on the 27B,
    // in one job of 2026-11-03. What its shape asks waits for the user.
    let sentence = "The function returns the number 4 for the sum of two and two.";
    let mut candidate = lesson(EXCERPT);
    candidate["blocks"][0]["kind"] = json!("guided_practice");
    candidate["blocks"][0]["body"] = json!(sentence);
    let protected = [crate::lessons::content_verification::Protected {
        unit: 0,
        field: "body".into(),
        quote: sentence.into(),
    }];
    let model = model(vec![review(None)]);

    let reviewed = review_protecting(&model, &candidate, &protected).await?;

    assert!(
        !model
            .prompts
            .lock()
            .unwrap()
            .iter()
            .any(|prompt| prompt.contains("Give the complete corrected material")),
        "the whole lesson was asked for again"
    );
    let waiting: Vec<_> = reviewed
        .set_aside
        .iter()
        .map(|waiting| waiting.finding.key().id())
        .collect();
    assert_eq!(waiting, ["structure:section-0"]);
    assert_eq!(
        serde_json::from_str::<Value>(&reviewed.candidate)?,
        candidate
    );
    Ok(())
}

#[test]
fn a_missing_citation_does_not_hold_up_the_content_review_but_other_structure_does() {
    let note = "Something to fix in this section.";
    let citation = ReviewFinding::new(FindingKind::Quotation, Some(FindingSite::Section(1)), note);
    let shape = ReviewFinding::new(FindingKind::Structure, Some(FindingSite::Lesson), note);
    let passes = Passes::default();

    assert!(!holds_up_review(
        std::slice::from_ref(&citation),
        Some(&passes)
    ));
    assert!(holds_up_review(
        &[citation.clone(), shape.clone()],
        Some(&passes)
    ));
    // Without a lesson's passes every structure finding holds the review up.
    assert!(holds_up_review(std::slice::from_ref(&shape), None));
    // Structure the machine stopped repairing does not.
    let stopped = Passes::new(BTreeSet::from([shape.key()]));
    assert!(!holds_up_review(&[citation, shape], Some(&stopped)));
}

#[tokio::test]
async fn a_crisp_block_is_reviewed_not_sent_to_repair_for_its_length() -> Result<()> {
    let mut candidate = lesson(EXCERPT);
    candidate["blocks"][0]["body"] =
        json!("The function returns the number 4 for the sum of two and two.");
    let model = model(vec![review(None)]);
    let reviewed = run(&model, candidate.clone()).await?;
    assert!(reviewed.set_aside.is_empty(), "{:?}", reviewed.set_aside);
    assert_eq!(
        serde_json::from_str::<Value>(&reviewed.candidate)?,
        candidate
    );
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_short_review_issue_is_checked_and_repaired_not_dropped() -> Result<()> {
    let mut brief: Value = serde_json::from_str(&review(None))?;
    brief["issues"] = json!(["Wrong."]);
    let model = model(vec![
        brief.to_string(),
        actionable(),
        retitle("Corrected example"),
        review(None),
    ]);
    let reviewed = run(&model, lesson(EXCERPT)).await?;
    assert!(reviewed.set_aside.is_empty(), "{:?}", reviewed.set_aside);
    let candidate: Value = serde_json::from_str(&reviewed.candidate)?;
    assert_eq!(candidate["blocks"][0]["title"], "Corrected example");
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 4);
    assert!(prompts[2].contains("\"issuesToFix\":[\"Wrong.\"]"));
    Ok(())
}

#[tokio::test]
async fn a_long_review_issue_is_repaired_where_a_review_fails_closed() -> Result<()> {
    let long = format!(
        "The example prints the result as a string: {}",
        "it never states the number it returns. ".repeat(25)
    );
    assert!(long.chars().count() > 700);
    let mut first: Value = serde_json::from_str(&review(None))?;
    first["issues"] = json!([long]);
    let model = model(vec![
        first.to_string(),
        retitle("Corrected example"),
        review(None),
    ]);
    let repaired = review_and_repair(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        &json!({"type":"object"}),
        lesson(EXCERPT).to_string(),
        4000,
    )
    .await?;
    let repaired: Value = serde_json::from_str(&repaired)?;
    assert_eq!(repaired["blocks"][0]["title"], "Corrected example");
    assert!(model.prompts.lock().unwrap()[1].contains(&long));
    Ok(())
}

const INPUT: &str = "teaching-review-resume";

/// The scripted review, its connection lost on the first call after its first
/// repair is answered: the recheck. The error leaves the review as a requeue
/// does.
struct DropsAfterRepair {
    model: ScriptedModel,
    repaired: AtomicBool,
    dropped: AtomicBool,
}

#[async_trait::async_trait]
impl lattice_core::application::ports::llm_port::LLMPort for DropsAfterRepair {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if self.repaired.load(Ordering::SeqCst) && !self.dropped.swap(true, Ordering::SeqCst) {
            return Err(AppError::Network("the connection dropped".into()));
        }
        let response = self.model.complete(request).await;
        if asks_for_repair(request) {
            self.repaired.store(true, Ordering::SeqCst);
        }
        response
    }
    fn model_name(&self) -> &str {
        self.model.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.model.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.model.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn retitle_again(after: &str) -> String {
    json!({"edits":[{"path":"/blocks/0/title","before":"Corrected example","after":after}]})
        .to_string()
}

/// The defect of `a_defect_one_repair_did_not_fix_publishes_and_waits_for_the_user`,
/// with the calls a second repair and its recheck would make after the first
/// recheck.
fn defect_no_repair_fixes() -> Vec<String> {
    vec![
        review(Some(
            "The example shows \"result\": \"4\" although the code returns the number 4.",
        )),
        actionable(),
        retitle("Corrected example"),
        review(Some(
            "Section 1 still prints the result as the string \"4\" instead of the number.",
        )),
        actionable(),
        retitle_again("Corrected twice"),
        review(Some(
            "Section 1 still prints the result as the string \"4\" after two repairs.",
        )),
        actionable(),
    ]
}

/// What a claim repair does to a draft: it rewrites a sentence.
fn claim_repair(draft: &mut Value) {
    let body = draft["blocks"][0]["body"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    draft["blocks"][0]["body"] = json!(body.replacen("returns the number 4", "returns 4", 1));
}

fn repairs_asked(llm: &DropsAfterRepair) -> usize {
    let prompts = llm.model.prompts.lock().unwrap();
    prompts
        .iter()
        .filter(|prompt| prompt.contains("issuesToFix"))
        .count()
}

/// A lesson's review run twice in one job, the first run cut short after its
/// first repair as a requeue cuts one. The second starts on the draft that
/// repair saved, after `between` has happened to it. Returns the repairs asked
/// of the model across both runs and the second run's review.
async fn cut_short_then_resumed(between: fn(&mut Value)) -> Result<(usize, Reviewed)> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson_id) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&pool).await?;
    let llm = DropsAfterRepair {
        model: model(defect_no_repair_fixes()),
        repaired: AtomicBool::new(false),
        dropped: AtomicBool::new(false),
    };
    let first = lesson_drafts::run(&repo, &job, &lesson_id, async {
        lesson_drafts::resume(INPUT.into()).await?;
        run(&llm, lesson(EXCERPT)).await
    })
    .await;
    assert!(matches!(first, Err(AppError::Network(_))));
    assert_eq!(repairs_asked(&llm), 1, "cut short after the first repair");
    let second = lesson_drafts::run(&repo, &job, &lesson_id, async {
        let saved = lesson_drafts::resume(INPUT.into())
            .await?
            .expect("the repair saved its draft");
        let mut draft: Value = serde_json::from_str(&saved)?;
        assert_eq!(draft["blocks"][0]["title"], "Corrected example");
        let repaired = draft.clone();
        between(&mut draft);
        if draft != repaired {
            lesson_drafts::save(&draft.to_string()).await?;
        }
        run(&llm, draft).await
    })
    .await?;
    Ok((repairs_asked(&llm), second))
}

/// Run through, the review above repairs the defect once, rechecks, sees
/// nothing fixed and stops. Cut short after that repair and run again, it
/// stops the same way instead of repairing from zero.
#[tokio::test]
async fn a_review_cut_short_after_its_first_repair_resumes_with_the_repairs_it_had_made(
) -> Result<()> {
    let (repairs, reviewed) = cut_short_then_resumed(|_| {}).await?;
    assert_eq!(repairs, 1);
    assert_eq!(reviewed.set_aside.len(), 1);
    let waiting = &reviewed.set_aside[0];
    assert_eq!(waiting.finding.key().id(), "teaching:section-0");
    assert_eq!(waiting.repairs, 1);
    Ok(())
}

/// The repairs a review had made describe the draft it made them to: once a
/// claim repair has changed that draft, the review starts again.
#[tokio::test]
async fn a_review_cut_short_resumes_afresh_on_a_draft_changed_since_its_repair() -> Result<()> {
    let (repairs, reviewed) = cut_short_then_resumed(claim_repair).await?;
    assert_eq!(repairs, 2);
    assert_eq!(reviewed.set_aside.len(), 1);
    let waiting = &reviewed.set_aside[0];
    assert_eq!(waiting.finding.key().id(), "teaching:section-0");
    assert_eq!(waiting.repairs, 1);
    Ok(())
}

#[test]
fn whole_lesson_and_fully_guarded_section_findings_wait_when_claims_are_protected() {
    use crate::lessons::content_verification::Protected;
    let sentence = "The function returns the number 4 for the sum of two and two.";
    let mut lesson = lesson(EXCERPT);
    lesson["blocks"][0]["body"] = json!(sentence);
    let protected = vec![
        Protected {
            unit: 0,
            field: "body".into(),
            quote: sentence.into(),
        },
        Protected {
            unit: 0,
            field: "title".into(),
            quote: "Example".into(),
        },
    ];
    let lesson_finding = ReviewFinding::new(
        FindingKind::Structure,
        Some(FindingSite::Lesson),
        "no contrast block",
    );
    let section_finding = ReviewFinding::new(
        FindingKind::Teaching,
        Some(FindingSite::Section(0)),
        "expand on the explanation",
    );
    let findings = vec![lesson_finding.clone(), section_finding.clone()];
    let held = protected::on_protected(&lesson, &findings, &protected);
    let expected = vec![lesson_finding.key(), section_finding.key()];
    assert_eq!(held, expected);
}

fn question_lesson() -> Value {
    let mut lesson = lesson(EXCERPT);
    lesson["questions"] = json!([{
        "kind": "quiz",
        "prompt": "What does the function return for two plus two?",
        "options": ["Three is returned.", "Five is returned.", "Four is returned.", "Zero is returned."],
        "correctIndex": 2,
        "rationales": ["Three is wrong.", "Five is wrong.", "Four is right.", "Zero is wrong."],
        "explanation": "The function adds its two arguments and returns four.",
        "sourceIndex": 0,
        "quote": EXCERPT,
        "citations": []
    }]);
    lesson
}

fn protect(
    unit: usize,
    field: &str,
    quote: &str,
) -> crate::lessons::content_verification::Protected {
    crate::lessons::content_verification::Protected {
        unit,
        field: field.into(),
        quote: quote.into(),
    }
}

#[test]
fn a_finding_on_an_option_is_not_held_when_only_the_explanation_is_protected() {
    let lesson = question_lesson();
    let protected = vec![protect(
        1,
        "explanation",
        "The function adds its two arguments and returns four.",
    )];
    let finding = ReviewFinding::new(
        FindingKind::Teaching,
        Some(FindingSite::Question(0)),
        "the option is ambiguous",
    )
    .with_passage("Four is returned.");
    assert!(protected::on_protected(&lesson, &[finding], &protected).is_empty());
}

#[test]
fn a_quotation_finding_on_the_source_quote_is_not_held_by_a_protected_body() {
    let mut lesson = lesson(EXCERPT);
    let sentence = "The function returns the number 4 for the sum of two and two.";
    lesson["blocks"][0]["body"] = json!(sentence);
    let protected = vec![protect(0, "body", sentence)];
    let finding = ReviewFinding::new(
        FindingKind::Quotation,
        Some(FindingSite::Section(0)),
        "the quotation matches no reference",
    )
    .with_passage(EXCERPT);
    assert!(protected::on_protected(&lesson, &[finding], &protected).is_empty());
}

#[test]
fn a_question_finding_with_no_passage_is_not_held_while_options_are_unprotected() {
    let lesson = question_lesson();
    let protected = vec![protect(
        1,
        "explanation",
        "The function adds its two arguments and returns four.",
    )];
    let finding = ReviewFinding::new(
        FindingKind::Teaching,
        Some(FindingSite::Question(0)),
        "the distractors are weak",
    );
    assert!(protected::on_protected(&lesson, &[finding], &protected).is_empty());
}

#[test]
fn a_section_finding_with_no_passage_is_not_held_while_the_title_is_unprotected() {
    let mut lesson = lesson(EXCERPT);
    let sentence = "The function returns the number 4 for the sum of two and two.";
    lesson["blocks"][0]["body"] = json!(sentence);
    let protected = vec![protect(0, "body", sentence)];
    let finding = ReviewFinding::new(
        FindingKind::Teaching,
        Some(FindingSite::Section(0)),
        "expand on the explanation",
    );
    assert!(protected::on_protected(&lesson, &[finding], &protected).is_empty());
}

#[test]
fn a_review_repair_with_no_passage_cannot_rewrite_the_kind_or_quote() -> Result<()> {
    let mut previous = lesson(EXCERPT);
    previous["blocks"][0]["body"] =
        json!("The first sentence is protected here. The second sentence may change.");
    let protected = vec![protect(0, "body", "The first sentence is protected here.")];
    let mut repaired = previous.clone();
    repaired["blocks"][0]["body"] =
        json!("The first sentence is protected here. The second sentence now reads better.");
    repaired["blocks"][0]["quote"] = json!("A different quotation that was never a target.");
    repaired["blocks"][0]["kind"] = json!("example");
    let finding = ReviewFinding::new(
        FindingKind::Teaching,
        Some(FindingSite::Section(0)),
        "improve the second sentence",
    );
    let (confined, _) =
        protected::confine_review(&previous, &repaired.to_string(), &[finding], &protected)?;
    let confined: Value = serde_json::from_str(&confined.unwrap())?;
    assert_eq!(
        confined["blocks"][0]["quote"],
        previous["blocks"][0]["quote"]
    );
    assert_eq!(confined["blocks"][0]["kind"], previous["blocks"][0]["kind"]);
    assert_eq!(confined["blocks"][0]["body"], repaired["blocks"][0]["body"]);
    Ok(())
}

#[tokio::test]
async fn a_protected_repair_that_edits_one_section_holds_another_section_whose_edit_was_reverted(
) -> Result<()> {
    use crate::lessons::content_verification::Protected;
    const V1: &str = "Mitochondria produce most of a cell's ATP through oxidative phosphorylation.";
    const V2: &str = "Their inner membrane folds into cristae that enlarge its surface.";
    let block0_body = format!(
        "{V1} {V2} They carry their own circular DNA, passed down from the mother, and divide on their own whenever the cell needs more energy.\nFor example, a muscle cell that works harder grows fewer mitochondria over time.\nTry it yourself: find an electron micrograph of a muscle cell, count the mitochondria you can see in one part of it, then count them in a micrograph of a skin cell taken at the same scale, and write down which cell holds more of them and why you think it does."
    );
    let block1_body = "The ribosome is a complex molecular machine found within all living cells that serves as the site of biological protein synthesis. Ribosomes link amino acids together in the specified order.";
    let candidate = json!({
        "blocks": [
            {
                "kind": "explanation",
                "title": "Block 0",
                "body": block0_body,
                "rubric": [],
                "sourceIndex": 0,
                "quote": EXCERPT,
            },
            {
                "kind": "explanation",
                "title": "Block 1",
                "body": block1_body,
                "rubric": [],
                "sourceIndex": 0,
                "quote": EXCERPT,
            }
        ],
        "questions": []
    });
    let protected = vec![Protected {
        unit: 0,
        field: "body".into(),
        quote: V1.into(),
    }];

    let pass1_review = json!({
        "issues": [],
        "blockChecks": crate::teaching_review::fixture_checks(vec![
            json!({
                "index": 0,
                "passageId": "section-0-passage-1",
                "finding": "The example contradicts the rule on mitochondrial division.",
                "hasDefect": true,
            }),
            json!({
                "index": 1,
                "passageId": "section-1-passage-0",
                "finding": "Block 1 needs a clearer description.",
                "hasDefect": true,
            }),
        ])
    })
    .to_string();

    let pass1_evidence = json!({"decisions": [
        {
            "issueId": "issue-0",
            "verdict": "actionable",
            "basis": "internal_conflict",
            "evidenceIds": ["blocks-0"],
            "partId": "blocks-0",
            "requirementId": null,
            "reason": "Section 0 defect."
        },
        {
            "issueId": "issue-1",
            "verdict": "actionable",
            "basis": "internal_conflict",
            "evidenceIds": ["blocks-1"],
            "partId": "blocks-1",
            "requirementId": null,
            "reason": "Section 1 defect."
        }
    ]})
    .to_string();

    let repair = json!({
        "edits": [
            {
                "path": "/blocks/0/body",
                "before": block0_body,
                "after": block0_body.replace(V1, "Mitochondria make ATP directly."),
            },
            {
                "path": "/blocks/1/body",
                "before": "Ribosomes link amino acids together in the specified order.",
                "after": "Ribosomes link amino acids together in the order messenger RNA specifies.",
            }
        ]
    })
    .to_string();

    let pass2_review = json!({
        "issues": [],
        "blockChecks": crate::teaching_review::fixture_checks(vec![
            json!({
                "index": 0,
                "passageId": "section-0-passage-1",
                "finding": "The example contradicts the rule on mitochondrial division.",
                "hasDefect": true,
            }),
            json!({
                "index": 1,
                "passageId": "section-1-passage-0",
                "finding": "Block 1 is now fine.",
                "hasDefect": false,
            }),
        ])
    })
    .to_string();

    let pass2_evidence = json!({"decisions": [
        {
            "issueId": "issue-0",
            "verdict": "actionable",
            "basis": "internal_conflict",
            "evidenceIds": ["blocks-0"],
            "partId": "blocks-0",
            "requirementId": null,
            "reason": "Section 0 defect."
        }
    ]})
    .to_string();

    let model = model(vec![
        pass1_review,
        pass1_evidence,
        repair,
        pass2_review,
        pass2_evidence,
    ]);

    let reviewed = review_protecting(&model, &candidate, &protected).await?;

    assert_eq!(reviewed.set_aside.len(), 1);
    assert_eq!(
        reviewed.set_aside[0].finding.key().id(),
        "teaching:section-0"
    );

    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    assert_eq!(after["blocks"][0]["body"], block0_body);
    assert_eq!(
        after["blocks"][1]["body"],
        "The ribosome is a complex molecular machine found within all living cells that serves as the site of biological protein synthesis. Ribosomes link amino acids together in the order messenger RNA specifies."
    );

    assert_eq!(model.prompts.lock().unwrap().len(), 5);
    Ok(())
}
