//! A lesson's teaching review publishes with what the repairs could not
//! settle waiting for the user.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
use crate::features::learning::reference_collection::ReferenceCollection;
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
    json!({"issues":[],"blockChecks":crate::features::learning::teaching_review::fixture_checks(vec![json!({"index":0,"passageId":"section-0-passage-0","finding":finding,"hasDefect":defect.is_some()})])}).to_string()
}

fn actionable() -> String {
    json!({"issue-0":{"verdict":"actionable","basis":"internal_conflict","evidenceIds":["blocks-0"],"requirementId":null,"reason":"The example's printed result contradicts the value the code returns."}}).to_string()
}

fn retitle(after: &str) -> String {
    json!({"edits":[{"path":"/blocks/0/title","before":"Example","after":after}]}).to_string()
}

fn references() -> Vec<crate::features::learning::dto::LearningSourceDto> {
    vec![crate::features::learning::dto::LearningSourceDto {
        id: "saved-reference".into(),
        title: "Example reference".into(),
        url: None,
        acquired_at: 0,
        excerpt: EXCERPT.into(),
    }]
}

async fn run(model: &ScriptedModel, candidate: Value) -> Result<Reviewed> {
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
    protected: &[crate::features::learning::lessons::content_verification::Protected],
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

/// The scripted review, with every teaching repair request longer than the
/// model's window.
struct RepairTooLong(ScriptedModel);

#[async_trait::async_trait]
impl lattice_core::application::ports::llm_port::LLMPort for RepairTooLong {
    async fn complete(
        &self,
        request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        let repair = request.input.iter().any(|item| {
            matches!(
                item,
                lattice_core::application::ports::llm_port::CompletionInput::Message { content, .. }
                    if content.contains("issuesToFix")
            )
        });
        if repair {
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
    let protected = [
        crate::features::learning::lessons::content_verification::Protected {
            unit: 0,
            field: "body".into(),
            quote: sentence.into(),
        },
    ];
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
    use crate::features::learning::lessons::content_verification::Protected;
    const V1: &str = "Mitochondria produce most of a cell's ATP through oxidative phosphorylation.";
    const V2: &str = "Their inner membrane folds into cristae that enlarge its surface.";
    let body = format!(
        "{V1} {V2} They carry their own circular DNA, passed down from the mother, and divide on their own whenever the cell needs more energy.\nFor example, a muscle cell that works harder grows fewer mitochondria over time.\nTry it yourself: find an electron micrograph of a muscle cell, count the mitochondria you can see in one part of it, then count them in a micrograph of a skin cell taken at the same scale, and write down which cell holds more of them and why you think it does."
    );
    let candidate = json!({"blocks":[{"kind":"explanation","title":"Example","body":body,"rubric":[],"sourceIndex":0,"quote":EXCERPT}],"questions":[]});
    let defect = || {
        json!({"issues":[],"blockChecks":crate::features::learning::teaching_review::fixture_checks(vec![json!({"index":0,"passageId":"section-0-passage-1","finding":"The example says a harder-working muscle cell grows fewer mitochondria, which contradicts the section's point that cells divide them when they need more energy.","hasDefect":true})])}).to_string()
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
