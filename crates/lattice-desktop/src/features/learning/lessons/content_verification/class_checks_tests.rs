//! The claim classes over the fixture model: statements sorted, the lesson's
//! own left out, a Reasoning statement read for consistency and put to the
//! falsity veto, a Common one put to the model alone, and what neither can
//! pass sent to the strict check as Sourced.
use super::claim_ledger_tests::{pass_report, passage_finding, prepare, section};
use super::*;
use crate::features::learning::lesson_drafts;
use crate::features::learning::lessons::claim_reviews::repository::ClaimReviewRepository;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::application::services::claim_verification::BRIDGE_SYSTEM;
use lattice_core::domain::learning::claim_review::{ClaimEvent, ClaimState};

const COMMON: &str = "A page number identifies the page of a document that a value appears on.";
const REASONED: &str =
    "The reader hands over one row at a time, so memory stays flat however long the file is.";
const OWN: &str = "The sources name two trade-offs between the designs.";
const REWRITTEN: &str =
    "The reader hands over one row at a time, so it never holds the whole file in memory.";

/// A model that sorts, judges consistency and answers for knowledge as
/// scripted by statement, extracts the statements it is given from a lesson,
/// and answers everything else as the evidence fixture does.
pub(super) struct Classes {
    inner: Model,
    statements: Vec<&'static str>,
    classes: Vec<(&'static str, &'static str)>,
    consistency: Vec<(&'static str, &'static str)>,
    knowledge: Vec<(&'static str, [&'static str; 3])>,
    rewrite: Option<(&'static str, &'static str)>,
    asked: Mutex<Vec<&'static str>>,
    samples: Mutex<HashMap<String, usize>>,
}

impl Classes {
    pub(super) fn new(statements: &[&'static str]) -> Self {
        Self {
            inner: Model::new(),
            statements: statements.to_vec(),
            classes: Vec::new(),
            consistency: Vec::new(),
            knowledge: Vec::new(),
            rewrite: None,
            asked: Mutex::new(Vec::new()),
            samples: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn classed(mut self, classes: &[(&'static str, &'static str)]) -> Self {
        self.classes = classes.to_vec();
        self
    }

    pub(super) fn consistent(mut self, verdicts: &[(&'static str, &'static str)]) -> Self {
        self.consistency = verdicts.to_vec();
        self
    }

    pub(super) fn knows(mut self, samples: &[(&'static str, [&'static str; 3])]) -> Self {
        self.knowledge = samples.to_vec();
        self
    }

    /// A repair writes `to` where the section's body says `from`.
    pub(super) fn rewrites(mut self, from: &'static str, to: &'static str) -> Self {
        self.rewrite = Some((from, to));
        self
    }

    /// How many requests of this kind the model has been sent.
    pub(super) fn calls(&self, kind: &str) -> usize {
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
        if let (true, Some((from, to))) = (
            prompt.starts_with("Repair verified lesson defects."),
            self.rewrite,
        ) {
            self.asked.lock().unwrap().push("repair");
            let data = context(prompt);
            let body = data["candidate"]["blocks"][0]["body"].as_str().unwrap();
            return Ok(json!({"edits":[{"path":"/blocks/0/body","before":body,"after":body.replace(from, to)}]}).to_string());
        }
        if prompt.starts_with("Update claims after a lesson edit.") {
            let data = context(prompt);
            let passages = data["revisedSection"]["passages"].as_array().unwrap();
            let passage = passages
                .iter()
                .find(|p| p["field"] == "/body")
                .unwrap_or(&passages[0]);
            let text = passage["text"].as_str().unwrap();
            let statement = self.statements.iter().find(|part| text.contains(**part));
            let changes: Vec<Value> = data["existingClaims"]
                .as_array()
                .unwrap()
                .iter()
                .map(|claim| {
                    let statement = match statement {
                        Some(part) if claim["statement"] != json!(part) => json!(part),
                        _ => Value::Null,
                    };
                    json!({"claimId":claim["claimId"],"passageId":passage["id"],"statement":statement})
                })
                .collect();
            return Ok(json!({"changes":changes,"nonFactualReason":""}).to_string());
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

    let (_, report) = verify_and_repair(
        &model,
        "{}",
        &json!({}),
        lesson(&body).to_string(),
        &[source()],
        3000,
    )
    .await?;

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

/// A lesson whose one section cites `sentence`, as its writer lists it.
fn cited(sentence: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"CSV header names","body":sentence,"rubric":[],
        "sourceIndex":0,"quote":GOOD,
        "citations":[{"sentence":sentence,"sourceIndex":0,"quote":GOOD}]}],"questions":[]})
}

#[tokio::test]
async fn a_sentence_the_writer_cited_is_sourced_by_the_writer_with_no_sort_call() -> Result<()> {
    let model = Classes::new(&[REASONED]).classed(&[(REASONED, "reasoning")]);
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let candidate = cited(REASONED);
    let citations = section_citations(&candidate, &references)?;
    let claims = vec![(
        0,
        Claim {
            quote: REASONED.to_owned(),
            statement: REASONED.to_owned(),
        },
    )];
    let nothing = claim_ledger::Rewrites::default();

    let classes = class_checks::sort(
        &model,
        &StandardPolicy::new(),
        &candidate,
        &claims,
        &[None],
        &nothing,
        &citations,
    )
    .await?;

    assert_eq!(
        classes,
        [Classed {
            class: Class::Sourced,
            by: ClassifiedBy::Writer
        }]
    );
    assert_eq!(model.calls("sort"), 0);
    // Uncited, the model's word stands.
    let uncited = class_checks::sort(
        &model,
        &StandardPolicy::new(),
        &lesson(REASONED),
        &claims,
        &[None],
        &nothing,
        &Citations::default(),
    )
    .await?;
    assert_eq!(
        uncited,
        [Classed {
            class: Class::Reasoning,
            by: ClassifiedBy::Model
        }]
    );
    assert_eq!(model.calls("sort"), 1);
    Ok(())
}

/// A worked example's figures are its case's own, as the writer's rule has
/// it: the writer cites none of them, so the rules do not send them to a
/// source, and the model sorts them. What the example attributes to a source
/// the rules still send.
#[tokio::test]
async fn a_worked_examples_figures_are_the_models_to_sort_and_its_attributions_the_rules(
) -> Result<()> {
    const CASE: &str = "5 documents have confidence scores below 0.70.";
    const ATTRIBUTED: &str = "According to the survey, most teams retrain monthly.";
    let model = Classes::new(&[CASE, ATTRIBUTED]).classed(&[(CASE, "internal")]);
    let example = json!({"blocks":[{"kind":"worked_example","title":"Thresholds",
        "body":format!("{CASE} {ATTRIBUTED}"),"rubric":[]}],"questions":[]});
    let claims: Vec<(usize, Claim)> = [CASE, ATTRIBUTED]
        .into_iter()
        .map(|statement| {
            (
                0,
                Claim {
                    quote: statement.to_owned(),
                    statement: statement.to_owned(),
                },
            )
        })
        .collect();
    let nothing = claim_ledger::Rewrites::default();

    let classes = class_checks::sort(
        &model,
        &StandardPolicy::new(),
        &example,
        &claims,
        &[None, None],
        &nothing,
        &Citations::default(),
    )
    .await?;

    assert_eq!(
        classes,
        [
            Classed {
                class: Class::Internal,
                by: ClassifiedBy::Model
            },
            Classed {
                class: Class::Sourced,
                by: ClassifiedBy::Rule
            },
        ]
    );
    // An explanation states what is so: there the figure is the rules' to send.
    let stated = class_checks::sort(
        &model,
        &StandardPolicy::new(),
        &lesson(CASE),
        &claims[..1],
        &[None],
        &nothing,
        &Citations::default(),
    )
    .await?;
    assert_eq!(
        stated,
        [Classed {
            class: Class::Sourced,
            by: ClassifiedBy::Rule
        }]
    );
    Ok(())
}

/// An answer explanation reasons from its question's givens: a figure it
/// shares with the question's prompt or options is the question's, and the
/// model sorts a statement that states no other. A figure of its own is the
/// rules' to send to a source.
#[tokio::test]
async fn a_figure_an_answer_shares_with_its_question_is_a_given_not_a_specific() -> Result<()> {
    const GIVEN: &str =
        "With 300 labeled examples, fine-tuning is expected to beat zero-shot prompting.";
    const MEASURED: &str = "Fine-tuning Mistral-7B with 160 samples achieved 72.5% accuracy.";
    let model = Classes::new(&[GIVEN, MEASURED]).classed(&[(GIVEN, "reasoning")]);
    let candidate = json!({"blocks":[],"questions":[{"kind":"practice",
        "prompt":"You have 300 labeled examples of 5 document types. Which strategy is best supported by the evidence?",
        "options":["Zero-shot prompting","Fine-tuning","Embeddings and kNN","Rules"],
        "explanation":format!("{GIVEN} {MEASURED}")}]});
    let claims: Vec<(usize, Claim)> = [GIVEN, MEASURED]
        .into_iter()
        .map(|statement| {
            (
                0,
                Claim {
                    quote: statement.to_owned(),
                    statement: statement.to_owned(),
                },
            )
        })
        .collect();
    let nothing = claim_ledger::Rewrites::default();

    let classes = class_checks::sort(
        &model,
        &StandardPolicy::new(),
        &candidate,
        &claims,
        &[None, None],
        &nothing,
        &Citations::default(),
    )
    .await?;

    assert_eq!(
        classes,
        [
            Classed {
                class: Class::Reasoning,
                by: ClassifiedBy::Model
            },
            Classed {
                class: Class::Sourced,
                by: ClassifiedBy::Rule
            },
        ]
    );
    Ok(())
}

#[tokio::test]
async fn a_cited_statement_meets_the_strict_check_whatever_the_model_would_have_sorted(
) -> Result<()> {
    // Sorted, it would be Reasoning and meet the consistency check and the veto.
    let model = Classes::new(&[GOOD]).classed(&[(GOOD, "reasoning")]);

    let (_, report) = verify_and_repair(
        &model,
        "{}",
        &json!({}),
        cited(GOOD).to_string(),
        &[source()],
        3000,
    )
    .await?;

    assert_eq!(
        outcomes(&report),
        vec![(GOOD, ClaimVerdict::Supported, Some(Class::Sourced))]
    );
    assert_eq!(
        report.findings[0].class,
        Some(Classed {
            class: Class::Sourced,
            by: ClassifiedBy::Writer
        })
    );
    assert_eq!(model.calls("sort"), 0);
    assert_eq!(model.calls("consistency"), 0);
    assert_eq!(model.calls("knowledge"), 0);
    assert!(model.strict_calls() >= 1);
    Ok(())
}

/// The ledger after a pass found `statement` supported, checked as `classed`.
async fn ledger_holding(
    model: &Classes,
    statement: &str,
    classed: Classed,
) -> Result<claim_ledger::ClaimLedger> {
    let mut ledger = claim_ledger::ClaimLedger::open(model, "{}").await?;
    let mut finding = passage_finding(statement, statement, true, SOURCE);
    finding.class = Some(classed);
    ledger
        .observe(&lesson(statement), &pass_report(vec![finding]))
        .await?;
    Ok(ledger)
}

#[tokio::test]
async fn a_class_a_later_pass_gives_a_held_statement_is_recorded_only_when_it_prevails(
) -> Result<()> {
    let model = Classes::new(&[]);
    let cited = Classed {
        class: Class::Sourced,
        by: ClassifiedBy::Writer,
    };
    let mut ledger = ledger_holding(&model, GOOD, cited).await?;
    let id = ledger.finding_claims()[0].clone();

    for lighter in [Class::Common, Class::Reasoning] {
        let mut finding = passage_finding(GOOD, GOOD, true, SOURCE);
        finding.class = Some(Classed {
            class: lighter,
            by: ClassifiedBy::Model,
        });
        ledger
            .observe(&lesson(GOOD), &pass_report(vec![finding]))
            .await?;
    }

    let entry = ledger.entry(&id).unwrap();
    let classified = entry
        .events()
        .iter()
        .filter(|record| matches!(record.event, ClaimEvent::Classified { .. }))
        .count();
    assert_eq!(classified, 1);
    assert_eq!(
        entry.classified(),
        Some((Class::Sourced, ClassifiedBy::Writer))
    );
    Ok(())
}

#[tokio::test]
async fn a_rewritten_statement_is_checked_as_its_parent_was_and_the_calls_are_the_plans(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    // Sorted again, the rewrite would be Common and meet no consistency check.
    let model = Classes::new(&[REASONED, REWRITTEN])
        .classed(&[(REASONED, "reasoning"), (REWRITTEN, "common")])
        .consistent(&[(REASONED, "contradicted")])
        .rewrites(REASONED, REWRITTEN);

    let (prepared, trace) = crate::features::learning::lesson_progress::traced(prepare(
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(lesson(&section(REASONED)).to_string()),
        &[source()],
    ))
    .await;
    let (raw, report) = prepared?;

    let written: Value = serde_json::from_str(&raw)?;
    assert!(written["blocks"][0]["body"]
        .as_str()
        .is_some_and(|body| body.contains(REWRITTEN) && !body.contains(REASONED)));
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    // The first pass sorted; the rewrite did not meet the sort again but the
    // consistency check and the veto, as the statement it replaced would have.
    assert_eq!(model.calls("sort"), 1);
    assert_eq!(model.calls("repair"), 1);
    assert_eq!(model.calls("consistency"), 2);
    assert_eq!(model.calls("knowledge"), 3);
    let ledger = ClaimReviewRepository::new(pool.clone())
        .load(&lesson_id)
        .await?;
    let [claim] = ledger.claims.as_slice() else {
        panic!("one claim: {:?}", ledger.claims);
    };
    assert!(matches!(claim.state(), ClaimState::Consistent { .. }));
    assert_eq!(
        claim.classified(),
        Some((Class::Reasoning, ClassifiedBy::Model))
    );
    // The calls are the sum of the plan's terms along this path: the sort,
    // the statement's consistency call, then the rewrite, its consistency
    // re-check and the veto's three samples of the part of G a rewrite that
    // passes uses.
    assert_eq!(trace.calls, 1 + 1 + (1 + 1 + 3));
    assert!(trace.only_came_down(), "{:?}", trace.ceilings);
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
        verify_and_repair(
            &model,
            "{}",
            &json!({}),
            lesson(&body).to_string(),
            &sources,
            3000,
        )
        .await?;
        let asked = (
            model.calls("sort"),
            model.calls("consistency"),
            model.calls("knowledge"),
        );
        assert_eq!(asked, (1, 1, 6));
        verify_and_repair(
            &model,
            "{}",
            &json!({}),
            lesson(&body).to_string(),
            &sources,
            3000,
        )
        .await?;
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

/// A resolution is the user's: the machine's pass of a Common statement is
/// not one, so the learner is shown its class and not a verdict of someone's.
#[tokio::test]
async fn the_report_records_no_resolution_for_a_statement_the_machine_passed() -> Result<()> {
    let model = Classes::new(&[]);
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let body = format!("{COMMON} {GOOD}");
    let mut common = passage_finding(&body, COMMON, true, SOURCE);
    common.evidence.clear();
    common.supporting_quote = None;
    common.class = Some(Classed {
        class: Class::Common,
        by: ClassifiedBy::Model,
    });
    let mut sourced = passage_finding(&body, GOOD, true, SOURCE);
    sourced.class = Some(Classed {
        class: Class::Sourced,
        by: ClassifiedBy::Rule,
    });
    let report = pass_report(vec![common, sourced]);
    ledger.observe(&lesson(&body), &report).await?;
    let ids = ledger.finding_claims().to_vec();
    assert!(matches!(
        ledger.entry(&ids[0]).unwrap().state(),
        ClaimState::GeneralKnowledge { .. }
    ));

    let bound = ledger.bind(report);

    assert!(bound
        .findings
        .iter()
        .all(|finding| finding.resolution.is_none()));
    Ok(())
}

const REWORDED: &str = "A page number tells a reader which page of a document a value appears on.";
const SKIPPED: &str =
    "The skipinitialspace setting makes the reader ignore spaces that follow a delimiter.";
const SKIPPED_PASSAGE: &str =
    "The optional skipinitialspace setting ignores spaces immediately after delimiters; it defaults to false.";

/// A `Classes` model whose part-wise judge reads every statement as the parts
/// `kinds` name, and whose strict judge finds none of the `unstated`
/// statements stated.
struct Reads {
    inner: Classes,
    kinds: Vec<&'static str>,
    unstated: Vec<&'static str>,
}

impl Reads {
    fn new(inner: Classes, kinds: &[&'static str]) -> Self {
        Self {
            inner,
            kinds: kinds.to_vec(),
            unstated: Vec::new(),
        }
    }

    fn finds_unstated(mut self, statements: &[&'static str]) -> Self {
        self.unstated = statements.to_vec();
        self
    }

    /// The part-wise judge's reply: a stated part rests on the first passage.
    fn parts(&self) -> String {
        let parts: Vec<Value> = self
            .kinds
            .iter()
            .map(|kind| {
                let passage = if *kind == "stated" {
                    json!("passage-0")
                } else {
                    Value::Null
                };
                json!({"part": format!("the {kind} part of the claim"), "kind": kind, "passage": passage})
            })
            .collect();
        json!({"parts": parts, "verdict": "supported_with_common", "reason": "Each part was read."})
            .to_string()
    }
}

#[async_trait::async_trait]
impl LLMPort for Reads {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, user) = fixture_messages(request);
        if system == BRIDGE_SYSTEM {
            return Ok(CompletionResponse::from_text(self.parts()));
        }
        let strict =
            user.starts_with("Source passages:") && !user.contains("\n\nExplanatory statement: ");
        if strict
            && self
                .unstated
                .iter()
                .any(|statement| user.contains(*statement))
        {
            return Ok(CompletionResponse::from_text(
                "Reason: No passage states it outright.\nSource passage: none\nVerdict: unsupported",
            ));
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

/// A lesson of two sections: the first states `first` and cites `quote` for
/// the section, and for the sentence itself when `of_sentence`; the second
/// states what the source says and cites nothing.
fn two_sections(first: &str, quote: &str, of_sentence: bool) -> Value {
    let mut block = json!({"kind":"explanation","title":"Section 0","body":section(first),
        "rubric":[],"sourceIndex":0,"quote":quote});
    if of_sentence {
        block["citations"] = json!([{"sentence":first,"sourceIndex":0,"quote":quote}]);
    }
    json!({"blocks":[block,{"kind":"explanation","title":"Section 1","body":section(GOOD),"rubric":[]}],"questions":[]})
}

#[tokio::test]
async fn a_cited_sentence_whose_every_part_is_common_stays_loses_its_own_citation_and_ends_as_general_knowledge(
) -> Result<()> {
    // §1 #1 of the acceptance policy design
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let model = Reads::new(
        Classes::new(&[COMMON, REWORDED, GOOD])
            .knows(&[(COMMON, ["true_common"; 3])])
            .rewrites(COMMON, REWORDED),
        &["common", "common"],
    )
    .finds_unstated(&[COMMON]);

    let (raw, _) = prepare(
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(two_sections(COMMON, GOOD, true).to_string()),
        &[source()],
    )
    .await?;

    let written: Value = serde_json::from_str(&raw)?;
    let block = &written["blocks"][0];
    assert!(
        block["body"]
            .as_str()
            .is_some_and(|body| body.contains(COMMON)),
        "the sentence stays in the lesson, but its section now reads: {}",
        block["body"]
    );
    assert!(
        !block["citations"]
            .as_array()
            .is_some_and(|entries| entries.iter().any(|entry| entry["sentence"] == COMMON)),
        "the sentence loses its own citation entry: {}",
        block["citations"]
    );
    let ledger = ClaimReviewRepository::new(pool.clone())
        .load(&lesson_id)
        .await?;
    let claim = ledger
        .claims
        .iter()
        .find(|claim| claim.statement() == COMMON);
    assert!(
        matches!(
            claim.map(|claim| claim.state()),
            Some(ClaimState::GeneralKnowledge { .. })
        ),
        "{:?}",
        claim.map(|claim| claim.state())
    );
    assert_eq!(model.inner.calls("repair"), 0);
    Ok(())
}

#[tokio::test]
async fn a_common_statement_most_samples_call_false_stays_and_passes_when_its_sections_passages_state_it(
) -> Result<()> {
    // §1 #5 of the acceptance policy design
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let model = Reads::new(
        Classes::new(&[SKIPPED, GOOD])
            .classed(&[(SKIPPED, "common")])
            .knows(&[(SKIPPED, ["false", "false", "true_common"])]),
        &["stated"],
    );

    let (raw, _) = prepare(
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(two_sections(SKIPPED, SKIPPED_PASSAGE, false).to_string()),
        &[source()],
    )
    .await?;

    let written: Value = serde_json::from_str(&raw)?;
    assert!(
        written["blocks"][0]["body"]
            .as_str()
            .is_some_and(|body| body.contains(SKIPPED)),
        "the statement stays in the lesson, but its section now reads: {}",
        written["blocks"][0]["body"]
    );
    let ledger = ClaimReviewRepository::new(pool.clone())
        .load(&lesson_id)
        .await?;
    let claim = ledger
        .claims
        .iter()
        .find(|claim| claim.statement() == SKIPPED);
    assert!(
        matches!(
            claim.map(|claim| claim.state()),
            Some(ClaimState::GeneralKnowledge { .. })
        ),
        "{:?}",
        claim.map(|claim| claim.state())
    );
    Ok(())
}

#[tokio::test]
async fn a_consistent_statement_most_samples_call_false_stays_and_passes_when_its_sections_passages_ground_it_part_by_part(
) -> Result<()> {
    // §1 #4 of the acceptance policy design
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let model = Reads::new(
        Classes::new(&[REASONED, REWRITTEN, GOOD])
            .classed(&[(REASONED, "reasoning")])
            .knows(&[(REASONED, ["false", "false", "true_common"])])
            .rewrites(REASONED, REWRITTEN),
        &["stated", "common"],
    )
    .finds_unstated(&[REASONED]);

    let (raw, _) = prepare(
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(two_sections(REASONED, GOOD, false).to_string()),
        &[source()],
    )
    .await?;

    let written: Value = serde_json::from_str(&raw)?;
    assert!(
        written["blocks"][0]["body"]
            .as_str()
            .is_some_and(|body| body.contains(REASONED)),
        "the statement stays in the lesson, but its section now reads: {}",
        written["blocks"][0]["body"]
    );
    let ledger = ClaimReviewRepository::new(pool.clone())
        .load(&lesson_id)
        .await?;
    let claim = ledger
        .claims
        .iter()
        .find(|claim| claim.statement() == REASONED);
    assert!(
        matches!(
            claim.map(|claim| claim.state()),
            Some(ClaimState::Consistent { .. })
        ),
        "{:?}",
        claim.map(|claim| claim.state())
    );
    assert_eq!(model.inner.calls("repair"), 0);
    Ok(())
}
