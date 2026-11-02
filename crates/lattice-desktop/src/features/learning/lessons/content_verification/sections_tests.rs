#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
use super::*;
use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
use lattice_core::domain::learning::claim_review::{
    Actor, Check, CheckResult, Checker, ClaimEvent, ClaimLocation, ClaimState, EvidenceFingerprint,
    UnitKind,
};
use std::sync::Mutex;

fn block(kind: &str, title: &str, body: &str) -> Value {
    json!({"kind":kind,"title":title,"body":body,"rubric":[]})
}

fn question(prompt: &str) -> Value {
    json!({"kind":"quiz","prompt":prompt,"options":["One","Two","Three","Four"],"correctIndex":0,"explanation":"Because."})
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

fn model(replies: &[Value]) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(replies.iter().map(Value::to_string).collect()),
        prompts: Mutex::new(Vec::new()),
    }
}

fn sources() -> Vec<LearningSourceDto> {
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
        two_pass: Default::default(),
    }
}

/// A report of `units` units with one statement each.
fn report(units: usize) -> LessonVerificationReport {
    LessonVerificationReport {
        policy: POLICY.into(),
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
    }
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
    assert_eq!(too_little(&lesson, &cuts), [0]);

    // Headings and code are no teaching; two sentences are enough.
    lesson["blocks"][0]["body"] = json!("### Title\n```python\nprint(1)\n```\nOne.\nTwo.");
    assert!(too_little(&lesson, &cuts).is_empty());

    // A check whose answer went is too little however much is left.
    cuts.take(3, "Recap".into(), (Vec::new(), true));
    assert_eq!(too_little(&lesson, &cuts), [3]);
    // A question is never too little: its cut leaves its options.
    cuts.take(4, "Which?".into(), (Vec::new(), true));
    assert_eq!(too_little(&lesson, &cuts), [3]);
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
    assert!(questions.contains("all of its questions"), "{questions}");
    // A third exactly is not more than a third.
    let three = json!({"blocks":[
        block("explanation","A","One. Two."), block("worked_example","B","One. Two."),
        block("recap","C","One. Two."),
    ],"questions":[question("Which?")]});
    assert!(thin(&three, &Going::new([1])).is_none());
    assert!(thin(&three, &Going::new([1, 2])).is_some());
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
    let (raw, out, report) =
        excise(&mut ledger, lesson.to_string(), lesson.clone(), report(6)).await?;

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
async fn a_lesson_nothing_goes_from_is_left_as_it_is() -> Result<()> {
    let llm = model(&[]);
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    let lesson = lesson();

    let (raw, out, report) =
        excise(&mut ledger, lesson.to_string(), lesson.clone(), report(6)).await?;

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
        ("{}", &json!({"type":"object"}), 4_000),
        &references,
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
        ("{}", &json!({"type":"object"}), 4_000),
        &references,
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
        ("{}", &json!({"type":"object"}), 4_000),
        &references,
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
        ("{}", &json!({"type":"object"}), 4_000),
        &references,
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
        ("{}", &json!({"type":"object"}), 4_000),
        &references,
        &lesson().to_string(),
        &mut ledger,
    )
    .await?;

    assert!(matches!(settled, Settled::Done));
    assert!(llm.prompts.lock().unwrap().is_empty());
    Ok(())
}

/// A section the cut left with a claim beside the gap.
const AFTER_CUT: &str = "Intro. This follows from it. Kept claim. Closing note.";

fn mended_section() -> (Value, Taken, Vec<Protected>) {
    let section = block("explanation", "Idea", AFTER_CUT);
    let taken = Taken {
        label: "Idea".into(),
        gaps: vec![gap(Some("Intro."), Some("This follows from it."))],
        answer_lost: false,
    };
    let protected = vec![Protected {
        unit: 0,
        field: "body".into(),
        quote: "Kept claim.".into(),
    }];
    (section, taken, protected)
}

async fn mend(
    llm: &ScriptedModel,
    taken: &Taken,
    protected: &[Protected],
) -> Result<Option<Value>> {
    let (section, _, _) = mended_section();
    mend_section(
        llm,
        (&json!({"type":"object"}), 4_000),
        (0, true, &section),
        (taken, protected),
    )
    .await
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
        "Intro. That follows from the setup. Kept claim. Closing note."
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

    let error = mend(&llm, &taken, &protected).await.unwrap_err();

    assert!(rejected(&error), "{error}");
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
            vec![gap(Some("Intro."), Some("This follows from it."))],
            false,
        ),
    );
    let sources = sources();
    let references = ReferenceCollection::lexical(&sources)?;

    for _ in 0..2 {
        let settled = settle(
            &llm,
            ("{}", &json!({"type":"object"}), 4_000),
            &references,
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
        ("{}", &json!({"type":"object"}), 4_000),
        &references,
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
