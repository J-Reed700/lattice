//! The claim classes over the fixture model: statements sorted, the lesson's
//! own left out, a Reasoning statement read for consistency and put to the
//! falsity veto, a Common one put to the model alone, and what neither can
//! pass sent to the strict check as Sourced.
use super::acceptance_switch_tests::{claim, identified, node_of, open, stood, walk};
use super::claim_ledger_tests::{
    identify_findings, pass_report, passage_finding, prepare, section,
};
use super::*;
use crate::lesson_drafts;
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::application::services::claim_verification::BRIDGE_SYSTEM;
use lattice_core::domain::learning::acceptance::{
    most_calls, start, End, Outcome, Reading, Step, Then,
};
use lattice_core::domain::learning::claim_review::{
    ClaimEvent, ClaimLedgerEntry, ClaimState, VerdictKind,
};

const COMMON: &str = "A page number identifies the page of a document that a value appears on.";
const REASONED: &str =
    "The reader hands over one row at a time, so memory stays flat however long the file is.";
const RUBRIC_LINE: &str = "The answer names two trade-offs between the designs.";
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

    pub(super) async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Extract lesson claims.") {
            let data = fixture_task(prompt).unwrap();
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
                        let text = passage_text(passage);
                        let claims: Vec<Value> = self
                            .statements
                            .iter()
                            .filter(|statement| text.contains(**statement))
                            .map(|statement| json!({"sentenceId":sentence_id(passage, statement),"statement":statement}))
                            .collect();
                        json!({"index":unit["index"],"claims":claims,"nonFactualReason":""})
                    })
                    .collect(),
            )
            .to_string());
        }
        if prompt.starts_with("You sort statements extracted from a lesson") {
            self.asked.lock().unwrap().push("sort");
            let Some(Value::Object(statements)) = fixture_task(prompt) else {
                panic!("a JSON task");
            };
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
            let data = fixture_task(prompt).unwrap();
            let body = data["candidate"]["blocks"][0]["body"].as_str().unwrap();
            return Ok(json!({"edits":[{"path":"/blocks/0/body","before":body,"after":body.replace(from, to)}]}).to_string());
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
async fn each_statement_meets_the_check_its_class_names() -> Result<()> {
    let model = Classes::new(&[COMMON, REASONED, GOOD])
        .classed(&[(COMMON, "common"), (REASONED, "reasoning")]);
    let body = format!("{COMMON} {REASONED} {GOOD}");

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
    // The report accounts for the three.
    let accounted: Vec<&str> = report.coverage[0]
        .claims
        .iter()
        .map(|claim| claim.statement.as_str())
        .collect();
    assert_eq!(accounted, [COMMON, REASONED, GOOD]);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    // The model sorted the three in one call. The Common statement and the
    // Reasoning one met the knowledge judge's three samples, the Reasoning
    // one also the consistency judge, and only the Sourced one the strict
    // check.
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

    let classes = class_checks::sort(&model, &candidate, &claims, &[None], &citations).await?;

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
        &lesson(REASONED),
        &claims,
        &[None],
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
/// source, and the model sorts them. What the example quotes the rules still
/// send.
#[tokio::test]
async fn a_worked_examples_figures_are_the_models_to_sort_and_its_quotations_the_rules(
) -> Result<()> {
    const CASE: &str = "5 documents have confidence scores below 0.70.";
    const QUOTED: &str = "The page reads \"Total due on receipt of goods\" at the foot.";
    let model = Classes::new(&[CASE, QUOTED]).classed(&[(CASE, "internal")]);
    let example = json!({"blocks":[{"kind":"worked_example","title":"Thresholds",
        "body":format!("{CASE} {QUOTED}"),"rubric":[]}],"questions":[]});
    let claims: Vec<(usize, Claim)> = [CASE, QUOTED]
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

    let classes = class_checks::sort(
        &model,
        &example,
        &claims,
        &[None, None],
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
    // An explanation states what is so: there a figure statement is never left
    // Internal, and what the model calls Internal is read as Reasoning.
    let stated = class_checks::sort(
        &model,
        &lesson(CASE),
        &claims[..1],
        &[None],
        &Citations::default(),
    )
    .await?;
    assert_eq!(
        stated,
        [Classed {
            class: Class::Reasoning,
            by: ClassifiedBy::Model
        }]
    );
    Ok(())
}

/// An answer explanation reasons from its question's givens: a figure it
/// shares with the question's prompt or options is the question's, and the
/// model sorts a statement that states no other. A figure of its own is
/// never left Internal: the model sorts it, and Internal is read as Reasoning.
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

    let classes = class_checks::sort(
        &model,
        &candidate,
        &claims,
        &[None, None],
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
                by: ClassifiedBy::Model
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

/// The ledger after a pass identified `statement`, sorted as `classed`.
async fn ledger_holding(
    model: &Classes,
    statement: &str,
    classed: Classed,
) -> Result<claim_ledger::ClaimLedger> {
    let mut ledger = claim_ledger::ClaimLedger::open(model, "{}").await?;
    let mut finding = passage_finding(statement, statement, true, SOURCE);
    finding.class = Some(classed);
    identify_findings(&mut ledger, &lesson(statement), &pass_report(vec![finding])).await?;
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
        identify_findings(&mut ledger, &lesson(GOOD), &pass_report(vec![finding])).await?;
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
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    // Sorted again, the rewrite would be Common and meet no consistency check.
    let model = Classes::new(&[REASONED, REWRITTEN])
        .classed(&[(REASONED, "reasoning"), (REWRITTEN, "common")])
        .consistent(&[(REASONED, "contradicted")])
        .rewrites(REASONED, REWRITTEN);

    let (prepared, trace) = crate::lesson_progress::traced(prepare(
        &db,
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
    let ledger = ClaimReviewRepository::new(db.clone())
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
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let model = Classes::new(&[COMMON, REASONED, GOOD])
        .classed(&[(COMMON, "common"), (REASONED, "reasoning")]);
    let body = format!("{COMMON} {REASONED} {GOOD}");
    let sources = [source()];

    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
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
    let ids = identify_findings(&mut ledger, &lesson(&body), &report).await?;
    let read = (Reading::Knowledge, report.findings[0].clone());
    stood(&mut ledger, &ids[0], Class::Common, read).await?;
    let read = (Reading::Parts, report.findings[1].clone());
    stood(&mut ledger, &ids[1], Class::Sourced, read).await?;
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
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let model = Reads::new(
        Classes::new(&[COMMON, REWORDED, GOOD])
            .knows(&[(COMMON, ["true_common"; 3])])
            .rewrites(COMMON, REWORDED),
        &["common", "common"],
    )
    .finds_unstated(&[COMMON]);

    let (raw, _) = prepare(
        &db,
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
    let ledger = ClaimReviewRepository::new(db.clone())
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
async fn a_sourced_statement_the_passages_do_not_state_but_every_sample_confirms_is_general_knowledge_and_never_rewritten(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let model = Reads::new(
        Classes::new(&[COMMON, REWORDED, GOOD])
            .knows(&[(COMMON, ["true_common"; 3])])
            .rewrites(COMMON, REWORDED),
        &["specific_missing"],
    )
    .finds_unstated(&[COMMON]);

    let (raw, _) = prepare(
        &db,
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(two_sections(COMMON, GOOD, true).to_string()),
        &[source()],
    )
    .await?;

    assert_eq!(model.inner.calls("repair"), 0);
    let written: Value = serde_json::from_str(&raw)?;
    assert_eq!(written["blocks"][0]["body"], section(COMMON));
    let ledger = ClaimReviewRepository::new(db.clone())
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
    Ok(())
}

#[tokio::test]
async fn a_common_statement_every_sample_calls_false_stays_and_passes_when_its_sections_passages_state_it(
) -> Result<()> {
    // §1 #5 of the acceptance policy design
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let model = Reads::new(
        Classes::new(&[SKIPPED, GOOD])
            .classed(&[(SKIPPED, "common")])
            .knows(&[(SKIPPED, ["false"; 3])]),
        &["stated"],
    );

    let (raw, _) = prepare(
        &db,
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
    let ledger = ClaimReviewRepository::new(db.clone())
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
async fn a_consistent_statement_every_sample_calls_false_stays_and_passes_when_its_sections_passages_state_it(
) -> Result<()> {
    // §1 #4 of the acceptance policy design
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let model = Reads::new(
        Classes::new(&[REASONED, REWRITTEN, GOOD])
            .classed(&[(REASONED, "reasoning")])
            .knows(&[(REASONED, ["false"; 3])])
            .rewrites(REASONED, REWRITTEN),
        &["stated"],
    )
    .finds_unstated(&[REASONED]);

    let (raw, _) = prepare(
        &db,
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
    let ledger = ClaimReviewRepository::new(db.clone())
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

const CONSISTENCY: Step = Step::Read(Reading::Consistency);
const KNOWLEDGE: Step = Step::Read(Reading::Knowledge);
const GROUND: Step = Step::Read(Reading::Ground);
const PARTS: Step = Step::Read(Reading::Parts);
const VETO: Step = Step::Read(Reading::Veto);

/// One pass over `lesson` with a fresh ledger: its statements identified, read
/// along their graphs as far as the readers take them, and recorded.
async fn one_pass(
    model: &dyn LLMPort,
    lesson: &Value,
    sources: &[LearningSourceDto],
) -> Result<(claim_ledger::ClaimLedger, passes::Pass)> {
    let policy = StandardPolicy::new();
    let references = ReferenceCollection::lexical(sources)?;
    let mut checks = ClaimChecks::default();
    let mut ledger = claim_ledger::ClaimLedger::open(model, &lesson.to_string()).await?;
    let extraction = passes::extract(model, &BuiltinRuntimes::for_tests(), lesson, &checks).await?;
    let pass = passes::identify(
        model,
        &policy,
        &mut ledger,
        (lesson, &references),
        extraction,
        &checks,
    )
    .await?;
    let due = ledger.due(&policy);
    let readings = passes::read_steps(
        model,
        &policy,
        &readings::Readers::standard(),
        &ledger,
        &checks,
        (lesson, &references),
        (&pass, &due),
    )
    .await?;
    passes::record_readings(&policy, &mut ledger, &mut checks, readings).await?;
    Ok((ledger, pass))
}

/// The entry of the pass's statement `index`.
fn statement<'a>(
    ledger: &'a claim_ledger::ClaimLedger,
    pass: &passes::Pass,
    index: usize,
) -> &'a ClaimLedgerEntry {
    let known = &pass.statements[index];
    ledger.entry(&known.id).unwrap()
}

/// The steps the policy has decided for `entry`, in order.
fn steps_of(entry: &ClaimLedgerEntry) -> Vec<(Step, Outcome)> {
    entry
        .events()
        .iter()
        .filter_map(|record| match &record.event {
            ClaimEvent::Stepped { step, outcome, .. } => Some((*step, *outcome)),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_reasoning_statement_a_passage_contradicts_waits_at_its_rewrite_with_the_judges_reason_and_is_never_put_to_the_veto(
) -> Result<()> {
    let model = Classes::new(&[REASONED])
        .classed(&[(REASONED, "reasoning")])
        .consistent(&[(REASONED, "contradicted")]);

    let (ledger, pass) = one_pass(&model, &lesson(REASONED), &[source()]).await?;

    let entry = statement(&ledger, &pass, 0);
    assert_eq!(node_of(&ledger, entry.claim_id()), Some("r.rewrite"));
    assert!(
        matches!(
            entry.state(),
            ClaimState::Open { kind: VerdictKind::Contradicted, reason, .. }
                if reason == "The passages say otherwise."
        ),
        "{:?}",
        entry.state()
    );
    assert_eq!(model.calls("consistency"), 1);
    assert_eq!(model.calls("knowledge"), 0);
    Ok(())
}

#[tokio::test]
async fn a_reasoning_statement_with_an_unfounded_specific_enters_sourced_by_evidence_and_is_read_by_parts_in_the_same_pass(
) -> Result<()> {
    let model = Classes::new(&[REASONED])
        .classed(&[(REASONED, "reasoning")])
        .consistent(&[(REASONED, "unfounded_specific")]);

    let (ledger, pass) = one_pass(&model, &lesson(REASONED), &[source()]).await?;

    let entry = statement(&ledger, &pass, 0);
    assert_eq!(
        entry.classified(),
        Some((Class::Sourced, ClassifiedBy::Evidence))
    );
    // Its section cites nothing, and the course's page has nothing on its
    // subject.
    assert_eq!(
        steps_of(entry),
        [
            (CONSISTENCY, Outcome::NeedsSource),
            (PARTS, Outcome::Unproven)
        ]
    );
    assert_eq!(model.calls("knowledge"), 0);
    Ok(())
}

#[tokio::test]
async fn a_common_statement_ends_as_general_knowledge_only_when_every_sample_agrees() -> Result<()>
{
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
            (FALSE, ["false"; 3]),
            (LONE, ["true_common", "true_common", "false"]),
            (WANTS, ["needs_source", "true_common", "true_common"]),
        ]);
    let body = format!("{COMMON} {FALSE} {LONE} {WANTS}");

    let (ledger, pass) = one_pass(&model, &lesson(&body), &[source()]).await?;

    let [unanimous, refuted, lone, wants] =
        [0, 1, 2, 3].map(|index| statement(&ledger, &pass, index));
    // Unanimity ends as general knowledge.
    assert!(matches!(
        unanimous.state(),
        ClaimState::GeneralKnowledge { .. }
    ));
    assert_eq!(
        unanimous.classified(),
        Some((Class::Common, ClassifiedBy::Model))
    );
    // A statement every sample calls false is refuted, and its knowledge reading's check says so.
    assert_eq!(
        steps_of(refuted),
        [(KNOWLEDGE, Outcome::Refuted), (GROUND, Outcome::Refuted)]
    );
    assert_eq!(node_of(&ledger, refuted.claim_id()), Some("c.rewrite"));
    let reasons: Vec<&str> = refuted
        .events()
        .iter()
        .filter_map(|record| match &record.event {
            ClaimEvent::Checked(check) => Some(check.reason.as_str()),
            _ => None,
        })
        .collect();
    assert!(reasons[0].contains("(3 of 3)"), "{reasons:?}");
    assert_eq!(
        refuted.classified(),
        Some((Class::Common, ClassifiedBy::Model))
    );
    // A lone dissent forfeits the pass without failing the statement, as a
    // request for a source does: both enter Sourced, and are read by Parts.
    // Their section cites nothing, so Parts reads them on the course's page,
    // which states both.
    for entry in [lone, wants] {
        assert_eq!(
            entry.classified(),
            Some((Class::Sourced, ClassifiedBy::Evidence))
        );
        assert_eq!(
            steps_of(entry),
            [(KNOWLEDGE, Outcome::NeedsSource), (PARTS, Outcome::Stands)]
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_calls_ceiling_only_comes_down_while_the_loop_checks_a_mix_of_classes() -> Result<()> {
    const FALSE: &str = "A header row names the columns of a CSV file after its data.";
    const FIXED: &str = "A header row names the columns of a CSV file before its data.";
    const LONE: &str = "DictReader keeps whitespace in field names.";
    const WANTS: &str = "Missing values in nonblank rows default to None.";
    let model = Classes::new(&[COMMON, FALSE, FIXED, LONE, WANTS, REASONED, GOOD])
        .classed(&[
            (COMMON, "common"),
            (FALSE, "common"),
            (LONE, "common"),
            (WANTS, "common"),
            (REASONED, "reasoning"),
        ])
        .knows(&[
            (FALSE, ["false"; 3]),
            (LONE, ["true_common", "true_common", "false"]),
            (WANTS, ["needs_source", "true_common", "true_common"]),
        ])
        .rewrites(FALSE, FIXED);
    let body = section(&format!(
        "{COMMON} {FALSE} {LONE} {WANTS} {REASONED} {GOOD}"
    ));

    let (finished, trace) = crate::lesson_progress::traced(verify_and_repair(
        &model,
        "{}",
        &json!({}),
        lesson(&body).to_string(),
        &[source()],
        3000,
    ))
    .await;

    // The statements sorted: the plan's first event, and each pass that moves
    // them through their graphs another.
    assert!(!trace.ceilings.is_empty(), "{:?}", trace.ceilings);
    assert!(trace.only_came_down(), "{:?}", trace.ceilings);
    assert!(
        trace.within_the_ceiling(),
        "{} calls, ceilings {:?}, finished {:?}",
        trace.calls,
        trace.ceilings,
        finished.as_ref().map(|_| ())
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_a_repair_wrote_keeps_its_parents_class_with_no_sort_and_is_read_where_its_rewrite_left_it(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let model = Classes::new(&[]);
    // A repair that wrote it over a checked statement keeps that statement's
    // class, and the check.
    let mut ledger = open().await?;
    let parent = claim(
        &mut ledger,
        "parent",
        (COMMON, COMMON),
        COMMON,
        (Class::Common, ClassifiedBy::Model),
    )
    .await?;
    walk(
        &mut ledger,
        &parent,
        Class::Common,
        &[(KNOWLEDGE, Outcome::Refuted), (GROUND, Outcome::Refuted)],
    )
    .await?;
    ledger
        .begin_repairs(&policy, &lesson(COMMON), std::slice::from_ref(&parent))
        .await?;

    let pass = identified(
        &mut ledger,
        &model,
        &lesson(REWRITTEN),
        &[(REWRITTEN, REWRITTEN)],
        &[],
    )
    .await?;

    let [kept] = pass.statements.as_slice() else {
        panic!("the rewrite is a statement: {:?}", pass.statements.len());
    };
    assert_eq!(kept.id, parent);
    assert_eq!(
        ledger.entry(&parent).unwrap().classified(),
        Some((Class::Common, ClassifiedBy::Model))
    );
    assert_eq!(model.calls("sort"), 0);
    // It is read from where its rewrite left it.
    assert_eq!(node_of(&ledger, &parent), Some("c.final"));
    assert_eq!(
        ledger
            .due(&policy)
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        [parent]
    );
    Ok(())
}

#[tokio::test]
async fn the_class_a_rewrite_took_is_kept_by_a_resumed_pass_after_the_repairs_record_is_gone(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    // Sorted again, the rewrite would be Common.
    let model = Classes::new(&[]).classed(&[(REWRITTEN, "common")]);
    let policy = StandardPolicy::new();

    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        lesson_drafts::resume("class-fixture".into()).await?;
        let mut ledger = open().await?;
        let parent = claim(
            &mut ledger,
            "parent",
            (REASONED, REASONED),
            REASONED,
            (Class::Reasoning, ClassifiedBy::Model),
        )
        .await?;
        walk(
            &mut ledger,
            &parent,
            Class::Reasoning,
            &[(CONSISTENCY, Outcome::Refuted)],
        )
        .await?;
        ledger
            .begin_repairs(&policy, &lesson(REASONED), std::slice::from_ref(&parent))
            .await?;
        let first = identified(
            &mut ledger,
            &model,
            &lesson(REWRITTEN),
            &[(REWRITTEN, REWRITTEN)],
            &[],
        )
        .await?;
        assert_eq!(first.statements[0].id, parent);

        // After a restart the repair's record is gone.
        let mut resumed = open().await?;
        assert!(resumed.pending.repairs.is_empty());
        let again = identified(
            &mut resumed,
            &model,
            &lesson(REWRITTEN),
            &[(REWRITTEN, REWRITTEN)],
            &[],
        )
        .await?;

        assert_eq!(again.statements[0].id, parent);
        assert_eq!(
            resumed.entry(&parent).unwrap().classified(),
            Some((Class::Reasoning, ClassifiedBy::Model))
        );
        assert_eq!(model.calls("sort"), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_resumed_pass_whose_readings_are_all_restored_counts_each_statement_reused() -> Result<()>
{
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let model = Classes::new(&[COMMON, REASONED, GOOD])
        .classed(&[(COMMON, "common"), (REASONED, "reasoning")]);
    let cited_section = json!({"kind":"explanation","title":"Cited","body":GOOD,"rubric":[],
        "sourceIndex":0,"quote":GOOD,"citations":[{"sentence":GOOD,"sourceIndex":0,"quote":GOOD}]});
    let uncited_section = json!({"kind":"explanation","title":"Uncited","body":format!("{COMMON} {REASONED}"),"rubric":[]});
    let lesson = json!({"blocks":[uncited_section, cited_section],"questions":[]});
    let sources = [source()];
    let policy = StandardPolicy::new();
    let readers = readings::Readers::standard();

    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        lesson_drafts::resume("class-fixture".into()).await?;
        let references = ReferenceCollection::lexical(&sources)?;
        let mut checks = ClaimChecks::default();
        let mut ledger = claim_ledger::ClaimLedger::open(&model, &lesson.to_string()).await?;
        let extraction =
            passes::extract(&model, &BuiltinRuntimes::for_tests(), &lesson, &checks).await?;
        let pass = passes::identify(
            &model,
            &policy,
            &mut ledger,
            (&lesson, &references),
            extraction,
            &checks,
        )
        .await?;
        let due = ledger.due(&policy);
        assert_eq!(due.len(), 3);
        // A run that read the statements and stopped before it recorded them
        // leaves its judgments as saved checks.
        passes::read_steps(
            &model,
            &policy,
            &readers,
            &ledger,
            &checks,
            (&lesson, &references),
            (&pass, &due),
        )
        .await?;
        let asked = (model.calls("knowledge"), model.calls("consistency"));

        // The run that resumes it reads them again, and records.
        let (recorded, trace) = crate::lesson_progress::traced(async {
            let readings = passes::read_steps(
                &model,
                &policy,
                &readers,
                &ledger,
                &checks,
                (&lesson, &references),
                (&pass, &due),
            )
            .await?;
            passes::record_readings(&policy, &mut ledger, &mut checks, readings).await
        })
        .await;

        assert!(recorded?.is_empty());
        assert_eq!(
            (model.calls("knowledge"), model.calls("consistency")),
            asked,
            "nothing was asked again"
        );
        assert_eq!((trace.checked, trace.reused), (3, 3));
        Ok(())
    })
    .await
}

/// Statements about the exercise itself, which the rules leave to the model
/// and the model sorts Internal: one its samples call false, one they do not.
const MISSTATED: &str =
    "In this exercise the header row names the columns of the data that comes before it.";
const SCENARIO: &str =
    "In this exercise the reader opens the file once and keeps it open until the last row.";

#[tokio::test]
async fn a_statement_the_model_calls_internal_is_put_to_the_veto_and_then_to_its_passages_and_waits_at_its_rewrite_when_neither_clears_it(
) -> Result<()> {
    // §1 #15 of the acceptance policy design
    let model = Classes::new(&[MISSTATED])
        .classed(&[(MISSTATED, "internal")])
        .knows(&[(MISSTATED, ["false"; 3])]);

    let (ledger, pass) = one_pass(&model, &lesson(MISSTATED), &[source()]).await?;

    // Its samples were asked: the statement is read, not left out.
    assert_eq!(model.calls("knowledge"), 3);
    assert_eq!(pass.statements.len(), 1);
    let entry = statement(&ledger, &pass, 0);
    assert_eq!(
        entry.classified(),
        Some((Class::Internal, ClassifiedBy::Model))
    );
    // A statement every sample calls false is refuted, and its section cites no passage that could
    // ground it.
    assert_eq!(
        steps_of(entry),
        [(VETO, Outcome::Refuted), (GROUND, Outcome::Refuted)]
    );
    assert_eq!(node_of(&ledger, entry.claim_id()), Some("i.rewrite"));
    assert!(
        matches!(
            entry.state(),
            ClaimState::Open {
                kind: VerdictKind::Contradicted,
                ..
            }
        ),
        "{:?}",
        entry.state()
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_the_model_calls_internal_that_its_samples_do_not_call_false_ends_as_not_a_claim_after_three_calls_and_the_lesson_publishes_it(
) -> Result<()> {
    // §1 #15 of the acceptance policy design
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let model = Classes::new(&[SCENARIO]).classed(&[(SCENARIO, "internal")]);

    let (raw, report) = prepare(
        &db,
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(lesson(&section(SCENARIO)).to_string()),
        &[source()],
    )
    .await?;

    assert_eq!(model.calls("knowledge"), 3);
    let written: Value = serde_json::from_str(&raw)?;
    assert!(
        written["blocks"][0]["body"]
            .as_str()
            .is_some_and(|body| body.contains(SCENARIO)),
        "the sentence stays in the lesson, which now reads: {}",
        written["blocks"][0]["body"]
    );
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    // The learner is shown no class for it (`learner_view`), but the report
    // answers for it like any statement.
    assert_eq!(
        outcomes(&report),
        vec![(SCENARIO, ClaimVerdict::Supported, Some(Class::Internal))]
    );
    let ledger = ClaimReviewRepository::new(db.clone())
        .load(&lesson_id)
        .await?;
    let claim = ledger
        .claims
        .iter()
        .find(|claim| claim.statement() == SCENARIO);
    assert_eq!(claim.map(steps_of), Some(vec![(VETO, Outcome::Stands)]));
    assert_eq!(
        claim.and_then(|claim| claim.decided()),
        Some(&Then::End(End::NotAClaim))
    );
    assert!(
        claim.is_some_and(|claim| claim.state().is_resolved()),
        "{:?}",
        claim.map(|claim| claim.state())
    );
    Ok(())
}

/// What a pass extracts from a lesson whose statements are their own quotes:
/// each claim as its section's index and what it states, grouped by section.
fn extracting(claims: &[(usize, &str)]) -> passes::Extraction {
    let mut units: Vec<UnitClaims> = Vec::new();
    for (index, statement) in claims {
        let claim = Claim {
            quote: (*statement).to_owned(),
            statement: (*statement).to_owned(),
        };
        match units.iter_mut().find(|unit| unit.index == *index) {
            Some(unit) => unit.claims.push(claim),
            None => units.push(UnitClaims {
                index: *index,
                claims: vec![claim],
                non_factual_reason: String::new(),
            }),
        }
    }
    passes::Extraction {
        inventory: Inventory { units },
        coverage: Coverage { units: vec![] },
        executions: vec![],
    }
}

#[tokio::test]
async fn what_the_rules_make_internal_never_enters_the_ledger_and_asks_the_model_nothing(
) -> Result<()> {
    const RUBRIC: &str = "The answer names two trade-offs between the designs.";
    const OPTION: &str = "Each stage re-reads the page image.";
    let model = Classes::new(&[]);
    // A rubric line and an option.
    let candidate = json!({
        "blocks": [{"kind":"explanation","title":"Rows","body":"A stage hands its output to the next.",
            "rubric":[{"title":"Trade-offs","description":RUBRIC}]}],
        "questions": [{"kind":"practice","prompt":"Which stage re-reads the page?",
            "options":["Only the first stage.", OPTION],"correctIndex":1,
            "explanation":"The last one does."}]
    });
    let references = ReferenceCollection::lexical(&[])?;
    let mut ledger = open().await?;

    let pass = passes::identify(
        &model,
        &StandardPolicy::new(),
        &mut ledger,
        (&candidate, &references),
        extracting(&[(0, RUBRIC), (1, OPTION)]),
        &ClaimChecks::default(),
    )
    .await?;

    assert!(pass.statements.is_empty());
    assert!(ledger.entries().is_empty(), "{:?}", ledger.entries());
    assert_eq!(
        (
            model.calls("sort"),
            model.calls("consistency"),
            model.calls("knowledge"),
            model.strict_calls()
        ),
        (0, 0, 0, 0)
    );
    Ok(())
}

#[tokio::test]
async fn the_ceiling_made_with_the_identified_pass_counts_a_statement_the_model_calls_internal_from_its_entry_and_one_the_rules_leave_out_not_at_all(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let model = Classes::new(&[SCENARIO]).classed(&[(SCENARIO, "internal")]);
    let candidate = json!({"blocks":[{"kind":"explanation","title":"Rows","body":SCENARIO,
        "rubric":[{"title":"Trade-offs","description":RUBRIC_LINE}]}],"questions":[]});
    let references = ReferenceCollection::lexical(&[])?;
    let mut ledger = open().await?;

    let (done, trace) = crate::lesson_progress::traced(async {
        let pass = crate::lesson_progress::checks(passes::identify(
            &model,
            &policy,
            &mut ledger,
            (&candidate, &references),
            extracting(&[(0, SCENARIO), (0, RUBRIC_LINE)]),
            &ClaimChecks::default(),
        ))
        .await?;
        repair_loop::plan_moved(&policy, &ledger, &candidate);
        Ok::<_, AppError>(pass)
    })
    .await;

    let pass = done?;
    let statements: Vec<&str> = pass
        .statements
        .iter()
        .map(|known| known.claim.statement.as_str())
        .collect();
    assert_eq!(statements, [SCENARIO]);
    // The one sorting call, made; the model's statement at the longest path of
    // the Internal graph from its entry; a flow repair for its section; and
    // nothing for the rubric line the rules left out.
    let entry = start(&policy, Class::Internal);
    assert_eq!(trace.calls, 1);
    assert_eq!(trace.ceilings, [1 + most_calls(&policy, entry) + 1]);
    Ok(())
}

#[tokio::test]
async fn a_claim_the_ledger_holds_as_the_models_internal_is_identified_again_and_never_sorted_again(
) -> Result<()> {
    // Sorted again, the model would call it Sourced.
    let model = Classes::new(&[]).classed(&[(SCENARIO, "sourced")]);
    let mut ledger = open().await?;
    let held = claim(
        &mut ledger,
        "held",
        (SCENARIO, SCENARIO),
        SCENARIO,
        (Class::Internal, ClassifiedBy::Model),
    )
    .await?;

    let pass = identified(
        &mut ledger,
        &model,
        &lesson(SCENARIO),
        &[(SCENARIO, SCENARIO)],
        &[],
    )
    .await?;

    assert_eq!(model.calls("sort"), 0);
    let ids: Vec<&ClaimId> = pass.statements.iter().map(|known| &known.id).collect();
    assert_eq!(ids, [&held]);
    assert_eq!(
        ledger.entry(&held).unwrap().classified(),
        Some((Class::Internal, ClassifiedBy::Model))
    );
    assert_eq!(node_of(&ledger, &held), Some("i.veto"));
    Ok(())
}

#[test]
fn only_what_the_rule_layer_makes_internal_is_left_out_of_the_claims() {
    let classed = |class, by| Classed { class, by };
    let kept = [
        (Class::Internal, ClassifiedBy::Model),
        (Class::Sourced, ClassifiedBy::Rule),
        (Class::Sourced, ClassifiedBy::Writer),
        (Class::Reasoning, ClassifiedBy::Model),
        (Class::Common, ClassifiedBy::Model),
        (Class::Reasoning, ClassifiedBy::Evidence),
    ];

    for (class, by) in kept {
        assert!(classed(class, by).checked(), "{class:?} by {by:?}");
    }
    assert!(!classed(Class::Internal, ClassifiedBy::Rule).checked());
}

#[tokio::test]
async fn the_plan_and_the_pass_agree_on_which_statements_are_checked_so_the_ceiling_only_comes_down_beside_one_the_model_calls_internal(
) -> Result<()> {
    let model = Classes::new(&[SCENARIO, GOOD]).classed(&[(SCENARIO, "internal")]);
    let body = section(&format!("{SCENARIO} {GOOD}"));

    let (finished, trace) = crate::lesson_progress::traced(verify_and_repair(
        &model,
        "{}",
        &json!({}),
        lesson(&body).to_string(),
        &[source()],
        3000,
    ))
    .await;

    let (_, report) = finished?;
    assert_eq!(
        outcomes(&report),
        vec![
            (SCENARIO, ClaimVerdict::Supported, Some(Class::Internal)),
            (GOOD, ClaimVerdict::Supported, Some(Class::Sourced)),
        ]
    );
    // The report accounts for both, as the pass identified them.
    let accounted: Vec<&str> = report.coverage[0]
        .claims
        .iter()
        .map(|claim| claim.statement.as_str())
        .collect();
    assert_eq!(accounted, [SCENARIO, GOOD]);
    assert!(!trace.ceilings.is_empty(), "{:?}", trace.ceilings);
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
async fn a_sentence_that_only_names_the_documentation_is_the_models_to_sort() -> Result<()> {
    const NAMES: &str = "The documentation is available from the tool's own help screen.";
    let model = Classes::new(&[NAMES]).classed(&[(NAMES, "common")]);
    let claims = [(
        0,
        Claim {
            quote: NAMES.to_owned(),
            statement: NAMES.to_owned(),
        },
    )];

    let classes = class_checks::sort(
        &model,
        &lesson(NAMES),
        &claims,
        &[None],
        &Citations::default(),
    )
    .await?;

    assert_eq!(
        classes,
        [Classed {
            class: Class::Common,
            by: ClassifiedBy::Model
        }]
    );
    Ok(())
}

#[tokio::test]
async fn a_saved_class_is_the_models_answer_and_is_settled_where_it_is_restored() -> Result<()> {
    const CASE: &str = "5 documents have confidence scores below 0.70.";
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let model = Classes::new(&[CASE]).classed(&[(CASE, "internal")]);
    let example = json!({"blocks":[{"kind":"worked_example","title":"Thresholds",
        "body":CASE,"rubric":[]}],"questions":[]});
    let claims = [(
        0,
        Claim {
            quote: CASE.to_owned(),
            statement: CASE.to_owned(),
        },
    )];

    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        lesson_drafts::resume("class-fixture".into()).await?;
        // An explanation settles what the model calls Internal to Reasoning.
        let stated = class_checks::sort(
            &model,
            &lesson(CASE),
            &claims,
            &[None],
            &Citations::default(),
        )
        .await?;
        assert_eq!(
            stated,
            [Classed {
                class: Class::Reasoning,
                by: ClassifiedBy::Model
            }]
        );
        // A worked example's ruling leaves it as the model sorted it.
        let hypothetical =
            class_checks::sort(&model, &example, &claims, &[None], &Citations::default()).await?;
        assert_eq!(
            hypothetical,
            [Classed {
                class: Class::Internal,
                by: ClassifiedBy::Model
            }]
        );
        assert_eq!(model.calls("sort"), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_statement_naming_context_its_sentence_leaves_implicit_is_sourced_by_the_writer_who_cited_the_sentence(
) -> Result<()> {
    use super::recite_tests::{IMPLICIT_SENTENCE, IMPLICIT_STATEMENT};
    assert!(lattice_core::shared::text::claim_sentence(
        IMPLICIT_SENTENCE,
        (0, IMPLICIT_SENTENCE.len()),
        IMPLICIT_STATEMENT
    )
    .is_none());
    let model = Classes::new(&[]);
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let candidate = cited(IMPLICIT_SENTENCE);
    let citations = section_citations(&candidate, &references)?;
    let claims = vec![(
        0,
        Claim {
            quote: IMPLICIT_SENTENCE.to_owned(),
            statement: IMPLICIT_STATEMENT.to_owned(),
        },
    )];

    let classes = class_checks::sort(&model, &candidate, &claims, &[None], &citations).await?;

    assert_eq!(
        classes,
        [Classed {
            class: Class::Sourced,
            by: ClassifiedBy::Writer
        }]
    );
    assert_eq!(model.calls("sort"), 0);
    Ok(())
}

#[tokio::test]
async fn a_statement_sharing_words_with_another_cited_sentence_is_not_sourced_by_the_writer(
) -> Result<()> {
    const CITED: &str =
        "The installer verifies the written image against its checksum before it reports success.";
    const UNCITED: &str = "Remove the card only after the installer reports that it has finished.";
    const STATEMENT: &str = "The installer verifies the written image against its checksum.";
    assert!(
        lattice_core::shared::text::claim_sentence(CITED, (0, CITED.len()), STATEMENT).is_some()
    );
    let model = Classes::new(&[]);
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let candidate = json!({"blocks":[{"kind":"explanation","title":"CSV header names",
        "body":format!("{CITED} {UNCITED}"),"rubric":[],"sourceIndex":0,"quote":GOOD,
        "citations":[{"sentence":CITED,"sourceIndex":0,"quote":GOOD}]}],"questions":[]});
    let citations = section_citations(&candidate, &references)?;
    let claims = vec![(
        0,
        Claim {
            quote: UNCITED.to_owned(),
            statement: STATEMENT.to_owned(),
        },
    )];

    let classes = class_checks::sort(&model, &candidate, &claims, &[None], &citations).await?;

    assert_eq!(classes.len(), 1);
    assert_ne!(
        classes[0].by,
        ClassifiedBy::Writer,
        "the writer cited another sentence"
    );
    Ok(())
}
