#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
use super::*;
use crate::lessons::course_generation_tests::{clean_review, verdict, ScriptedModel};
use crate::lessons::shape;
use lattice_core::application::services::model_contract::CORRECTION_LEAD;
use lattice_core::domain::learning::claim_review::{
    Actor, Check, CheckResult, Checker, ClaimEvent, ClaimLocation, ClaimState, EvidenceFingerprint,
    UnitKind,
};
use std::sync::Mutex;

fn block(kind: &str, title: &str, body: &str) -> Value {
    json!({"kind":kind,"title":title,"body":body,"rubric":[],"quote":""})
}

fn question(prompt: &str) -> Value {
    json!({"kind":"quiz","prompt":prompt,"options":["One","Two","Three","Four"],"correctIndex":0,"explanation":"Because.","quote":""})
}

/// Four sections (units 0 to 3) and two questions (units 4 and 5).
fn lesson() -> Value {
    json!({"blocks":[
        block("explanation","First idea","One. Two. Three."),
        block("worked_example","Example","Alpha. Beta. Gamma."),
        block("explanation","Second idea","Red. Green. Blue."),
        block("recap","Recap","Done. Over."),
    ],"questions":[question("Which?"), question("And?")]})
}

pub(super) fn model(replies: &[Value]) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(replies.iter().map(Value::to_string).collect()),
        prompts: Mutex::new(Vec::new()),
    }
}

pub(super) fn sources() -> Vec<LearningSourceDto> {
    vec![LearningSourceDto {
        id: "source".into(),
        title: "Source".into(),
        url: None,
        excerpt: "Red. Green. Blue.".into(),
        acquired_at: 0,
    }]
}

fn checked(llm: &dyn LLMPort) -> Check {
    Check {
        checker: Checker::Local {
            model: llm.model_name().into(),
        },
        result: CheckResult::Supported {
            source_version_id: "source".into(),
            supporting_quote: "Red. Green. Blue.".into(),
        },
        reason: "Stated.".into(),
        fingerprint: EvidenceFingerprint::of("fixture-v1", llm.model_name(), &[]),
        evidence: Vec::new(),
        window_tokens: 4_000,
    }
}

/// A claim standing on `sentence` of unit `unit`, whose body is `body`.
async fn standing(
    ledger: &mut ClaimLedger,
    llm: &dyn LLMPort,
    (id, unit): (&str, usize),
    (sentence, body): (&str, &str),
) -> Result<ClaimId> {
    let id = ClaimId(id.to_owned());
    let location = ClaimLocation::locate(unit, UnitKind::Block, "body", sentence, body);
    ledger
        .append(
            &id,
            ClaimEvent::Extracted {
                location,
                statement: sentence.to_owned(),
            },
        )
        .await?;
    ledger
        .append(&id, ClaimEvent::Checked(checked(llm)))
        .await?;
    Ok(id)
}

fn finding(unit: usize, statement: &str) -> Finding {
    Finding {
        unit,
        quote: statement.into(),
        statement: statement.into(),
        verdict: ClaimVerdict::Supported,
        reason: String::new(),
        evidence: Vec::new(),
        supporting_quote: None,
        resolution: None,
        trust: None,
        label: None,
        class: None,
        claim_id: None,
        two_pass: Default::default(),
    }
}

/// A report of `units` units with one statement each.
fn report(units: usize) -> LessonVerificationReport {
    LessonVerificationReport {
        policy: POLICY.to_string(),
        lesson_id: String::new(),
        content_sha256: String::new(),
        checker_model: "fixture".into(),
        checked_at: 0,
        sources: Vec::new(),
        coverage: (0..units)
            .map(|index| UnitClaims {
                index,
                claims: vec![Claim {
                    quote: format!("s{index}"),
                    statement: format!("s{index}"),
                }],
                non_factual_reason: String::new(),
            })
            .collect(),
        coverage_audit: (0..units)
            .map(|index| CoverageUnit {
                index,
                complete: true,
                reason: "Complete.".into(),
                missing_assertions: Vec::new(),
            })
            .collect(),
        findings: (0..units)
            .map(|unit| finding(unit, &format!("s{unit}")))
            .collect(),
        executions: Vec::new(),
        confirmed_examples: None,
        issues: Vec::new(),
        retrieval_mode: "lexical".into(),
        embedding_model: None,
        page_budget: None,
        unexecuted_languages: Vec::new(),
        replaced_findings: Vec::new(),
    }
}

/// A schema of the test's own under the kept promise, for tests of what the
/// schema is not about.
pub(super) fn loose() -> LessonShape<'static> {
    static POLICY: std::sync::LazyLock<shape::FixedSchema> =
        std::sync::LazyLock::new(|| shape::FixedSchema(json!({"type":"object"})));
    static SCHEMA: std::sync::LazyLock<shape::LessonSchema> =
        std::sync::LazyLock::new(|| POLICY.schema(shape::Phase::Kept, 0));
    LessonShape::new(&SCHEMA, &*POLICY)
}

/// Why the lesson without the units that go could not be published.
fn thin(lesson: &Value, going: &Going) -> Option<String> {
    shape::policy().floor(lesson, going.units())
}

fn gap(before: Option<&str>, after: Option<&str>) -> Gap {
    Gap {
        field: "body".into(),
        before: before.map(str::to_owned),
        after: after.map(str::to_owned),
    }
}

#[test]
fn the_units_that_go_leave_the_lesson_and_the_others_move_up() {
    let going = Going::new([1, 4]);
    assert_eq!(
        [0, 1, 2, 3, 4, 5].map(|unit| going.after(unit)),
        [Some(0), None, Some(1), Some(2), None, Some(3)]
    );
    let out = going.out_of(&lesson());
    let titles: Vec<&str> = out["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["First idea", "Second idea", "Recap"]);
    assert_eq!(out["questions"].as_array().unwrap().len(), 1);
    assert_eq!(out["questions"][0]["prompt"], "And?");
}

#[test]
fn a_review_of_the_lesson_without_them_comes_back_to_the_sections_places() {
    let going = Going::new([1]);
    let mut reviewed = going.out_of(&lesson());
    reviewed["blocks"][2]["body"] = json!("Done. Over. Reviewed.");

    let merged = going.back_in(&lesson(), &reviewed).unwrap();

    assert_eq!(merged["blocks"][1], lesson()["blocks"][1]);
    assert_eq!(merged["blocks"][3]["body"], "Done. Over. Reviewed.");
    // A review that added a section did not read the lesson as it will be.
    reviewed["blocks"]
        .as_array_mut()
        .unwrap()
        .push(block("recap", "Extra", "Extra. Words."));
    assert!(going.back_in(&lesson(), &reviewed).is_none());
}

#[test]
fn a_section_that_took_a_cut_with_fewer_than_two_teaching_sentences_left_is_too_little() {
    let mut lesson = lesson();
    lesson["blocks"][0]["body"] = json!("One.");
    lesson["blocks"][1]["body"] = json!("Alpha.");
    let mut cuts = Cuts::default();
    cuts.take(0, "First idea".into(), (Vec::new(), false));
    // Only the section that took a cut is judged.
    assert_eq!(too_little(&lesson, &cuts, shape::policy()), [0]);

    // Headings and code are no teaching; two sentences are enough.
    lesson["blocks"][0]["body"] = json!("### Title\n```python\nprint(1)\n```\nOne.\nTwo.");
    assert!(too_little(&lesson, &cuts, shape::policy()).is_empty());

    // A check whose answer went is too little however much is left.
    cuts.take(3, "Recap".into(), (Vec::new(), true));
    assert_eq!(too_little(&lesson, &cuts, shape::policy()), [3]);
    // A question is never too little: its cut leaves its options.
    cuts.take(4, "Which?".into(), (Vec::new(), true));
    assert_eq!(too_little(&lesson, &cuts, shape::policy()), [3]);
}

#[test]
fn a_lesson_that_loses_more_than_a_third_of_its_sections_or_its_first_explanation_is_thin() {
    let lesson = lesson();
    assert!(thin(&lesson, &Going::default()).is_none());
    assert!(thin(&lesson, &Going::new([1])).is_none());
    let two = thin(&lesson, &Going::new([1, 3])).unwrap();
    assert!(two.contains("2 of its 4 sections"), "{two}");
    let first = thin(&lesson, &Going::new([0])).unwrap();
    assert!(first.contains("first explanation"), "{first}");
    let questions = thin(&lesson, &Going::new([4, 5])).unwrap();
    assert!(questions.contains("its last quiz question"), "{questions}");
    // A third exactly is not more than a third.
    let three = json!({"blocks":[
        block("explanation","A","One. Two."), block("worked_example","B","One. Two."),
        block("recap","C","One. Two."),
    ],"questions":[question("Which?")]});
    assert!(thin(&three, &Going::new([1])).is_none());
    assert!(thin(&three, &Going::new([1, 2])).is_some());
}

#[test]
fn a_lesson_the_cuts_would_leave_without_a_question_of_a_kind_it_asks_is_thin() {
    // The retry of 2026-11-03 took the only practice question out and could
    // not save the lesson: a lesson is saved only with every kind of question.
    let asked = |kind: &str, prompt: &str| {
        let mut question = question(prompt);
        question["kind"] = json!(kind);
        question
    };
    let mut lesson = lesson();
    lesson["questions"] = json!([
        asked("practice", "P?"),
        asked("quiz", "Q1?"),
        asked("quiz", "Q2?"),
        asked("test", "T?"),
    ]);

    let practice = thin(&lesson, &Going::new([4])).unwrap();
    assert!(practice.contains("practice question"), "{practice}");
    // A kind that keeps a question of its own is no loss.
    assert!(thin(&lesson, &Going::new([5])).is_none());
}

#[test]
fn a_cut_remembers_the_sentences_kept_beside_it() {
    assert_eq!(
        gaps(
            "body",
            "Keep a. Cut b. Cut c. Keep d. Cut e.",
            "Keep a. Keep d."
        ),
        [
            gap(Some("Keep a."), Some("Keep d.")),
            gap(Some("Keep d."), None)
        ]
    );
    assert_eq!(
        gaps("body", "Cut. Keep.", "Keep."),
        [gap(None, Some("Keep."))]
    );
    assert!(gaps("body", "Same.", "Same.").is_empty());
}

#[test]
fn the_removal_of_a_code_block_leaves_a_gap_between_the_sentences_beside_it() {
    assert_eq!(
        gap_around_code(
            "body",
            "Run it.\n\n```python\nprint(1)\n```\n\nThe output is 1.",
            "Run it.\n\nThe output is 1."
        ),
        [gap(Some("Run it."), Some("The output is 1."))]
    );
    // Code beside the cut is no sentence to change.
    assert_eq!(
        gaps(
            "body",
            "Intro.\n```text\nsome code. more\n```\nCut me.\nOutro.",
            "Intro.\n```text\nsome code. more\n```\nOutro."
        ),
        [gap(Some("Intro."), Some("Outro."))]
    );
}

#[test]
fn a_cut_that_takes_the_answer_of_a_check_is_known() {
    let body = |answer: &str| format!("Predict it.\n\n### Check your thinking\n\n{answer}");
    let answer = "The lid bulges. It is because of pressure.";
    assert!(answer_lost(
        &body(answer),
        &body("It is because of pressure.")
    ));
    assert!(!answer_lost(&body(answer), &body("The lid bulges.")));
    assert!(!answer_lost(
        "No reveal. Nothing to lose.",
        "Nothing to lose."
    ));
}

#[test]
fn the_report_without_the_sections_that_go_is_numbered_as_the_lesson_is() {
    let without = report(6).without(&Going::new([1, 4]));

    let coverage: Vec<usize> = without.coverage.iter().map(|unit| unit.index).collect();
    assert_eq!(coverage, [0, 1, 2, 3]);
    let audit: Vec<usize> = without
        .coverage_audit
        .iter()
        .map(|unit| unit.index)
        .collect();
    assert_eq!(audit, [0, 1, 2, 3]);
    let findings: Vec<(usize, &str)> = without
        .findings
        .iter()
        .map(|finding| (finding.unit, finding.statement.as_str()))
        .collect();
    assert_eq!(findings, [(0, "s0"), (1, "s2"), (2, "s3"), (3, "s5")]);
}

#[tokio::test]
async fn taking_the_sections_that_go_out_moves_the_claims_of_the_others_with_the_lesson(
) -> Result<()> {
    let llm = model(&[]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    let kept = standing(
        &mut ledger,
        &llm,
        ("second", 2),
        ("Red.", "Red. Green. Blue."),
    )
    .await?;
    let last = standing(&mut ledger, &llm, ("recap", 3), ("Done.", "Done. Over.")).await?;
    let cut = standing(
        &mut ledger,
        &llm,
        ("example", 1),
        ("Alpha.", "Alpha. Beta. Gamma."),
    )
    .await?;
    // The pass before left the claims of the section that goes.
    ledger
        .append(
            &cut,
            ClaimEvent::Superseded {
                successor: None,
                by: Actor::Machine,
            },
        )
        .await?;
    ledger.cuts.go(1, "Example".into());

    let lesson = lesson();
    let (raw, out, report) = excise(
        &llm,
        &mut ledger,
        lesson.to_string(),
        lesson.clone(),
        report(6),
    )
    .await?;

    assert_eq!(out["blocks"].as_array().unwrap().len(), 3);
    assert!(!raw.contains("Example"));
    assert_eq!(ledger.entry(&kept).unwrap().location().unit_index, 1);
    assert_eq!(ledger.entry(&last).unwrap().location().unit_index, 2);
    assert!(matches!(
        ledger.entry(&cut).unwrap().state(),
        ClaimState::Removed {
            by: Actor::Machine,
            ..
        }
    ));
    assert_eq!(ledger.cuts, Cuts::default());
    assert_eq!(report.coverage.len(), 5);
    assert_eq!(report.findings.len(), 5);
    Ok(())
}

#[tokio::test]
async fn the_ledger_pairs_the_findings_a_lesson_keeps_with_their_own_claims() -> Result<()> {
    let llm = model(&[]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    let lesson = lesson();
    let mut pass = report(3);
    for (finding, sentence) in pass.findings.iter_mut().zip(["One.", "Alpha.", "Red."]) {
        finding.quote = sentence.into();
        finding.statement = sentence.into();
    }
    let statements: Vec<(usize, Claim)> = pass
        .findings
        .iter()
        .map(|finding| {
            let (quote, statement) = (finding.quote.clone(), finding.statement.clone());
            (finding.unit, Claim { quote, statement })
        })
        .collect();
    let extracted = ledger.extracted(&lesson, &statements, &[]);
    let matched = ledger.match_pass(&lesson, &extracted);
    let policy = lattice_core::domain::learning::acceptance::StandardPolicy::new();
    ledger
        .identify(&policy, &lesson, extracted, matched)
        .await?;
    let claims = ledger.finding_claims().to_vec();
    ledger.cuts.go(1, "Example".into());

    let (_, _, report) =
        excise(&llm, &mut ledger, lesson.to_string(), lesson.clone(), pass).await?;

    // What binds a resolution to a finding is its place beside its claim.
    let statements: Vec<&str> = report
        .findings
        .iter()
        .map(|finding| finding.statement.as_str())
        .collect();
    assert_eq!(statements, ["One.", "Red."]);
    assert_eq!(
        ledger.finding_claims(),
        [claims[0].clone(), claims[2].clone()]
    );
    Ok(())
}

#[tokio::test]
async fn a_lesson_nothing_goes_from_is_left_as_it_is() -> Result<()> {
    let llm = model(&[]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    let lesson = lesson();

    let (raw, out, report) = excise(
        &llm,
        &mut ledger,
        lesson.to_string(),
        lesson.clone(),
        report(6),
    )
    .await?;

    assert_eq!((raw, out), (lesson.to_string(), lesson));
    assert_eq!(report.coverage.len(), 6);
    Ok(())
}

#[tokio::test]
async fn a_lesson_that_loses_its_first_explanation_stops_with_the_reason_that_names_the_sources(
) -> Result<()> {
    let llm = model(&[]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    ledger.cuts.go(0, "First idea".into());
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;

    let settled = settle(
        &llm,
        ("{}", loose(), 4_000),
        (&references, &[]),
        &lesson().to_string(),
        &mut ledger,
    )
    .await?;

    let Settled::Stopped(error) = settled else {
        panic!("the lesson should stop");
    };
    let message = error.to_string();
    assert!(
        message.contains("The sources do not support enough of this lesson"),
        "{message}"
    );
    assert!(message.contains("first explanation"), "{message}");
    assert!(llm.prompts.lock().unwrap().is_empty());
    Ok(())
}

/// Claims that ended removed by the machine, as the cuts of the passes
/// before leave them.
async fn removed(ledger: &mut ClaimLedger, llm: &dyn LLMPort, claims: &[(&str, usize, &str)]) {
    for (id, unit, sentence) in claims {
        let id = standing(ledger, llm, (*id, *unit), (*sentence, *sentence))
            .await
            .unwrap();
        ledger
            .append(
                &id,
                ClaimEvent::Superseded {
                    successor: None,
                    by: Actor::Machine,
                },
            )
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn a_lesson_every_statement_of_which_ended_removed_stops_with_the_reason_that_names_the_sources(
) -> Result<()> {
    let llm = model(&[]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    removed(
        &mut ledger,
        &llm,
        &[("first", 0, "One."), ("second", 2, "Red.")],
    )
    .await;
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;

    let settled = settle(
        &llm,
        ("{}", loose(), 4_000),
        (&references, &[]),
        &lesson().to_string(),
        &mut ledger,
    )
    .await?;

    let Settled::Stopped(error) = settled else {
        panic!("the lesson should stop");
    };
    assert_eq!(
        error.to_string(),
        "The sources do not support enough of this lesson: every statement in it would have to be taken out."
    );
    assert!(llm.prompts.lock().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_lesson_with_a_statement_left_is_settled_as_before_whatever_was_removed() -> Result<()> {
    let llm = model(&[]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    removed(&mut ledger, &llm, &[("first", 0, "One.")]).await;
    standing(&mut ledger, &llm, ("second", 2), ("Red.", "Red.")).await?;
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;

    let settled = settle(
        &llm,
        ("{}", loose(), 4_000),
        (&references, &[]),
        &lesson().to_string(),
        &mut ledger,
    )
    .await?;

    assert!(matches!(settled, Settled::Done));
    Ok(())
}

#[tokio::test]
async fn a_section_left_with_too_little_goes_and_the_checks_let_go_of_its_statements_first(
) -> Result<()> {
    let llm = model(&[]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    let mut lesson = lesson();
    lesson["blocks"][1]["body"] = json!("Alpha.");
    ledger.cuts.take(
        1,
        "Example".into(),
        (vec![gap(None, Some("Alpha."))], false),
    );
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;

    let settled = settle(
        &llm,
        ("{}", loose(), 4_000),
        (&references, &[]),
        &lesson.to_string(),
        &mut ledger,
    )
    .await?;

    // The same draft is verified again with the section left out of the
    // checks; no flow repair is spent on a section that goes.
    let Settled::Again(raw) = settled else {
        panic!("the draft should be verified again");
    };
    assert_eq!(raw, lesson.to_string());
    assert!(ledger.cuts.going.contains_key(&1));
    assert!(ledger.cuts.mended.is_empty());
    assert!(llm.prompts.lock().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_lesson_with_nothing_cut_is_settled_without_a_call() -> Result<()> {
    let llm = model(&[]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;

    let settled = settle(
        &llm,
        ("{}", loose(), 4_000),
        (&references, &[]),
        &lesson().to_string(),
        &mut ledger,
    )
    .await?;

    assert!(matches!(settled, Settled::Done));
    assert!(llm.prompts.lock().unwrap().is_empty());
    Ok(())
}

/// A section the cut left with a claim beside the gap.
const AFTER_CUT: &str =
    "An intro to the whole idea. This follows from it. Kept claim. Closing note.";

fn mended_section() -> (Value, Taken, Vec<Protected>) {
    let section = block("explanation", "Idea", AFTER_CUT);
    let taken = Taken {
        label: "Idea".into(),
        gaps: vec![gap(
            Some("An intro to the whole idea."),
            Some("This follows from it."),
        )],
        answer_lost: false,
    };
    let protected = vec![Protected {
        unit: 0,
        field: "body".into(),
        quote: "Kept claim.".into(),
        released: false,
    }];
    (section, taken, protected)
}

async fn mend(
    llm: &ScriptedModel,
    taken: &Taken,
    protected: &[Protected],
) -> Result<Option<Value>> {
    mend_in(llm, taken, protected, &json!({})).await
}

async fn mend_in(
    llm: &ScriptedModel,
    taken: &Taken,
    protected: &[Protected],
    lesson: &Value,
) -> Result<Option<Value>> {
    let (section, _, _) = mended_section();
    mend_section(
        llm,
        (&json!({"type":"object"}), 4_000),
        (0, true, &section),
        (taken, protected),
        (lesson, &Default::default()),
    )
    .await
}

#[tokio::test]
async fn a_flow_repair_that_inserts_a_sentence_the_lesson_already_has_is_asked_once_more(
) -> Result<()> {
    let (section, taken, protected) = mended_section();
    let nothing = json!({"edits":[]});
    let own = json!({"edits":[
        {"path":"/blocks/0/body","before":"This follows from it.","after":"An intro to the whole idea."}
    ]});
    let llm = model(&[own, nothing.clone()]);
    assert_eq!(
        mend(&llm, &taken, &protected).await?,
        None,
        "the section already has An intro to the whole idea."
    );
    let prompts = llm.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains(CORRECTION_LEAD), "{}", prompts[1]);

    let elsewhere = "Another section says exactly this.";
    let lesson = json!({"blocks":[section, block("explanation","Other",elsewhere)],"questions":[]});
    let other = json!({"edits":[
        {"path":"/blocks/0/body","before":"This follows from it.","after":elsewhere}
    ]});
    let llm = model(&[other, nothing]);
    assert_eq!(
        mend_in(&llm, &taken, &protected, &lesson).await?,
        None,
        "another section already has the sentence"
    );
    assert_eq!(llm.prompts.lock().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_flow_repair_changes_the_sentences_beside_a_gap_and_nothing_else() -> Result<()> {
    let (_, taken, protected) = mended_section();
    let llm = model(&[json!({"edits":[
        {"path":"/blocks/0/body","before":"This follows from it.","after":"That follows from the setup."},
        {"path":"/blocks/0/body","before":"Closing note.","after":"Closing remark."}
    ]})]);

    let repaired = mend(&llm, &taken, &protected).await?.unwrap();

    // The edit beside the gap stands; the one outside it is taken back.
    assert_eq!(
        repaired["body"],
        "An intro to the whole idea. That follows from the setup. Kept claim. Closing note."
    );
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert!(prompts[0].contains("verifiedSentences"));
    assert!(prompts[0].contains("Kept claim."));
    assert!(prompts[0].contains("Add no statement of fact"));
    Ok(())
}

#[tokio::test]
async fn a_flow_repair_that_rewrites_a_sentence_a_claim_stands_on_is_no_repair() -> Result<()> {
    let (_, mut taken, protected) = mended_section();
    taken.gaps = vec![gap(Some("This follows from it."), Some("Kept claim."))];
    let llm = model(&[json!({"edits":[
        {"path":"/blocks/0/body","before":"Kept claim.","after":"Changed claim."}
    ]})]);

    // The change is put back, and nothing is recorded for it: the section
    // keeps the text as cut.
    assert_eq!(mend(&llm, &taken, &protected).await?, None);
    Ok(())
}

#[tokio::test]
async fn a_flow_repair_that_leaves_the_section_no_lesson_leaves_it_as_cut() -> Result<()> {
    let section = json!({"kind":"explanation","title":"Idea","body":AFTER_CUT,"rubric":[{"title":"One","description":"Observable.","dimension":"recall"}],"sourceIndex":null,"quote":"","citations":[]});
    let taken = Taken {
        label: "Idea".into(),
        gaps: vec![Gap {
            field: "rubric/0/dimension".into(),
            before: Some("recall".into()),
            after: None,
        }],
        answer_lost: false,
    };
    let llm = model(&[json!({"edits":[
        {"path":"/blocks/0/rubric/0/dimension","before":"recall","after":"mastery"}
    ]})]);

    let repaired = mend_section(
        &llm,
        (&json!({"type":"object"}), 4_000),
        (0, true, &section),
        (&taken, &[]),
        (&json!({}), &Default::default()),
    )
    .await?;

    assert_eq!(repaired, None);
    assert_eq!(llm.prompts.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_flow_repair_that_asks_for_nothing_changes_nothing() -> Result<()> {
    let (_, taken, protected) = mended_section();
    let llm = model(&[json!({"edits":[],"citations":[]})]);

    assert_eq!(mend(&llm, &taken, &protected).await?, None);
    Ok(())
}

#[tokio::test]
async fn a_gap_whose_neighbours_are_gone_is_no_call() -> Result<()> {
    let (_, mut taken, protected) = mended_section();
    taken.gaps = vec![gap(Some("A sentence since rewritten."), None)];
    let llm = model(&[]);

    assert_eq!(mend(&llm, &taken, &protected).await?, None);
    assert!(llm.prompts.lock().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn each_section_that_took_a_cut_gets_one_flow_repair_and_never_a_second() -> Result<()> {
    let llm = model(&[json!({"edits":[],"citations":[]})]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    let mut lesson = lesson();
    lesson["blocks"][2]["body"] = json!(AFTER_CUT);
    standing(&mut ledger, &llm, ("claim", 2), ("Kept claim.", AFTER_CUT)).await?;
    ledger.cuts.take(
        2,
        "Second idea".into(),
        (
            vec![gap(
                Some("An intro to the whole idea."),
                Some("This follows from it."),
            )],
            false,
        ),
    );
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;

    for _ in 0..2 {
        let settled = settle(
            &llm,
            ("{}", loose(), 4_000),
            (&references, &[]),
            &lesson.to_string(),
            &mut ledger,
        )
        .await?;
        assert!(matches!(settled, Settled::Done));
    }

    assert_eq!(
        llm.prompts.lock().unwrap().len(),
        1,
        "the section is mended once, not at every settling"
    );
    assert!(ledger.cuts.mended[&2].checked);
    Ok(())
}

#[tokio::test]
async fn a_flow_repair_that_cannot_stand_leaves_the_section_as_cut() -> Result<()> {
    let llm = model(&[json!({"edits":[
        {"path":"/blocks/0/body","before":"Kept claim.","after":"Changed claim."}
    ]})]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    let mut lesson = lesson();
    lesson["blocks"][0]["body"] = json!(AFTER_CUT);
    standing(&mut ledger, &llm, ("claim", 0), ("Kept claim.", AFTER_CUT)).await?;
    ledger.cuts.take(
        0,
        "First idea".into(),
        (
            vec![gap(Some("This follows from it."), Some("Kept claim."))],
            false,
        ),
    );
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;

    let settled = settle(
        &llm,
        ("{}", loose(), 4_000),
        (&references, &[]),
        &lesson.to_string(),
        &mut ledger,
    )
    .await?;

    assert!(matches!(settled, Settled::Done));
    assert!(ledger.cuts.mended[&0].checked);
    Ok(())
}

#[tokio::test]
async fn a_flow_repair_that_wrote_a_statement_is_taken_back_after_the_pass() -> Result<()> {
    let llm = model(&[]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    let before = block("explanation", "First idea", "One. Two. Three.");
    let old = standing(&mut ledger, &llm, ("old", 0), ("One.", "One. Two. Three.")).await?;
    ledger.cuts.mended.insert(
        0,
        Mended {
            label: "First idea".into(),
            before: before.clone(),
            known: vec![old],
            checked: false,
        },
    );
    // The pass after the repair found a statement the repair wrote.
    let mut candidate = lesson();
    candidate["blocks"][0]["body"] = json!("One. Two. Three. And so on.");
    standing(
        &mut ledger,
        &llm,
        ("fresh", 0),
        ("And so on.", "One. Two. Three. And so on."),
    )
    .await?;

    let restored = unmend(&mut ledger, &candidate).await?.unwrap();

    let restored: Value = serde_json::from_str(&restored)?;
    assert_eq!(restored["blocks"][0], before);
    assert_eq!(restored["blocks"][1], candidate["blocks"][1]);
    assert!(ledger.cuts.mended[&0].checked);
    // It is judged once: the next pass does not take it back again.
    assert!(unmend(&mut ledger, &candidate).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn a_flow_repair_that_wrote_no_statement_stands() -> Result<()> {
    let llm = model(&[]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    let old = standing(&mut ledger, &llm, ("old", 0), ("One.", "One. Two. Three.")).await?;
    ledger.cuts.mended.insert(
        0,
        Mended {
            label: "First idea".into(),
            before: block("explanation", "First idea", "One. Three."),
            known: vec![old],
            checked: false,
        },
    );

    assert!(unmend(&mut ledger, &lesson()).await?.is_none());
    assert!(ledger.cuts.mended[&0].checked);
    Ok(())
}

#[test]
fn what_was_recorded_of_a_section_that_is_no_longer_there_is_dropped() {
    let mut cuts = Cuts::default();
    cuts.take(0, "First idea".into(), (Vec::new(), false));
    cuts.take(1, "Example".into(), (Vec::new(), false));
    cuts.go(1, "Example".into());
    let mut restructured = lesson();
    restructured["blocks"]
        .as_array_mut()
        .unwrap()
        .insert(1, block("explanation", "New", "One. Two."));

    cuts.keep_current(&restructured);

    // Unit 1 is another section now; unit 0 is the one it was recorded for.
    assert_eq!(cuts.taken.keys().copied().collect::<Vec<_>>(), [0]);
    assert!(cuts.going.is_empty());
}

#[tokio::test]
async fn settle_does_not_rerun_teaching_review_on_cut_lesson_when_surviving_sections_already_settled(
) -> Result<()> {
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;

    let defect_review = json!({
        "issues": [],
        "blockChecks": crate::teaching_review::fixture_checks(vec![
            json!({
                "index": 0,
                "passageId": "section-0-passage-0",
                "finding": "Section 0 promises expert certification of the learner's work.",
                "hasDefect": true,
                "concerns": ["promise"],
            }),
            json!({
                "index": 1,
                "passageId": "section-1-passage-0",
                "finding": "Section 1 is good.",
                "hasDefect": false,
            }),
            json!({
                "index": 2,
                "passageId": "section-2-passage-0",
                "finding": "Section 2 is good.",
                "hasDefect": false,
            }),
        ])
    })
    .to_string();

    let actionable_evidence = json!({"decisions": [
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

    let fix_edit = json!({
        "edits": [
            {
                "path": "/blocks/0/body",
                "before": "One. Two. Three.",
                "after": "One. Two. Refined.",
            }
        ]
    })
    .to_string();

    let llm = model(&[
        serde_json::from_str(&defect_review)?,
        serde_json::from_str(&actionable_evidence)?,
        serde_json::from_str(&fix_edit)?,
        serde_json::from_str(&clean_review(&[0], &[verdict("reviewer/1", "resolved")]))?,
    ]);

    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    let mut lesson = lesson();
    lesson["questions"] = json!([]);
    ledger.cuts.go(1, "Example".into());

    let settled = settle(
        &llm,
        ("{}", loose(), 4_000),
        (&references, &[]),
        &lesson.to_string(),
        &mut ledger,
    )
    .await?;

    let Settled::Again(edited_str) = settled else {
        panic!("expected Settled::Again on first pass");
    };

    let settled_again = settle(
        &llm,
        ("{}", loose(), 4_000),
        (&references, &[]),
        &edited_str,
        &mut ledger,
    )
    .await?;

    assert!(matches!(settled_again, Settled::Done));
    assert_eq!(llm.outputs.lock().unwrap().len(), 0);

    Ok(())
}

pub(super) fn shaped_block(kind: &str, title: &str) -> Value {
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
    json!({"kind":kind,"title":title,"body":body,"rubric":rubric,"sourceIndex":null,"quote":"","citations":[]})
}

pub(super) fn shaped_question(kind: &str, prompt: &str) -> Value {
    json!({"kind":kind,"prompt":prompt,"options":["One","Two","Three","Four"],"correctIndex":0,
        "rationales":["","Two reverses the order.","Three mistakes the cause for the effect.","Four states it backwards."],
        "explanation":"Because.","sourceIndex":null,"quote":"","citations":[]})
}

/// A lesson in the writer's full shape: eleven sections, the checks at units 3
/// and 6, the worked examples at units 1 and 5; six questions.
pub(super) fn shaped_lesson() -> Value {
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

#[tokio::test]
async fn a_check_whose_only_worked_example_goes_goes_with_it() -> Result<()> {
    let llm = model(&[]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    let mut lesson = shaped_lesson();
    lesson["blocks"][1]["body"] = json!("Alpha.");
    ledger
        .cuts
        .take(1, "First example".into(), (Vec::new(), false));
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;

    let settled = settle(
        &llm,
        ("{}", loose(), 4_000),
        (&references, &[]),
        &lesson.to_string(),
        &mut ledger,
    )
    .await?;

    // The check after the example would test what the lesson has not shown.
    assert_eq!(
        ledger.cuts.going.keys().copied().collect::<Vec<_>>(),
        [1, 3]
    );
    assert!(matches!(settled, Settled::Again(_)));
    Ok(())
}

#[tokio::test]
async fn the_review_after_cuts_with_no_claim_left_asks_for_no_count_the_cuts_spent() {
    let mut lesson = shaped_lesson();
    lesson["questions"].as_array_mut().unwrap().pop();
    let without = Going::new([2]).out_of(&lesson);
    let questions = without["questions"].as_array().unwrap().len();
    let solved = json!({"answers":(0..questions).map(|index| json!({"index":index,"reason":"The fixture's key.","correctIndices":[0],"answer":"","blankAnswers":[],"matches":[],"orderedValues":[]})).collect::<Vec<_>>()});
    let judged = json!({"checks":(0..questions).map(|index| json!({"id":format!("new-{index}"),"finding":"","hasDefect":false,"repeats":[]})).collect::<Vec<_>>()});
    let checks = (0..without["blocks"].as_array().unwrap().len())
        .map(|index| json!({"index":index,"passageId":format!("section-{index}-passage-0"),"finding":"Reviewed; no teaching defect was found.","hasDefect":false}))
        .collect();
    let review = json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(checks)});
    let llm = model(&[solved, judged, review]);
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources).unwrap();

    let _ = crate::lessons::content_verification::settle_after_cuts(
        &llm,
        "{}",
        shape::policy(),
        lesson.to_string(),
        &references,
        &[(2, "Not to be confused with")],
    )
    .await;

    let prompts = llm.prompts.lock().unwrap();
    assert!(
        !prompts
            .iter()
            .any(|prompt| prompt.contains("Give the complete corrected material")),
        "the whole lesson was asked for again to restore the writer's counts"
    );
}

/// A model whose window is below any flow repair's prompt.
struct TinyWindow;

#[async_trait::async_trait]
impl LLMPort for TinyWindow {
    async fn complete(
        &self,
        _request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        Ok(lattice_core::application::ports::llm_port::CompletionResponse::from_text("{}"))
    }
    fn model_name(&self) -> &str {
        "tiny-window"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        200
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_flow_repair_that_does_not_fit_keeps_the_cut_text() -> Result<()> {
    let llm = TinyWindow;
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    let mut lesson = lesson();
    lesson["blocks"][0]["body"] = json!(AFTER_CUT);
    standing(&mut ledger, &llm, ("claim", 0), ("Kept claim.", AFTER_CUT)).await?;
    ledger.cuts.take(
        0,
        "First idea".into(),
        (
            vec![gap(
                Some("An intro to the whole idea."),
                Some("This follows from it."),
            )],
            false,
        ),
    );
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;

    let settled = settle(
        &llm,
        ("{}", loose(), 4_000),
        (&references, &[]),
        &lesson.to_string(),
        &mut ledger,
    )
    .await;

    assert!(matches!(settled, Ok(Settled::Done)), "{:?}", settled.err());
    assert!(ledger.cuts.mended[&0].checked);
    Ok(())
}

#[tokio::test]
async fn a_flow_repair_whose_edits_do_not_apply_is_asked_again() -> Result<()> {
    let (_, taken, protected) = mended_section();
    let absent = json!({"edits":[
        {"path":"/blocks/0/body","before":"Words the section never had.","after":"Anything."}
    ]});
    let applies = json!({"edits":[
        {"path":"/blocks/0/body","before":"This follows from it.","after":"That follows from the setup."}
    ]});
    let llm = model(&[absent, applies]);

    let repaired = mend(&llm, &taken, &protected).await;

    assert_eq!(
        repaired
            .ok()
            .flatten()
            .map(|section| section["body"].clone()),
        Some(json!(
            "An intro to the whole idea. That follows from the setup. Kept claim. Closing note."
        ))
    );
    Ok(())
}

#[test]
fn the_flow_repair_contract_reads_what_it_renders() {
    use crate::lessons::text_edits::{Cite, Edit, NamedUncite, Patch, Uncite};
    let edit = |finding: Option<&str>| Edit {
        finding: finding.map(str::to_owned),
        path: "/blocks/0/body".into(),
        before: json!("Old sentence."),
        after: json!("New sentence."),
    };
    let cite = |finding: Option<&str>, source_index: Option<u64>| Cite {
        finding: finding.map(str::to_owned),
        path: "/blocks/0/citations".into(),
        sentence: "New sentence.".into(),
        source_index,
        quote: "A quoted span.".into(),
    };
    lattice_core::application::services::model_contract::test_support::assert_round_trip::<
        FlowRepair,
    >(&Patch {
        edits: vec![edit(None), edit(Some("issue-0"))],
        citations: vec![cite(None, Some(0)), cite(Some("issue-0"), None)],
        uncite: vec![
            Uncite::Path("/blocks/0/citations/0".into()),
            Uncite::Named(NamedUncite {
                finding: "issue-0".into(),
                path: "/blocks/0/citations/1".into(),
            }),
        ],
    });
}
