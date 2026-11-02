//! The claim classes over the fixture model: statements sorted, the lesson's
//! own left out, a Reasoning statement read for consistency and put to the
//! falsity veto, a Common one put to the model alone, and what neither can
//! pass sent to the strict check as Sourced.
use super::claim_ledger_tests::{pass_report, passage_finding};
use super::*;
use crate::features::learning::lesson_drafts;
use lattice_core::domain::learning::claim_review::{Actor, ClaimState, VerdictKind};

const COMMON: &str = "A page number identifies the page of a document that a value appears on.";
const REASONED: &str =
    "The reader hands over one row at a time, so memory stays flat however long the file is.";
const OWN: &str = "The sources name two trade-offs between the designs.";

/// A model that sorts, judges consistency and answers for knowledge as
/// scripted by statement, extracts the statements it is given from a lesson,
/// and answers everything else as the evidence fixture does.
struct Classes {
    inner: Model,
    statements: Vec<&'static str>,
    classes: Vec<(&'static str, &'static str)>,
    consistency: Vec<(&'static str, &'static str)>,
    knowledge: Vec<(&'static str, [&'static str; 3])>,
    asked: Mutex<Vec<&'static str>>,
    samples: Mutex<HashMap<String, usize>>,
}

impl Classes {
    fn new(statements: &[&'static str]) -> Self {
        Self {
            inner: Model::new(),
            statements: statements.to_vec(),
            classes: Vec::new(),
            consistency: Vec::new(),
            knowledge: Vec::new(),
            asked: Mutex::new(Vec::new()),
            samples: Mutex::new(HashMap::new()),
        }
    }

    fn classed(mut self, classes: &[(&'static str, &'static str)]) -> Self {
        self.classes = classes.to_vec();
        self
    }

    fn consistent(mut self, verdicts: &[(&'static str, &'static str)]) -> Self {
        self.consistency = verdicts.to_vec();
        self
    }

    fn knows(mut self, samples: &[(&'static str, [&'static str; 3])]) -> Self {
        self.knowledge = samples.to_vec();
        self
    }

    /// How many requests of this kind the model has been sent.
    fn calls(&self, kind: &str) -> usize {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .filter(|asked| **asked == kind)
            .count()
    }

    fn strict_calls(&self) -> usize {
        *self.inner.judge_calls.lock().unwrap()
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Extract lesson claims.") {
            let data = context(prompt);
            let units = data["units"].as_array().unwrap();
            return Ok(inventory::fixture_response(
                units
                    .iter()
                    .map(|unit| {
                        let passage = unit["passages"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|p| p["field"] == "/body")
                            .unwrap();
                        let text = passage["text"].as_str().unwrap();
                        let claims: Vec<Value> = self
                            .statements
                            .iter()
                            .filter(|statement| text.contains(**statement))
                            .map(|statement| json!({"passageId":passage["id"],"statement":statement}))
                            .collect();
                        json!({"index":unit["index"],"claims":claims,"nonFactualReason":""})
                    })
                    .collect(),
            )
            .to_string());
        }
        if prompt.starts_with("You sort statements extracted from a lesson") {
            self.asked.lock().unwrap().push("sort");
            let statements: serde_json::Map<String, Value> =
                serde_json::from_str(prompt.rsplit("\n\n").next().unwrap())?;
            let answer: serde_json::Map<String, Value> = statements
                .iter()
                .map(|(id, statement)| {
                    let statement = statement.as_str().unwrap_or_default();
                    let class = self
                        .classes
                        .iter()
                        .find(|(part, _)| statement.contains(part))
                        .map_or("sourced", |(_, class)| *class);
                    (id.clone(), json!(class))
                })
                .collect();
            return Ok(Value::Object(answer).to_string());
        }
        if let Some((_, statement)) = prompt.split_once("\n\nExplanatory statement: ") {
            self.asked.lock().unwrap().push("consistency");
            let verdict = self
                .consistency
                .iter()
                .find(|(part, _)| statement.contains(part))
                .map_or("consistent", |(_, verdict)| *verdict);
            let reason = if verdict == "consistent" {
                "The passages say nothing against it."
            } else {
                "The passages say otherwise."
            };
            return Ok(json!({"verdict":verdict,"reason":reason}).to_string());
        }
        if prompt.starts_with("You decide whether one statement is standard knowledge") {
            self.asked.lock().unwrap().push("knowledge");
            let statement = prompt.rsplit("\n\nStatement: ").next().unwrap();
            let verdicts = self
                .knowledge
                .iter()
                .find(|(part, _)| statement.contains(part))
                .map_or(["true_common"; 3], |(_, verdicts)| *verdicts);
            let mut samples = self.samples.lock().unwrap();
            let taken = samples.entry(statement.to_owned()).or_insert(0);
            let verdict = verdicts[*taken % 3];
            *taken += 1;
            return Ok(json!({"verdict":verdict,"reason":"A textbook says so."}).to_string());
        }
        self.inner.respond(prompt).await
    }
}

#[async_trait::async_trait]
impl LLMPort for Classes {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "classes-fixture"
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

fn lesson(body: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"CSV header names","body":body,"rubric":[]}],"questions":[]})
}

/// What a report says of each finding: its statement, verdict and class.
fn outcomes(report: &LessonVerificationReport) -> Vec<(&str, ClaimVerdict, Option<Class>)> {
    report
        .findings
        .iter()
        .map(|finding| {
            (
                finding.statement.as_str(),
                finding.verdict,
                finding.class.map(|classed| classed.class),
            )
        })
        .collect()
}

#[tokio::test]
async fn each_statement_meets_the_check_its_class_names_and_the_lessons_own_is_left_out(
) -> Result<()> {
    let model = Classes::new(&[COMMON, REASONED, OWN, GOOD])
        .classed(&[(COMMON, "common"), (REASONED, "reasoning")]);
    let body = format!("{COMMON} {REASONED} {OWN} {GOOD}");

    let report = verify(&model, &lesson(&body), &[source()]).await?;

    assert_eq!(
        outcomes(&report),
        vec![
            (COMMON, ClaimVerdict::Supported, Some(Class::Common)),
            (REASONED, ClaimVerdict::Supported, Some(Class::Reasoning)),
            (GOOD, ClaimVerdict::Supported, Some(Class::Sourced)),
        ]
    );
    // The lesson's own remark is no claim: the report accounts for three.
    let accounted: Vec<&str> = report.coverage[0]
        .claims
        .iter()
        .map(|claim| claim.statement.as_str())
        .collect();
    assert_eq!(accounted, [COMMON, REASONED, GOOD]);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    // The rule layer sorted the remark; the model sorted the other three in
    // one call. The Common statement and the Reasoning one met the knowledge
    // judge's three samples, the Reasoning one also the consistency judge,
    // and only the Sourced one the strict check.
    assert_eq!(model.calls("sort"), 1);
    assert_eq!(model.calls("consistency"), 1);
    assert_eq!(model.calls("knowledge"), 6);
    assert_eq!(model.strict_calls(), 1);
    let common = &report.findings[0];
    assert!(common.evidence.is_empty() && common.supporting_quote.is_none());
    assert!(
        common.reason.starts_with("Standard knowledge"),
        "{}",
        common.reason
    );
    assert!(report.findings[1].supporting_quote.is_none());
    Ok(())
}

#[tokio::test]
async fn a_reasoning_statement_a_passage_contradicts_fails_without_the_veto() -> Result<()> {
    let model = Classes::new(&[REASONED])
        .classed(&[(REASONED, "reasoning")])
        .consistent(&[(REASONED, "contradicted")]);

    let report = verify(&model, &lesson(REASONED), &[source()]).await?;

    assert_eq!(
        outcomes(&report),
        vec![(REASONED, ClaimVerdict::Contradicted, Some(Class::Reasoning))]
    );
    assert_eq!(report.findings[0].reason, "The passages say otherwise.");
    assert_eq!(model.calls("knowledge"), 0);
    assert!(!report.issues.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_reasoning_statement_with_an_unfounded_specific_is_checked_against_its_sources(
) -> Result<()> {
    let model = Classes::new(&[REASONED])
        .classed(&[(REASONED, "reasoning")])
        .consistent(&[(REASONED, "unfounded_specific")]);

    let report = verify(&model, &lesson(REASONED), &[source()]).await?;

    let [finding] = report.findings.as_slice() else {
        panic!("one finding: {:?}", outcomes(&report));
    };
    assert_eq!(
        finding.class.map(|classed| classed.class),
        Some(Class::Sourced)
    );
    assert_eq!(model.calls("knowledge"), 0);
    Ok(())
}

#[tokio::test]
async fn a_consistent_statement_most_samples_call_false_fails_unless_the_passages_state_it(
) -> Result<()> {
    let model = Classes::new(&[REASONED])
        .classed(&[(REASONED, "reasoning")])
        .knows(&[(REASONED, ["false", "false", "true_common"])]);

    let report = verify(&model, &lesson(REASONED), &[source()]).await?;

    assert_eq!(
        outcomes(&report),
        vec![(REASONED, ClaimVerdict::Contradicted, Some(Class::Reasoning))]
    );
    assert!(
        report.findings[0].reason.contains("(2 of 3)"),
        "{}",
        report.findings[0].reason
    );
    assert_eq!(model.strict_calls(), 0, "the section cites nothing to ask");

    // The sources outrank the model: this section cites a passage that
    // states the statement outright.
    let model = Classes::new(&[GOOD])
        .classed(&[(GOOD, "reasoning")])
        .knows(&[(GOOD, ["false", "false", "false"])]);
    let mut cited = lesson(GOOD);
    cited["blocks"][0]["quote"] = json!(GOOD);
    cited["blocks"][0]["sourceIndex"] = json!(0);

    let report = verify(&model, &cited, &[source()]).await?;

    assert_eq!(
        outcomes(&report),
        vec![(GOOD, ClaimVerdict::Supported, Some(Class::Reasoning))]
    );
    assert!(report.findings[0].supporting_quote.is_some());
    assert_eq!(model.strict_calls(), 1);
    Ok(())
}

#[tokio::test]
async fn a_common_statement_passes_only_when_every_sample_agrees() -> Result<()> {
    const FALSE: &str = "A header row names the columns of a CSV file after its data.";
    const LONE: &str = "DictReader keeps whitespace in field names.";
    const WANTS: &str = "Missing values in nonblank rows default to None.";
    let model = Classes::new(&[COMMON, FALSE, LONE, WANTS])
        .classed(&[
            (COMMON, "common"),
            (FALSE, "common"),
            (LONE, "common"),
            (WANTS, "common"),
        ])
        .knows(&[
            (FALSE, ["false", "false", "true_common"]),
            (LONE, ["true_common", "true_common", "false"]),
            (WANTS, ["needs_source", "true_common", "true_common"]),
        ]);
    let body = format!("{COMMON} {FALSE} {LONE} {WANTS}");

    let report = verify(&model, &lesson(&body), &[source()]).await?;

    let classes: Vec<(&str, Option<Class>)> = outcomes(&report)
        .into_iter()
        .map(|(statement, _, class)| (statement, class))
        .collect();
    assert_eq!(
        classes,
        vec![
            (COMMON, Some(Class::Common)),
            (FALSE, Some(Class::Common)),
            // A lone dissent forfeits the pass without failing the statement,
            // as a request for a source does: both go to the strict check.
            (LONE, Some(Class::Sourced)),
            (WANTS, Some(Class::Sourced)),
        ]
    );
    assert_eq!(report.findings[0].verdict, ClaimVerdict::Supported);
    assert_eq!(report.findings[1].verdict, ClaimVerdict::Contradicted);
    assert!(
        report.findings[1].reason.contains("(2 of 3)"),
        "{}",
        report.findings[1].reason
    );
    Ok(())
}

#[tokio::test]
async fn the_calls_ceiling_only_comes_down_while_a_pass_checks_a_mix_of_classes() -> Result<()> {
    const FALSE: &str = "A header row names the columns of a CSV file after its data.";
    const LONE: &str = "DictReader keeps whitespace in field names.";
    const WANTS: &str = "Missing values in nonblank rows default to None.";
    let model = Classes::new(&[COMMON, FALSE, LONE, WANTS, REASONED, GOOD])
        .classed(&[
            (COMMON, "common"),
            (FALSE, "common"),
            (LONE, "common"),
            (WANTS, "common"),
            (REASONED, "reasoning"),
        ])
        .knows(&[
            (FALSE, ["false", "false", "true_common"]),
            (LONE, ["true_common", "true_common", "false"]),
            (WANTS, ["needs_source", "true_common", "true_common"]),
        ]);
    let body = format!("{COMMON} {FALSE} {LONE} {WANTS} {REASONED} {GOOD}");

    let (report, trace) = crate::features::learning::lesson_progress::traced(verify(
        &model,
        &lesson(&body),
        &[source()],
    ))
    .await;
    report?;

    // The statements sorted, the Reasoning and Common ones checked, the
    // strict check done: three events, each at or below the one before, two
    // Common statements sent on to their source check among them.
    assert_eq!(trace.ceilings.len(), 3, "{:?}", trace.ceilings);
    assert!(trace.only_came_down(), "{:?}", trace.ceilings);
    // The sort, the knowledge samples of five statements, the consistency
    // call and the strict checks.
    assert!(trace.calls >= 1 + 5 * 3 + 1 + 1, "{} calls", trace.calls);
    assert!(
        trace.within_the_ceiling(),
        "{} calls, ceilings {:?}",
        trace.calls,
        trace.ceilings
    );
    Ok(())
}

#[tokio::test]
async fn a_judgment_is_asked_once_and_a_resumed_pass_restores_it() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let model = Classes::new(&[COMMON, REASONED, GOOD])
        .classed(&[(COMMON, "common"), (REASONED, "reasoning")]);
    let body = format!("{COMMON} {REASONED} {GOOD}");
    let sources = [source()];

    lesson_drafts::run(&repo, &job, &lesson_id, async {
        lesson_drafts::resume("class-fixture".into()).await?;
        verify(&model, &lesson(&body), &sources).await?;
        let asked = (
            model.calls("sort"),
            model.calls("consistency"),
            model.calls("knowledge"),
        );
        assert_eq!(asked, (1, 1, 6));
        verify(&model, &lesson(&body), &sources).await?;
        assert_eq!(
            (
                model.calls("sort"),
                model.calls("consistency"),
                model.calls("knowledge"),
            ),
            asked,
            "the sorting and both judges are receipts"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_ledger_records_each_class_and_the_state_its_check_ends_in() -> Result<()> {
    let model = Classes::new(&[]);
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let body = format!("{COMMON} {REASONED} {GOOD} {FALSE_COMMON}");
    const FALSE_COMMON: &str = "A header row names the columns of a CSV file after its data.";
    let classed = |class, by| Some(Classed { class, by });
    let mut common = passage_finding(&body, COMMON, true, SOURCE);
    common.evidence.clear();
    common.supporting_quote = None;
    common.class = classed(Class::Common, ClassifiedBy::Model);
    let mut reasoning = passage_finding(&body, REASONED, true, SOURCE);
    reasoning.supporting_quote = None;
    reasoning.class = classed(Class::Reasoning, ClassifiedBy::Model);
    let mut sourced = passage_finding(&body, GOOD, true, SOURCE);
    sourced.class = classed(Class::Sourced, ClassifiedBy::Rule);
    let mut false_common = passage_finding(&body, FALSE_COMMON, false, SOURCE);
    false_common.verdict = ClaimVerdict::Contradicted;
    false_common.evidence.clear();
    false_common.class = classed(Class::Common, ClassifiedBy::Model);

    ledger
        .observe(
            &lesson(&body),
            &pass_report(vec![common, reasoning, sourced, false_common]),
        )
        .await?;

    let ids = ledger.finding_claims().to_vec();
    let state = |index: usize| ledger.entry(&ids[index]).unwrap().state().clone();
    let class = |index: usize| ledger.entry(&ids[index]).unwrap().class();
    assert!(matches!(
        state(0),
        ClaimState::GeneralKnowledge {
            by: Actor::Machine,
            ..
        }
    ));
    assert!(matches!(state(1), ClaimState::Consistent { .. }));
    assert!(matches!(state(2), ClaimState::ResolvedByEvidence { .. }));
    assert!(matches!(
        state(3),
        ClaimState::Open {
            kind: VerdictKind::Contradicted,
            ..
        }
    ));
    assert_eq!(
        (class(0), class(1), class(2), class(3)),
        (
            Some(Class::Common),
            Some(Class::Reasoning),
            Some(Class::Sourced),
            Some(Class::Common)
        )
    );
    // A Common statement the model called false has no rewrite: its first
    // repair takes its sentence out. One that passed is not repaired at all.
    assert!(ledger.cuts_next(&ids[3]));
    assert!(!ledger.cuts_next(&ids[2]));
    Ok(())
}
