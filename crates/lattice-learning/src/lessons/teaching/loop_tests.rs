//! A lesson's teaching review publishes with what the repairs could not
//! settle waiting for the user.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::content_verification::tests::fixture_task;
use crate::lesson_drafts;
use crate::lessons::course_generation_tests::{
    present, review_reply, verdict, ScriptedModel, Windowed,
};
use crate::reference_collection::ReferenceCollection;
use crate::review_findings::{FindingKey, Held};
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::shared::persistence::Database;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

const NUMBER_FOUR: &str = "The function returns the number 4 for the sum of two and two.";
/// A defect of the lesson's explanation section that the lesson's reviewer owns.
const PROMISED: &str = "The section promises that the tutor runs the learner's code and reports whether the tests pass.";
const STILL_PROMISED: &str =
    "Section 1 still promises that the tutor runs the learner's code and reports the result.";
const STILL_PROMISED_TWICE: &str =
    "Section 1 still promises that the tutor runs the learner's code after two repairs.";
const FIRST_IDEA_PROMISE: &str =
    "The first idea promises the learner an expert certification of their work.";
pub(super) const EXCERPT: &str =
    "This saved reference describes the exact behavior of the example tool.";

fn lesson(quote: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Example","body":"The function returns the number 4 for the sum of two and two. ".repeat(12),"rubric":[],"sourceIndex":0,"quote":quote}],"questions":[]})
}

pub(super) fn context() -> String {
    json!({"sources":[{"sourceIndex":0,"excerpt":EXCERPT}]}).to_string()
}

fn review(defect: Option<&str>) -> String {
    judged(defect, &[])
}

/// A review of the lesson's one section, with `verdicts` on the findings it
/// was asked about; a defect names the lesson concern it fails.
fn judged(defect: Option<&str>, verdicts: &[Value]) -> String {
    let mut check = section_check(defect);
    if defect.is_some() {
        check["concerns"] = json!(["promise"]);
    }
    review_reply(&[], vec![check], verdicts)
}

/// [`review`] of material the general entry point reviews: its checks name no concerns.
fn review_open(defect: Option<&str>) -> String {
    judged_open(defect, &[])
}

/// [`judged`] of material the general entry point reviews: its checks name no concerns.
fn judged_open(defect: Option<&str>, verdicts: &[Value]) -> String {
    review_reply(&[], vec![section_check(defect)], verdicts)
}

/// The check of the lesson's one section: a defect when `defect` is given.
fn section_check(defect: Option<&str>) -> Value {
    let finding =
        defect.unwrap_or("The complete explanation was reviewed and no teaching defect was found.");
    json!({"index":0,"passageId":"section-0-passage-0","finding":finding,"hasDefect":defect.is_some()})
}

fn actionable() -> String {
    json!({"decisions":[{"issueId":"issue-0","verdict":"actionable","basis":"internal_conflict","evidenceIds":["blocks-0"],"partId":"blocks-0","requirementId":null,"reason":"The example's printed result contradicts the value the code returns."}]}).to_string()
}

fn retitle(after: &str) -> String {
    json!({"edits":[{"path":"/blocks/0/title","before":"Example","after":after}]}).to_string()
}

fn retitle_from(before: &str, after: &str) -> String {
    json!({"edits":[{"path":"/blocks/0/title","before":before,"after":after}]}).to_string()
}

pub(super) fn references() -> Vec<crate::dto::LearningSourceDto> {
    vec![crate::dto::LearningSourceDto {
        id: "saved-reference".into(),
        title: "Example reference".into(),
        url: None,
        acquired_at: 0,
        excerpt: EXCERPT.into(),
    }]
}

pub(super) async fn run(model: &dyn LLMPort, candidate: Value) -> Result<Reviewed> {
    let sources = references();
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
        ReviewSchema::Other(&json!({"type":"object"})),
        candidate.to_string(),
        4000,
        &mut LessonSubject::new(&references, &sources, protected, Held::default()),
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
async fn a_defect_no_repair_fixes_waits_after_its_second_repair() -> Result<()> {
    // The failure that ended a two-hour job: the recheck still found the
    // defect, reworded. Repairs are counted so that job ends.
    let model = model(defect_no_repair_fixes());
    let reviewed = run(&model, lesson(EXCERPT)).await?;
    let candidate: Value = serde_json::from_str(&reviewed.candidate)?;
    assert_eq!(candidate["blocks"][0]["title"], "Corrected twice");
    assert_eq!(reviewed.set_aside.len(), 1);
    let waiting = &reviewed.set_aside[0];
    assert_eq!(waiting.finding.place().id(), "teaching:section-0");
    assert_eq!(waiting.finding.id().as_str(), "reviewer/1");
    assert_eq!(waiting.repairs, 2);
    assert!(waiting
        .finding
        .note
        .contains("reports whether the tests pass"));
    assert!(!waiting.finding.note.contains("after two repairs"));
    assert!(waiting.finding.passage.is_some());
    assert_eq!(model.prompts.lock().unwrap().len(), 6);
    let asked = repair_requests(&model);
    assert_eq!(asked.len(), 2);
    assert!(asked[1]["issuesToFix"][0]["lastAttempt"]
        .as_str()
        .is_some_and(
            |attempt| attempt.starts_with("The last repair's edits for this finding were applied")
        ));
    Ok(())
}

#[tokio::test]
async fn the_reviewer_is_told_to_name_a_part_by_its_label_or_title() -> Result<()> {
    let model = model(vec![review(None)]);
    run(&model, lesson(EXCERPT)).await?;
    let prompts = model.prompts.lock().unwrap();
    assert!(prompts[0].contains("by its label"));
    assert!(prompts[0].contains("never by its section-N ID or its index"));
    Ok(())
}

#[tokio::test]
async fn a_pass_that_fixes_the_defect_publishes_with_nothing_waiting() -> Result<()> {
    let model = model(vec![
        review(Some(PROMISED)),
        actionable(),
        retitle("Corrected example"),
        judged(None, &[verdict("reviewer/1", "resolved")]),
    ]);
    let reviewed = run(&model, lesson(EXCERPT)).await?;
    assert!(reviewed.set_aside.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_lessons_own_repair_stays_in_the_review_phase() -> Result<()> {
    let model = model(vec![
        review(Some(PROMISED)),
        actionable(),
        retitle("Corrected example"),
        judged(None, &[verdict("reviewer/1", "resolved")]),
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
    let model = model(vec![
        issue,
        patch,
        json!({"issues":[],"openFindings":[verdict("reviewer/1", "resolved")]}).to_string(),
    ]);
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
        retitle_from("Example, revised", "Example, revised again"),
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
        .map(|waiting| waiting.finding.place().id())
        .collect();
    assert_eq!(keys, ["quotation:section-0"]);
    // The repair was asked to fix the quotation, with the checks it failed.
    let prompts = model.prompts.lock().unwrap();
    assert!(prompts[1].contains("sourceQuoteChecks"));
    assert!(prompts[1].contains("does not match its saved reference"));
    Ok(())
}

#[tokio::test]
async fn a_finding_whose_repairs_cannot_be_used_waits_after_its_second() -> Result<()> {
    let unusable =
        json!({"edits":[{"path":"/blocks/0/title","before":"Missing text","after":"X"}]})
            .to_string();
    let model = model(vec![
        review(Some(PROMISED)),
        actionable(),
        unusable.clone(),
        unusable.clone(),
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
        reviewed.set_aside[0].finding.place().id(),
        "teaching:section-0"
    );
    assert_eq!(reviewed.set_aside[0].repairs, 2);
    let asked = repair_requests(&model);
    assert!(asked
        .last()
        .is_some_and(|request| request["issuesToFix"][0]["lastAttempt"]
            .as_str()
            .is_some_and(
                |attempt| attempt.starts_with("The last repair's answer could not be used")
            )));
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
    let model = RepairTooLong(model(vec![review(Some(PROMISED)), actionable()]));
    let original = lesson(EXCERPT);
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    let reviewed = review_material(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        ReviewSchema::Other(&json!({"type":"object"})),
        original.to_string(),
        4000,
        &mut LessonSubject::new(&references, &sources, &[], Held::default()),
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
        review(Some(PROMISED)),
        actionable(),
        retitle("Corrected example"),
        judged(None, &[verdict("reviewer/1", "resolved")]),
    ]);
    let window = Windowed(&scripted, 13_000);
    let sources: Vec<_> = (0..6)
        .map(|index| crate::dto::LearningSourceDto {
            id: format!("source-{index}"),
            title: format!("Source {index}"),
            url: None,
            acquired_at: 0,
            excerpt: "x ".repeat(4000),
        })
        .collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let reviewed = review_material(
        &window,
        LESSON_REPAIR_SYSTEM,
        &json!({"sources":excerpts}).to_string(),
        ReviewSchema::Other(&json!({"type":"object"})),
        candidate.to_string(),
        4000,
        &mut LessonSubject::new(&references, &sources, &[], Held::default()),
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
    let sent: Value = fixture_task(repair).expect("a JSON task");
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
    let model = model(vec![review(Some(PROMISED)), actionable()]);
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    let reviewed = review_material(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        ReviewSchema::Other(&json!({"type":"object"})),
        lesson(EXCERPT).to_string(),
        4000,
        &mut LessonSubject::new(
            &references,
            &sources,
            &[],
            Held {
                places: BTreeSet::from([FindingKey {
                    kind: FindingKind::Teaching,
                    site: FindingSite::Section(0),
                }]),
                ..Held::default()
            },
        ),
    )
    .await?;
    assert_eq!(reviewed.set_aside.len(), 1);
    // Two calls: the review and its evidence check; no repair.
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn outlines_and_assessments_still_fail_closed_once_their_repairs_are_spent() {
    let still = || {
        judged_open(
            Some("Section 1 still prints the result as the string \"4\" instead of the number."),
            &[present("reviewer/1", NUMBER_FOUR)],
        )
    };
    let model = model(vec![
        review_open(Some(
            "The example shows \"result\": \"4\" although the code returns the number 4.",
        )),
        retitle("Corrected example"),
        still(),
        retitle_again("Corrected again"),
        still(),
    ]);
    let result = review_and_repair(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        &json!({"type":"object"}),
        lesson(EXCERPT).to_string(),
        4000,
    )
    .await;
    assert!(
        result.is_err_and(|error| error.to_string().contains("still found an issue")),
        "the material was published or failed for another reason"
    );
}

/// A schema that has a `modules` property does not make material an outline:
/// what is reviewed comes from the entry point, so this material is repaired
/// by span edits, as other material is, and not rewritten whole.
#[tokio::test]
async fn the_subject_comes_from_the_entry_point_not_the_schema() {
    let scripted = model(vec![
        review_open(Some(
            "The example shows \"result\": \"4\" although the code returns the number 4.",
        )),
        retitle("Corrected example"),
        judged(None, &[verdict("reviewer/1", "resolved")]),
    ]);
    let schema = json!({"type":"object","properties":{"modules":{"type":"array"}}});
    let _ = review_and_repair(
        &scripted,
        LESSON_REPAIR_SYSTEM,
        &context(),
        &schema,
        lesson(EXCERPT).to_string(),
        4000,
    )
    .await;
    let prompts = scripted.prompts.lock().unwrap();
    let repair = prompts
        .iter()
        .find(|prompt| prompt.contains("issuesToFix"))
        .map(String::as_str);
    assert!(repair.is_some(), "no repair request was sent");
    assert!(
        repair
            .unwrap()
            .contains("Correct only the established teaching defects"),
        "the repair was asked for as a rewrite of the whole material"
    );
}

#[tokio::test]
async fn a_teaching_finding_on_a_protected_sentence_waits_for_the_user_and_the_repair_leaves_it(
) -> Result<()> {
    // After a claim repair, the sentence a verified claim stands on is
    // protected: the reviewer's finding about it is held, not repaired.
    let model = model(vec![review(Some(PROMISED)), actionable()]);
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    let candidate = lesson(EXCERPT);
    let sentence = "The function returns the number 4 for the sum of two and two.";
    let protected = [crate::lessons::content_verification::Protected {
        unit: 0,
        field: "body".into(),
        quote: sentence.into(),
        released: false,
    }];
    let reviewed = review_material(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        ReviewSchema::Other(&json!({"type":"object"})),
        candidate.to_string(),
        4000,
        &mut LessonSubject::new(&references, &sources, &protected, Held::default()),
    )
    .await?;
    assert_eq!(reviewed.set_aside.len(), 1);
    assert_eq!(
        reviewed.set_aside[0].finding.place().id(),
        "teaching:section-0"
    );
    // The review and its evidence check; no repair was asked for.
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    assert_eq!(after["blocks"][0]["body"], candidate["blocks"][0]["body"]);
    Ok(())
}

#[tokio::test]
async fn a_teaching_finding_on_the_sentence_of_a_machine_claim_with_attempts_left_is_repaired_and_the_change_kept(
) -> Result<()> {
    use crate::lessons::content_verification::Protected;
    const V1: &str = "Mitochondria produce most of a cell's ATP through oxidative phosphorylation.";
    let body = format!(
        "{V1} Their inner membrane folds into cristae that enlarge its surface. They carry their own circular DNA, passed down from the mother, and divide on their own whenever the cell needs more energy."
    );
    let candidate = json!({"blocks":[{"kind":"explanation","title":"Example","body":body,"rubric":[],"sourceIndex":0,"quote":EXCERPT}],"questions":[]});
    let finding = || review(Some(PROMISED));
    let fix = json!({"edits":[{"path":"/blocks/0/body","before":"produce most of","after":"produce some of"}]}).to_string();
    let sentence = |released: bool| Protected {
        unit: 0,
        field: "body".into(),
        quote: V1.into(),
        released,
    };

    // The claim is the user's, or has no attempts left: held, as before.
    let held = model(vec![finding(), actionable()]);
    let outcome = review_protecting(&held, &candidate, &[sentence(false)]).await?;
    assert_eq!(outcome.set_aside.len(), 1);
    assert_eq!(
        held.prompts.lock().unwrap().len(),
        2,
        "no repair was asked for"
    );

    // The machine's claim with attempts left: repaired, and the repair kept.
    let repaired = model(vec![
        finding(),
        actionable(),
        fix,
        judged(None, &[verdict("reviewer/1", "resolved")]),
    ]);
    let outcome = review_protecting(&repaired, &candidate, &[sentence(true)]).await?;
    assert!(outcome.set_aside.is_empty(), "{:?}", outcome.set_aside);
    let after: Value = serde_json::from_str(&outcome.candidate)?;
    assert_eq!(
        after["blocks"][0]["body"].as_str().unwrap(),
        body.replace("produce most of", "produce some of")
    );
    Ok(())
}

#[test]
fn a_finding_with_no_passage_about_a_released_sentence_changes_that_sentence_and_keeps_the_change()
{
    let sentence = "The function returns the number 4 for the sum of two and two.";
    let mut previous = lesson(EXCERPT);
    previous["blocks"][0]["body"] = json!(sentence);
    let mut repaired = previous.clone();
    repaired["blocks"][0]["body"] = json!("The function returns the number 4 for two plus two.");
    let protected = vec![crate::lessons::content_verification::Protected {
        unit: 0,
        field: "body".into(),
        quote: sentence.into(),
        released: true,
    }];
    let finding =
        ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "the explanation is wordy");

    assert!(
        protected::on_protected(&previous, std::slice::from_ref(&finding), &protected).is_empty()
    );
    let touched = std::collections::BTreeMap::from([("/blocks/0".to_owned(), vec![&finding])]);
    let kept = protected::confine_review(&previous, &repaired, &touched, &protected)
        .unwrap()
        .unwrap();

    assert_eq!(kept["blocks"][0]["body"], repaired["blocks"][0]["body"]);
}

#[tokio::test]
async fn a_finding_on_a_sentence_no_claim_stands_on_is_repaired_inside_a_paragraph_with_verified_claims(
) -> Result<()> {
    // Three paragraphs: two verified claims located to their sentences in the
    // first; the teaching finding is about a sentence no claim stands on, in
    // the second (claim sources design §18.5).
    use crate::lessons::content_verification::Protected;
    const V1: &str = "Mitochondria produce most of a cell's ATP through oxidative phosphorylation.";
    const V2: &str = "Their inner membrane folds into cristae that enlarge its surface.";
    let body = format!(
        "{V1} {V2} They carry their own circular DNA, passed down from the mother, and divide on their own whenever the cell needs more energy.\n\nFor example, a muscle cell that works harder grows fewer mitochondria over time.\n\nTry it yourself: find an electron micrograph of a muscle cell, count the mitochondria you can see in one part of it, then count them in a micrograph of a skin cell taken at the same scale, and write down which cell holds more of them and why you think it does."
    );
    let candidate = json!({"blocks":[{"kind":"explanation","title":"Example","body":body,"rubric":[],"sourceIndex":0,"quote":EXCERPT}],"questions":[]});
    let defect = || {
        json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![json!({"index":0,"passageId":"section-0-passage-1","finding":"This passage promises that the tutor will certify the learner's mitochondria count as expert reviewed.","hasDefect":true,"concerns":["promise"]})])}).to_string()
    };
    let fix = json!({"edits":[{"path":"/blocks/0/body","before":"grows fewer mitochondria","after":"grows more mitochondria"}]}).to_string();
    let sentence = |quote: &str| Protected {
        unit: 0,
        field: "body".into(),
        quote: quote.into(),
        released: false,
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
    let repaired = model(vec![
        defect(),
        actionable(),
        fix,
        judged(None, &[verdict("reviewer/1", "resolved")]),
    ]);
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
    // A rewrite of the whole lesson changes the sentences its claims stand on
    // and is always refused: four of them, fourteen minutes each on the 27B,
    // in one job of 2026-11-03. What is wrong with the form of what stands
    // waits for the user.
    let sentence = "The function returns the number 4 for the sum of two and two.";
    let mut candidate = lesson(EXCERPT);
    candidate["blocks"][0]["kind"] = json!("guided_practice");
    candidate["blocks"][0]["body"] = json!(sentence);
    let protected = [crate::lessons::content_verification::Protected {
        unit: 0,
        field: "body".into(),
        quote: sentence.into(),
        released: false,
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
        .map(|waiting| waiting.finding.place().id())
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
    let citation = ReviewFinding::rule(
        Rule::Quotation,
        FindingKind::Quotation,
        FindingSite::Section(1),
        note,
    );
    let shape = ReviewFinding::rule(
        Rule::SectionMix,
        FindingKind::Structure,
        FindingSite::Lesson,
        note,
    );
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
    let stopped = Passes::new(Held {
        places: BTreeSet::from([shape.place()]),
        ..Held::default()
    });
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
    brief["issues"] = json!([{"note":"Promises certification.","concern":"promise"}]);
    let model = model(vec![
        brief.to_string(),
        actionable(),
        retitle("Corrected example"),
        judged(None, &[verdict("reviewer/1", "resolved")]),
    ]);
    let reviewed = run(&model, lesson(EXCERPT)).await?;
    assert!(reviewed.set_aside.is_empty(), "{:?}", reviewed.set_aside);
    let candidate: Value = serde_json::from_str(&reviewed.candidate)?;
    assert_eq!(candidate["blocks"][0]["title"], "Corrected example");
    assert_eq!(model.prompts.lock().unwrap().len(), 4);
    let asked = repair_requests(&model);
    assert_eq!(asked.len(), 1);
    let entries = asked[0]["issuesToFix"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["note"], "Promises certification.");
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
        judged(None, &[verdict("reviewer/1", "resolved")]),
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

#[tokio::test]
async fn an_outline_review_sends_every_issue_it_reports_to_the_repair() -> Result<()> {
    let issues: Vec<String> = (1..=7)
        .map(|n| format!("Defect number {n} of the outline."))
        .collect();
    let model = model(vec![
        json!({"issues": issues}).to_string(),
        json!({"modules":[{"title":"Revised"}]}).to_string(),
        json!({"issues":[],"openFindings":[verdict("reviewer/1", "resolved"), verdict("reviewer/2", "resolved"), verdict("reviewer/3", "resolved"), verdict("reviewer/4", "resolved"), verdict("reviewer/5", "resolved"), verdict("reviewer/6", "resolved"), verdict("reviewer/7", "resolved")]}).to_string(),
    ]);
    let progress = crate::outline_progress::OutlineProgress::default();
    let sources = references();
    review_and_repair_with_progress(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        &json!({"type":"object","properties":{"modules":{"type":"array"}}}),
        json!({"modules":[]}).to_string(),
        4000,
        OutlineReviewContext {
            progress: &progress,
            sources: &sources,
        },
    )
    .await?;
    let prompts = model.prompts.lock().unwrap();
    assert!(!prompts[0].contains("at most six"), "{}", prompts[0]);
    assert!(prompts[0].contains("every material defect"));
    let sent = prompts
        .iter()
        .find(|prompt| prompt.contains("issuesToFix"))
        .unwrap();
    let request: Value = fixture_task(sent).expect("a JSON task");
    let notes: Vec<&str> = request["issuesToFix"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["note"].as_str().unwrap())
        .collect();
    assert_eq!(notes, issues, "{sent}");
    Ok(())
}

#[tokio::test]
async fn the_lesson_review_asks_for_every_concern_it_owns_with_no_count_cap() -> Result<()> {
    let model = model(vec![review(None)]);
    run(&model, lesson(EXCERPT)).await?;
    let prompts = model.prompts.lock().unwrap();
    assert!(!prompts[0].contains("at most six"), "{}", prompts[0]);
    assert!(!prompts[0].contains("every material defect"));
    assert!(prompts[0].contains("name each such concern in concerns"));
    assert!(prompts[0].contains("so one repair can fix them all"));
    Ok(())
}

/// The scripted review, with each teaching repair request answered by
/// `answer` from the ids of the findings it carries; the requests are kept.
struct Answering {
    review: ScriptedModel,
    answer: fn(&Value) -> Result<String>,
    requests: Mutex<Vec<Value>>,
}

#[async_trait::async_trait]
impl LLMPort for Answering {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if asks_for_repair(request) {
            let sent = crate::content_verification::tests::fixture_prompts(request)
                .iter()
                .filter_map(|prompt| fixture_task(prompt))
                .find(|sent| sent.get("issuesToFix").is_some())
                .unwrap();
            self.requests.lock().unwrap().push(sent.clone());
            let reply = (self.answer)(&sent)?;
            return Ok(crate::content_verification::tests::fixture_completion(
                request,
                vec![reply],
            ));
        }
        self.review.complete(request).await
    }
    fn model_name(&self) -> &str {
        self.review.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.review.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.review.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

impl Answering {
    fn new(review: Vec<String>, answer: fn(&Value) -> Result<String>) -> Self {
        Self {
            review: model(review),
            answer,
            requests: Mutex::new(Vec::new()),
        }
    }

    /// The ids each repair was sent, in order; a request asked again for a
    /// refused answer counts once.
    fn asked(&self) -> Vec<Vec<String>> {
        let mut asked: Vec<Vec<String>> = Vec::new();
        for request in self.requests.lock().unwrap().iter() {
            let ids = ids_of(request);
            if asked.last() != Some(&ids) {
                asked.push(ids);
            }
        }
        asked
    }
}

fn ids_of(request: &Value) -> Vec<String> {
    request["issuesToFix"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["id"].as_str().unwrap().to_owned())
        .collect()
}

/// The id of the finding raised for part `index` when every part is
/// reported in order.
fn section_id(index: usize) -> String {
    format!("reviewer/{}", index + 1)
}

fn unusable_edits() -> String {
    json!({"edits":[{"path":"/blocks/0/title","before":"Missing text","after":"X"}]}).to_string()
}

/// A better idea for each part a repair request names, as its finding's edit.
fn better_ideas(sent: &Value) -> String {
    edits_answer(
        sent["issuesToFix"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|entry| {
                let index = entry["part"]
                    .as_str()?
                    .strip_prefix("/blocks/")?
                    .parse()
                    .ok()?;
                Some(better_idea(index, entry["id"].as_str()?))
            })
            .collect(),
    )
}

#[tokio::test]
async fn an_unusable_answer_for_several_findings_is_asked_again_in_halves() -> Result<()> {
    let model = Answering::new(
        vec![
            parts_review(4, &[0, 1, 2, 3]),
            parts_evidence(&[0, 1, 2, 3]),
            parts_recheck(
                &[0, 1, 2, 3],
                &["reviewer/1", "reviewer/2", "reviewer/3", "reviewer/4"],
            ),
        ],
        |sent| {
            if ids_of(sent).len() > 2 {
                Ok(unusable_edits())
            } else {
                Ok(better_ideas(sent))
            }
        },
    );
    let reviewed = run(&model, parts_lesson(4)).await?;
    assert!(reviewed.set_aside.is_empty(), "{:?}", reviewed.set_aside);
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    for index in 0..4 {
        assert!(
            after["blocks"][index]["body"]
                .as_str()
                .unwrap()
                .contains("better idea"),
            "part {index}"
        );
    }
    let all: Vec<String> = (0..4).map(section_id).collect();
    assert_eq!(
        model.asked(),
        [
            all,
            vec![section_id(0), section_id(1)],
            vec![section_id(2), section_id(3)]
        ]
    );
    Ok(())
}

#[tokio::test]
async fn one_finding_s_unusable_answer_does_not_stop_the_others() -> Result<()> {
    let model = Answering::new(
        vec![
            parts_review(2, &[0, 1]),
            parts_evidence(&[0, 1]),
            parts_checked(
                &[1],
                &[],
                &[
                    present("reviewer/1", &fact_of(0)),
                    verdict("reviewer/2", "resolved"),
                ],
            ),
        ],
        |sent| {
            if ids_of(sent).contains(&section_id(0)) {
                Ok(unusable_edits())
            } else {
                Ok(better_ideas(sent))
            }
        },
    );
    let reviewed = run(&model, parts_lesson(2)).await?;
    assert_eq!(reviewed.set_aside.len(), 1);
    assert_eq!(reviewed.set_aside[0].finding.id().as_str(), "reviewer/1");
    assert_eq!(reviewed.set_aside[0].repairs, 2);
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    assert!(after["blocks"][1]["body"]
        .as_str()
        .unwrap()
        .contains("better idea"));
    let last = model.requests.lock().unwrap().last().cloned().unwrap();
    assert_eq!(ids_of(&last), [section_id(0)]);
    assert!(last["issuesToFix"][0]["lastAttempt"]
        .as_str()
        .is_some_and(|attempt| attempt.starts_with("The last repair's answer could not be used")));
    Ok(())
}

#[tokio::test]
async fn findings_whose_repair_does_not_fit_wait_and_the_others_are_still_repaired() -> Result<()> {
    let model = Answering::new(
        vec![
            parts_review(2, &[0, 1]),
            parts_evidence(&[0, 1]),
            parts_checked(
                &[0],
                &[],
                &[
                    verdict("reviewer/1", "resolved"),
                    present("reviewer/2", &fact_of(1)),
                ],
            ),
        ],
        |sent| {
            if ids_of(sent).contains(&section_id(1)) {
                Err(AppError::PromptExceedsWindow {
                    prompt_tokens: 251_588,
                    window: 131_072,
                    counted_exactly: true,
                    message: "over the window".into(),
                })
            } else {
                Ok(better_ideas(sent))
            }
        },
    );
    let reviewed = run(&model, parts_lesson(2)).await?;
    assert_eq!(reviewed.set_aside.len(), 1);
    assert_eq!(reviewed.set_aside[0].finding.id().as_str(), "reviewer/2");
    assert_eq!(reviewed.set_aside[0].repairs, 0);
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    assert!(after["blocks"][0]["body"]
        .as_str()
        .unwrap()
        .contains("better idea"));
    Ok(())
}

/// `run` under a schema of the test's own.
async fn run_with_schema(
    model: &dyn LLMPort,
    candidate: Value,
    schema: &Value,
) -> Result<Reviewed> {
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    review_material(
        model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        ReviewSchema::Other(schema),
        candidate.to_string(),
        4000,
        &mut LessonSubject::new(&references, &sources, &[], Held::default()),
    )
    .await
}

#[tokio::test]
async fn a_rewrite_that_cannot_be_used_is_asked_again_whole_not_in_halves() -> Result<()> {
    let schema = json!({
        "type": "object",
        "required": ["blocks", "questions", "title"],
        "properties": {"questions": {"type": "array", "minItems": 1}},
    });
    let model = Answering::new(vec![parts_review(2, &[])], |_| {
        Err(AppError::ModelAnswerUnusable("it was empty.".into()))
    });
    let reviewed = run_with_schema(&model, parts_lesson(2), &schema).await?;
    let requests = model.requests.lock().unwrap();
    assert!(
        requests.iter().all(|request| ids_of(request).len() == 2),
        "{:?}",
        requests.iter().map(ids_of).collect::<Vec<_>>()
    );
    assert_eq!(requests.len(), 2);
    assert_eq!(ids_of(&requests[0]), ids_of(&requests[1]));
    for entry in requests[1]["issuesToFix"].as_array().unwrap() {
        assert!(entry["lastAttempt"].as_str().is_some_and(
            |attempt| attempt.starts_with("The last repair's answer could not be used")
        ));
    }
    assert_eq!(reviewed.set_aside.len(), 2);
    assert!(reviewed
        .set_aside
        .iter()
        .all(|waiting| waiting.repairs == 2));
    Ok(())
}

#[tokio::test]
async fn a_lesson_whose_structure_no_repair_fixes_still_has_its_content_reviewed() -> Result<()> {
    let schema = json!({"type": "object", "required": ["blocks", "questions", "title"]});
    let model = Answering::new(
        vec![
            parts_review(2, &[0]),
            parts_evidence(&[0]),
            parts_recheck(&[0], &["reviewer/1"]),
        ],
        |sent| {
            if ids_of(sent).contains(&section_id(0)) {
                Ok(better_ideas(sent))
            } else {
                Ok(parts_lesson(2).to_string())
            }
        },
    );
    let reviewed = run_with_schema(&model, parts_lesson(2), &schema).await?;
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    assert!(after["blocks"][0]["body"]
        .as_str()
        .unwrap()
        .contains("better idea"));
    assert_eq!(reviewed.set_aside.len(), 1);
    assert_ne!(reviewed.set_aside[0].finding.id().as_str(), "reviewer/1");
    assert_eq!(reviewed.set_aside[0].repairs, 2);
    let reviews = model
        .review
        .prompts
        .lock()
        .unwrap()
        .iter()
        .filter(|prompt| prompt.starts_with("Review instructional quality."))
        .count();
    assert_eq!(reviews, 2);
    Ok(())
}

/// A repair answer whose first edit leaves the lesson unreadable as a lesson,
/// and whose later ones retitle the section.
fn breaks_then_retitles(sent: &Value, path: &str, before: Value, after: Value) -> Result<String> {
    let finding = ids_of(sent).remove(0);
    let first = sent["issuesToFix"][0]["lastAttempt"].is_null();
    let edit = if first {
        json!({"finding":finding,"path":path,"before":before,"after":after})
    } else {
        json!({"finding":finding,"path":"/blocks/0/title","before":"Example","after":"Corrected example"})
    };
    Ok(json!({"edits":[edit]}).to_string())
}

fn lesson_with_criterion() -> Value {
    let mut lesson = lesson(EXCERPT);
    lesson["blocks"][0]["rubric"] =
        json!([{"title":"One","description":"Observable.","dimension":"recall"}]);
    lesson
}

async fn an_edit_that_unmakes_the_lesson_is_charged_and_the_repair_asked_again(
    candidate: Value,
    answer: fn(&Value) -> Result<String>,
) -> Result<()> {
    let model = Answering::new(
        vec![
            review(Some(PROMISED)),
            actionable(),
            judged(None, &[verdict("reviewer/1", "resolved")]),
        ],
        answer,
    );
    let reviewed = run(&model, candidate.clone()).await?;
    let requests = model.requests.lock().unwrap();
    assert_eq!(requests.len(), 2, "the first answer was charged unusable");
    assert!(requests[1]["issuesToFix"][0]["lastAttempt"]
        .as_str()
        .is_some_and(|attempt| attempt.starts_with("The last repair's answer could not be used")));
    assert!(reviewed.set_aside.is_empty());
    let mut kept: Value = serde_json::from_str(&reviewed.candidate)?;
    assert_eq!(kept["blocks"][0]["title"], "Corrected example");
    kept["blocks"][0]["title"] = json!("Example");
    assert_eq!(kept, candidate);
    Ok(())
}

#[tokio::test]
async fn an_edit_that_makes_a_number_negative_is_an_unusable_repair() -> Result<()> {
    an_edit_that_unmakes_the_lesson_is_charged_and_the_repair_asked_again(lesson(EXCERPT), |sent| {
        breaks_then_retitles(sent, "/blocks/0/sourceIndex", json!(0), json!(-1))
    })
    .await
}

#[tokio::test]
async fn an_edit_that_puts_a_criterion_outside_its_dimensions_is_an_unusable_repair() -> Result<()>
{
    an_edit_that_unmakes_the_lesson_is_charged_and_the_repair_asked_again(
        lesson_with_criterion(),
        |sent| {
            breaks_then_retitles(
                sent,
                "/blocks/0/rubric/0/dimension",
                json!("recall"),
                json!("mastery"),
            )
        },
    )
    .await
}

async fn a_rewrite_that_is_no_lesson_is_charged_and_leaves_the_lesson_as_it_was(
    answer: fn(&Value) -> Result<String>,
) -> Result<()> {
    let schema = json!({"type": "object", "required": ["blocks", "questions", "title"]});
    let model = Answering::new(
        std::iter::repeat_with(|| parts_review(2, &[]))
            .take(6)
            .collect(),
        answer,
    );
    let reviewed = run_with_schema(&model, parts_lesson(2), &schema).await?;
    assert_eq!(
        serde_json::from_str::<Value>(&reviewed.candidate).ok(),
        Some(parts_lesson(2))
    );
    let requests = model.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[1]["issuesToFix"][0]["lastAttempt"]
        .as_str()
        .is_some_and(|attempt| attempt.starts_with("The last repair's answer could not be used")));
    assert_eq!(reviewed.set_aside.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_rewrite_that_is_not_json_is_an_unusable_repair() -> Result<()> {
    a_rewrite_that_is_no_lesson_is_charged_and_leaves_the_lesson_as_it_was(|_| {
        Ok("Here is the lesson, rewritten.".to_owned())
    })
    .await
}

#[tokio::test]
async fn a_rewrite_without_a_required_field_is_an_unusable_repair() -> Result<()> {
    a_rewrite_that_is_no_lesson_is_charged_and_leaves_the_lesson_as_it_was(|_| {
        Ok(json!({"blocks": []}).to_string())
    })
    .await
}

#[tokio::test]
async fn a_rewrite_with_no_sections_is_an_unusable_repair() -> Result<()> {
    a_rewrite_that_is_no_lesson_is_charged_and_leaves_the_lesson_as_it_was(|_| {
        Ok(json!({"blocks": [], "questions": []}).to_string())
    })
    .await
}

#[tokio::test]
async fn the_same_unusable_repair_twice_leaves_the_finding_waiting_and_the_lesson_whole(
) -> Result<()> {
    let model = Answering::new(vec![review(Some(PROMISED)), actionable()], |sent| {
        let finding = ids_of(sent).remove(0);
        Ok(json!({"edits":[{"finding":finding,"path":"/blocks/0/sourceIndex","before":0,"after":-1}]}).to_string())
    });
    let original = lesson(EXCERPT);
    let reviewed = run(&model, original.clone()).await?;
    assert_eq!(reviewed.set_aside.len(), 1);
    assert_eq!(reviewed.set_aside[0].repairs, 2);
    let kept: Value = serde_json::from_str(&reviewed.candidate)?;
    assert_eq!(kept, original);
    assert!(kept["blocks"][0]["sourceIndex"].as_u64().is_some());
    Ok(())
}

pub(super) const INPUT: &str = "teaching-review-resume";

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

/// The defect of `a_defect_no_repair_fixes_waits_after_its_second_repair`: the
/// calls of two repairs, each rechecked, and the pass that stops at the second.
/// The reviewer's verdict on a finding is not checked against the evidence
/// again.
fn defect_no_repair_fixes() -> Vec<String> {
    vec![
        review(Some(PROMISED)),
        actionable(),
        retitle("Corrected example"),
        judged(Some(STILL_PROMISED), &[present("reviewer/1", NUMBER_FOUR)]),
        retitle_again("Corrected twice"),
        judged(
            Some(STILL_PROMISED_TWICE),
            &[present("reviewer/1", NUMBER_FOUR)],
        ),
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
    let db = Database::new(crate::tests::pool().await?);
    let (repo, job, lesson_id) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&db).await?;
    let llm = DropsAfterRepair {
        model: model(defect_no_repair_fixes()),
        repaired: AtomicBool::new(false),
        dropped: AtomicBool::new(false),
    };
    let first = lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        lesson_drafts::resume(INPUT.into()).await?;
        run(&llm, lesson(EXCERPT)).await
    })
    .await;
    assert!(matches!(first, Err(AppError::Network(_))));
    assert_eq!(repairs_asked(&llm), 1, "cut short after the first repair");
    let second = lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
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

/// Run through, the review above repairs the defect twice and stops. Cut short
/// after its first repair and run again, it makes the second and stops the
/// same way instead of repairing from zero.
#[tokio::test]
async fn a_review_cut_short_after_its_first_repair_resumes_with_the_repairs_it_had_made(
) -> Result<()> {
    let (repairs, reviewed) = cut_short_then_resumed(|_| {}).await?;
    assert_eq!(repairs, 2);
    assert_eq!(reviewed.set_aside.len(), 1);
    let waiting = &reviewed.set_aside[0];
    assert_eq!(waiting.finding.id().as_str(), "reviewer/1");
    assert_eq!(waiting.repairs, 2);
    Ok(())
}

/// The repairs a finding has had belong to the finding, not to the draft they
/// were made to: a claim repair that changed the draft leaves them counted.
#[tokio::test]
async fn a_review_cut_short_resumes_with_its_repairs_on_a_draft_changed_since() -> Result<()> {
    let (repairs, reviewed) = cut_short_then_resumed(claim_repair).await?;
    assert_eq!(repairs, 2);
    assert_eq!(reviewed.set_aside.len(), 1);
    let waiting = &reviewed.set_aside[0];
    assert_eq!(waiting.finding.id().as_str(), "reviewer/1");
    assert_eq!(waiting.repairs, 2);
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
            released: false,
        },
        Protected {
            unit: 0,
            field: "title".into(),
            quote: "Example".into(),
            released: false,
        },
    ];
    let lesson_finding = ReviewFinding::rule(
        Rule::SectionMix,
        FindingKind::Structure,
        FindingSite::Lesson,
        "no contrast block",
    );
    let section_finding = ReviewFinding::reviewer(
        1,
        Some(FindingSite::Section(0)),
        "expand on the explanation",
    );
    let findings = vec![lesson_finding.clone(), section_finding.clone()];
    let held = protected::on_protected(&lesson, &findings, &protected);
    let expected = vec![lesson_finding.id(), section_finding.id()];
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
        released: false,
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
    let finding =
        ReviewFinding::reviewer(1, Some(FindingSite::Question(0)), "the option is ambiguous")
            .with_passage("Four is returned.");
    assert!(protected::on_protected(&lesson, &[finding], &protected).is_empty());
}

#[test]
fn a_quotation_finding_on_the_source_quote_is_not_held_by_a_protected_body() {
    let mut lesson = lesson(EXCERPT);
    let sentence = "The function returns the number 4 for the sum of two and two.";
    lesson["blocks"][0]["body"] = json!(sentence);
    let protected = vec![protect(0, "body", sentence)];
    let finding = ReviewFinding::rule(
        Rule::Quotation,
        FindingKind::Quotation,
        FindingSite::Section(0),
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
    let finding = ReviewFinding::reviewer(
        1,
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
    let finding = ReviewFinding::reviewer(
        1,
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
    let finding = ReviewFinding::reviewer(
        1,
        Some(FindingSite::Section(0)),
        "improve the second sentence",
    );
    let touched = std::collections::BTreeMap::from([("/blocks/0".to_owned(), vec![&finding])]);
    let reasons = protected::confine_review(&previous, &repaired, &touched, &protected)?
        .expect_err("the kind and the quote were changed, which no finding may do");
    assert!(!reasons.is_empty());
    assert!(reasons
        .iter()
        .all(|reason| reason.starts_with("it changes \"")));
    Ok(())
}

/// The repair requests a scripted model was sent, as the JSON each carries.
fn repair_requests(model: &ScriptedModel) -> Vec<Value> {
    model
        .prompts
        .lock()
        .unwrap()
        .iter()
        .filter_map(|prompt| fixture_task(prompt))
        .filter(|request| request.get("issuesToFix").is_some())
        .collect()
}

const PART_NAMES: [&str; 4] = ["Part A", "Part B", "Part C", "Part D"];

fn fact_of(index: usize) -> String {
    format!(
        "{} states one plain fact about the tool.",
        PART_NAMES[index]
    )
}

fn part_body(index: usize) -> String {
    let name = PART_NAMES[index];
    format!("{name} states one plain fact about the tool. {name} then gives a first idea.")
}

pub(super) fn parts_lesson(count: usize) -> Value {
    let blocks: Vec<Value> = (0..count)
        .map(|index| {
            json!({
                "kind": "explanation",
                "title": PART_NAMES[index],
                "body": part_body(index),
                "rubric": [],
                "sourceIndex": 0,
                "quote": EXCERPT,
            })
        })
        .collect();
    json!({"blocks": blocks, "questions": []})
}

/// An edit of the part's body, for the finding it names.
fn better_idea(index: usize, finding: &str) -> Value {
    let name = PART_NAMES[index];
    json!({
        "finding": finding,
        "path": format!("/blocks/{index}/body"),
        "before": format!("{name} then gives a first idea."),
        "after": format!("{name} then gives a better idea."),
    })
}

fn parts_review(count: usize, defective: &[usize]) -> String {
    let at_first: Vec<(usize, usize)> = defective.iter().map(|part| (*part, 0)).collect();
    parts_review_at(count, &at_first)
}

/// A review whose defective parts are `(part, paragraph)`: the paragraph
/// each one's finding is about.
fn parts_review_at(count: usize, defective: &[(usize, usize)]) -> String {
    let all: Vec<usize> = (0..count).collect();
    parts_checked(&all, defective, &[])
}

/// A review of the parts `asked` alone, whose defective parts are
/// `(part, paragraph)`, with `verdicts` on the findings it was asked about.
fn parts_checked(asked: &[usize], defective: &[(usize, usize)], verdicts: &[Value]) -> String {
    let checks: Vec<Value> = asked
        .iter()
        .map(|index| {
            let found = defective.iter().find(|(part, _)| part == index);
            json!({
                "index": index,
                "passageId": format!("section-{index}-passage-{}", found.map_or(0, |(_, paragraph)| *paragraph)),
                "finding": if found.is_some() {
                    FIRST_IDEA_PROMISE
                } else {
                    "Nothing is wrong here."
                },
                "hasDefect": found.is_some(),
                "concerns": if found.is_some() { vec!["promise"] } else { vec![] },
            })
        })
        .collect();
    review_reply(&[], checks, verdicts)
}

/// A review of the parts `asked` alone that finds no defect and resolves the
/// findings `resolved`.
fn parts_recheck(asked: &[usize], resolved: &[&str]) -> String {
    let verdicts: Vec<Value> = resolved.iter().map(|id| verdict(id, "resolved")).collect();
    parts_checked(asked, &[], &verdicts)
}

fn parts_evidence(defective: &[usize]) -> String {
    let decisions: Vec<Value> = defective
        .iter()
        .enumerate()
        .map(|(position, index)| {
            json!({
                "issueId": format!("issue-{position}"),
                "verdict": "actionable",
                "basis": "internal_conflict",
                "evidenceIds": [format!("blocks-{index}")],
                "partId": format!("blocks-{index}"),
                "requirementId": null,
                "reason": FIRST_IDEA_PROMISE
            })
        })
        .collect();
    json!({"decisions": decisions}).to_string()
}

fn edits_answer(edits: Vec<Value>) -> String {
    json!({"edits": edits}).to_string()
}

#[tokio::test]
async fn the_repair_is_told_each_finding_s_id_part_and_passage() -> Result<()> {
    let model = model(vec![
        review(Some("The section promises expert certification.")),
        actionable(),
        retitle("Corrected example"),
        judged(None, &[verdict("reviewer/1", "resolved")]),
    ]);
    let candidate = lesson(EXCERPT);
    run(&model, candidate.clone()).await?;
    let asked = repair_requests(&model);
    assert_eq!(asked.len(), 1);
    let entry = &asked[0]["issuesToFix"][0];
    assert!(entry["id"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(entry["part"], "/blocks/0");
    assert!(entry["also"].is_null());
    let body = candidate["blocks"][0]["body"].as_str().unwrap();
    assert!(entry["passage"]
        .as_str()
        .is_some_and(|passage| body.contains(passage)));
    assert!(entry["note"]
        .as_str()
        .unwrap()
        .contains("promises expert certification"));
    assert!(entry["lastAttempt"].is_null());
    Ok(())
}

#[tokio::test]
async fn the_repair_is_asked_for_one_answer_for_each_finding_sent() -> Result<()> {
    let scripted = model(vec![
        review(Some("The section promises expert certification.")),
        actionable(),
        retitle("Corrected example"),
        judged(None, &[verdict("reviewer/1", "resolved")]),
    ]);
    let recorder = RecordsSchemas {
        inner: &scripted,
        schemas: Mutex::new(Vec::new()),
    };
    run(&recorder, lesson(EXCERPT)).await?;
    let schemas = recorder.schemas.lock().unwrap();
    let schema = schemas
        .iter()
        .find(|schema| schema.pointer("/properties/answers").is_some())
        .unwrap();
    assert!(schema["properties"].get("edits").is_none());
    let asked = repair_requests(&scripted);
    let ids: Vec<Value> = asked[0]["issuesToFix"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["id"].clone())
        .collect();
    let answers = &schema["properties"]["answers"];
    assert_eq!(answers["minItems"], json!(ids.len()));
    assert_eq!(answers["maxItems"], json!(ids.len()));
    let entry = &answers["items"];
    assert_eq!(entry["properties"]["finding"]["enum"], json!(ids));
    assert!(entry["required"]
        .as_array()
        .unwrap()
        .contains(&json!("plan")));
    Ok(())
}

/// A repair's answer as the schema asks for it: an entry with its plan and
/// edits for each finding.
fn answers_for(entries: Vec<(&str, &str, Vec<Value>)>) -> String {
    let entries: Vec<Value> = entries
        .into_iter()
        .map(|(finding, plan, edits)| json!({"finding":finding,"plan":plan,"edits":edits}))
        .collect();
    json!({ "answers": entries }).to_string()
}

/// `better_idea` as an entry's edit, which names no finding of its own.
fn better_idea_edit(index: usize) -> Value {
    let mut edit = better_idea(index, "");
    edit.as_object_mut().unwrap().remove("finding");
    edit
}

#[tokio::test]
async fn an_answer_that_leaves_a_finding_out_is_asked_again_and_the_second_is_applied() -> Result<()>
{
    let model = model(vec![
        parts_review(2, &[0, 1]),
        parts_evidence(&[0, 1]),
        answers_for(vec![(
            "reviewer/1",
            "Give Part A a better idea.",
            vec![better_idea_edit(0)],
        )]),
        answers_for(vec![
            (
                "reviewer/1",
                "Give Part A a better idea.",
                vec![better_idea_edit(0)],
            ),
            (
                "reviewer/2",
                "Give Part B a better idea.",
                vec![better_idea_edit(1)],
            ),
        ]),
        parts_recheck(&[0, 1], &["reviewer/1", "reviewer/2"]),
    ]);
    let reviewed = run(&model, parts_lesson(2)).await?;
    assert_eq!(model.prompts.lock().unwrap().len(), 5);
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    for index in 0..2 {
        assert!(
            after["blocks"][index]["body"]
                .as_str()
                .unwrap()
                .contains("better idea"),
            "part {index}"
        );
    }
    assert!(reviewed.set_aside.is_empty(), "{:?}", reviewed.set_aside);
    Ok(())
}

#[tokio::test]
async fn an_entry_s_edit_outside_its_finding_s_part_is_refused_and_the_other_entry_applies(
) -> Result<()> {
    let model = model(vec![
        parts_review(3, &[0, 1]),
        parts_evidence(&[0, 1]),
        answers_for(vec![
            (
                "reviewer/1",
                "Give Part A a better idea.",
                vec![better_idea_edit(0)],
            ),
            (
                "reviewer/2",
                "Give Part B a better idea.",
                vec![better_idea_edit(2)],
            ),
        ]),
        parts_checked(
            &[0],
            &[],
            &[
                verdict("reviewer/1", "resolved"),
                present("reviewer/2", &fact_of(1)),
            ],
        ),
        answers_for(vec![(
            "reviewer/2",
            "Give Part B a better idea.",
            vec![better_idea_edit(1)],
        )]),
        parts_recheck(&[1], &["reviewer/2"]),
    ]);
    let reviewed = run(&model, parts_lesson(3)).await?;
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    assert_eq!(after["blocks"][2]["body"], part_body(2));
    for index in 0..2 {
        assert!(
            after["blocks"][index]["body"]
                .as_str()
                .unwrap()
                .contains("better idea"),
            "part {index}"
        );
    }
    let asked = repair_requests(&model);
    assert_eq!(asked.len(), 2);
    assert!(asked[1]["issuesToFix"][0]["lastAttempt"]
        .as_str()
        .is_some_and(|attempt| attempt.contains("could not be applied")));
    assert!(reviewed.set_aside.is_empty());
    Ok(())
}

#[tokio::test]
async fn an_entry_with_a_plan_and_no_change_is_told_its_plan_on_the_next_attempt() -> Result<()> {
    const PLAN: &str = "Part B already says it plainly, so leave it.";
    let model = model(vec![
        parts_review(2, &[0, 1]),
        parts_evidence(&[0, 1]),
        answers_for(vec![
            (
                "reviewer/1",
                "Give Part A a better idea.",
                vec![better_idea_edit(0)],
            ),
            ("reviewer/2", PLAN, Vec::new()),
        ]),
        parts_checked(
            &[0],
            &[],
            &[
                verdict("reviewer/1", "resolved"),
                present("reviewer/2", &fact_of(1)),
            ],
        ),
        answers_for(vec![(
            "reviewer/2",
            "Give Part B a better idea.",
            vec![better_idea_edit(1)],
        )]),
        parts_recheck(&[1], &["reviewer/2"]),
    ]);
    let reviewed = run(&model, parts_lesson(2)).await?;
    let asked = repair_requests(&model);
    assert_eq!(asked.len(), 2);
    assert_eq!(
        asked[1]["issuesToFix"][0]["lastAttempt"],
        format!("The last repair gave no change for this finding; its plan was: \"{PLAN}\"")
    );
    assert!(reviewed.set_aside.is_empty());
    Ok(())
}

#[tokio::test]
async fn an_edit_outside_its_finding_s_part_leaves_the_other_findings_edits_applied() -> Result<()>
{
    let model = model(vec![
        parts_review(3, &[0, 1]),
        parts_evidence(&[0, 1]),
        edits_answer(vec![
            better_idea(0, "reviewer/1"),
            better_idea(2, "reviewer/2"),
        ]),
        parts_checked(
            &[0],
            &[],
            &[
                verdict("reviewer/1", "resolved"),
                present("reviewer/2", &fact_of(1)),
            ],
        ),
        edits_answer(vec![better_idea(1, "reviewer/2")]),
        parts_recheck(&[1], &["reviewer/2"]),
    ]);
    let reviewed = run(&model, parts_lesson(3)).await?;
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    assert_eq!(after["blocks"][2]["body"], part_body(2));
    assert!(after["blocks"][0]["body"]
        .as_str()
        .unwrap()
        .contains("better idea"));
    let asked = repair_requests(&model);
    assert_eq!(asked.len(), 2);
    assert!(asked[1]["issuesToFix"][0]["lastAttempt"]
        .as_str()
        .is_some_and(|attempt| attempt.contains("could not be applied")));
    assert!(reviewed.set_aside.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_finding_the_repair_gave_no_change_for_is_told_its_plan_on_its_second_attempt(
) -> Result<()> {
    let model = model(vec![
        parts_review(2, &[0, 1]),
        parts_evidence(&[0, 1]),
        edits_answer(vec![better_idea(0, "reviewer/1")]),
        parts_checked(
            &[0],
            &[],
            &[
                verdict("reviewer/1", "resolved"),
                present("reviewer/2", &fact_of(1)),
            ],
        ),
        edits_answer(vec![better_idea(1, "reviewer/2")]),
        parts_recheck(&[1], &["reviewer/2"]),
    ]);
    run(&model, parts_lesson(2)).await?;
    let asked = repair_requests(&model);
    assert_eq!(asked.len(), 2);
    assert_eq!(
        asked[1]["issuesToFix"][0]["lastAttempt"],
        format!(
            "The last repair gave no change for this finding; its plan was: \"{}\"",
            crate::content_verification::tests::FIXTURE_PLAN
        )
    );
    Ok(())
}

#[tokio::test]
async fn a_reverted_edit_gets_a_second_attempt() -> Result<()> {
    use crate::lessons::content_verification::Protected;
    const KEPT: &str = "Part A states one plain fact about the tool.";
    let block0_body = format!("{KEPT}\n\nPart A then gives a first idea.");
    let block1_body = part_body(1);
    let candidate = json!({
        "blocks": [
            {
                "kind": "explanation",
                "title": "Part A",
                "body": block0_body,
                "rubric": [],
                "sourceIndex": 0,
                "quote": EXCERPT,
            },
            {
                "kind": "explanation",
                "title": "Part B",
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
        quote: KEPT.into(),
        released: false,
    }];
    let touches_the_protected_sentence = json!({
        "finding": "reviewer/1",
        "path": "/blocks/0/body",
        "before": KEPT,
        "after": "Part A states a different fact about the tool.",
    });
    let model = model(vec![
        parts_review_at(2, &[(0, 1), (1, 0)]),
        parts_evidence(&[0, 1]),
        edits_answer(vec![
            touches_the_protected_sentence.clone(),
            better_idea(1, "reviewer/2"),
        ]),
        parts_checked(
            &[1],
            &[],
            &[
                present("reviewer/1", "Part A then gives a first idea."),
                verdict("reviewer/2", "resolved"),
            ],
        ),
        edits_answer(vec![touches_the_protected_sentence]),
    ]);
    let reviewed = review_protecting(&model, &candidate, &protected).await?;
    let asked = repair_requests(&model);
    assert_eq!(asked.len(), 2);
    assert!(asked[1]["issuesToFix"][0]["lastAttempt"]
        .as_str()
        .is_some_and(|attempt| attempt.contains("must stay as it is")));
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    assert_eq!(after["blocks"][0]["body"], block0_body);
    assert!(after["blocks"][1]["body"]
        .as_str()
        .unwrap()
        .contains("better idea"));
    assert_eq!(reviewed.set_aside.len(), 1);
    assert_eq!(
        reviewed.set_aside[0].finding.place().id(),
        "teaching:section-0"
    );
    assert_eq!(reviewed.set_aside[0].repairs, 2);
    Ok(())
}

const PROTECTED_SENTENCE: &str = "Body of First idea.";

fn shaped_block(kind: &str, title: &str) -> Value {
    let body = if kind == "check" {
        format!(
            "Case for {title}: what happens to the lid?\n\n{}\n\nAnswer for {title}: it bulges.",
            crate::generation::CHECK_REVEAL
        )
    } else {
        format!("Body of {title}.")
    };
    let rubric = if kind.ends_with("practice") {
        json!([
            {"title":"One","description":"Observable.","dimension":"recall"},
            {"title":"Two","description":"Observable.","dimension":"application"}
        ])
    } else {
        json!([])
    };
    json!({"kind":kind,"title":title,"body":body,"rubric":rubric,"sourceIndex":0,"quote":EXCERPT,"citations":[]})
}

fn shaped_question(kind: &str, prompt: &str) -> Value {
    json!({"kind":kind,"prompt":prompt,"options":["One","Two","Three","Four"],"correctIndex":0,
        "rationales":["","Two reverses the order.","Three mistakes the cause for the effect.","Four states it backwards."],
        "explanation":"Because.","sourceIndex":0,"quote":EXCERPT,"citations":[]})
}

/// A lesson in the writer's full shape, whose questions are practice,
/// practice, quiz, quiz, test, test.
fn shaped_lesson() -> Value {
    json!({"blocks":[
        shaped_block("explanation","First idea"), shaped_block("worked_example","First example"),
        shaped_block("contrast","Not to be confused with"), shaped_block("check","Your turn"),
        shaped_block("explanation","Second idea"), shaped_block("worked_example","Second example"),
        shaped_block("check","Your turn again"), shaped_block("guided_practice","Guided"),
        shaped_block("independent_practice","On your own"), shaped_block("reflection","From memory"),
        shaped_block("recap","Recap"),
    ],"questions":[
        shaped_question("practice","P1?"), shaped_question("practice","P2?"),
        shaped_question("quiz","Q1?"), shaped_question("quiz","Q2?"),
        shaped_question("test","T1?"), shaped_question("test","T2?"),
    ]})
}

/// What a verification cut leaves: the first test question is gone.
fn shaped_lesson_after_a_cut() -> Value {
    let mut lesson = shaped_lesson();
    lesson["questions"].as_array_mut().unwrap().pop();
    lesson
}

/// The blind solver's answers for the lesson's questions: each picks the first option.
fn shaped_solutions(candidate: &Value) -> String {
    let count = candidate["questions"].as_array().unwrap().len();
    json!({"answers":(0..count).map(|index| json!({"index":index,"reason":"The fixture's key.","correctIndices":[0],"answer":"","blankAnswers":[],"matches":[],"orderedValues":[]})).collect::<Vec<_>>()}).to_string()
}

/// The item judge's checks for the lesson's questions: sound, except `fault`'s question.
fn shaped_judgement(candidate: &Value, fault: Option<(usize, &str)>) -> String {
    let count = candidate["questions"].as_array().unwrap().len();
    let checks = (0..count).map(|index| {
        let finding = fault.filter(|(at, _)| *at == index).map_or("", |(_, text)| text);
        json!({"id":format!("new-{index}"),"finding":finding,"hasDefect":!finding.is_empty(),"repeats":[]})
    });
    json!({"checks":checks.collect::<Vec<_>>()}).to_string()
}

fn shaped_review(candidate: &Value) -> String {
    let count = candidate["blocks"].as_array().unwrap().len();
    let checks = (0..count)
        .map(|index| json!({"index":index,"passageId":format!("section-{index}-passage-0"),"finding":"Reviewed; no teaching defect was found.","hasDefect":false}))
        .collect();
    json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(checks)}).to_string()
}

async fn waiting_after_review(candidate: &Value, protected: bool) -> Result<Vec<String>> {
    let model = model(vec![
        shaped_solutions(candidate),
        shaped_judgement(candidate, None),
        shaped_review(candidate),
    ]);
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    let sentences = [crate::lessons::content_verification::Protected {
        unit: 0,
        field: "body".into(),
        quote: PROTECTED_SENTENCE.into(),
        released: false,
    }];
    let policy = crate::lessons::shape::policy();
    let phase = if protected {
        crate::lessons::shape::Phase::Kept
    } else {
        crate::lessons::shape::Phase::Draft
    };
    let schema = policy.schema(phase, 1);
    let reviewed = review_material(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        ReviewSchema::Lesson(LessonShape::new(&schema, policy)),
        candidate.to_string(),
        4000,
        &mut LessonSubject::new(
            &references,
            &sources,
            if protected { &sentences } else { &[] },
            Held::default(),
        ),
    )
    .await?;
    Ok(reviewed
        .set_aside
        .iter()
        .map(|waiting| waiting.finding.place().id())
        .collect())
}

#[tokio::test]
async fn a_lesson_whose_cuts_spent_a_question_waits_for_no_count_after_a_protected_review(
) -> Result<()> {
    let candidate = shaped_lesson_after_a_cut();
    assert_eq!(candidate["questions"].as_array().unwrap().len(), 5);
    let waiting = waiting_after_review(&candidate, true).await?;
    assert!(
        !waiting.iter().any(|id| id == "structure:lesson"),
        "{waiting:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_protected_review_still_holds_a_question_with_three_choices() -> Result<()> {
    let mut candidate = shaped_lesson_after_a_cut();
    candidate["questions"][0]["options"] = json!(["One", "Two", "Three"]);
    let waiting = waiting_after_review(&candidate, true).await?;
    assert!(
        waiting.iter().any(|id| id == "structure:question-0"),
        "{waiting:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_protected_review_still_holds_a_section_without_a_title() -> Result<()> {
    let mut candidate = shaped_lesson_after_a_cut();
    candidate["blocks"][1]["title"] = json!("");
    let waiting = waiting_after_review(&candidate, true).await?;
    assert!(
        waiting.iter().any(|id| id == "structure:section-1"),
        "{waiting:?}"
    );
    Ok(())
}

#[test]
fn the_draft_review_still_reports_a_question_shortfall_and_a_section_shortfall() {
    let policy = crate::lessons::shape::policy();
    let schema = policy.schema(crate::lessons::shape::Phase::Draft, 1);
    let at_lesson = |lesson: &Value| -> usize {
        structural_findings(
            ReviewSchema::Lesson(LessonShape::new(&schema, policy)),
            &lesson.to_string(),
            &Default::default(),
        )
        .unwrap()
        .iter()
        .filter(|finding| finding.place().id() == "structure:lesson")
        .count()
    };
    assert_eq!(at_lesson(&shaped_lesson()), 0);
    assert_eq!(at_lesson(&shaped_lesson_after_a_cut()), 2);
    let mut fewer_sections = shaped_lesson();
    fewer_sections["blocks"].as_array_mut().unwrap().remove(5);
    assert_eq!(at_lesson(&fewer_sections), 2);
}

#[tokio::test]
async fn a_lesson_a_cut_left_without_a_test_question_waits_for_one_after_a_protected_review(
) -> Result<()> {
    let mut candidate = shaped_lesson_after_a_cut();
    candidate["questions"]
        .as_array_mut()
        .unwrap()
        .retain(|question| question["kind"] != "test");
    assert_eq!(candidate["questions"].as_array().unwrap().len(), 4);
    let waiting = waiting_after_review(&candidate, true).await?;
    assert!(
        waiting.iter().any(|id| id == "structure:lesson"),
        "{waiting:?}"
    );
    Ok(())
}

/// Records the schema of every request it passes on.
struct RecordsSchemas<'a> {
    inner: &'a ScriptedModel,
    schemas: Mutex<Vec<Value>>,
}

#[async_trait::async_trait]
impl LLMPort for RecordsSchemas<'_> {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if let Some(schema) = &request.json_schema {
            self.schemas.lock().unwrap().push(schema.clone());
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

#[tokio::test]
async fn a_repair_of_a_lesson_the_cuts_left_five_questions_is_not_asked_for_six() {
    let mut candidate = shaped_lesson_after_a_cut();
    candidate["questions"][0]["options"] = json!(["One", "Two", "Three"]);
    let scripted = model(vec![candidate.to_string()]);
    let recorder = RecordsSchemas {
        inner: &scripted,
        schemas: Mutex::new(Vec::new()),
    };
    let sources = references();
    let references = ReferenceCollection::lexical(&sources).unwrap();
    let _ = crate::lessons::content_verification::settle_after_cuts(
        &recorder,
        &context(),
        crate::lessons::shape::policy(),
        candidate.to_string(),
        &references,
        &[(2, "Not to be confused with")],
    )
    .await;
    let schemas = recorder.schemas.lock().unwrap();
    let asked = |pointer: &str| -> Vec<u64> {
        schemas
            .iter()
            .filter_map(|schema| schema.pointer(pointer)?.as_u64())
            .collect()
    };
    let questions = asked("/properties/questions/minItems");
    assert!(!questions.is_empty(), "the lesson was not sent to a repair");
    assert!(questions.iter().all(|least| *least <= 5), "{questions:?}");
    let sections = asked("/properties/blocks/minItems");
    assert!(sections.iter().all(|least| *least < 10), "{sections:?}");
}

/// The scripted review, keeping the text of every request it is sent.
struct Recording<'a>(&'a ScriptedModel, Mutex<Vec<String>>);

#[async_trait::async_trait]
impl lattice_core::application::ports::llm_port::LLMPort for Recording<'_> {
    async fn complete(
        &self,
        request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        let text: Vec<&str> = request
            .input
            .iter()
            .filter_map(|item| match item {
                CompletionInput::Message { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        self.1
            .lock()
            .map_err(|_| AppError::InternalError("fixture lock".into()))?
            .push(text.join("\n"));
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

/// An edit that drops a sentence's emphasis markers was refused with a
/// reason the model could not copy from, so its second answer failed the
/// same way and the finding waited for the user.
#[tokio::test]
async fn a_repair_edit_that_drops_a_sentences_markers_is_asked_again_with_the_exact_sentence(
) -> Result<()> {
    const MARKED: &str = "The **function** returns the number 4 for the sum of two and two.";
    let candidate = json!({"blocks":[{"kind":"explanation","title":"Example","body":format!("{MARKED} It never returns text."),"rubric":[],"sourceIndex":0,"quote":EXCERPT}],"questions":[]});
    let edit = |before: &str| {
        json!({"edits":[{"path":"/blocks/0/body","before":before,"after":"The function returns the number 4 for the sum of two and two, as the code shows."}]}).to_string()
    };
    let scripted = model(vec![
        review(Some(PROMISED)),
        actionable(),
        edit("The function returns the number 4 for the sum of two and two."),
        edit(MARKED),
        judged(None, &[verdict("reviewer/1", "resolved")]),
    ]);
    let recording = Recording(&scripted, Mutex::new(Vec::new()));
    let reviewed = run(&recording, candidate).await?;
    let sent = recording.1.lock().unwrap().clone();
    assert!(
        sent.iter()
            .any(|request| request.contains(&format!("its words stand there as \"{MARKED}\""))),
        "the second request lacks the exact sentence"
    );
    assert!(reviewed.set_aside.is_empty(), "{:?}", reviewed.set_aside);
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    assert_eq!(
        after["blocks"][0]["body"],
        "The function returns the number 4 for the sum of two and two, as the code shows. It never returns text."
    );
    Ok(())
}

/// What a job kept of its lesson's repairs: each `teaching-repair-answer`
/// checkpoint's value.
async fn kept_repairs(pool: &sqlx::SqlitePool, job: &str, lesson_id: &str) -> Vec<Value> {
    sqlx::query_scalar::<_, String>(
        "SELECT data_json FROM job_checkpoints WHERE job_id = ?1 AND key LIKE ?2 ORDER BY key",
    )
    .bind(job)
    .bind(format!("{lesson_id}/teaching-repair-answer:%"))
    .fetch_all(pool)
    .await
    .unwrap()
    .iter()
    .map(|data| serde_json::from_str(data).unwrap())
    .collect()
}

#[tokio::test]
async fn a_repair_s_answer_is_kept_as_it_was_given() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&db).await?;
    let model = model(defect_no_repair_fixes());
    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        run(&model, lesson(EXCERPT)).await
    })
    .await?;
    let kept = kept_repairs(&pool, &job, &lesson_id).await;
    assert!(kept.iter().all(|row| row["answer"].is_string()), "{kept:?}");
    let first = json!({"answers":[{"finding":"reviewer/1","plan":crate::content_verification::tests::FIXTURE_PLAN,"edits":[{"path":"/blocks/0/title","before":"Example","after":"Corrected example"}]}]});
    let given = |row: &Value| {
        row["answer"]
            .as_str()
            .and_then(|answer| serde_json::from_str::<Value>(answer).ok())
    };
    assert_eq!(
        kept.iter()
            .filter(|row| given(row).as_ref() == Some(&first))
            .count(),
        1,
        "{kept:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_repair_answer_is_kept_with_its_reason() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&db).await?;
    let unusable =
        json!({"edits":[{"path":"/blocks/0/title","before":"Missing text","after":"X"}]})
            .to_string();
    let model = model(vec![
        review(Some(PROMISED)),
        actionable(),
        unusable.clone(),
        unusable.clone(),
        unusable.clone(),
        unusable,
    ]);
    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        run(&model, lesson(EXCERPT)).await
    })
    .await?;
    let kept = kept_repairs(&pool, &job, &lesson_id).await;
    assert_eq!(kept.len(), 2, "{kept:?}");
    for row in &kept {
        assert!(row.get("answer").is_none(), "{kept:?}");
        assert!(
            row["refused"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty()),
            "{kept:?}"
        );
    }
    Ok(())
}

/// The scripted review with its repair requests answered from a script of
/// their own, so the order of the other calls does not depend on how many
/// repairs are asked.
struct RepairsApart {
    repairs: ScriptedModel,
    rest: ScriptedModel,
}

#[async_trait::async_trait]
impl lattice_core::application::ports::llm_port::LLMPort for RepairsApart {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if asks_for_repair(request) {
            self.repairs.complete(request).await
        } else {
            self.rest.complete(request).await
        }
    }
    fn model_name(&self) -> &str {
        self.rest.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.rest.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.rest.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_repair_asked_again_in_its_job_is_read_back() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&db).await?;
    let script = defect_no_repair_fixes();
    // The reviews of the second run are read from their checkpoints; the
    // evidence check of the first finding is asked again.
    let rest = vec![
        script[0].clone(),
        script[1].clone(),
        script[3].clone(),
        script[5].clone(),
        script[1].clone(),
    ];
    let llm = RepairsApart {
        repairs: model(vec![
            retitle("Corrected example"),
            retitle_again("Corrected twice"),
        ]),
        rest: model(rest),
    };
    let first = lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        run(&llm, lesson(EXCERPT)).await
    })
    .await?;
    let second = lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        run(&llm, lesson(EXCERPT)).await
    })
    .await?;
    assert_eq!(llm.repairs.prompts.lock().unwrap().len(), 2);
    assert_eq!(second.candidate, first.candidate);
    let waiting = |reviewed: &Reviewed| -> Vec<(String, _)> {
        reviewed
            .set_aside
            .iter()
            .map(|waiting| (waiting.finding.note.clone(), waiting.repairs))
            .collect()
    };
    assert_eq!(waiting(&second), waiting(&first));
    Ok(())
}

#[tokio::test]
async fn a_saved_answer_in_the_answers_shape_is_reused_without_asking_again() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&db).await?;
    let script = defect_no_repair_fixes();
    let rest = vec![
        script[0].clone(),
        script[1].clone(),
        script[3].clone(),
        script[5].clone(),
        script[1].clone(),
    ];
    let retitled = |before: &str, after: &str| {
        let edit = json!({"path":"/blocks/0/title","before":before,"after":after});
        answers_for(vec![("reviewer/1", "Retitle the section.", vec![edit])])
    };
    let llm = RepairsApart {
        repairs: model(vec![
            retitled("Example", "Corrected example"),
            retitled("Corrected example", "Corrected twice"),
        ]),
        rest: model(rest),
    };
    let first = lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        run(&llm, lesson(EXCERPT)).await
    })
    .await?;
    assert_eq!(llm.repairs.prompts.lock().unwrap().len(), 2);
    let second = lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        run(&llm, lesson(EXCERPT)).await
    })
    .await?;
    assert_eq!(llm.repairs.prompts.lock().unwrap().len(), 2);
    assert_eq!(second.candidate, first.candidate);
    let kept = kept_repairs(&pool, &job, &lesson_id).await;
    assert_eq!(kept.len(), 2, "{kept:?}");
    assert!(kept.iter().all(|row| row["answer"]
        .as_str()
        .is_some_and(|answer| answer.contains("\"answers\""))));
    Ok(())
}

use crate::lessons::claim_reviews::teaching_findings::{
    TeachingFindingsRepository, TeachingSettleDto,
};
use crate::review_findings::{PlanCheck, SetAside};

fn plan_at(subject: &str, site: FindingSite) -> ReviewFinding {
    ReviewFinding::plan(
        PlanCheck::Used,
        subject,
        site,
        "An idea is used before it is shown.",
    )
}

#[tokio::test]
async fn keeping_one_finding_leaves_another_finding_in_its_section_repairable() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (_, job, lesson_id) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&db).await?;
    let program_id: String =
        sqlx::query_scalar("SELECT program_id FROM learning_lessons WHERE id=?")
            .bind(&lesson_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let store = TeachingFindingsRepository::new(db.clone());
    let candidate = lesson(EXCERPT);
    let first = plan_at("aaaa000000000001", FindingSite::Section(0));
    let set_aside = [SetAside {
        finding: first.clone(),
        repairs: 2,
    }];
    store
        .record(&lesson_id, &job, &candidate, &set_aside, true)
        .await?;
    let listed = store.list(&lesson_id).await?;
    let key = listed.first().unwrap().finding_key.clone();
    store
        .settle(&program_id, &lesson_id, &[key], TeachingSettleDto::Keep)
        .await?;
    let passes = Passes::new(store.held(&lesson_id, &candidate).await?);
    assert!(!passes.repairable(&first));
    let other = plan_at("aaaa000000000002", FindingSite::Section(0));
    assert!(passes.repairable(&other));
    Ok(())
}

#[tokio::test]
async fn a_row_stored_by_place_holds_every_finding_of_its_kind_there() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (_, _, lesson_id) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&db).await?;
    let candidate = lesson(EXCERPT);
    sqlx::query(
        "INSERT INTO learning_lesson_teaching_findings (lesson_id, finding_key, kind, site, unit_index, unit_title, unit_sha256, note, passage, repairs, state, job_id, found_at, kept_at)          VALUES (?, 'teaching:section-0', 'teaching', 'section', 0, 'T', ?, 'A note.', NULL, 0, 'kept', NULL, 1, 5)",
    )
    .bind(&lesson_id)
    .bind(crate::review_findings::site_sha256(&candidate, FindingSite::Section(0)))
    .execute(&pool)
    .await
    .unwrap();
    let store = TeachingFindingsRepository::new(db.clone());
    let passes = Passes::new(store.held(&lesson_id, &candidate).await?);
    assert!(!passes.repairable(&plan_at("aaaa000000000001", FindingSite::Section(0))));
    assert!(!passes.repairable(&plan_at("aaaa000000000002", FindingSite::Section(0))));
    assert!(passes.repairable(&plan_at("aaaa000000000003", FindingSite::Section(1))));
    Ok(())
}

const DRAFT_QUESTIONS: &str =
    "Create exactly two distinct multiple-choice items of each kind: practice, quiz, and test.";

#[tokio::test]
async fn a_review_after_a_cut_and_its_repair_are_told_the_shape_the_lesson_is_kept_with(
) -> Result<()> {
    use crate::lessons::shape::{FixedSchema, Phase, ShapePolicy};
    let model = model(vec![
        review(Some(PROMISED)),
        actionable(),
        retitle("Corrected example"),
        judged(None, &[verdict("reviewer/1", "resolved")]),
    ]);
    let policy = FixedSchema(json!({"type":"object"}));
    let schema = policy.schema(Phase::Kept, 1);
    let request = json!({
        "sources":[{"sourceIndex":0,"excerpt":EXCERPT}],
        "lessonShape":[DRAFT_QUESTIONS]
    })
    .to_string();
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    let reviewed = review_material(
        &model,
        LESSON_REPAIR_SYSTEM,
        &request,
        ReviewSchema::Lesson(LessonShape::new(&schema, &policy)),
        lesson(EXCERPT).to_string(),
        4000,
        &mut LessonSubject::new(&references, &sources, &[], Held::default()),
    )
    .await?;
    assert!(reviewed.set_aside.is_empty());
    {
        let prompts = model.prompts.lock().unwrap();
        assert!(
            prompts
                .iter()
                .all(|prompt| !prompt.contains(DRAFT_QUESTIONS)),
            "a model was told the draft's questions for a lesson held to the floor"
        );
        assert!(prompts
            .iter()
            .any(|prompt| prompt.contains("authoringContext")));
    }
    let requests = repair_requests(&model);
    assert_eq!(requests.len(), 1);
    let lines = requests[0]["originalRequirements"]["lessonShape"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(!lines.is_empty(), "the repair was not told the shape");
    let words: Vec<&str> = lines.iter().filter_map(Value::as_str).collect();
    assert!(
        words
            .join("\n")
            .contains("at least one question of each kind"),
        "{words:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_draft_review_is_told_what_the_writer_was() -> Result<()> {
    use crate::lessons::shape::{self, FixedSchema, Phase, ShapePolicy};
    let model = model(vec![
        review(Some(PROMISED)),
        actionable(),
        retitle("Corrected example"),
        judged(None, &[verdict("reviewer/1", "resolved")]),
    ]);
    let draft = shape::policy().asked(Phase::Draft);
    let policy = FixedSchema(json!({"type":"object"}));
    let schema = policy.schema(Phase::Draft, 1);
    let request = json!({
        "sources":[{"sourceIndex":0,"excerpt":EXCERPT}],
        "lessonShape":draft
    })
    .to_string();
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    review_material(
        &model,
        LESSON_REPAIR_SYSTEM,
        &request,
        ReviewSchema::Lesson(LessonShape::new(&schema, &policy)),
        lesson(EXCERPT).to_string(),
        4000,
        &mut LessonSubject::new(&references, &sources, &[], Held::default()),
    )
    .await?;
    {
        let prompts = model.prompts.lock().unwrap();
        let reviews: Vec<_> = prompts
            .iter()
            .filter(|prompt| prompt.contains("authoringContext"))
            .collect();
        assert!(!reviews.is_empty());
        for prompt in reviews {
            for line in &draft {
                assert!(prompt.contains(line.as_str()), "{line}");
            }
        }
    }
    let requests = repair_requests(&model);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]["originalRequirements"]["lessonShape"],
        json!(draft)
    );
    Ok(())
}

/// A model that answers each kind of request from what the request asks: the
/// reviewer from the passages it is shown and the sections and verdicts its
/// schema allows, the evidence check from the issues it lists, a repair from
/// `repair`. The reviews and repairs it is sent are kept.
pub(super) struct ByRequest {
    /// Whether a passage's words are a defect.
    defective: fn(&str) -> bool,
    /// Whether a section's check reports a defect again that an open finding
    /// is already about.
    restates: bool,
    /// The defects of the whole material at the nth review (from 1), given
    /// the candidate the review shows.
    whole: fn(&Value, usize) -> Vec<&'static str>,
    /// The edits of the nth repair (from 0), given the request.
    repair: fn(&Value, usize) -> Vec<Value>,
    seen_reviews: Mutex<Vec<Value>>,
    seen_repairs: Mutex<Vec<Value>>,
    seen_schemas: Mutex<Vec<Value>>,
}

fn no_whole_defects(_: &Value, _: usize) -> Vec<&'static str> {
    Vec::new()
}

impl ByRequest {
    pub(super) fn new(
        defective: fn(&str) -> bool,
        repair: fn(&Value, usize) -> Vec<Value>,
    ) -> Self {
        Self {
            defective,
            restates: false,
            whole: no_whole_defects,
            repair,
            seen_reviews: Mutex::new(Vec::new()),
            seen_repairs: Mutex::new(Vec::new()),
            seen_schemas: Mutex::new(Vec::new()),
        }
    }

    fn restating(mut self) -> Self {
        self.restates = true;
        self
    }

    fn about_the_whole(mut self, whole: fn(&Value, usize) -> Vec<&'static str>) -> Self {
        self.whole = whole;
        self
    }

    /// The review requests, as the JSON each carries.
    pub(super) fn reviews(&self) -> Vec<Value> {
        self.seen_reviews.lock().unwrap().clone()
    }

    /// The response schema each review was sent with.
    pub(super) fn schemas(&self) -> Vec<Value> {
        self.seen_schemas.lock().unwrap().clone()
    }

    /// The repair requests, as the JSON each carries.
    pub(super) fn repairs(&self) -> Vec<Value> {
        self.seen_repairs.lock().unwrap().clone()
    }

    fn review_answer(&self, sent: &Value, schema: &Value, number: usize) -> String {
        let candidate = &sent["candidate"];
        let listed =
            |key: &str| -> Vec<Value> { sent[key].as_array().cloned().unwrap_or_default() };
        let open = listed("openFindings");
        let sections: Vec<usize> = schema
            .pointer("/properties/blockChecks/items/properties/sectionId/enum")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|id| id.as_str()?.strip_prefix("section-")?.parse().ok())
            .collect();
        let passages_of = |index: usize| -> Vec<(String, String)> {
            candidate["blocks"][index]["bodyPassages"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|passage| {
                    Some((
                        passage["id"].as_str()?.to_owned(),
                        passage["text"].as_str()?.to_owned(),
                    ))
                })
                .collect()
        };
        let covered = |index: usize, id: &str| {
            let part = format!("Section {}", index + 1);
            open.iter()
                .any(|entry| entry["part"] == part.as_str() && entry["passageId"] == id)
        };
        let mut checks = Vec::new();
        for index in &sections {
            let passages = passages_of(*index);
            let found = passages.iter().find(|(_, text)| (self.defective)(text));
            let id = found
                .or(passages.first())
                .map(|(id, _)| id.clone())
                .unwrap_or_default();
            let has_defect = found.is_some_and(|(id, _)| self.restates || !covered(*index, id));
            let mut check = json!({
                "sectionId": format!("section-{index}"),
                "passageId": id,
                "finding": if has_defect { FIRST_IDEA_PROMISE } else { "Nothing is wrong here." },
                "hasDefect": has_defect,
            });
            if schema
                .pointer("/properties/blockChecks/items/properties/concerns")
                .is_some()
            {
                check["concerns"] = json!(if has_defect { vec!["promise"] } else { vec![] });
            }
            checks.push(check);
        }
        let wanted = (self.whole)(candidate, number);
        let issues: Vec<String> = wanted
            .iter()
            .filter(|note| {
                !open
                    .iter()
                    .any(|entry| entry["note"].as_str() == Some(**note))
            })
            .map(|note| (*note).to_owned())
            .collect();
        let concerned = schema
            .pointer("/properties/issues/items/properties/concern")
            .is_some();
        let issues: Vec<Value> = issues
            .into_iter()
            .map(|note| {
                if concerned {
                    let concern = if note.contains("later lesson") {
                        "later_lesson"
                    } else {
                        "promise"
                    };
                    json!({"note": note, "concern": concern})
                } else {
                    json!(note)
                }
            })
            .collect();
        let mut answer = json!({"issues": issues});
        if schema.pointer("/properties/blockChecks").is_some() {
            answer["blockChecks"] = json!(checks);
        }
        if let Some(ids) = schema
            .pointer("/properties/openFindings/items/properties/id/enum")
            .and_then(Value::as_array)
        {
            let verdicts: Vec<Value> = ids
                .iter()
                .filter_map(Value::as_str)
                .map(|id| {
                    let entry = open
                        .iter()
                        .find(|entry| entry["id"] == id)
                        .cloned()
                        .unwrap_or(Value::Null);
                    let section = entry["part"]
                        .as_str()
                        .and_then(|part| part.strip_prefix("Section "))
                        .and_then(|number| number.parse::<usize>().ok());
                    let quote = match section {
                        Some(number) => passages_of(number.saturating_sub(1))
                            .into_iter()
                            .find(|(passage, text)| {
                                entry["passageId"].as_str().is_none_or(|own| own == passage)
                                    && (self.defective)(text)
                            })
                            .map(|(_, text)| text),
                        None => wanted
                            .iter()
                            .any(|note| entry["note"].as_str() == Some(*note))
                            .then(|| passages_of(0).into_iter().next().map(|(_, text)| text))
                            .flatten(),
                    };
                    match quote {
                        Some(quote) => json!({"id": id, "quote": quote, "verdict": "present"}),
                        None => json!({"id": id, "quote": "", "verdict": "resolved"}),
                    }
                })
                .collect();
            answer["openFindings"] = json!(verdicts);
        }
        answer.to_string()
    }
}

/// The evidence check's answer: each proposal is actionable, at the section
/// its note names, or at the lesson.
fn evidence_answer(sent: &Value, schema: &Value) -> String {
    let keys = schema
        .pointer("/properties/decisions/items/properties/issueId/enum")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let notes = sent["proposals"].as_array().cloned().unwrap_or_default();
    let first = sent["candidateParts"][0]["id"]
        .as_str()
        .unwrap_or("blocks-0")
        .to_owned();
    let decisions: Vec<Value> = keys
        .iter()
        .zip(&notes)
        .map(|(key, note)| {
            let section = note
                .as_str()
                .and_then(|note| note.split("Section ").nth(1))
                .and_then(|rest| {
                    rest.split(|c: char| !c.is_ascii_digit())
                        .next()?
                        .parse::<usize>()
                        .ok()
                })
                .map(|number| format!("blocks-{}", number.saturating_sub(1)));
            json!({
                "issueId": key,
                "verdict": "actionable",
                "basis": "internal_conflict",
                "evidenceIds": [section.clone().unwrap_or(first.clone())],
                "requirementId": null,
                "reason": "The note describes a concrete conflict in the lesson.",
                "partId": section.unwrap_or_else(|| "lesson".to_owned()),
            })
        })
        .collect();
    json!({"decisions": decisions}).to_string()
}

#[async_trait::async_trait]
impl LLMPort for ByRequest {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let texts = crate::content_verification::tests::fixture_prompts(request);
        let Some(sent) = texts.iter().find_map(|text| fixture_task(text)) else {
            return Ok(crate::content_verification::tests::fixture_completion(
                request,
                Vec::new(),
            ));
        };
        let schema = request.json_schema.clone().unwrap_or(Value::Null);
        let reply = if asks_for_repair(request) {
            let number = {
                let mut seen = self.seen_repairs.lock().unwrap();
                seen.push(sent.clone());
                seen.len() - 1
            };
            json!({"edits": (self.repair)(&sent, number)}).to_string()
        } else if schema.pointer("/properties/decisions").is_some() {
            evidence_answer(&sent, &schema)
        } else if schema.pointer("/properties/issues").is_some() {
            let number = {
                self.seen_schemas.lock().unwrap().push(schema.clone());
                let mut seen = self.seen_reviews.lock().unwrap();
                seen.push(sent.clone());
                seen.len()
            };
            self.review_answer(&sent, &schema, number)
        } else {
            return Err(AppError::InternalError(
                "ByRequest was sent a request of a kind it does not answer".into(),
            ));
        };
        Ok(crate::content_verification::tests::fixture_completion(
            request,
            vec![reply],
        ))
    }
    fn model_name(&self) -> &str {
        "by-request"
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

/// The blocks a repair request lists findings for.
fn parts_sent(sent: &Value) -> Vec<usize> {
    sent["issuesToFix"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            entry["part"]
                .as_str()?
                .strip_prefix("/blocks/")?
                .parse()
                .ok()
        })
        .collect()
}

/// An edit of a block's title: a change that fixes nothing.
pub(super) fn retitled(sent: &Value, index: usize) -> Value {
    let title = sent["candidate"]["blocks"][index]["title"]
        .as_str()
        .unwrap_or_default();
    json!({"path": format!("/blocks/{index}/title"), "before": title, "after": format!("{title}.")})
}

/// The edit that takes the first idea out of a block; once it is out, a
/// retitle.
fn mend(sent: &Value, index: usize) -> Value {
    let has_it = sent["candidate"]["blocks"][index]["body"]
        .as_str()
        .is_some_and(|body| body.contains("gives a first idea"));
    if has_it {
        json!({"path": format!("/blocks/{index}/body"), "before": "gives a first idea", "after": "gives a better idea"})
    } else {
        retitled(sent, index)
    }
}

fn mend_every_part(sent: &Value, _: usize) -> Vec<Value> {
    parts_sent(sent)
        .into_iter()
        .map(|index| mend(sent, index))
        .collect()
}

/// An edit of each block listed that changes its words and keeps the first
/// idea.
fn reword_every_part(sent: &Value, _: usize) -> Vec<Value> {
    parts_sent(sent)
        .into_iter()
        .map(|index| {
            json!({"path": format!("/blocks/{index}/body"), "before": "gives a first idea", "after": "gives a first idea, again"})
        })
        .collect()
}

/// The first repair takes up only the first block; the others, every block
/// listed.
fn mend_the_first_part_first(sent: &Value, number: usize) -> Vec<Value> {
    if number == 0 {
        vec![mend(sent, 0)]
    } else {
        mend_every_part(sent, number)
    }
}

/// The first repair corrects the first paragraph of the block; every later one
/// changes the last paragraph and leaves its defect.
fn correct_the_first_paragraph_then_touch_the_last(sent: &Value, number: usize) -> Vec<Value> {
    if number == 0 {
        return vec![json!({
            "path": "/blocks/0/body",
            "before": "takes a wrong fact as input.",
            "after": "takes a plain fact as input.",
        })];
    }
    let body = sent["candidate"]["blocks"][0]["body"]
        .as_str()
        .unwrap_or_default();
    let last = body.rsplit("\n\n").next().unwrap_or_default();
    vec![json!({"path": "/blocks/0/body", "before": last, "after": format!("{last} Again.")})]
}

/// A single part whose first idea is wrong, reviewed and repaired once.
async fn one_repair_that_fixes_it() -> Result<(ByRequest, Reviewed)> {
    let model = ByRequest::new(|text| text.contains("first idea"), mend_every_part);
    let reviewed = run(&model, parts_lesson(1)).await?;
    Ok((model, reviewed))
}

fn notes_waiting(reviewed: &Reviewed) -> Vec<(String, u8)> {
    reviewed
        .set_aside
        .iter()
        .map(|waiting| (waiting.finding.note.clone(), waiting.repairs))
        .collect()
}

#[tokio::test]
async fn a_resolved_finding_is_a_verdict_never_an_issue() -> Result<()> {
    let (model, reviewed) = one_repair_that_fixes_it().await?;
    assert!(
        reviewed.set_aside.is_empty(),
        "{:?}",
        notes_waiting(&reviewed)
    );
    assert_eq!(model.repairs().len(), 1);
    Ok(())
}

#[tokio::test]
async fn the_recheck_shows_the_open_finding_with_its_id() -> Result<()> {
    let (model, _) = one_repair_that_fixes_it().await?;
    let reviews = model.reviews();
    assert!(reviews.len() >= 2, "{}", reviews.len());
    assert!(
        reviews[1]["openFindings"][0]["id"].is_string(),
        "{}",
        reviews[1]
    );
    assert!(reviews[1].get("originalIssuesToRecheck").is_none());
    Ok(())
}

#[tokio::test]
async fn a_section_no_repair_touched_is_not_reviewed_again() -> Result<()> {
    let model = ByRequest::new(
        |text| text.contains("Part A") && text.contains("first idea"),
        mend_every_part,
    );
    let reviewed = run(&model, parts_lesson(3)).await?;
    assert!(reviewed.set_aside.is_empty());
    let reviews = model.reviews();
    assert_eq!(reviews[0]["sectionIndicesToReview"], json!([0, 1, 2]));
    assert_eq!(reviews[1]["sectionIndicesToReview"], json!([0]));
    Ok(())
}

#[tokio::test]
async fn a_finding_in_an_untouched_part_is_asked_about_again_and_stays() -> Result<()> {
    let model = ByRequest::new(
        |text| text.contains("first idea"),
        mend_the_first_part_first,
    );
    run(&model, parts_lesson(2)).await?;
    let (reviews, repairs) = (model.reviews(), model.repairs());
    assert_eq!(
        reviews[1]["openFindings"].as_array().map(Vec::len),
        Some(2),
        "{}",
        reviews[1]
    );
    assert_eq!(reviews[1]["openFindings"][0]["part"], "Section 1");
    assert_eq!(reviews[1]["openFindings"][1]["part"], "Section 2");
    assert_eq!(repairs.len(), 2);
    let second = repairs[1]["issuesToFix"].as_array().unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0]["part"], "/blocks/1");
    assert!(second[0]["lastAttempt"].is_string(), "{}", second[0]);
    Ok(())
}

#[tokio::test]
async fn a_new_reviewer_defect_gets_its_own_number_and_budget() -> Result<()> {
    let model = ByRequest::new(
        |text| text.contains("wrong fact") || text.contains("first idea"),
        correct_the_first_paragraph_then_touch_the_last,
    );
    let mut candidate = parts_lesson(1);
    candidate["blocks"][0]["body"] =
        json!("The tool takes a wrong fact as input.\n\nThe tool then gives a first idea.");
    let reviewed = run(&model, candidate).await?;
    assert_eq!(
        model.repairs().len(),
        3,
        "one for the first defect, two for the second"
    );
    assert_eq!(reviewed.set_aside.len(), 1);
    assert_eq!(reviewed.set_aside[0].repairs, 2);
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    let body = after["blocks"][0]["body"].as_str().unwrap();
    assert_eq!(
        reviewed.set_aside[0].finding.passage.as_deref(),
        body.rsplit("\n\n").next()
    );
    Ok(())
}

const WHEN_TO_USE: &str = "The lesson says a later lesson teaches when to use the tool, which no later lesson in the plan teaches.";
const WHAT_IT_CANNOT_DO: &str = "The lesson says a later lesson teaches what the tool cannot do, which no later lesson in the plan teaches.";

#[tokio::test]
async fn a_new_whole_lesson_issue_continues_the_count_of_the_one_before() -> Result<()> {
    let model = ByRequest::new(|_| false, |sent, _| vec![retitled(sent, 0)]).about_the_whole(
        |_, review| {
            if review == 1 {
                vec![WHEN_TO_USE]
            } else {
                vec![WHEN_TO_USE, WHAT_IT_CANNOT_DO]
            }
        },
    );
    let reviewed = run(&model, parts_lesson(1)).await?;
    let waiting = notes_waiting(&reviewed);
    assert_eq!(waiting.len(), 2, "{waiting:?}");
    assert!(
        waiting.iter().all(|(_, repairs)| *repairs == 2),
        "{waiting:?}"
    );
    assert_eq!(model.repairs().len(), 3);
    Ok(())
}

#[tokio::test]
async fn a_stored_reviewer_finding_is_judged_again_by_the_first_review_and_is_not_repaired(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (_, job, lesson_id) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&db).await?;
    let program_id: String =
        sqlx::query_scalar("SELECT program_id FROM learning_lessons WHERE id=?")
            .bind(&lesson_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let store = TeachingFindingsRepository::new(db.clone());
    let before = parts_lesson(2);
    let stored = ReviewFinding::reviewer(
        1,
        Some(FindingSite::Section(0)),
        "Section 1 (candidate blocks-0): The first idea is wrong.",
    )
    .with_passage(&part_body(0));
    store
        .record(
            &lesson_id,
            &job,
            &before,
            &[SetAside {
                finding: stored,
                repairs: 2,
            }],
            true,
        )
        .await?;
    let key = store
        .list(&lesson_id)
        .await?
        .first()
        .unwrap()
        .finding_key
        .clone();
    store
        .settle(
            &program_id,
            &lesson_id,
            std::slice::from_ref(&key),
            TeachingSettleDto::Keep,
        )
        .await?;
    let mut after = before.clone();
    after["blocks"][1]["body"] =
        json!("Part B states one plain fact about the tool. Part B then gives a better idea.");
    let held = store.held(&lesson_id, &after).await?;
    let model = ByRequest::new(
        |text| text.contains("Part A") && text.contains("first idea"),
        mend_every_part,
    );
    let sources = references();
    let collection = ReferenceCollection::lexical(&sources)?;
    let reviewed = review_material(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        ReviewSchema::Other(&json!({"type":"object"})),
        after.to_string(),
        4000,
        &mut LessonSubject::new(&collection, &sources, &[], held),
    )
    .await?;
    let first = &model.reviews()[0];
    assert_eq!(first["sectionIndicesToReview"], json!([0, 1]));
    let asked: Vec<Value> = first["openFindings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["id"].clone())
        .collect();
    assert_eq!(asked, [json!("reviewer/1")]);
    assert!(model.repairs().is_empty());
    assert_eq!(reviewed.set_aside.len(), 1);
    assert_eq!(reviewed.set_aside[0].finding.id().as_str(), "reviewer/1");
    let reviewed_candidate: Value = serde_json::from_str(&reviewed.candidate)?;
    store
        .record(
            &lesson_id,
            &job,
            &reviewed_candidate,
            &reviewed.set_aside,
            true,
        )
        .await?;
    let listed = store.list(&lesson_id).await?;
    let row = listed
        .iter()
        .find(|finding| finding.unit_index == Some(0))
        .unwrap();
    assert_eq!(row.finding_key, key);
    assert_eq!(
        row.state,
        crate::lessons::claim_reviews::teaching_findings::TeachingFindingStateDto::Kept
    );
    Ok(())
}

/// The review of `candidate` in the lesson job under way, the reviewer shown
/// `authoring` as the lesson's authoring context.
async fn review_in_the_job(
    model: &ByRequest,
    authoring: &str,
    candidate: Value,
) -> Result<Reviewed> {
    lesson_drafts::resume(INPUT.into()).await?;
    let sources = references();
    let collection = ReferenceCollection::lexical(&sources)?;
    review_material(
        model,
        LESSON_REPAIR_SYSTEM,
        authoring,
        ReviewSchema::Other(&json!({"type":"object"})),
        candidate.to_string(),
        4000,
        &mut LessonSubject::new(&collection, &sources, &[], Held::default()),
    )
    .await
}

pub(super) fn part_a_is_wrong(text: &str) -> bool {
    text.contains("Part A") && text.contains("first idea")
}

#[tokio::test]
async fn a_draft_the_reviewer_saw_under_one_authoring_context_is_reviewed_again_under_another(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&db).await?;
    let first = ByRequest::new(part_a_is_wrong, |_, _| Vec::new());
    let before = lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        review_in_the_job(&first, &context(), parts_lesson(2)).await
    })
    .await?;
    assert_eq!(before.set_aside.len(), 1);
    let other =
        json!({"sources":[{"sourceIndex":0,"excerpt":EXCERPT}],"lessonShape":["Two sections."]})
            .to_string();
    let second = ByRequest::new(part_a_is_wrong, |_, _| Vec::new());
    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        review_in_the_job(&second, &other, serde_json::from_str(&before.candidate)?).await
    })
    .await?;
    let reviews = second.reviews();
    assert!(
        !reviews.is_empty(),
        "the same text under other inputs was not reviewed"
    );
    assert_eq!(reviews[0]["sectionIndicesToReview"], json!([0, 1]));
    let asked: Vec<Value> = reviews[0]["openFindings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["id"].clone())
        .collect();
    assert_eq!(asked, [json!("reviewer/1")]);
    Ok(())
}

#[tokio::test]
async fn a_contradiction_between_two_parts_is_kept_with_both_and_repaired() -> Result<()> {
    let none_wrong = |index: usize| json!({"index":index,"passageId":format!("section-{index}-passage-0"),"finding":"Nothing is wrong here.","hasDefect":false});
    let recheck = |id: &str, issues: Vec<&str>, section: usize| {
        json!({
            "openFindings": [{"id": id, "quote": "", "verdict": "resolved"}],
            "issues": issues.iter().map(|note| json!({"note": note, "concern": "promise"})).collect::<Vec<_>>(),
            "blockChecks": crate::teaching_review::fixture_checks(vec![none_wrong(section)]),
        })
        .to_string()
    };
    let clash = json!({"decisions":[{
        "issueId": "issue-0",
        "verdict": "actionable",
        "basis": "internal_conflict",
        "evidenceIds": ["blocks-0", "blocks-1"],
        "partId": "blocks-1",
        "requirementId": null,
        "reason": "Part A and Part B state opposite ideas."
    }]})
    .to_string();
    let model = model(vec![
        parts_review(2, &[0]),
        parts_evidence(&[0]),
        edits_answer(vec![better_idea(0, "reviewer/1")]),
        recheck("reviewer/1", vec!["Part A now promises that the tutor runs the learner's code, which Part B says it never does."], 0),
        clash,
        edits_answer(vec![better_idea(1, "reviewer/2")]),
        recheck("reviewer/2", Vec::new(), 1),
    ]);
    let reviewed = run(&model, parts_lesson(2)).await?;
    let repairs = repair_requests(&model);
    assert_eq!(repairs.len(), 2, "the contradiction reached no repair");
    let sent = &repairs[1]["issuesToFix"][0];
    assert_eq!(sent["part"], "/blocks/1");
    assert_eq!(sent["also"], "/blocks/0");
    assert!(reviewed.set_aside.is_empty());
    Ok(())
}

const WRONG_COMPARISON: &str =
    "The exercise answer contradicts its stated condition. Correct the comparison.";

#[tokio::test]
async fn a_closed_review_s_recheck_takes_a_resolved_verdict_not_an_issue() -> Result<()> {
    let model = ByRequest::new(
        |_| false,
        |_, _| vec![json!({"path":"/wrong","before":true,"after":false})],
    )
    .about_the_whole(|candidate, _| {
        if candidate["wrong"] == true {
            vec![WRONG_COMPARISON]
        } else {
            Vec::new()
        }
    });
    let result = review_and_repair(
        &model,
        "Author quiz questions",
        "Require a correct comparison",
        &json!({"type":"object"}),
        json!({"wrong":true}).to_string(),
        2000,
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    Ok(())
}

#[tokio::test]
async fn material_that_fails_closed_whose_repair_changes_nothing_twice_fails_without_another_review(
) {
    let model = ByRequest::new(
        |_| false,
        |_, _| vec![json!({"path":"/wrong","before":"true","after":"true"})],
    )
    .about_the_whole(|_, _| vec![WRONG_COMPARISON]);
    let result = review_and_repair(
        &model,
        "Author quiz questions",
        "Require a correct comparison",
        &json!({"type":"object"}),
        json!({"wrong":true}).to_string(),
        2000,
    )
    .await;
    let error = result.unwrap_err().to_string();
    assert!(error.contains("changed nothing"), "{error}");
    assert_eq!(model.reviews().len(), 1);
    assert_eq!(model.repairs().len(), 2);
}

#[tokio::test]
async fn a_present_finding_its_section_check_reports_again_is_one_finding() -> Result<()> {
    let model = ByRequest::new(|text| text.contains("first idea"), reword_every_part).restating();
    let reviewed = run(&model, parts_lesson(1)).await?;
    let repairs = model.repairs();
    assert_eq!(repairs.len(), 2);
    assert_eq!(repairs[1]["issuesToFix"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        reviewed.set_aside.len(),
        1,
        "{:?}",
        notes_waiting(&reviewed)
    );
    Ok(())
}

/// The writer's full lesson whose first section copies the source's sentence.
fn lesson_that_copies() -> Value {
    let mut lesson = shaped_lesson();
    lesson["blocks"][0]["body"] = json!(format!("Body of First idea. {EXCERPT}"));
    lesson
}

fn copied_id() -> String {
    format!(
        "rule/copied-wording/section-0/{}",
        crate::review_findings::subject_of(EXCERPT)
    )
}

fn say_it_yourself(id: &str) -> String {
    json!({"edits":[{"finding":id,"path":"/blocks/0/body","before":EXCERPT,"after":"The widget does what its notes say it does."}]}).to_string()
}

const EDITS_FORM: &str =
    "You are editing a rejected draft. Correct only the established teaching defects";
const WHOLE_FORM: &str = "Repair the teaching material";

async fn review_shaped(
    model: &ScriptedModel,
    candidate: &Value,
    protected: &[crate::lessons::content_verification::Protected],
) -> Result<Reviewed> {
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    let policy = crate::lessons::shape::policy();
    let phase = if protected.is_empty() {
        crate::lessons::shape::Phase::Draft
    } else {
        crate::lessons::shape::Phase::Kept
    };
    let schema = policy.schema(phase, 1);
    review_material(
        model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        ReviewSchema::Lesson(LessonShape::new(&schema, policy)),
        candidate.to_string(),
        4000,
        &mut LessonSubject::new(&references, &sources, protected, Held::default()),
    )
    .await
}

/// The prompts of the repair requests a scripted model was sent, in order.
fn repair_prompts(model: &ScriptedModel) -> Vec<String> {
    model
        .prompts
        .lock()
        .unwrap()
        .iter()
        .filter(|prompt| prompt.contains("issuesToFix"))
        .cloned()
        .collect()
}

#[tokio::test]
async fn a_lesson_question_the_judge_faults_reaches_the_repair_as_a_finding_at_that_question() {
    const FAULT: &str = "One option stands apart from the others in length.";
    let candidate = shaped_lesson();
    let model = model(vec![
        shaped_solutions(&candidate),
        shaped_judgement(&candidate, Some((2, FAULT))),
        shaped_review(&candidate),
    ]);

    let _ = review_shaped(&model, &candidate, &[]).await;

    let prompts = model.prompts.lock().unwrap().clone();
    assert!(
        prompts.iter().any(|prompt| prompt.contains("issuesToFix")
            && prompt.contains(&format!("Question 3: {FAULT}"))),
        "{prompts:?}"
    );
}

const REFERENCE_CHECK: &str = "Check whether proposed lesson-review findings";

#[tokio::test]
async fn a_question_finding_is_repaired_without_the_reference_check() {
    const FAULT: &str = "One option stands apart from the others in length.";
    let candidate = shaped_lesson();
    let model = model(vec![
        shaped_solutions(&candidate),
        shaped_judgement(&candidate, Some((2, FAULT))),
        shaped_review(&candidate),
        retitle("Unused"),
    ]);

    let _ = review_shaped(&model, &candidate, &[]).await;

    let prompts = model.prompts.lock().unwrap().clone();
    assert!(
        !prompts
            .iter()
            .any(|prompt| prompt.starts_with(REFERENCE_CHECK)),
        "{prompts:?}"
    );
    assert!(
        prompts.iter().any(|prompt| prompt.contains("issuesToFix")
            && prompt.contains(&format!("Question 3: {FAULT}"))),
        "{prompts:?}"
    );
}

#[tokio::test]
async fn the_reference_check_is_asked_only_about_the_reviewers_findings() {
    const FAULT: &str = "One option stands apart from the others in length.";
    const NOTE: &str = "The first section promises that the tutor certifies the learner's work as expert reviewed.";
    let candidate = shaped_lesson();
    let checks = (0..candidate["blocks"].as_array().unwrap().len())
        .map(|index| json!({"index":index,"passageId":format!("section-{index}-passage-0"),"finding":if index == 0 {NOTE} else {"Reviewed; no teaching defect was found."},"hasDefect":index == 0,"concerns":if index == 0 {vec!["promise"]} else {vec![]}}))
        .collect();
    let model = model(vec![
        shaped_solutions(&candidate),
        shaped_judgement(&candidate, Some((2, FAULT))),
        crate::lessons::course_generation_tests::review_reply(&[], checks, &[]),
        actionable(),
    ]);

    let _ = review_shaped(&model, &candidate, &[]).await;

    let prompts = model.prompts.lock().unwrap().clone();
    let asked: Vec<&String> = prompts
        .iter()
        .filter(|prompt| prompt.starts_with(REFERENCE_CHECK))
        .collect();
    assert_eq!(asked.len(), 1, "{prompts:?}");
    let sent: Value = fixture_task(asked[0]).unwrap();
    let proposals = sent["proposals"].as_array().unwrap();
    assert_eq!(proposals.len(), 1, "{proposals:?}");
    assert!(proposals[0]["proposal"].as_str().unwrap().contains(NOTE));
}

#[tokio::test]
async fn a_copied_sentence_is_repaired_by_one_patch_after_the_content_was_reviewed() -> Result<()> {
    let candidate = lesson_that_copies();
    let model = model(vec![
        shaped_solutions(&candidate),
        shaped_judgement(&candidate, None),
        shaped_review(&candidate),
        say_it_yourself(&copied_id()),
        shaped_solutions(&candidate),
        shaped_judgement(&candidate, None),
        crate::lessons::course_generation_tests::clean_review(&[0], &[]),
    ]);

    let reviewed = review_shaped(&model, &candidate, &[]).await;

    let repairs = repair_prompts(&model);
    assert!(
        repairs
            .first()
            .is_some_and(|prompt| prompt.contains(EDITS_FORM)),
        "the first repair was not asked for edits"
    );
    assert!(repairs.iter().all(|prompt| !prompt.contains(WHOLE_FORM)));
    let prompts = model.prompts.lock().unwrap().clone();
    let reviewed_at = prompts
        .iter()
        .position(|prompt| prompt.contains("sectionIndicesToReview"));
    let repaired_at = prompts
        .iter()
        .position(|prompt| prompt.contains("issuesToFix"));
    assert!(
        reviewed_at
            .zip(repaired_at)
            .is_some_and(|(review, repair)| review < repair),
        "the content was not reviewed before the first repair"
    );
    assert_eq!(repairs.len(), 1);
    let reviewed = reviewed?;
    assert!(reviewed.set_aside.is_empty());
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    assert!(!after["blocks"][0]["body"]
        .as_str()
        .unwrap()
        .contains(EXCERPT));
    Ok(())
}

#[tokio::test]
async fn a_whole_repair_is_sent_only_what_needs_the_whole_lesson_and_the_copied_sentence_is_patched_after_it(
) -> Result<()> {
    let full = lesson_that_copies();
    let mut cut = full.clone();
    cut["questions"].as_array_mut().unwrap().pop();
    let model = model(vec![
        full.to_string(),
        shaped_solutions(&full),
        shaped_judgement(&full, None),
        shaped_review(&full),
        say_it_yourself(&copied_id()),
        shaped_solutions(&full),
        shaped_judgement(&full, None),
        crate::lessons::course_generation_tests::clean_review(&[0], &[]),
    ]);

    let reviewed = review_shaped(&model, &cut, &[]).await;

    let asked = repair_requests(&model);
    assert!(!asked.is_empty(), "no repair was asked");
    let first = ids_of(&asked[0]);
    assert!(
        first.iter().any(|id| id == "rule/question-count/lesson"),
        "{first:?}"
    );
    assert!(
        first.iter().all(|id| !id.contains("copied-wording")),
        "the whole rewrite was sent the copied sentence: {first:?}"
    );
    let prompts = repair_prompts(&model);
    assert!(prompts[0].contains(WHOLE_FORM));
    assert_eq!(asked.len(), 2, "{prompts:?}");
    assert!(prompts[1].contains(EDITS_FORM));
    assert_eq!(ids_of(&asked[1]), [copied_id()]);
    assert!(asked[1]["issuesToFix"][0]["lastAttempt"].is_null());
    let reviewed = reviewed?;
    assert!(reviewed.set_aside.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_copied_sentence_no_claim_stands_on_is_repaired_by_edits_while_claims_are_protected(
) -> Result<()> {
    let candidate = lesson_that_copies();
    let protected = [crate::lessons::content_verification::Protected {
        unit: 1,
        field: "body".into(),
        quote: "Body of First example.".into(),
        released: false,
    }];
    let model = model(vec![
        shaped_solutions(&candidate),
        shaped_judgement(&candidate, None),
        shaped_review(&candidate),
        say_it_yourself(&copied_id()),
        shaped_solutions(&candidate),
        shaped_judgement(&candidate, None),
        crate::lessons::course_generation_tests::clean_review(&[0], &[]),
    ]);

    let reviewed = review_shaped(&model, &candidate, &protected).await?;

    let waiting: Vec<_> = reviewed
        .set_aside
        .iter()
        .map(|waiting| waiting.finding.id().as_str().to_owned())
        .collect();
    assert!(waiting.is_empty(), "{waiting:?}");
    let after: Value = serde_json::from_str(&reviewed.candidate)?;
    assert!(!after["blocks"][0]["body"]
        .as_str()
        .unwrap()
        .contains(EXCERPT));
    assert_eq!(after["blocks"][1]["body"], candidate["blocks"][1]["body"]);
    Ok(())
}

#[test]
fn a_copied_sentence_does_not_hold_up_the_content_review() {
    let copy = ReviewFinding::rule_about(
        Rule::CopiedWording,
        FindingKind::Teaching,
        FindingSite::Section(0),
        "A copied sentence.",
        "Say it in the lesson's own words.",
    );
    assert!(!holds_up_review(
        std::slice::from_ref(&copy),
        Some(&Passes::default())
    ));
    assert!(!holds_up_review(&[copy], None));
}

#[test]
fn an_answer_of_plans_and_no_change_declines_each_finding_and_changes_nothing() -> Result<()> {
    use super::repair::Repaired;
    use super::subject::{Settled, Subject};
    let previous = shaped_lesson();
    let about = |sentence: &str| {
        ReviewFinding::rule_about(
            Rule::SentenceRepeated,
            FindingKind::Teaching,
            FindingSite::Section(0),
            sentence,
            "Say it once.",
        )
        .with_passage(sentence)
    };
    let (first, second) = (about("Beta is second."), about("Body of First idea."));
    let plans = std::collections::BTreeMap::from([
        (first.id().as_str().to_owned(), "Leave it.".to_owned()),
        (second.id().as_str().to_owned(), "Say it once.".to_owned()),
    ]);
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    let mut subject = LessonSubject::new(&references, &sources, &[], Held::default());

    let settled = subject.settle_repair(
        &previous,
        &[first.clone(), second.clone()],
        Repaired::Declined { plans },
    )?;

    assert!(matches!(settled, Settled::Unchanged));
    assert_eq!(
        subject.last_attempt(&first).as_deref(),
        Some("The last repair gave no change for this finding; its plan was: \"Leave it.\"")
    );
    assert_eq!(
        subject.last_attempt(&second).as_deref(),
        Some("The last repair gave no change for this finding; its plan was: \"Say it once.\"")
    );
    Ok(())
}

#[test]
fn a_finding_whose_edit_overlapped_another_findings_is_not_charged_and_is_told_to_give_it_again(
) -> Result<()> {
    use super::repair::Repaired;
    use super::subject::{Settled, Subject};
    let mut previous = shaped_lesson();
    previous["blocks"][0]["body"] = json!("Body of First idea. Beta is second.");
    let about = |sentence: &str| {
        ReviewFinding::rule_about(
            Rule::SentenceRepeated,
            FindingKind::Teaching,
            FindingSite::Section(0),
            sentence,
            "Say it once.",
        )
        .with_passage(sentence)
    };
    let (first, second) = (about("Beta is second."), about("Body of First idea."));
    let patch = json!({"edits":[
        {"finding":first.id().as_str(),"path":"/blocks/0/body","before":"Beta is second.","after":"Beta comes second."},
        {"finding":second.id().as_str(),"path":"/blocks/0/body","before":"is second.","after":"is last."}
    ]})
    .to_string();
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    let mut subject = LessonSubject::new(&references, &sources, &[], Held::default());

    let settled = subject.settle_repair(
        &previous,
        &[first.clone(), second.clone()],
        Repaired::Patch {
            patch,
            plans: std::collections::BTreeMap::new(),
        },
    )?;

    assert!(matches!(settled, Settled::Changed(_)));
    assert!(subject.last_attempt(&first).is_some_and(
        |told| told.starts_with("The last repair's edits for this finding were applied")
    ));
    assert_eq!(
        subject.last_attempt(&second).as_deref(),
        Some("The last repair's edit for this finding overlapped an edit for another finding and was left out; give it again.")
    );
    Ok(())
}

#[tokio::test]
async fn a_whole_repair_is_neither_sent_nor_charged_a_failing_quotation() -> Result<()> {
    const BAD: &str = "An imaginary handbook says this widget behaves in a different way entirely.";
    let mut full = shaped_lesson();
    full["blocks"][0]["quote"] = json!(BAD);
    let mut cut = full.clone();
    cut["questions"].as_array_mut().unwrap().pop();
    let quotation_id = format!(
        "rule/quotation/section-0/{}",
        crate::review_findings::subject_of(BAD)
    );
    let fix = json!({"edits":[{"finding":quotation_id,"path":"/blocks/0/quote","before":BAD,"after":EXCERPT}]}).to_string();
    let model = model(vec![
        full.to_string(),
        shaped_solutions(&full),
        shaped_judgement(&full, None),
        shaped_review(&full),
        fix,
        shaped_solutions(&full),
        shaped_judgement(&full, None),
        crate::lessons::course_generation_tests::clean_review(&[0], &[]),
    ]);

    let reviewed = review_shaped(&model, &cut, &[]).await;

    let asked = repair_requests(&model);
    assert!(!asked.is_empty(), "no repair was asked");
    let first = ids_of(&asked[0]);
    assert!(
        first.iter().any(|id| id == "rule/question-count/lesson"),
        "{first:?}"
    );
    assert!(
        first.iter().all(|id| !id.starts_with("rule/quotation")),
        "the whole rewrite was sent the quotation: {first:?}"
    );
    assert_eq!(asked.len(), 2);
    assert_eq!(ids_of(&asked[1]), [quotation_id]);
    assert!(asked[1]["issuesToFix"][0]["lastAttempt"].is_null());
    assert!(reviewed?.set_aside.is_empty());
    Ok(())
}

async fn repaired_outline(model: &dyn LLMPort, outline: Value) -> Result<String> {
    let progress = crate::outline_progress::OutlineProgress::default();
    let sources = references();
    review_and_repair_with_progress(
        model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        &json!({"type":"object","properties":{"modules":{"type":"array"}}}),
        outline.to_string(),
        4000,
        OutlineReviewContext {
            progress: &progress,
            sources: &sources,
        },
    )
    .await
}

#[tokio::test]
async fn an_outline_repair_that_leaves_a_finding_is_repaired_again() {
    let model = model(vec![
        json!({"issues":["Defect one of the outline."]}).to_string(),
        json!({"modules":[{"title":"Revised"}]}).to_string(),
        json!({"issues":["Defect two of the outline."],"openFindings":[verdict("reviewer/1", "resolved")]}).to_string(),
        json!({"modules":[{"title":"Revised again"}]}).to_string(),
        json!({"issues":[],"openFindings":[verdict("reviewer/2", "resolved")]}).to_string(),
    ]);
    let result = repaired_outline(&model, json!({"modules":[]})).await;
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn an_outline_revision_whose_quote_matches_no_reference_is_repaired() {
    let cited = |quote: &str| json!({"modules":[{"summary":{"text":"Summary","sourceIndex":0,"quote":quote}}]});
    let model = model(vec![
        json!({"issues":[]}).to_string(),
        cited("A different imaginary handbook says this tool behaves in another way entirely.")
            .to_string(),
        json!({"issues":[]}).to_string(),
        cited(EXCERPT).to_string(),
        json!({"issues":[]}).to_string(),
    ]);
    let result = repaired_outline(
        &model,
        cited("An imaginary handbook says this tool behaves in a different way entirely."),
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn a_diagnostic_edit_repair_that_cannot_be_used_is_split() {
    let model = Answering::new(
        vec![
            judged_issues(&[
                "First defect of the lesson.",
                "Second defect of the lesson.",
            ]),
            judged(
                None,
                &[
                    verdict("reviewer/1", "resolved"),
                    verdict("reviewer/2", "resolved"),
                ],
            ),
        ],
        |sent| {
            Ok(match ids_of(sent).as_slice() {
                [only] if only == "reviewer/1" => retitle_from("Example", "Example one"),
                [only] if only == "reviewer/2" => retitle_from("Example one", "Example two"),
                _ => unusable_edits(),
            })
        },
    );
    let result = review_and_repair(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context(),
        &json!({"type":"object"}),
        lesson(EXCERPT).to_string(),
        4000,
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        model.asked(),
        [
            vec!["reviewer/1".to_owned(), "reviewer/2".to_owned()],
            vec!["reviewer/1".to_owned()],
            vec!["reviewer/2".to_owned()]
        ]
    );
}

/// A review of the lesson's one section that reports `issues` about the
/// lesson as a whole.
fn judged_issues(issues: &[&str]) -> String {
    review_reply(
        issues,
        vec![
            json!({"index":0,"passageId":"section-0-passage-0","finding":"The complete explanation was reviewed and no teaching defect was found.","hasDefect":false}),
        ],
        &[],
    )
}

const ALPHA: &str = "Alpha is first.";
const GAMMA: &str = "Gamma is third.";

fn guarded_lesson() -> Value {
    let mut lesson = shaped_lesson();
    lesson["blocks"][0]["body"] = json!(format!("{ALPHA} Beta is second. {GAMMA}"));
    lesson
}

fn gamma_protected() -> crate::lessons::content_verification::Protected {
    crate::lessons::content_verification::Protected {
        unit: 0,
        field: "body".into(),
        quote: GAMMA.into(),
        released: false,
    }
}

fn change(finding: &ReviewFinding, path: &str, before: &str, after: &str) -> Value {
    json!({"finding":finding.id().as_str(),"path":path,"before":before,"after":after})
}

/// What a repair settles to under protected sentences: the lesson it left,
/// if it changed it, and what each sent finding's next attempt is told.
fn settle_protected(
    previous: &Value,
    send: &[ReviewFinding],
    repaired: super::repair::Repaired,
) -> Result<(Option<Value>, Vec<Option<String>>)> {
    use super::subject::{Settled, Subject};
    let sources = references();
    let references = ReferenceCollection::lexical(&sources)?;
    let protected = [gamma_protected()];
    let mut subject = LessonSubject::new(&references, &sources, &protected, Held::default());
    let settled = subject.settle_repair(previous, send, repaired)?;
    let told = send
        .iter()
        .map(|finding| subject.last_attempt(finding))
        .collect();
    let changed = match settled {
        Settled::Changed(text) => Some(serde_json::from_str(&text)?),
        _ => None,
    };
    Ok((changed, told))
}

fn patch_of(edits: Vec<Value>) -> super::repair::Repaired {
    super::repair::Repaired::Patch {
        patch: json!({ "edits": edits }).to_string(),
        plans: std::collections::BTreeMap::new(),
    }
}

/// What the next attempt of a finding whose correction changed GAMMA is told.
const REFUSED_FOR_GAMMA: &str = "The last repair's correction for this finding was refused, and none of its edits were applied: it changes \"Gamma is third.\", which states a checked claim and must stay as it is.";

#[test]
fn a_correction_that_edits_a_protected_sentence_is_refused_whole() -> Result<()> {
    let previous = guarded_lesson();
    let finding = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "It is wrong.");
    let (changed, told) = settle_protected(
        &previous,
        std::slice::from_ref(&finding),
        patch_of(vec![
            change(&finding, "/blocks/0/body", ALPHA, "Alpha comes first."),
            change(&finding, "/blocks/0/body", GAMMA, "Gamma comes third."),
        ]),
    )?;
    assert_eq!(changed, None, "no edit of the refused correction stands");
    assert_eq!(told[0].as_deref(), Some(REFUSED_FOR_GAMMA));
    Ok(())
}

#[test]
fn a_correction_inside_its_targets_applies_while_a_sentence_is_protected() -> Result<()> {
    let previous = guarded_lesson();
    let finding = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "It is wrong.");
    let (changed, told) = settle_protected(
        &previous,
        std::slice::from_ref(&finding),
        patch_of(vec![change(
            &finding,
            "/blocks/0/body",
            ALPHA,
            "Alpha comes first.",
        )]),
    )?;
    let changed = changed.unwrap();
    assert_eq!(
        changed["blocks"][0]["body"],
        format!("Alpha comes first. Beta is second. {GAMMA}")
    );
    assert!(told[0].as_deref().is_some_and(
        |told| told.starts_with("The last repair's edits for this finding were applied")
    ));
    Ok(())
}

#[test]
fn of_two_corrections_only_the_one_inside_its_targets_applies() -> Result<()> {
    let previous = guarded_lesson();
    let refused = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "It is wrong.");
    let inside = ReviewFinding::reviewer(2, Some(FindingSite::Section(1)), "It is vague.");
    let (changed, told) = settle_protected(
        &previous,
        &[refused.clone(), inside.clone()],
        patch_of(vec![
            change(&refused, "/blocks/0/body", ALPHA, "Alpha comes first."),
            change(&refused, "/blocks/0/body", GAMMA, "Gamma comes third."),
            change(
                &inside,
                "/blocks/1/body",
                "Body of First example.",
                "Body of the first example.",
            ),
        ]),
    )?;
    let changed = changed.unwrap();
    assert_eq!(
        changed["blocks"][0]["body"], previous["blocks"][0]["body"],
        "the refused finding's edits are all left out"
    );
    assert_eq!(changed["blocks"][1]["body"], "Body of the first example.");
    assert_eq!(told[0].as_deref(), Some(REFUSED_FOR_GAMMA));
    assert!(told[1].as_deref().is_some_and(
        |told| told.starts_with("The last repair's edits for this finding were applied")
    ));
    Ok(())
}

#[test]
fn a_whole_rewrite_that_changes_a_protected_sentence_changes_nothing_and_charges_every_finding(
) -> Result<()> {
    let previous = guarded_lesson();
    let mut rewritten = previous.clone();
    rewritten["blocks"][0]["body"] = json!("Alpha comes first. Beta is second. Gamma comes third.");
    let send = [
        ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "It is wrong."),
        ReviewFinding::reviewer(2, Some(FindingSite::Section(0)), "It is vague."),
    ];
    let (changed, told) = settle_protected(
        &previous,
        &send,
        super::repair::Repaired::Whole(rewritten.to_string()),
    )?;
    assert_eq!(changed, None);
    for told in told {
        assert_eq!(told.as_deref(), Some(REFUSED_FOR_GAMMA));
    }
    Ok(())
}

#[tokio::test]
async fn a_lesson_asks_its_reviewer_for_concerns_and_an_outline_does_not() -> Result<()> {
    let lesson_model = ByRequest::new(|_| false, |_, _| Vec::new());
    run(&lesson_model, parts_lesson(1)).await?;
    let asked = lesson_model.schemas();
    assert!(!asked.is_empty());
    for schema in &asked {
        assert!(schema
            .pointer("/properties/blockChecks/items/properties/concerns")
            .is_some());
        assert!(schema
            .pointer("/properties/issues/items/properties/concern")
            .is_some());
    }
    let outline_model = ByRequest::new(|_| false, |_, _| Vec::new());
    repaired_outline(&outline_model, json!({"modules":[]})).await?;
    let asked = outline_model.schemas();
    assert!(!asked.is_empty());
    for schema in &asked {
        assert_eq!(
            schema.pointer("/properties/issues/items"),
            Some(&json!({"type":"string","minLength":1}))
        );
        assert!(schema.pointer("/properties/blockChecks").is_none());
    }
    Ok(())
}
