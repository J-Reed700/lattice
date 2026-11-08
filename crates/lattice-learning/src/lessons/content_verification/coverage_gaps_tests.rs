//! The coverage audit reports what the claim inventory leaves out. An
//! assertion the fidelity judge names as unrepresented is a finding for the
//! teaching review, not a claim: it is not checked, repaired or cut, and it
//! does not stop the lesson. The exception is a specific (a figure, a
//! quotation, an attribution) in a passage the inventory reads: it joins the
//! inventory in the first pass and gets the strict check (claim classes design
//! §3.4).
use super::*;
use crate::dto::{LearningBlockDto, LearningBlockKind, PreparedLearningLesson};
use crate::lesson_drafts;
use crate::review_findings::{FindingKind, FindingSite};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// An exercise that presupposes the false default without stating it in the
/// words the fixture extractor recognises, so extraction misses the premise.
const PREMISE: &str = "Exercise: assuming the reader strips field-name whitespace by default, write code that relies on that default.";
/// An exercise that presupposes a behaviour no supplied source mentions, the
/// shape of the live failure: the judge named a feature the sources lacked.
const UNSOURCED_PREMISE: &str = "Exercise: assuming the reader logs every parsed row to standard error by default, write code that silences that log.";
const UNSOURCED: &str = "DictReader logs every parsed row to standard error by default.";
/// A deliverable and rubric: the lesson's own choices, not assertions.
const RUBRIC: &str = "Deliverable: submit a markdown report. Rubric: Strong answers cite their sources; Adequate answers name the behaviour.";
/// An exercise that presupposes a figure the extractor missed.
const FIGURE_PREMISE: &str = "Exercise: assuming the reader buffers 100 rows before parsing, write code that sizes its memory for that buffer.";
const FIGURE: &str = "DictReader buffers 100 rows before parsing.";

struct JudgingModel {
    inner: Model,
    /// The mapper proposes an omission for this passage, as the live mapper
    /// did for instructions and rubrics; the judge answers independently.
    proposed_passage: &'static str,
    judge_reply: Value,
    /// The statement whose evidence check the test observes. When no source
    /// mentions it, the evidence judge answers unsupported, as it did live.
    watched_claim: &'static str,
    unsourced: bool,
    /// The extractor reads the exercise as the lesson's own instruction and
    /// records no claim on it, as it did live: the premise it presupposes is
    /// missed, and only the coverage judge names it. (A claim on the whole
    /// exercise sentence that the checker supports would protect that
    /// sentence from the repair: claim sources design §10.2.)
    premise_missed: bool,
    judged: AtomicUsize,
    judged_watched_claim: AtomicBool,
}

#[async_trait::async_trait]
impl LLMPort for JudgingModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
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

impl JudgingModel {
    fn new(proposed_passage: &'static str, missing: &[&str], watched_claim: &'static str) -> Self {
        let reason = if missing.is_empty() {
            "Deliverables and grading levels are the lesson's own choices."
        } else {
            "The exercise presupposes a default behaviour the inventory does not state."
        };
        Self {
            inner: Model::new(),
            proposed_passage,
            judge_reply: json!({"missingAssertions":missing,"reason":reason}),
            watched_claim,
            unsourced: watched_claim == UNSOURCED || watched_claim == FIGURE,
            premise_missed: !missing.is_empty(),
            judged: AtomicUsize::new(0),
            judged_watched_claim: AtomicBool::new(false),
        }
    }
    async fn respond(&self, prompt: &str) -> Result<String> {
        if self.premise_missed && prompt.starts_with("Extract lesson claims.") {
            let mut response: Value = serde_json::from_str(&fixture_response(prompt).unwrap())?;
            for unit in context(prompt)["units"].as_array().unwrap() {
                let exercise = unit["passages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|passage| passage["text"] == self.proposed_passage);
                if exercise {
                    response["units"][format!("unit-{}", unit["index"])] = json!({"claims":[],"nonFactualReason":"An exercise instruction: the lesson's own task."});
                }
            }
            return Ok(response.to_string());
        }
        if prompt.starts_with("Audit inventory fidelity.") {
            let data = context(prompt);
            if data["targetPassage"]["text"] == self.proposed_passage {
                self.judged.fetch_add(1, Ordering::Relaxed);
                return Ok(self.judge_reply.to_string());
            }
        }
        if prompt.starts_with("Audit claim coverage independently.") {
            let mut response: Value = serde_json::from_str(&fixture_response(prompt).unwrap())?;
            for unit in context(prompt)["units"].as_array().unwrap() {
                for passage in unit["passages"].as_array().unwrap() {
                    if passage["text"] == self.proposed_passage {
                        let id = passage["id"].as_str().unwrap();
                        response[id]["missingClaims"] =
                            json!(["The passage's premise is not in the inventory."]);
                    }
                }
            }
            return Ok(response.to_string());
        }
        if prompt.starts_with("Source passages:")
            && prompt.contains(&format!("\n\nClaim: {}", self.watched_claim))
        {
            self.judged_watched_claim.store(true, Ordering::Relaxed);
            if self.unsourced {
                return Ok("unsupported\nReason: No supplied passage mentions this behaviour.\nSource passage: none".into());
            }
        }
        self.inner.respond(prompt).await
    }
}

fn lesson(second_body: &str) -> Value {
    let mut lesson = candidate(GOOD);
    lesson["blocks"].as_array_mut().unwrap().push(json!({
        "kind":"explanation","title":"Header exercise","body":second_body,"rubric":[]
    }));
    lesson
}

/// The 2026-10-14 live failure's shape: extraction missed a factual premise
/// embedded in an exercise, the mapper proposed the omission and the judge
/// named it. With no figure, quotation or attribution in it, the assertion is
/// reported on its section, as a finding for the teaching review, and is no
/// claim: no evidence check runs for it, nothing is repaired, the lesson is
/// not stopped and its text is untouched.
async fn a_named_assertion_is_a_finding_not_a_claim(
    premise: &'static str,
    statement: &'static str,
) -> Result<()> {
    let model = JudgingModel::new(premise, &[statement], statement);
    let original = lesson(premise);

    let (raw, report) = verify_and_repair(
        &model,
        "{}",
        &json!({}),
        original.to_string(),
        &[source()],
        3000,
    )
    .await?;

    assert!(!report.coverage_audit[1].complete);
    assert_eq!(
        report.coverage_audit[1].missing_assertions[0].statement,
        statement
    );
    assert!(
        report.coverage[1].claims.is_empty(),
        "the audit does not add to the inventory"
    );
    assert!(!model.judged_watched_claim.load(Ordering::Relaxed));
    assert!(report.findings.iter().all(|f| f.statement != statement));
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert!(!*model.inner.repaired.lock().unwrap());
    assert_eq!(serde_json::from_str::<Value>(&raw)?, original);

    let findings = report.coverage_findings(2);
    let [finding] = findings.as_slice() else {
        panic!("one finding for the section: {findings:?}");
    };
    assert_eq!(finding.kind, FindingKind::Teaching);
    assert_eq!(finding.site, Some(FindingSite::Section(1)));
    assert!(finding.note.contains(statement), "{}", finding.note);
    Ok(())
}

#[tokio::test]
async fn an_assertion_no_source_mentions_is_a_finding_not_a_claim() -> Result<()> {
    a_named_assertion_is_a_finding_not_a_claim(UNSOURCED_PREMISE, UNSOURCED).await
}

#[tokio::test]
async fn an_assertion_the_sources_contradict_is_a_finding_not_a_claim() -> Result<()> {
    a_named_assertion_is_a_finding_not_a_claim(PREMISE, BAD).await
}

/// A figure the extractor missed in an exercise is a statement the lesson
/// makes, and a source must back it: the audit's gap joins the inventory,
/// located at the judged passage, and the rule layer sorts it Sourced.
#[tokio::test]
async fn a_body_gap_with_a_figure_becomes_a_sourced_claim_and_gets_its_strict_check() -> Result<()>
{
    let model = JudgingModel::new(FIGURE_PREMISE, &[FIGURE], FIGURE);

    let report = verify(&model, &lesson(FIGURE_PREMISE), &[source()]).await?;

    let adopted: Vec<_> = report.coverage[1]
        .claims
        .iter()
        .filter(|claim| claim.statement == FIGURE)
        .collect();
    assert_eq!(adopted.len(), 1);
    assert_eq!(adopted[0].quote, FIGURE_PREMISE);
    assert!(report.coverage_audit[1].complete);
    assert!(
        report.coverage_findings(2).is_empty(),
        "a claim now, no finding"
    );
    assert!(model.judged_watched_claim.load(Ordering::Relaxed));
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.statement == FIGURE)
        .unwrap();
    assert_eq!(
        finding.class,
        Some(Classed {
            class: Class::Sourced,
            by: ClassifiedBy::Rule
        })
    );
    assert_eq!(finding.verdict, ClaimVerdict::Unsupported);
    assert!(!report.issues.is_empty(), "an unbacked figure is an issue");
    Ok(())
}

/// A question's stem and options, and a rubric, are not passages the inventory
/// reads, so a gap cannot be placed in them; a gap that names one is a finding.
#[test]
fn a_gap_in_a_question_or_an_option_is_never_adopted() {
    let candidate = json!({"blocks":[{"kind":"explanation","title":"A","body":GOOD,"rubric":[]}],
        "questions":[{"kind":"quiz","prompt":"Which buffers 100 rows?",
            "options":["It buffers 100 rows.","It buffers none."],"correctIndex":0,
            "explanation":"Because the reference says so."}]});
    let content = units(&candidate).unwrap();
    let mut inventory = Inventory {
        units: vec![
            UnitClaims {
                index: 0,
                claims: vec![],
                non_factual_reason: "A section.".into(),
            },
            UnitClaims {
                index: 1,
                claims: vec![],
                non_factual_reason: "A question.".into(),
            },
        ],
    };
    let gap = |passage_id: &str| MissingAssertion {
        passage_id: passage_id.into(),
        statement: FIGURE.into(),
    };
    let mut coverage = Coverage {
        units: vec![CoverageUnit {
            index: 1,
            complete: false,
            reason: "gaps".into(),
            missing_assertions: vec![
                gap("unit-1-context-0"),
                gap("unit-1-context-1"),
                gap("unit-1-passage-9"),
            ],
        }],
    };

    let adopted = inventory_checkpoint::adopt_specifics(
        &mut inventory,
        &mut coverage,
        &content,
        (&candidate, &claim_ledger::Rewrites::default()),
    );

    assert_eq!(adopted, 0);
    assert!(inventory.units[1].claims.is_empty());
    assert!(!coverage.units[0].complete);
    assert_eq!(coverage.units[0].missing_assertions.len(), 3);
}

/// A rubric is never read, so a figure in one never reaches the audit.
#[tokio::test]
async fn a_figure_in_a_rubric_is_neither_read_nor_adopted() -> Result<()> {
    let rubric = "Cite the 2019 survey figure of 100 rows.";
    let mut original = lesson("A header row names the columns of a table in its first line.");
    original["blocks"][1]["rubric"] =
        json!([{"title":"Cites","description":rubric,"dimension":"application"}]);
    let model = JudgingModel::new(rubric, &[FIGURE], FIGURE);

    let report = verify(&model, &original, &[source()]).await?;

    assert_eq!(model.judged.load(Ordering::Relaxed), 0);
    assert!(report.coverage_audit.iter().all(|unit| unit.complete));
    assert!(report
        .coverage
        .iter()
        .flat_map(|unit| &unit.claims)
        .all(|claim| !claim.statement.contains("100")));
    Ok(())
}

/// After the first pass only a gap in a sentence written since the last is
/// adopted, whoever wrote it, so the audit cannot widen the inventory again.
#[test]
fn a_later_pass_adopts_only_a_gap_in_a_sentence_written_since_the_last() {
    let candidate = lesson(FIGURE_PREMISE);
    let content = units(&candidate).unwrap();
    let fresh = || {
        (
            Inventory {
                units: vec![
                    UnitClaims {
                        index: 0,
                        claims: vec![],
                        non_factual_reason: "A section.".into(),
                    },
                    UnitClaims {
                        index: 1,
                        claims: vec![],
                        non_factual_reason: "An exercise instruction.".into(),
                    },
                ],
            },
            Coverage {
                units: vec![CoverageUnit {
                    index: 1,
                    complete: false,
                    reason: FIGURE.into(),
                    missing_assertions: vec![MissingAssertion {
                        passage_id: "unit-1-passage-0".into(),
                        statement: FIGURE.into(),
                    }],
                }],
            },
        )
    };
    let adopt = |rewrites: &claim_ledger::Rewrites| {
        let (mut inventory, mut coverage) = fresh();
        let adopted = inventory_checkpoint::adopt_specifics(
            &mut inventory,
            &mut coverage,
            &content,
            (&candidate, rewrites),
        );
        (
            adopted,
            inventory.units[1].claims.len(),
            coverage.units[0].complete,
        )
    };

    // The first pass reads the whole lesson.
    assert_eq!(adopt(&claim_ledger::Rewrites::default()), (1, 1, true));
    // A later pass over text that did not change adopts nothing.
    let unchanged = claim_ledger::Rewrites::new(true, Some(candidate.clone()), Default::default());
    assert_eq!(adopt(&unchanged), (0, 0, false));
    // The sentence is new to a later pass when something wrote it since.
    let before = lesson("Exercise: write code that reads a file one row at a time.");
    let rewritten = claim_ledger::Rewrites::new(true, Some(before), Default::default());
    assert_eq!(adopt(&rewritten), (1, 1, true));
}

/// The audit of a section is a receipt: a second pass over it audits nothing
/// and adopts nothing again.
#[tokio::test]
async fn a_second_pass_over_an_unchanged_section_audits_and_adopts_nothing_again() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let model = JudgingModel::new(FIGURE_PREMISE, &[FIGURE], FIGURE);
    let original = lesson(FIGURE_PREMISE);

    lesson_drafts::run(&repo, &job, &lesson_id, async {
        lesson_drafts::resume("gaps-fixture".into()).await?;
        let first = verify(&model, &original, &[source()]).await?;
        let judged = model.judged.load(Ordering::Relaxed);
        assert_eq!((first.coverage[1].claims.len(), judged), (1, 1));

        let again = verify(&model, &original, &[source()]).await?;

        assert_eq!(again.coverage[1].claims.len(), 1);
        assert_eq!(model.judged.load(Ordering::Relaxed), judged);
        Ok(())
    })
    .await
}

/// The correct neighbour: a mapper proposing an omission for a deliverable
/// and rubric is answered by the judge with no missing assertion. Nothing is
/// reported, nothing is repaired, and the lesson publishes unchanged.
#[tokio::test]
async fn a_rubric_without_a_missing_assertion_reports_nothing() -> Result<()> {
    let model = JudgingModel::new(RUBRIC, &[], BAD);
    let original = lesson(RUBRIC);
    let (raw, report) = verify_and_repair(
        &model,
        "{}",
        &json!({}),
        original.to_string(),
        &[source()],
        3000,
    )
    .await?;
    assert_eq!(model.judged.load(Ordering::Relaxed), 1);
    assert!(report.issues.is_empty());
    assert!(!*model.inner.repaired.lock().unwrap());
    assert_eq!(serde_json::from_str::<Value>(&raw)?, original);
    assert!(report.coverage_audit.iter().all(|unit| unit.complete));
    assert!(report.coverage_findings(2).is_empty());
    assert_eq!(
        report.coverage[1].claims.len(),
        1,
        "only the extractor's own claim remains"
    );
    assert!(!model.judged_watched_claim.load(Ordering::Relaxed));
    Ok(())
}

#[test]
fn a_report_whose_audit_names_a_gap_is_still_a_complete_report() {
    let lesson = PreparedLearningLesson {
        blocks: vec![LearningBlockDto {
            kind: LearningBlockKind::Explanation,
            title: "CSV header names".into(),
            body: GOOD.into(),
            rubric: vec![],
            source_ids: vec![],
        }],
        questions: vec![],
        keys: vec![],
        verification: None,
    };
    let mut report = attest("lesson", &lesson);
    assert!(report.validate("lesson", &lesson).is_ok());

    report.coverage_audit[0] = CoverageUnit {
        index: 0,
        complete: false,
        reason: "unit-0-passage-0: A premise.".into(),
        missing_assertions: vec![MissingAssertion {
            passage_id: "unit-0-passage-0".into(),
            statement: "A premise.".into(),
        }],
    };

    assert!(
        report.validate("lesson", &lesson).is_ok(),
        "a gap is a finding, not a defect of the report"
    );
}

#[test]
fn gaps_in_questions_follow_the_sections_in_the_findings_sites() {
    let lesson = PreparedLearningLesson {
        blocks: vec![LearningBlockDto {
            kind: LearningBlockKind::Explanation,
            title: "CSV header names".into(),
            body: GOOD.into(),
            rubric: vec![],
            source_ids: vec![],
        }],
        questions: vec![],
        keys: vec![],
        verification: None,
    };
    let mut report = attest("lesson", &lesson);
    let gap = |index: usize, statement: &str| CoverageUnit {
        index,
        complete: false,
        reason: statement.into(),
        missing_assertions: vec![MissingAssertion {
            passage_id: format!("unit-{index}-passage-0"),
            statement: statement.into(),
        }],
    };
    report.coverage_audit = vec![gap(0, "In a section."), gap(2, "In a question.")];

    let sites: Vec<_> = report
        .coverage_findings(2)
        .into_iter()
        .map(|finding| (finding.site, finding.note))
        .collect();

    assert_eq!(sites.len(), 2);
    assert_eq!(sites[0].0, Some(FindingSite::Section(0)));
    assert!(sites[0].1.ends_with("In a section."));
    assert_eq!(sites[1].0, Some(FindingSite::Question(0)));
    assert!(sites[1].1.ends_with("In a question."));
}

#[test]
fn a_gap_is_summarised_with_every_statement_whole() {
    let lesson = PreparedLearningLesson {
        blocks: vec![LearningBlockDto {
            kind: LearningBlockKind::Explanation,
            title: "CSV header names".into(),
            body: GOOD.into(),
            rubric: vec![],
            source_ids: vec![],
        }],
        questions: vec![],
        keys: vec![],
        verification: None,
    };
    let mut report = attest("lesson", &lesson);
    let long = "DictReader buffers a hundred rows before it parses the first one and keeps every parsed row in memory until the file ends.";
    report.coverage_audit = vec![CoverageUnit {
        index: 0,
        complete: false,
        reason: "gaps".into(),
        missing_assertions: vec![
            MissingAssertion {
                passage_id: "unit-0-passage-0".into(),
                statement: long.into(),
            },
            MissingAssertion {
                passage_id: "unit-0-passage-1".into(),
                statement: "It is slow.".into(),
            },
        ],
    }];

    let findings = report.coverage_findings(1);

    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].summary,
        format!("These statements were never checked against a source: \u{201c}{long}\u{201d}, \u{201c}It is slow.\u{201d}.")
    );
}
