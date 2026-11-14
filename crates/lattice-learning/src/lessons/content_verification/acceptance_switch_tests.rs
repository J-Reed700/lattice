//! The switch (acceptance design §2.10): the acceptance policy decides every
//! machine-owned statement's next step. A pass reads the statements that wait
//! for a reading, the remedies act on the statements at their nodes and record
//! their outcome, and the loop's decisions follow from the nodes the statements
//! stand at.
use super::super::claim_ledger::{Decision, StepKind};
use super::super::class_checks::{finding_of, unjudged_finding};
use super::super::passes::{self, Extraction, Pass};
use super::super::readings::driver::ReadStep;
use super::super::readings::{Read, ReadContext, Reader, ReaderSet, Statement};
use super::super::research::Unresearched;
use super::claim_ledger_tests::{figure, pass_report, prepare, section, Figures};
use super::class_checks_tests::Classes;
use super::*;
use crate::lesson_drafts;
use crate::lessons::claim_reviews::dto::ClaimActionRequestDto;
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::lessons::claim_reviews::service::ClaimReviewService;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::acceptance::{
    decide, rewrite_ahead, start, AcceptancePolicy, End, Goal, Outcome, Position, Reading, Step,
    Taken, Then, Walked,
};
use lattice_core::domain::learning::claim_review::{
    text_sha256, Actor, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLedgerEntry,
    ClaimLocation, ClaimState, EvidenceFingerprint, EvidenceSeen, LessonReviewState, ReopenCause,
    RepairOutcome, StopCode, UnitKind, UnverifiedReason, VerdictKind,
};
use lattice_core::shared::persistence::Database;
use std::collections::VecDeque;

const SENTENCE: &str = "The reader hands over one row at a time.";
const REWRITTEN: &str = "The reader hands over a single row at a time.";
const HEADER: &str = "A header row names the columns of the file.";
const HEADER_REWORDED: &str = "The first row of the file gives the column names.";
const HEADER_REWORDED_AGAIN: &str = "Column names come from the file's header row.";
const REWORDED: &str = "Each time it is asked, the reader gives one row.";

const PARTS: Step = Step::Read(Reading::Parts);
const PAGE: Step = Step::Read(Reading::Page);
const CONSISTENCY: Step = Step::Read(Reading::Consistency);
const VETO: Step = Step::Read(Reading::Veto);
const REWRITE: Step = Step::Rewrite(Goal::WhatThePassageStates);

/// Steps that leave a Sourced statement at `s.research`, `s.rewrite`,
/// `s.recite`, `s.cut`.
pub(super) const RESEARCHING: [(Step, Outcome); 1] = [(PARTS, Outcome::Unproven)];
pub(super) const REWRITING: [(Step, Outcome); 3] = [
    (PARTS, Outcome::Unproven),
    (Step::Research, Outcome::FoundNothing),
    (PAGE, Outcome::Refuted),
];
const RECITING: [(Step, Outcome); 3] = [
    (PARTS, Outcome::Unproven),
    (Step::Research, Outcome::FoundNothing),
    (PAGE, Outcome::StandsElsewhere),
];
pub(super) const CUTTING: [(Step, Outcome); 5] = [
    (PARTS, Outcome::Unproven),
    (Step::Research, Outcome::FoundNothing),
    (PAGE, Outcome::Refuted),
    (REWRITE, Outcome::Rewritten),
    (PARTS, Outcome::Refuted),
];
/// Steps that end a Reasoning statement Consistent.
const CONSISTENT: [(Step, Outcome); 2] = [(CONSISTENCY, Outcome::Stands), (VETO, Outcome::Stands)];

pub(in crate::lessons::content_verification) const SOURCED: (Class, ClassifiedBy) =
    (Class::Sourced, ClassifiedBy::Writer);
pub(super) const REASONED: (Class, ClassifiedBy) = (Class::Reasoning, ClassifiedBy::Model);

pub(super) fn lesson_of(body: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Rows","body":body,"rubric":[]}],"questions":[]})
}

pub(in crate::lessons::content_verification) async fn open() -> Result<claim_ledger::ClaimLedger> {
    claim_ledger::ClaimLedger::open(&Model::new(), "{}").await
}

/// A claim of section 0 the ledger holds, standing on `quote` of `body`,
/// sorted into `class` by `by`.
pub(in crate::lessons::content_verification) async fn claim(
    ledger: &mut claim_ledger::ClaimLedger,
    name: &str,
    (quote, statement): (&str, &str),
    body: &str,
    (class, by): (Class, ClassifiedBy),
) -> Result<ClaimId> {
    let id = ClaimId(name.into());
    let location = ClaimLocation::locate(0, UnitKind::Block, "body", quote, body);
    let extracted = ClaimEvent::Extracted {
        location,
        statement: statement.into(),
    };
    ledger.append(&id, extracted).await?;
    ledger
        .append(&id, ClaimEvent::Classified { class, by })
        .await?;
    Ok(id)
}

/// Records `steps` for `id`, a statement of `class`, each with the decision the
/// policy makes of it.
pub(super) async fn walk(
    ledger: &mut claim_ledger::ClaimLedger,
    id: &ClaimId,
    class: Class,
    steps: &[(Step, Outcome)],
) -> Result<()> {
    let because: Vec<_> = steps
        .iter()
        .map(|&(step, outcome)| (step, outcome, "Because."))
        .collect();
    walk_because(ledger, id, class, &because).await
}

/// `walk`, each step giving its own reason.
pub(super) async fn walk_because(
    ledger: &mut claim_ledger::ClaimLedger,
    id: &ClaimId,
    class: Class,
    steps: &[(Step, Outcome, &str)],
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut here = Some(start(&policy, class));
    for &(step, outcome, reason) in steps {
        let at = here.expect("a step after the statement ended");
        let (then, next) = decide(&policy, at, Taken { step, outcome }).expect("an edge");
        here = next;
        let stepped = ClaimEvent::Stepped {
            policy: policy.version().into(),
            step,
            outcome,
            then,
            reason: reason.into(),
            rewrite_ahead: next.is_some_and(|next| rewrite_ahead(&policy, next)),
        };
        ledger.append(id, stepped).await?;
    }
    Ok(())
}

/// Records the first reading of `id`, a statement of `class`, which stood: the
/// check it observed, and the decision the policy makes of it.
pub(super) async fn stood(
    ledger: &mut claim_ledger::ClaimLedger,
    id: &ClaimId,
    class: Class,
    (reading, finding): (Reading, Finding),
) -> Result<()> {
    let policy = StandardPolicy::new();
    let at = start(&policy, class);
    let taken = Taken {
        step: Step::Read(reading),
        outcome: Outcome::Stands,
    };
    let (then, next) = decide(&policy, at, taken).expect("an edge");
    let step = ReadStep {
        reading,
        read: Read::new(Outcome::Stands, finding, false),
        then,
        rewrite_ahead: next.is_some_and(|next| rewrite_ahead(&policy, next)),
        class: next
            .map(|next| next.class)
            .filter(|changed| *changed != class),
    };
    ledger.record_reading(&policy, id, &step).await
}

/// The statements of section 0 as an extraction holds them: a quote and what
/// it states.
fn extraction(claims: &[(&str, &str)]) -> Extraction {
    let claims = claims
        .iter()
        .map(|(quote, statement)| Claim {
            quote: (*quote).into(),
            statement: (*statement).into(),
        })
        .collect();
    Extraction {
        inventory: Inventory {
            units: vec![UnitClaims {
                index: 0,
                claims,
                non_factual_reason: String::new(),
            }],
        },
        coverage: Coverage { units: vec![] },
        executions: vec![],
    }
}

/// A pass over `lesson` that identifies `claims` with the ledger's.
pub(super) async fn identified(
    ledger: &mut claim_ledger::ClaimLedger,
    model: &dyn LLMPort,
    lesson: &Value,
    claims: &[(&str, &str)],
    sources: &[LearningSourceDto],
) -> Result<Pass> {
    let references = ReferenceCollection::lexical(sources)?;
    passes::identify(
        model,
        &StandardPolicy::new(),
        ledger,
        (lesson, &references),
        extraction(claims),
        &ClaimChecks::default(),
    )
    .await
}

/// The course the claims resolved on `source-1` are read in.
fn course() -> Vec<LearningSourceDto> {
    vec![LearningSourceDto {
        id: "source-1".into(),
        title: "The reader".into(),
        url: None,
        excerpt: SENTENCE.into(),
        acquired_at: 0,
    }]
}

/// A finding that a page supports `statement` of section 0 on the bytes `at`
/// of `source`, which its section does not cite.
fn elsewhere(statement: &str, source: &str, at: std::ops::Range<usize>) -> Finding {
    Finding {
        unit: 0,
        quote: statement.into(),
        statement: statement.into(),
        verdict: ClaimVerdict::Supported,
        reason: "Stated elsewhere.".into(),
        evidence: vec![],
        supporting_quote: None,
        resolution: None,
        trust: None,
        label: None,
        class: None,
        claim_id: None,
        two_pass: TwoPass {
            supported_by: Some(SupportedElsewhere {
                source_id: source.into(),
                start_byte: at.start,
                end_byte: at.end,
                cited: vec![],
            }),
            ..Default::default()
        },
    }
}

fn passage(source: &str) -> EvidencePassage {
    EvidencePassage {
        source_id: source.into(),
        text: SENTENCE.into(),
        start_byte: 0,
        end_byte: SENTENCE.len(),
        retrieval_kind: "lexical".into(),
        score: 1.0,
    }
}

/// A reader that answers each statement it reads with the next outcome it is
/// given (the last again), and keeps what it was asked.
struct Says {
    outcomes: Mutex<VecDeque<Outcome>>,
    /// Whether each reading was restored from a saved check, in the same order
    /// (the last again).
    restored: Mutex<VecDeque<bool>>,
    asked: Mutex<Vec<String>>,
}

impl Says {
    fn new(outcomes: &[Outcome]) -> Self {
        Self::restoring(outcomes, &[false])
    }

    fn restoring(outcomes: &[Outcome], restored: &[bool]) -> Self {
        Self {
            outcomes: Mutex::new(outcomes.iter().copied().collect()),
            restored: Mutex::new(restored.iter().copied().collect()),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl ReaderSet for Says {
    fn get(&self, _: Reading) -> &dyn Reader {
        self
    }
}

#[async_trait::async_trait]
impl Reader for Says {
    fn reading(&self) -> Reading {
        Reading::Parts
    }

    async fn read(&self, _: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>> {
        let mut reads = Vec::new();
        for statement in statements {
            self.asked
                .lock()
                .unwrap()
                .push(statement.claim.statement.clone());
            let mut outcomes = self.outcomes.lock().unwrap();
            let next = if outcomes.len() > 1 {
                outcomes.pop_front()
            } else {
                outcomes.front().copied()
            };
            let outcome = next.expect("an outcome to answer with");
            let mut restored = self.restored.lock().unwrap();
            let reused = if restored.len() > 1 {
                restored.pop_front()
            } else {
                restored.front().copied()
            }
            .unwrap_or(false);
            let at = (statement.unit, statement.claim, statement.classed);
            let finding = match outcome {
                Outcome::Stands | Outcome::StandsElsewhere => finding_of(
                    at,
                    ClaimVerdict::Supported,
                    "Stated.".into(),
                    &[passage("source-1")],
                    Some(SENTENCE.into()),
                ),
                Outcome::Unjudged => {
                    unjudged_finding(statement.unit, statement.claim, statement.classed, &[])
                }
                _ => finding_of(at, ClaimVerdict::Unsupported, "Unstated.".into(), &[], None),
            };
            reads.push(Read::new(outcome, finding, reused));
        }
        Ok(reads)
    }
}

/// The statements of `pass` read as `reader` reads them, and recorded: the
/// claims the run stopped on.
async fn read_pass(
    ledger: &mut claim_ledger::ClaimLedger,
    reader: &Says,
    pass: &Pass,
    lesson: &Value,
) -> Result<Vec<ClaimId>> {
    let policy = StandardPolicy::new();
    let model = Model::new();
    let references = ReferenceCollection::lexical(&[])?;
    let mut checks = ClaimChecks::default();
    let due = ledger.due(&policy);
    let readings = passes::read_steps(
        &model,
        &policy,
        reader,
        ledger,
        &checks,
        (lesson, &references),
        (pass, &due),
    )
    .await?;
    passes::record_readings(&policy, ledger, &mut checks, readings).await
}

pub(super) fn events_of(entry: &ClaimLedgerEntry) -> Vec<&ClaimEvent> {
    entry.events().iter().map(|record| &record.event).collect()
}

pub(super) fn node_of(ledger: &claim_ledger::ClaimLedger, id: &ClaimId) -> Option<&'static str> {
    let policy = StandardPolicy::new();
    match ledger.position(&policy, id)? {
        Walked::At(at) => policy.graph(at.class).node(at.node).map(|node| node.name),
        _ => None,
    }
}

#[tokio::test]
async fn a_pass_reads_only_statements_at_reading_nodes_and_a_claim_at_an_end_is_not_read_again(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let body = format!("{SENTENCE} {HEADER} {REWRITTEN}");
    let fresh = claim(&mut ledger, "fresh", (SENTENCE, SENTENCE), &body, SOURCED).await?;
    let ended = claim(&mut ledger, "ended", (HEADER, HEADER), &body, REASONED).await?;
    walk(&mut ledger, &ended, Class::Reasoning, &CONSISTENT).await?;
    let waiting = claim(
        &mut ledger,
        "waiting",
        (REWRITTEN, REWRITTEN),
        &body,
        SOURCED,
    )
    .await?;
    walk(&mut ledger, &waiting, Class::Sourced, &RESEARCHING).await?;
    let lesson = lesson_of(&body);
    let claims = [
        (SENTENCE, SENTENCE),
        (HEADER, HEADER),
        (REWRITTEN, REWRITTEN),
    ];

    let pass = identified(&mut ledger, &Model::new(), &lesson, &claims, &[]).await?;
    let reader = Says::new(&[Outcome::Stands]);
    let stopped = read_pass(&mut ledger, &reader, &pass, &lesson).await?;

    // Only the statement at a reading node is read; the one at research waits
    // for it and the one that ended is done.
    assert!(stopped.is_empty());
    assert_eq!(reader.asked(), [SENTENCE]);
    assert_eq!(
        ledger.at(&policy, StepKind::Research),
        std::slice::from_ref(&waiting)
    );
    assert!(matches!(
        ledger.entry(&fresh).unwrap().state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    // The next pass reads none of them: the fresh one ended, the other two
    // wait for no reading.
    let pass = identified(&mut ledger, &Model::new(), &lesson, &claims, &course()).await?;
    assert!(ledger.due(&policy).is_empty());
    read_pass(&mut ledger, &reader, &pass, &lesson).await?;
    assert_eq!(reader.asked(), [SENTENCE], "nothing was asked again");
    Ok(())
}

#[tokio::test]
async fn an_invariant_breach_in_one_statement_holds_only_that_statement() -> Result<()> {
    let mut ledger = open().await?;
    let body = format!("{SENTENCE} {HEADER}");
    let sound = claim(&mut ledger, "sound", (SENTENCE, SENTENCE), &body, SOURCED).await?;
    let broken = claim(
        &mut ledger,
        "broken",
        (HEADER, HEADER),
        &body,
        (Class::Common, ClassifiedBy::Model),
    )
    .await?;
    let lesson = lesson_of(&body);
    let claims = [(SENTENCE, SENTENCE), (HEADER, HEADER)];
    let pass = identified(&mut ledger, &Model::new(), &lesson, &claims, &[]).await?;
    // The sourced statement is read first; the common one is answered with an
    // outcome its node has no edge for.
    let reader = Says::new(&[Outcome::Stands, Outcome::AllCommon]);

    let read = read_pass(&mut ledger, &reader, &pass, &lesson).await;

    assert!(read.is_ok(), "{read:?}");
    assert!(matches!(
        ledger.entry(&sound).unwrap().state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    assert!(!ledger.entry(&broken).unwrap().state().is_verified());
    let faults = ledger.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].claim, broken);
    assert_eq!(
        faults[0].stage,
        lattice_core::domain::learning::claim_review::FaultStage::Take
    );
    Ok(())
}

/// A reader that answers as `Says` does, but quotes for `header` words no
/// passage holds.
struct Misquotes(Says);

impl ReaderSet for Misquotes {
    fn get(&self, _: Reading) -> &dyn Reader {
        self
    }
}

#[async_trait::async_trait]
impl Reader for Misquotes {
    fn reading(&self) -> Reading {
        Reading::Parts
    }

    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>> {
        let reads = self.0.read(at, statements).await?;
        Ok(statements
            .iter()
            .zip(reads)
            .map(|(statement, read)| {
                if statement.claim.statement != HEADER {
                    return read;
                }
                let (outcome, mut finding, reused) = read.into_parts();
                finding.supporting_quote = Some("Words no passage holds.".into());
                Read::new(outcome, finding, reused)
            })
            .collect())
    }
}

#[tokio::test]
async fn a_breach_while_recording_one_statement_holds_only_that_statement() -> Result<()> {
    let mut ledger = open().await?;
    let body = format!("{SENTENCE} {HEADER}");
    let sound = claim(&mut ledger, "sound", (SENTENCE, SENTENCE), &body, SOURCED).await?;
    let broken = claim(&mut ledger, "broken", (HEADER, HEADER), &body, SOURCED).await?;
    let lesson = lesson_of(&body);
    let claims = [(SENTENCE, SENTENCE), (HEADER, HEADER)];
    let pass = identified(&mut ledger, &Model::new(), &lesson, &claims, &[]).await?;
    let reader = Misquotes(Says::new(&[Outcome::Stands]));
    let policy = StandardPolicy::new();
    let model = Model::new();
    let references = ReferenceCollection::lexical(&[])?;
    let mut checks = ClaimChecks::default();
    let due = ledger.due(&policy);
    let readings = passes::read_steps(
        &model,
        &policy,
        &reader,
        &ledger,
        &checks,
        (&lesson, &references),
        (&pass, &due),
    )
    .await?;

    let recorded = passes::record_readings(&policy, &mut ledger, &mut checks, readings).await;

    assert!(recorded.is_ok(), "{recorded:?}");
    assert!(matches!(
        ledger.entry(&sound).unwrap().state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    let faults = ledger.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].claim, broken);
    assert_eq!(
        faults[0].stage,
        lattice_core::domain::learning::claim_review::FaultStage::Record
    );
    Ok(())
}

#[tokio::test]
async fn a_sentence_written_over_outside_the_pipeline_is_a_new_statement_with_its_classes_whole_check(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let ended = claim(
        &mut ledger,
        "ended",
        (SENTENCE, SENTENCE),
        SENTENCE,
        REASONED,
    )
    .await?;
    walk(&mut ledger, &ended, Class::Reasoning, &CONSISTENT).await?;
    // Sorted, the statement written in its place is Sourced.
    let model = Classes::new(&[]).classed(&[(HEADER, "sourced")]);

    // No repair the job checkpointed wrote it: where the sentence stood, the
    // section now states something else.
    let lesson = lesson_of(HEADER);
    let pass = identified(&mut ledger, &model, &lesson, &[(HEADER, HEADER)], &[]).await?;

    // The claim ended with its sentence; what is written there is new, sorted
    // and read from its class's entry.
    let written = pass.statements[0].id.clone();
    assert_ne!(written, ended, "its end was reached on another sentence");
    assert!(matches!(
        ledger.entry(&ended).unwrap().state(),
        ClaimState::Removed {
            by: Actor::Machine,
            ..
        }
    ));
    assert_eq!(model.calls("sort"), 1);
    assert_eq!(
        ledger.entry(&written).unwrap().classified(),
        Some((Class::Sourced, ClassifiedBy::Model))
    );
    assert_eq!(node_of(&ledger, &written), Some("s.read"));

    // Its citation does not support it: it goes to research, with its rewrite
    // still ahead, not to a cut.
    let reader = Says::new(&[Outcome::Unproven]);
    read_pass(&mut ledger, &reader, &pass, &lesson).await?;
    assert_eq!(reader.asked(), [HEADER]);
    assert_eq!(node_of(&ledger, &written), Some("s.research"));
    assert!(matches!(
        events_of(ledger.entry(&written).unwrap()).last(),
        Some(ClaimEvent::Stepped {
            rewrite_ahead: true,
            ..
        })
    ));
    assert_eq!(ledger.at(&policy, StepKind::Research), [written]);
    Ok(())
}

#[tokio::test]
async fn a_sentence_a_checkpointed_repair_wrote_after_its_record_was_lost_is_read_after_the_rewrite_and_not_rewritten_again(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let body = format!("{SENTENCE} {HEADER}");
    let mut ledger = open().await?;
    let lost = claim(&mut ledger, "lost", (SENTENCE, SENTENCE), &body, SOURCED).await?;
    let kept = claim(&mut ledger, "kept", (HEADER, HEADER), &body, SOURCED).await?;
    for id in [&lost, &kept] {
        walk(&mut ledger, id, Class::Sourced, &REWRITING).await?;
    }
    ledger
        .begin_repairs(&policy, &lesson_of(&body), &[lost.clone(), kept.clone()])
        .await?;
    // The repair wrote both sentences anew and one claim's record was lost;
    // the repair of their section is still checkpointed, with the lesson it
    // started from.
    ledger
        .pending
        .repairs
        .retain(|record| record.claim_id != lost);
    assert!(ledger.pending.previous.is_some());

    let lesson = lesson_of(&format!("{REWRITTEN} {REWORDED}"));
    let pass = identified(
        &mut ledger,
        &Model::new(),
        &lesson,
        &[(REWRITTEN, REWRITTEN), (REWORDED, REWORDED)],
        &[],
    )
    .await?;

    // What the repair wrote where it has no record of the claim is that
    // claim's successor, read from the node after the rewrite with the class
    // the claim held; the claim whose record stands continues there itself.
    let successor = pass.statements[0].id.clone();
    assert_ne!(successor, lost);
    assert!(matches!(
        ledger.entry(&lost).unwrap().state(),
        ClaimState::Edited {
            successor: Some(next),
            by: Actor::Machine,
            ..
        } if *next == successor
    ));
    assert_eq!(
        ledger.entry(&successor).unwrap().classified(),
        Some(SOURCED)
    );
    assert_eq!(node_of(&ledger, &successor), Some("s.final"));
    assert_eq!(pass.statements[1].id, kept);
    assert_eq!(node_of(&ledger, &kept), Some("s.final"));

    // A reading that fails it sends it to its cut: it is not rewritten again.
    let reader = Says::new(&[Outcome::Refuted]);
    read_pass(&mut ledger, &reader, &pass, &lesson).await?;
    assert_eq!(node_of(&ledger, &successor), Some("s.cut"));
    assert!(matches!(
        events_of(ledger.entry(&successor).unwrap()).last(),
        Some(ClaimEvent::Stepped {
            rewrite_ahead: false,
            ..
        })
    ));
    Ok(())
}

#[tokio::test]
async fn a_claim_re_extracted_in_other_words_keeps_its_class_and_the_sort_is_not_asked_about_it(
) -> Result<()> {
    let mut ledger = open().await?;
    let kept = claim(
        &mut ledger,
        "kept",
        (SENTENCE, SENTENCE),
        SENTENCE,
        REASONED,
    )
    .await?;
    let from = ledger.entry(&kept).unwrap().events().len();
    // Sorted, the model would call the reworded statement Common.
    let model = Classes::new(&[]).classed(&[(REWORDED, "common")]);

    identified(
        &mut ledger,
        &model,
        &lesson_of(SENTENCE),
        &[(SENTENCE, REWORDED)],
        &[],
    )
    .await?;

    assert_eq!(model.calls("sort"), 0);
    let entry = ledger.entry(&kept).unwrap();
    assert_eq!(entry.statement(), REWORDED);
    assert_eq!(entry.classified(), Some(REASONED));
    assert!(
        events_of(entry)
            .iter()
            .skip(from)
            .all(|event| !matches!(event, ClaimEvent::Classified { .. })),
        "no class is recorded for a claim that continues"
    );
    Ok(())
}

#[tokio::test]
async fn only_a_statement_the_ledger_does_not_hold_is_sorted() -> Result<()> {
    let mut ledger = open().await?;
    claim(
        &mut ledger,
        "held",
        (SENTENCE, SENTENCE),
        &format!("{SENTENCE} {HEADER}"),
        REASONED,
    )
    .await?;
    let model = Classes::new(&[]).classed(&[(HEADER, "common"), (SENTENCE, "common")]);

    let pass = identified(
        &mut ledger,
        &model,
        &lesson_of(&format!("{SENTENCE} {HEADER}")),
        &[(SENTENCE, SENTENCE), (HEADER, HEADER)],
        &[],
    )
    .await?;

    // One call sorted the new statement alone.
    assert_eq!(model.calls("sort"), 1);
    let classes: Vec<_> = pass
        .statements
        .iter()
        .map(|known| ledger.entry(&known.id).unwrap().classified())
        .collect();
    assert_eq!(
        classes,
        [Some(REASONED), Some((Class::Common, ClassifiedBy::Model))]
    );
    Ok(())
}

/// A web whose every search fails, as the network gone would.
struct DownWeb;

#[async_trait::async_trait]
impl lattice_core::application::ports::WebServiceTrait for DownWeb {
    async fn search_web(
        &self,
        _: &lattice_core::application::contracts::web::WebSearchInput,
    ) -> Result<lattice_core::application::contracts::web::WebSearchOutput> {
        Err(AppError::Network(
            "fixture: the search engines are down".into(),
        ))
    }
    async fn fetch_url_content(
        &self,
        _: &str,
    ) -> Result<lattice_core::application::contracts::web::FetchUrlContentOutput> {
        Err(AppError::Network("fixture: the network is down".into()))
    }
    fn validate_url(&self, _: &str) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn research_whose_planner_gives_no_usable_plan_records_nothing_and_stops_naming_the_model(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let program_id = repo.job(&job).await?.program_id;
    // A statement the course's page does not state, so it waits for research.
    let statement = figure(0);
    let draft = lesson_of(&section(&statement)).to_string();
    // The fixture gives the research planner no answer it can use: the round
    // cannot plan its searches.
    let error = research::run(
        db.clone(),
        program_id,
        "CSV".into(),
        Some(research::tests::web(false)),
        prepare(
            &db,
            &repo,
            &job,
            &lesson_id,
            &Figures::new(),
            Some(draft.clone()),
            &[source()],
        ),
    )
    .await
    .err()
    .unwrap()
    .to_string();

    // The stop says what is wrong and how to mend it, in words and with no
    // node names.
    assert_eq!(
        error,
        format!("Research could not plan its searches: the Learning model gave no plan Lattice could use, so 1 statement still waits for a source: \"{statement}\".")
    );
    let ledger = ClaimReviewRepository::new(db.clone())
        .load(&lesson_id)
        .await?;
    let review = ledger.review.clone().unwrap();
    assert_eq!(review.state, LessonReviewState::Stopped);
    let reason = review.stop_reason.unwrap();
    assert_eq!(reason.code, StopCode::NoProgress);
    assert_eq!(
        reason.fix,
        "Resume this lesson to try again. If it stops here again, choose another Learning model in Settings → AI."
    );
    let [claim] = ledger.claims.as_slice() else {
        panic!("one claim: {:?}", ledger.claims);
    };
    let kinds: Vec<&str> = claim
        .events()
        .iter()
        .map(|record| record.event.kind_name())
        .collect();
    assert!(!kinds.contains(&"researched"), "{kinds:?}");
    assert!(
        events_of(claim).iter().all(|event| !matches!(
            event,
            ClaimEvent::Stepped {
                step: Step::Research,
                ..
            }
        )),
        "{kinds:?}"
    );
    // The claim waits where its reading left it, the draft as it was.
    assert_eq!(
        claim.decided(),
        Some(&Then::Node {
            name: "s.research".into(),
            step: Step::Research
        })
    );
    Ok(())
}

#[tokio::test]
async fn research_whose_searches_all_fail_records_nothing_and_stops_naming_the_connection(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let program_id = repo.job(&job).await?.program_id;
    let statement = figure(0);
    let draft = lesson_of(&section(&statement)).to_string();
    // The planner plans a search, and the web fails every one.
    let error = research::run(
        db.clone(),
        program_id,
        "CSV".into(),
        Some(std::sync::Arc::new(DownWeb)),
        prepare(
            &db,
            &repo,
            &job,
            &lesson_id,
            &Figures::planning("csv dictreader buffer size"),
            Some(draft.clone()),
            &[source()],
        ),
    )
    .await
    .err()
    .unwrap()
    .to_string();

    assert_eq!(
        error,
        format!("Research could not reach any search engine or web page, so 1 statement still waits for a source: \"{statement}\".")
    );
    let ledger = ClaimReviewRepository::new(db.clone())
        .load(&lesson_id)
        .await?;
    let review = ledger.review.clone().unwrap();
    assert_eq!(review.state, LessonReviewState::Stopped);
    let reason = review.stop_reason.unwrap();
    assert_eq!(reason.code, StopCode::NoProgress);
    assert_eq!(
        reason.fix,
        "Check your internet connection and the research settings, then resume this lesson."
    );
    let [claim] = ledger.claims.as_slice() else {
        panic!("one claim: {:?}", ledger.claims);
    };
    assert!(
        events_of(claim).iter().all(|event| !matches!(
            event,
            ClaimEvent::Stepped {
                step: Step::Research,
                ..
            }
        )),
        "a round that reached nothing is not research"
    );
    Ok(())
}

#[tokio::test]
async fn a_research_round_records_researched_then_its_step() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(
        &mut ledger,
        "needs",
        (SENTENCE, SENTENCE),
        SENTENCE,
        SOURCED,
    )
    .await?;
    walk(&mut ledger, &id, Class::Sourced, &RESEARCHING).await?;
    let searched = research::Searched {
        sources_added: vec!["source-2".into()],
        ..Default::default()
    };

    ledger
        .record_research(&policy, &[(id.clone(), 2)], &searched, true)
        .await?;

    let entry = ledger.entry(&id).unwrap();
    let last: Vec<&ClaimEvent> = events_of(entry).into_iter().rev().take(2).collect();
    assert!(
        matches!(
            last.as_slice(),
            [
                ClaimEvent::Stepped {
                    step: Step::Research,
                    outcome: Outcome::Saved,
                    ..
                },
                ClaimEvent::Researched {
                    passages_found: 2,
                    ..
                }
            ]
        ),
        "{last:?}"
    );
    assert_eq!(node_of(&ledger, &id), Some("s.reread"));
    Ok(())
}

#[tokio::test]
async fn claims_on_two_sentences_of_a_section_at_recite_are_both_recited_in_one_call() -> Result<()>
{
    const NONE: &str = "Missing values in nonblank rows default to None.";
    let policy = StandardPolicy::new();
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let excerpt = &sources[0].excerpt;
    let body = format!("{GOOD} {NONE}");
    let mut ledger = open().await?;
    let mut found = Vec::new();
    for (name, statement) in [("first", GOOD), ("second", NONE)] {
        let id = claim(&mut ledger, name, (statement, statement), &body, SOURCED).await?;
        walk(&mut ledger, &id, Class::Sourced, &RECITING).await?;
        let at = excerpt.find(statement).unwrap();
        found.push(elsewhere(
            statement,
            &sources[0].id,
            at..at + statement.len(),
        ));
    }
    let claims = [(GOOD, GOOD), (NONE, NONE)];
    let mut lesson = json!({"blocks":[{"kind":"explanation","title":"Rows","body":body,"rubric":[],
        "sourceIndex":0,"quote":"An unrelated quotation."}],"questions":[]});
    identified(&mut ledger, &Model::new(), &lesson, &claims, &sources).await?;
    assert_eq!(ledger.at(&policy, StepKind::Recite).len(), 2);
    let report = pass_report(found);
    let (first, second) = (ClaimId("first".into()), ClaimId("second".into()));
    let mut numbered = vec![sources[0].clone()];

    let moved = super::super::repair_loop::recite(
        &mut ledger,
        &policy,
        (&mut lesson, &report),
        (&mut numbered, &mut HashSet::new()),
        &references,
    )
    .await?;

    // Each claim is on a sentence of its own, so both are cited in the call,
    // each by an entry of its own; the section's quotation stands.
    assert!(moved);
    for id in [&first, &second] {
        assert_eq!(node_of(&ledger, id), Some("s.citeread"));
        assert!(events_of(ledger.entry(id).unwrap())
            .iter()
            .any(|event| matches!(event, ClaimEvent::CitationRepaired { .. })));
    }
    assert_eq!(lesson["blocks"][0]["quote"], "An unrelated quotation.");
    assert_eq!(
        lesson["blocks"][0]["citations"],
        json!([
            {"sentence":GOOD,"sourceIndex":0,"quote":GOOD},
            {"sentence":NONE,"sourceIndex":0,"quote":NONE},
        ])
    );
    Ok(())
}

#[tokio::test]
async fn a_recited_sentence_is_named_as_its_section_writes_it_not_as_the_extractor_quoted_it(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let at = sources[0].excerpt.find(GOOD).unwrap();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "worded", (GOOD, GOOD), GOOD, SOURCED).await?;
    walk(&mut ledger, &id, Class::Sourced, &RECITING).await?;
    let mut lesson = lesson_of(GOOD);
    identified(
        &mut ledger,
        &Model::new(),
        &lesson,
        &[(GOOD, GOOD)],
        &sources,
    )
    .await?;
    // The extractor quoted the sentence in words of its own; the ledger located
    // the claim on the section's.
    let report = pass_report(vec![Finding {
        quote: format!("In short, {GOOD}"),
        ..elsewhere(GOOD, &sources[0].id, at..at + GOOD.len())
    }]);

    super::super::repair_loop::recite(
        &mut ledger,
        &policy,
        (&mut lesson, &report),
        (&mut vec![sources[0].clone()], &mut HashSet::new()),
        &references,
    )
    .await?;

    assert_eq!(
        lesson["blocks"][0]["citations"],
        json!([{"sentence":GOOD,"sourceIndex":0,"quote":GOOD}]),
        "the entry names the sentence as the section writes it"
    );
    Ok(())
}

#[tokio::test]
async fn a_second_claim_on_a_sentence_at_recite_waits_a_round_then_is_done_on_the_citation_written(
) -> Result<()> {
    const WHITESPACE: &str = "DictReader preserves whitespace by default.";
    let policy = StandardPolicy::new();
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let at = sources[0].excerpt.find(GOOD).unwrap();
    let mut ledger = open().await?;
    let mut found = Vec::new();
    for (name, statement) in [("first", GOOD), ("second", WHITESPACE)] {
        let id = claim(&mut ledger, name, (GOOD, statement), GOOD, SOURCED).await?;
        walk(&mut ledger, &id, Class::Sourced, &RECITING).await?;
        // Both state their sentence, the same one, and a page supports both
        // on the same passage.
        found.push(Finding {
            quote: GOOD.into(),
            ..elsewhere(statement, &sources[0].id, at..at + GOOD.len())
        });
    }
    let mut lesson = json!({"blocks":[{"kind":"explanation","title":"Rows","body":GOOD,"rubric":[],
        "sourceIndex":0,"quote":"An unrelated quotation."}],"questions":[]});
    identified(
        &mut ledger,
        &Model::new(),
        &lesson,
        &[(GOOD, GOOD), (GOOD, WHITESPACE)],
        &sources,
    )
    .await?;
    let report = pass_report(found);
    let (first, second) = (ClaimId("first".into()), ClaimId("second".into()));
    let mut numbered = vec![sources[0].clone()];
    let mut tried = HashSet::new();
    let before = ledger.entry(&second).unwrap().events().len();

    let moved = super::super::repair_loop::recite(
        &mut ledger,
        &policy,
        (&mut lesson, &report),
        (&mut numbered, &mut tried),
        &references,
    )
    .await?;

    // One claim of a sentence is recited in a call; the other waits, with
    // nothing recorded.
    assert!(moved);
    assert_eq!(node_of(&ledger, &first), Some("s.citeread"));
    assert!(events_of(ledger.entry(&first).unwrap())
        .iter()
        .any(|event| matches!(event, ClaimEvent::CitationRepaired { .. })));
    assert_eq!(node_of(&ledger, &second), Some("s.recite"));
    assert_eq!(ledger.entry(&second).unwrap().events().len(), before);
    let entry = json!([{"sentence":GOOD,"sourceIndex":0,"quote":GOOD}]);
    assert_eq!(lesson["blocks"][0]["citations"], entry);

    // The next round finds the passage written already: the claim is done,
    // and the reading after it is of the citation that was written.
    let moved = super::super::repair_loop::recite(
        &mut ledger,
        &policy,
        (&mut lesson, &report),
        (&mut numbered, &mut tried),
        &references,
    )
    .await?;

    assert!(moved);
    assert_eq!(node_of(&ledger, &second), Some("s.citeread"));
    assert!(events_of(ledger.entry(&second).unwrap())
        .iter()
        .all(|event| !matches!(event, ClaimEvent::CitationRepaired { .. })));
    assert_eq!(lesson["blocks"][0]["citations"], entry);
    Ok(())
}

#[tokio::test]
async fn a_claim_naming_context_its_sentence_leaves_implicit_is_cited_at_the_passage_found_and_read_there_next(
) -> Result<()> {
    use super::recite_tests::{IMPLICIT_SENTENCE, IMPLICIT_STATEMENT};
    const OLD: &str = "Missing values in nonblank rows default to None.";
    let policy = StandardPolicy::new();
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let at = sources[0].excerpt.find(GOOD).unwrap();
    let mut ledger = open().await?;
    let id = claim(
        &mut ledger,
        "implicit",
        (IMPLICIT_SENTENCE, IMPLICIT_STATEMENT),
        IMPLICIT_SENTENCE,
        SOURCED,
    )
    .await?;
    walk(&mut ledger, &id, Class::Sourced, &RECITING).await?;
    let mut lesson = json!({"blocks":[{"kind":"explanation","title":"Rows","body":IMPLICIT_SENTENCE,
        "rubric":[],"sourceIndex":0,"quote":OLD}],"questions":[]});
    identified(
        &mut ledger,
        &Model::new(),
        &lesson,
        &[(IMPLICIT_SENTENCE, IMPLICIT_STATEMENT)],
        &sources,
    )
    .await?;
    assert_eq!(ledger.at(&policy, StepKind::Recite).len(), 1);
    let report = pass_report(vec![elsewhere(
        IMPLICIT_STATEMENT,
        &sources[0].id,
        at..at + GOOD.len(),
    )]);

    let moved = super::super::repair_loop::recite(
        &mut ledger,
        &policy,
        (&mut lesson, &report),
        (&mut vec![sources[0].clone()], &mut HashSet::new()),
        &references,
    )
    .await?;

    assert!(moved);
    assert_eq!(node_of(&ledger, &id), Some("s.citeread"));
    assert!(events_of(ledger.entry(&id).unwrap())
        .iter()
        .any(|event| matches!(event, ClaimEvent::CitationRepaired { .. })));
    assert_eq!(
        lesson["blocks"][0]["citations"],
        json!([{"sentence":IMPLICIT_SENTENCE,"sourceIndex":0,"quote":GOOD}])
    );
    // The next reading takes the passage the sentence now cites.
    let next = section_citations(&lesson, &references)?;
    assert!(next.is_cited(0, IMPLICIT_SENTENCE));
    let read = next.of_claim(0, IMPLICIT_SENTENCE);
    assert!(!read.is_empty() && read.iter().all(|passage| passage.text.contains(GOOD)));
    Ok(())
}

#[tokio::test]
async fn the_cut_of_a_statement_naming_context_its_sentence_leaves_implicit_takes_out_its_sentence(
) -> Result<()> {
    use super::recite_tests::{IMPLICIT_SENTENCE, IMPLICIT_STATEMENT};
    const NEXT: &str = "Keep the card away from heat.";
    assert!(lattice_core::shared::text::claim_sentence(
        IMPLICIT_SENTENCE,
        (0, IMPLICIT_SENTENCE.len()),
        IMPLICIT_STATEMENT
    )
    .is_none());
    let text = format!("{IMPLICIT_SENTENCE} {NEXT}");
    let mut ledger = open().await?;
    let id = claim(
        &mut ledger,
        "implicit",
        (IMPLICIT_SENTENCE, IMPLICIT_STATEMENT),
        &text,
        SOURCED,
    )
    .await?;

    assert_eq!(
        ledger.cut_of(&id, &text),
        claim_ledger::CutOf::Sentence(IMPLICIT_SENTENCE)
    );
    Ok(())
}

#[test]
fn uncite_drops_the_entry_holding_the_claims_sentence_and_no_other() -> Result<()> {
    const NONE: &str = "Missing values in nonblank rows default to None.";
    const WORDS_OF_GOOD: &str = "DictReader preserves whitespace by default.";
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let first = json!({"sentence":GOOD,"sourceIndex":0,"quote":GOOD});
    let second = json!({"sentence":NONE,"sourceIndex":0,"quote":NONE});
    let two = || {
        let mut lesson = cited_lesson(json!([first, second]));
        lesson["blocks"][0]["body"] = json!(format!("{GOOD} {NONE}"));
        lesson
    };

    let mut lesson = two();
    let dropped = repair::uncite(&mut lesson, &[(0, NONE.to_owned())], &[], &references)?;
    assert_eq!(dropped, vec![vec![sources[0].id.clone()]]);
    assert_eq!(lesson["blocks"][0]["citations"], json!([first]));

    // A sentence that only shares words with a cited one has no entry.
    let mut lesson = two();
    let dropped = repair::uncite(
        &mut lesson,
        &[(0, WORDS_OF_GOOD.to_owned())],
        &[],
        &references,
    )?;
    assert_eq!(dropped, vec![Vec::<String>::new()]);
    assert_eq!(lesson["blocks"][0]["citations"], json!([first, second]));

    // An entry of two sentences stays while a claim on the other one lives.
    let both = json!({"sentence":format!("{GOOD} {NONE}"),"sourceIndex":0,"quote":GOOD});
    let mut lesson = cited_lesson(json!([both]));
    let dropped = repair::uncite(
        &mut lesson,
        &[(0, GOOD.to_owned())],
        &[(0, NONE.to_owned())],
        &references,
    )?;
    assert_eq!(dropped, vec![Vec::<String>::new()]);
    assert_eq!(lesson["blocks"][0]["citations"], json!([both]));
    let dropped = repair::uncite(&mut lesson, &[(0, GOOD.to_owned())], &[], &references)?;
    assert_eq!(dropped, vec![vec![sources[0].id.clone()]]);
    assert_eq!(lesson["blocks"][0]["citations"], json!([]));
    Ok(())
}

fn cited_lesson(entries: Value) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Rows","body":GOOD,"rubric":[],
        "sourceIndex":0,"quote":GOOD,"citations":entries}],"questions":[]})
}

#[test]
fn uncite_keeps_an_entry_another_claim_on_the_sentence_stands_on() -> Result<()> {
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let entry = json!([{"sentence":GOOD,"sourceIndex":0,"quote":GOOD}]);
    let statements = [(0, GOOD.to_owned())];

    // Another live claim stands on the sentence: the entry is its citation.
    let mut lesson = cited_lesson(entry.clone());
    let kept = [(0, GOOD.to_owned())];
    let dropped = repair::uncite(&mut lesson, &statements, &kept, &references)?;
    assert_eq!(dropped, vec![Vec::<String>::new()]);
    assert_eq!(lesson["blocks"][0]["citations"], entry);

    // None does: the entry goes, and the sources it stood on are recorded.
    let mut lesson = cited_lesson(entry);
    let dropped = repair::uncite(&mut lesson, &statements, &[], &references)?;
    assert_eq!(dropped, vec![vec![sources[0].id.clone()]]);
    assert_eq!(lesson["blocks"][0]["citations"], json!([]));
    // The section's own quotation, which the others are read against, stays.
    assert_eq!(lesson["blocks"][0]["quote"], GOOD);

    // A sentence with no entry of its own keeps what it has.
    let mut lesson = cited_lesson(json!([]));
    let dropped = repair::uncite(&mut lesson, &statements, &[], &references)?;
    assert_eq!(dropped, vec![Vec::<String>::new()]);
    assert_eq!(lesson["blocks"][0]["quote"], GOOD);
    Ok(())
}

/// A ledger holding a target at `s.rewrite` and, beside it on one sentence, a
/// Reasoning statement that ended Consistent, with a repair of the target
/// sent.
async fn target_and_mate() -> Result<(claim_ledger::ClaimLedger, ClaimId, ClaimId)> {
    let mut ledger = open().await?;
    let target = claim(
        &mut ledger,
        "target",
        (SENTENCE, SENTENCE),
        SENTENCE,
        SOURCED,
    )
    .await?;
    walk(&mut ledger, &target, Class::Sourced, &REWRITING).await?;
    let mate = claim(&mut ledger, "mate", (SENTENCE, HEADER), SENTENCE, REASONED).await?;
    walk(&mut ledger, &mate, Class::Reasoning, &CONSISTENT).await?;
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(SENTENCE),
            std::slice::from_ref(&target),
        )
        .await?;
    Ok((ledger, target, mate))
}

#[tokio::test]
async fn a_mate_of_a_rewritten_target_is_read_from_its_classes_post_rewrite_node() -> Result<()> {
    let policy = StandardPolicy::new();
    let (mut ledger, target, mate) = target_and_mate().await?;
    // The mate rides on the target's repair; the target's goal is recorded.
    let records = &ledger.pending.repairs;
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].claim_id, target);
    assert_eq!(records[0].goal(), Some(Goal::WhatThePassageStates));
    assert_eq!(records[1].rides_on.as_ref(), Some(&target));

    // The sentence is rewritten, and both statements stand on the new one.
    let lesson = lesson_of(REWRITTEN);
    let pass = identified(
        &mut ledger,
        &Model::new(),
        &lesson,
        &[(REWRITTEN, SENTENCE), (REWRITTEN, HEADER)],
        &[],
    )
    .await?;

    let successor = pass.statements[1].id.clone();
    assert_ne!(successor, mate);
    // The mate is replaced by a machine successor of its class, which starts
    // at the class's post-rewrite node and is read there.
    assert!(matches!(
        ledger.entry(&mate).unwrap().state(),
        ClaimState::Edited {
            successor: Some(next),
            by: Actor::Machine,
            ..
        } if *next == successor
    ));
    assert_eq!(
        ledger.entry(&successor).unwrap().classified(),
        Some(REASONED)
    );
    assert_eq!(node_of(&ledger, &successor), Some("r.final"));
    let due: Vec<ClaimId> = ledger.due(&policy).into_iter().map(|(id, _)| id).collect();
    assert!(due.contains(&successor), "{due:?}");
    // The target itself continues at its class's post-rewrite node.
    assert_eq!(pass.statements[0].id, target);
    assert_eq!(node_of(&ledger, &target), Some("s.final"));
    Ok(())
}

#[tokio::test]
async fn a_rewrite_that_left_the_targets_sentence_as_it_was_is_not_applied_and_the_target_goes_on_to_the_retry(
) -> Result<()> {
    let (mut ledger, target, mate) = target_and_mate().await?;

    // The pass after the repair finds the sentence as it was.
    let pass = identified(
        &mut ledger,
        &Model::new(),
        &lesson_of(SENTENCE),
        &[(SENTENCE, SENTENCE), (SENTENCE, HEADER)],
        &[],
    )
    .await?;

    assert_eq!(pass.statements[0].id, target);
    assert_eq!(node_of(&ledger, &target), Some("s.rewrite.again"));
    let events = events_of(ledger.entry(&target).unwrap());
    assert!(
        events.iter().any(|event| matches!(
            event,
            ClaimEvent::RepairAttempted {
                attempt: 1,
                escalated: false,
                outcome: RepairOutcome::Rejected { .. },
                goal: Some(Goal::WhatThePassageStates),
                ..
            }
        )),
        "{events:?}"
    );
    assert!(matches!(
        events.last(),
        Some(ClaimEvent::Stepped {
            step: Step::Rewrite(Goal::WhatThePassageStates),
            outcome: Outcome::NotApplied,
            ..
        })
    ));
    // The sentence stands, so its mate has no successor and its record is dropped.
    assert_eq!(pass.statements[1].id, mate);
    assert!(ledger.pending.repairs.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_repair_that_produced_nothing_usable_leaves_its_rewrite_not_applied() -> Result<()> {
    let policy = StandardPolicy::new();
    let (mut ledger, target, _) = target_and_mate().await?;

    ledger
        .reject(
            &policy,
            std::slice::from_ref(&target),
            "The edit was unusable.",
        )
        .await?;

    assert_eq!(node_of(&ledger, &target), Some("s.rewrite.again"));
    assert!(ledger.pending.repairs.is_empty(), "the mate rode on it");
    let events = events_of(ledger.entry(&target).unwrap());
    assert!(
        events.iter().any(|event| matches!(
            event,
            ClaimEvent::RepairAttempted {
                outcome: RepairOutcome::Rejected { reason },
                goal: Some(_),
                ..
            } if reason == "The edit was unusable."
        )),
        "{events:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_rewrite_that_took_its_targets_sentence_out_ends_the_target_removed_and_the_lesson_publishes(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let body = format!("{SENTENCE} {HEADER}");
    let mut ledger = open().await?;
    let target = claim(&mut ledger, "target", (SENTENCE, SENTENCE), &body, SOURCED).await?;
    walk(&mut ledger, &target, Class::Sourced, &REWRITING).await?;
    let header = claim(&mut ledger, "header", (HEADER, HEADER), &body, REASONED).await?;
    walk(&mut ledger, &header, Class::Reasoning, &CONSISTENT).await?;
    ledger
        .begin_repairs(&policy, &lesson_of(&body), std::slice::from_ref(&target))
        .await?;

    // The rewrite took the target's sentence out and wrote nothing in its place.
    let pass = identified(
        &mut ledger,
        &Model::new(),
        &lesson_of(HEADER),
        &[(HEADER, HEADER)],
        &[],
    )
    .await?;

    assert_eq!(pass.statements.len(), 1);
    let state = ledger.entry(&target).unwrap().state();
    assert!(
        matches!(
            state,
            ClaimState::Removed {
                by: Actor::Machine,
                ..
            }
        ),
        "{state:?}"
    );
    assert!(matches!(ledger.decide(&policy, &[]), Decision::Publish));
    Ok(())
}

#[tokio::test]
async fn a_claim_that_ended_at_a_node_of_its_graph_is_planned_as_ended() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let target = claim(
        &mut ledger,
        "target",
        (SENTENCE, SENTENCE),
        SENTENCE,
        SOURCED,
    )
    .await?;
    walk(&mut ledger, &target, Class::Sourced, &REWRITING).await?;
    let gone = ClaimEvent::Superseded {
        successor: None,
        by: Actor::Machine,
    };
    ledger.append(&target, gone).await?;

    // Its rewrite was asked for and it went with its sentence: the plan holds
    // nothing more for it, so the call it made is not counted twice.
    let planned = ledger.planned(&policy, &lesson_of(SENTENCE), std::slice::from_ref(&target));
    assert_eq!(
        planned[0].at,
        crate::lessons::call_plan::PlannedAt::Ended(None)
    );
    Ok(())
}

/// A lesson of three sections whose second is `body`: a cut that takes the
/// second section leaves a lesson that publishes, its first explanation
/// untouched.
fn around(body: &str) -> Value {
    json!({"blocks":[
        {"kind":"explanation","title":"First","body":"The first section stands. It says two things.","rubric":[]},
        {"kind":"explanation","title":"Rows","body":body,"rubric":[]},
        {"kind":"explanation","title":"Last","body":"The last section stands. It says two things.","rubric":[]}
    ],"questions":[]})
}

/// A statement of the second section at `s.cut` on `quote` of `body`, with its
/// last attempt sent.
async fn cut_sent(quote: &str, body: &str) -> Result<(claim_ledger::ClaimLedger, ClaimId, Value)> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim_in(&mut ledger, "cut", 1, (quote, quote), body, Some(SOURCED)).await?;
    // Another statement stands, so the lesson keeps one.
    claim(&mut ledger, "other", (HEADER, HEADER), HEADER, SOURCED).await?;
    walk(&mut ledger, &id, Class::Sourced, &CUTTING).await?;
    assert_eq!(node_of(&ledger, &id), Some("s.cut"));
    ledger
        .begin_repairs(&policy, &around(body), std::slice::from_ref(&id))
        .await?;
    let context = json!({
        "candidate": around(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [{"unit":1,"claimId":"cut","sentence":quote,"evidence":[],"lastAttempt":true}],
    });
    Ok((ledger, id, context))
}

#[tokio::test]
async fn a_cut_that_takes_the_sentence_out_ends_the_statement_removed() -> Result<()> {
    let body = format!("{SENTENCE} {HEADER} {MORE}");
    let (mut ledger, id, mut context) = cut_sent(SENTENCE, &body).await?;

    repair::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    assert_eq!(
        context["candidate"]["blocks"][1]["body"],
        format!("{HEADER} {MORE}")
    );
    let entry = ledger.entry(&id).unwrap();
    assert_eq!(
        entry.decided(),
        Some(&Then::End(End::Removed {
            section_goes: false
        }))
    );
    assert!(matches!(
        entry.state(),
        ClaimState::Removed {
            by: Actor::Machine,
            ..
        }
    ));
    assert!(events_of(entry).iter().any(|event| matches!(
        event,
        ClaimEvent::RepairAttempted {
            attempt: 2,
            escalated: true,
            outcome: RepairOutcome::Removed,
            ..
        }
    )));
    Ok(())
}

#[tokio::test]
async fn a_refused_cut_keeps_its_section_and_publishes_the_statement_not_verified() -> Result<()> {
    // The sentence is the whole of its field: it cannot be taken out.
    let (mut ledger, id, mut context) = cut_sent(SENTENCE, SENTENCE).await?;

    repair::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    let entry = ledger.entry(&id).unwrap();
    assert_eq!(entry.decided(), Some(&Then::End(End::Unverified)));
    assert!(
        matches!(entry.state(), ClaimState::Unverified { .. }),
        "{:?}",
        entry.state()
    );
    assert!(ledger.cuts.going.is_empty(), "{:?}", ledger.cuts.going);
    assert_eq!(context["candidate"]["blocks"][1]["body"], SENTENCE);
    Ok(())
}

const MORE: &str = "A comma separates each value from the next.";

const WINDOW: &str = "The desktop and prompt are different roles, not the shell and window.";
const CORRECT: Step = Step::Rewrite(Goal::CorrectWhatWasFound);
/// Steps that leave a Reasoning statement at `r.cut`: the veto and the ground
/// reading refuted it and both corrections were rejected.
const CORRECTIONS_REJECTED: [(Step, Outcome); 5] = [
    (CONSISTENCY, Outcome::Stands),
    (VETO, Outcome::Refuted),
    (Step::Read(Reading::Ground), Outcome::Refuted),
    (CORRECT, Outcome::NotApplied),
    (CORRECT, Outcome::NotApplied),
];

fn question_of(kind: &str, prompt: &str, rationale: &str) -> Value {
    json!({
        "kind": kind, "prompt": prompt, "options": ["One", "Two", "Three", "Four"],
        "correctIndex": 0, "explanation": "The first option names the shell.",
        "rationales": ["Right.", "Wrong.", rationale, "Wrong as well."],
    })
}

/// Three sections (units 0 to 2) with `body` in the second, then `questions`
/// (units 3 and on).
fn lesson_asking(body: &str, questions: &[Value]) -> Value {
    let mut lesson = around(body);
    lesson["questions"] = json!(questions);
    lesson
}

/// A Reasoning statement at `r.cut` on its last attempt, whose sentence is
/// the whole of `field` of `unit` in `lesson`.
async fn refuted_at(
    lesson: &Value,
    (unit, kind, field): (usize, UnitKind, &str),
    text: &str,
) -> Result<(claim_ledger::ClaimLedger, ClaimId, Value)> {
    let mut ledger = open().await?;
    claim(&mut ledger, "other", (HEADER, HEADER), HEADER, SOURCED).await?;
    let mut context = json!({
        "candidate": lesson, "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [], "failedClaims": [],
    });
    let id = add_refuted(
        (&mut ledger, &mut context),
        "refuted",
        (unit, kind, field),
        text,
    )
    .await?;
    Ok((ledger, id, context))
}

/// Another Reasoning statement at `r.cut` on its last attempt, sent in
/// `context`, whose sentence is `text` of `field` of `unit`.
async fn add_refuted(
    (ledger, context): (&mut claim_ledger::ClaimLedger, &mut Value),
    name: &str,
    (unit, kind, field): (usize, UnitKind, &str),
    text: &str,
) -> Result<ClaimId> {
    let id = ClaimId(name.into());
    let lesson = context["candidate"].clone();
    let body = claim_ledger::field_text(&lesson, unit, field).unwrap_or_default();
    let extracted = ClaimEvent::Extracted {
        location: ClaimLocation::locate(unit, kind, field, text, &body),
        statement: text.into(),
    };
    ledger.append(&id, extracted).await?;
    let (class, by) = REASONED;
    ledger
        .append(&id, ClaimEvent::Classified { class, by })
        .await?;
    walk(ledger, &id, Class::Reasoning, &CORRECTIONS_REJECTED).await?;
    assert_eq!(node_of(ledger, &id), Some("r.cut"));
    ledger
        .begin_repairs(&StandardPolicy::new(), &lesson, std::slice::from_ref(&id))
        .await?;
    if let Some(sent) = context["failedClaims"].as_array_mut() {
        sent.push(
            json!({"unit":unit,"claimId":name,"sentence":text,"evidence":[],"lastAttempt":true}),
        );
    }
    Ok(id)
}

#[tokio::test]
async fn a_refuted_rationale_whose_corrections_were_rejected_takes_its_question_out_of_the_lesson(
) -> Result<()> {
    let lesson = lesson_asking(
        "The middle section stands. It says two things.",
        &[
            question_of("practice", "Practise?", "Fine."),
            question_of("quiz", "Which?", WINDOW),
            question_of("quiz", "Another?", "Fine."),
            question_of("test", "Tested?", "Fine."),
        ],
    );
    let (mut ledger, id, mut context) =
        refuted_at(&lesson, (4, UnitKind::AnswerKey, "rationales/2"), WINDOW).await?;
    // A claim of the last question, which is unit 6 now and 5 once the
    // question before it is out.
    let later = ClaimId("later".into());
    let extracted = ClaimEvent::Extracted {
        location: ClaimLocation::locate(
            6,
            UnitKind::AnswerKey,
            "explanation",
            "The first option names the shell.",
            "The first option names the shell.",
        ),
        statement: "The first option names the shell.".into(),
    };
    ledger.append(&later, extracted).await?;

    repair::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    assert_eq!(
        ledger.cuts.going.get(&4).map(String::as_str),
        Some("Which?"),
        "{:?}",
        ledger.cuts.going
    );
    let entry = ledger.entry(&id).unwrap();
    assert!(
        matches!(
            entry.state(),
            ClaimState::Removed {
                by: Actor::Machine,
                ..
            }
        ),
        "{:?}",
        entry.state()
    );
    let candidate = context["candidate"].clone();
    let report = pass_report(Vec::new());
    let (raw, saved, _) = super::super::sections::excise(
        &Model::new(),
        &mut ledger,
        candidate.to_string(),
        candidate,
        report,
    )
    .await?;
    let prompts: Vec<&str> = saved["questions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|question| question["prompt"].as_str())
        .collect();
    assert_eq!(prompts, ["Practise?", "Another?", "Tested?"]);
    assert!(!raw.contains(WINDOW), "{raw}");
    assert_eq!(ledger.entry(&later).unwrap().location().unit_index, 5);
    Ok(())
}

#[tokio::test]
async fn a_refuted_rationale_in_the_last_question_of_its_kind_stays_and_is_published_not_verified(
) -> Result<()> {
    let lesson = lesson_asking(
        "The middle section stands. It says two things.",
        &[
            question_of("practice", "Practise?", "Fine."),
            question_of("quiz", "Which?", WINDOW),
            question_of("test", "Tested?", "Fine."),
        ],
    );
    let (mut ledger, id, mut context) =
        refuted_at(&lesson, (4, UnitKind::AnswerKey, "rationales/2"), WINDOW).await?;

    repair::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    assert!(ledger.cuts.going.is_empty(), "{:?}", ledger.cuts.going);
    assert_eq!(context["candidate"], lesson);
    let entry = ledger.entry(&id).unwrap();
    assert_eq!(entry.decided(), Some(&Then::End(End::Unverified)));
    assert!(
        matches!(entry.state(), ClaimState::Unverified { .. }),
        "{:?}",
        entry.state()
    );
    Ok(())
}

#[tokio::test]
async fn a_refuted_rationale_in_the_last_question_of_the_lesson_stays_and_is_published_not_verified(
) -> Result<()> {
    let lesson = lesson_asking(
        "The middle section stands. It says two things.",
        &[question_of("quiz", "Which?", WINDOW)],
    );
    let (mut ledger, id, mut context) =
        refuted_at(&lesson, (3, UnitKind::AnswerKey, "rationales/2"), WINDOW).await?;

    repair::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    assert!(ledger.cuts.going.is_empty(), "{:?}", ledger.cuts.going);
    assert_eq!(context["candidate"], lesson);
    let entry = ledger.entry(&id).unwrap();
    assert_eq!(entry.decided(), Some(&Then::End(End::Unverified)));
    assert!(
        matches!(entry.state(), ClaimState::Unverified { .. }),
        "{:?}",
        entry.state()
    );
    Ok(())
}

/// The second section holds two sentences a refuted statement can be cut
/// out of, beside the one the guard of the questions is about.
fn cuttable() -> String {
    format!("{SENTENCE} {HEADER} {MORE}")
}

#[tokio::test]
async fn a_refused_statement_in_the_last_question_of_its_kind_does_not_hold_back_the_cuts_beside_it(
) -> Result<()> {
    let body = cuttable();
    let lesson = lesson_asking(
        &body,
        &[
            question_of("practice", "Practise?", "Fine."),
            question_of("quiz", "Which?", WINDOW),
            question_of("test", "Tested?", "Fine."),
        ],
    );
    let (mut ledger, rationale, mut context) =
        refuted_at(&lesson, (4, UnitKind::AnswerKey, "rationales/2"), WINDOW).await?;
    let sentence = add_refuted(
        (&mut ledger, &mut context),
        "sentence",
        (1, UnitKind::Block, "body"),
        SENTENCE,
    )
    .await?;

    repair::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    assert_eq!(
        context["candidate"]["blocks"][1]["body"],
        format!("{HEADER} {MORE}")
    );
    assert!(matches!(
        ledger.entry(&sentence).unwrap().state(),
        ClaimState::Removed {
            by: Actor::Machine,
            ..
        }
    ));
    assert!(ledger.cuts.going.is_empty(), "{:?}", ledger.cuts.going);
    assert_eq!(context["candidate"]["questions"], lesson["questions"]);
    let state = ledger.entry(&rationale).unwrap().state();
    assert!(matches!(state, ClaimState::Unverified { .. }), "{state:?}");
    Ok(())
}

#[tokio::test]
async fn questions_that_together_would_leave_no_question_of_a_kind_all_stay_and_the_cuts_beside_them_are_made(
) -> Result<()> {
    let body = cuttable();
    let lesson = lesson_asking(
        &body,
        &[
            question_of("practice", "Practise?", "Fine."),
            question_of("quiz", "Which?", WINDOW),
            question_of("quiz", "Another?", "The window is not the shell."),
            question_of("test", "Tested?", "Fine."),
        ],
    );
    let (mut ledger, first, mut context) =
        refuted_at(&lesson, (4, UnitKind::AnswerKey, "rationales/2"), WINDOW).await?;
    let second = add_refuted(
        (&mut ledger, &mut context),
        "second",
        (5, UnitKind::AnswerKey, "rationales/2"),
        "The window is not the shell.",
    )
    .await?;
    let sentence = add_refuted(
        (&mut ledger, &mut context),
        "sentence",
        (1, UnitKind::Block, "body"),
        SENTENCE,
    )
    .await?;

    repair::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    assert_eq!(
        context["candidate"]["blocks"][1]["body"],
        format!("{HEADER} {MORE}")
    );
    assert!(matches!(
        ledger.entry(&sentence).unwrap().state(),
        ClaimState::Removed {
            by: Actor::Machine,
            ..
        }
    ));
    assert!(ledger.cuts.going.is_empty(), "{:?}", ledger.cuts.going);
    assert_eq!(context["candidate"]["questions"], lesson["questions"]);
    for id in [first, second] {
        let state = ledger.entry(&id).unwrap().state();
        assert!(matches!(state, ClaimState::Unverified { .. }), "{state:?}");
    }
    Ok(())
}

#[tokio::test]
async fn a_refuted_sentence_that_is_the_whole_of_a_section_body_is_still_published_not_verified(
) -> Result<()> {
    let body = "The middle section stands.";
    let lesson = lesson_asking(
        body,
        &[
            question_of("practice", "Practise?", "Fine."),
            question_of("quiz", "Which?", "Fine."),
            question_of("quiz", "Another?", "Fine."),
            question_of("test", "Tested?", "Fine."),
        ],
    );
    let (mut ledger, id, mut context) =
        refuted_at(&lesson, (1, UnitKind::Block, "body"), body).await?;

    repair::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    assert!(ledger.cuts.going.is_empty(), "{:?}", ledger.cuts.going);
    assert_eq!(context["candidate"], lesson);
    let entry = ledger.entry(&id).unwrap();
    assert_eq!(entry.decided(), Some(&Then::End(End::Unverified)));
    assert!(
        matches!(entry.state(), ClaimState::Unverified { .. }),
        "{:?}",
        entry.state()
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_whose_narrowing_was_not_applied_keeps_the_sentence_it_shares_with_a_resolved_one(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let body = format!("{SENTENCE} {HEADER} {MORE}");
    let mut ledger = open().await?;
    let target = claim_in(
        &mut ledger,
        "target",
        1,
        (SENTENCE, SENTENCE),
        &body,
        Some(SOURCED),
    )
    .await?;
    claim(&mut ledger, "other", (HEADER, HEADER), HEADER, SOURCED).await?;
    walk(&mut ledger, &target, Class::Sourced, &CUTTING).await?;
    let mate = claim_in(
        &mut ledger,
        "mate",
        1,
        (SENTENCE, MORE),
        &body,
        Some(REASONED),
    )
    .await?;
    walk(&mut ledger, &mate, Class::Reasoning, &CONSISTENT).await?;
    ledger
        .begin_repairs(&policy, &around(&body), std::slice::from_ref(&target))
        .await?;
    ledger
        .reject(
            &policy,
            std::slice::from_ref(&target),
            "The edit was unusable.",
        )
        .await?;
    ledger
        .begin_repairs(&policy, &around(&body), std::slice::from_ref(&target))
        .await?;
    let mut context = json!({
        "candidate": around(&body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [{"unit":1,"claimId":"target","sentence":SENTENCE,"evidence":[],"lastAttempt":true}],
    });

    repair::last_attempts(
        &policy,
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    assert_eq!(context["candidate"]["blocks"][1]["body"], body);
    let entry = ledger.entry(&target).unwrap();
    assert_eq!(entry.decided(), Some(&Then::End(End::Unverified)));
    assert!(
        matches!(entry.state(), ClaimState::Unverified { .. }),
        "{:?}",
        entry.state()
    );
    assert!(
        matches!(
            ledger.entry(&mate).unwrap().state(),
            ClaimState::Consistent { .. }
        ),
        "{:?}",
        ledger.entry(&mate).unwrap().state()
    );
    assert!(ledger.cuts.going.is_empty(), "{:?}", ledger.cuts.going);
    Ok(())
}

#[tokio::test]
async fn a_cut_that_would_leave_a_section_fewer_than_two_teaching_sentences_is_not_made(
) -> Result<()> {
    let body = format!("{SENTENCE} {HEADER}");
    let (mut ledger, id, mut context) = cut_sent(SENTENCE, &body).await?;

    repair::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    assert_eq!(context["candidate"]["blocks"][1]["body"], body);
    assert!(ledger.cuts.going.is_empty(), "{:?}", ledger.cuts.going);
    assert!(ledger.cuts.taken.is_empty(), "{:?}", ledger.cuts.taken);
    let state = ledger.entry(&id).unwrap().state();
    assert!(matches!(state, ClaimState::Unverified { .. }), "{state:?}");
    Ok(())
}

/// A statement whose cut was withdrawn ended by the policy's cut step, with its
/// repair closed as refused.
fn assert_cut_withdrawn(ledger: &claim_ledger::ClaimLedger, id: &ClaimId) {
    let entry = ledger.entry(id).unwrap();
    let steps = entry.steps();
    assert!(
        matches!(
            steps.last(),
            Some((
                Taken {
                    step: Step::Cut,
                    outcome: Outcome::CannotCut
                },
                Then::End(End::Unverified)
            ))
        ),
        "{steps:?}"
    );
    let closed = entry
        .events()
        .iter()
        .rev()
        .find_map(|record| match &record.event {
            ClaimEvent::RepairAttempted {
                escalated, outcome, ..
            } => Some((*escalated, outcome.clone())),
            _ => None,
        });
    assert_eq!(
        closed,
        Some((
            true,
            RepairOutcome::Rejected {
                reason: "The sentence could not be cut out.".into()
            }
        ))
    );
}

const WIDE_A: &str = "A table lists its rows in order.";
const WIDE_B: &str = "A table names each column once.";
const WIDE_C: &str = "A table keeps one value in each cell.";
const WIDE_D: &str = "A table may hold many rows.";

#[tokio::test]
async fn a_cut_that_thins_one_section_is_held_back_while_a_cut_in_another_is_made() -> Result<()> {
    let policy = StandardPolicy::new();
    let thin = format!("{WIDE_A} {WIDE_B}");
    let wide = format!("{WIDE_C} {WIDE_D} {MORE}");
    let lesson = json!({"blocks":[
        {"kind":"explanation","title":"First","body":"The first section stands. It says two things.","rubric":[]},
        {"kind":"explanation","title":"Thin","body":thin,"rubric":[]},
        {"kind":"explanation","title":"Wide","body":wide,"rubric":[]}
    ],"questions":[]});
    let mut ledger = open().await?;
    let held = claim_in(
        &mut ledger,
        "held",
        1,
        (WIDE_A, WIDE_A),
        &thin,
        Some(SOURCED),
    )
    .await?;
    let made = claim_in(
        &mut ledger,
        "made",
        2,
        (WIDE_C, WIDE_C),
        &wide,
        Some(SOURCED),
    )
    .await?;
    claim(&mut ledger, "other", (HEADER, HEADER), HEADER, SOURCED).await?;
    for id in [&held, &made] {
        walk(&mut ledger, id, Class::Sourced, &CUTTING).await?;
        assert_eq!(node_of(&ledger, id), Some("s.cut"));
    }
    ledger
        .begin_repairs(&policy, &lesson, &[held.clone(), made.clone()])
        .await?;
    let mut context = json!({
        "candidate": lesson, "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":1,"claimId":"held","sentence":WIDE_A,"evidence":[],"lastAttempt":true},
            {"unit":2,"claimId":"made","sentence":WIDE_C,"evidence":[],"lastAttempt":true},
        ],
    });

    repair::last_attempts(
        &policy,
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    assert_eq!(context["candidate"]["blocks"][1]["body"], thin);
    let state = ledger.entry(&held).unwrap().state();
    assert!(matches!(state, ClaimState::Unverified { .. }), "{state:?}");
    assert_cut_withdrawn(&ledger, &held);
    assert_eq!(
        context["candidate"]["blocks"][2]["body"],
        format!("{WIDE_D} {MORE}")
    );
    let entry = ledger.entry(&made).unwrap();
    assert_eq!(
        entry.decided(),
        Some(&Then::End(End::Removed {
            section_goes: false
        }))
    );
    assert!(matches!(
        entry.state(),
        ClaimState::Removed {
            by: Actor::Machine,
            ..
        }
    ));
    assert!(ledger.cuts.going.is_empty(), "{:?}", ledger.cuts.going);
    Ok(())
}

const BARE_CODE: &str = "```python\nprint(1)\n```\n";

#[tokio::test]
async fn statements_whose_cuts_leave_no_claim_are_published_not_verified_and_the_lesson_does_not_stop(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let body = format!("{SENTENCE} {HEADER} {MORE}");
    let lesson = around(&body);
    let mut ledger = open().await?;
    let id = claim_in(
        &mut ledger,
        "only",
        1,
        (SENTENCE, SENTENCE),
        &body,
        Some(SOURCED),
    )
    .await?;
    walk(&mut ledger, &id, Class::Sourced, &CUTTING).await?;
    ledger
        .begin_repairs(&policy, &lesson, std::slice::from_ref(&id))
        .await?;
    let mut context = json!({
        "candidate": lesson, "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [{"unit":1,"claimId":"only","sentence":SENTENCE,"evidence":[],"lastAttempt":true}],
    });

    repair::last_attempts(
        &policy,
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    // Every claim is refuted and cleanly cut, and the sections keep their
    // teaching sentences; the lesson would hold no statement, so the
    // statements are published not verified and the sentences stay.
    assert_eq!(context["candidate"]["blocks"][1]["body"], body);
    let state = ledger.entry(&id).unwrap().state();
    assert!(matches!(state, ClaimState::Unverified { .. }), "{state:?}");
    assert_cut_withdrawn(&ledger, &id);
    assert!(ledger.cuts.going.is_empty(), "{:?}", ledger.cuts.going);
    Ok(())
}

#[tokio::test]
async fn statement_cuts_are_made_when_failed_examples_alone_take_more_than_a_third_of_the_sections(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let body = format!("{SENTENCE} {HEADER} {MORE}");
    let lesson = json!({"blocks":[
        section_of("explanation", "First", &body),
        section_of("worked_example", "Second", BARE_CODE),
        section_of("worked_example", "Third", BARE_CODE),
    ],"questions":[]});
    let mut ledger = open().await?;
    let id = claim_in(
        &mut ledger,
        "cut",
        0,
        (SENTENCE, SENTENCE),
        &body,
        Some(SOURCED),
    )
    .await?;
    walk(&mut ledger, &id, Class::Sourced, &CUTTING).await?;
    let second = failed_example_as(&mut ledger, "second", 1, BARE_CODE).await?;
    let third = failed_example_as(&mut ledger, "third", 2, BARE_CODE).await?;
    ledger
        .begin_repairs(&policy, &lesson, &[id.clone(), second, third])
        .await?;
    let mut context = json!({
        "candidate": lesson, "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [{"unit":0,"claimId":"cut","sentence":SENTENCE,"evidence":[],"lastAttempt":true}],
        "exampleClaims": [
            {"unit":1,"claimId":"second","lastAttempt":true},
            {"unit":2,"claimId":"third","lastAttempt":true},
        ],
    });

    repair::last_attempts(
        &policy,
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    // The examples' refused cuts take two of three sections, which no cut of
    // a statement changes: the guard holds nothing back, so the statement is
    // cut as it asked.
    let mut going: Vec<usize> = ledger.cuts.going.keys().copied().collect();
    going.sort_unstable();
    assert_eq!(going, [1, 2]);
    assert_eq!(
        context["candidate"]["blocks"][0]["body"],
        format!("{HEADER} {MORE}")
    );
    assert!(matches!(
        ledger.entry(&id).unwrap().state(),
        ClaimState::Removed {
            by: Actor::Machine,
            ..
        }
    ));
    Ok(())
}

/// A claim of section `unit` the ledger holds, standing on `quote` of
/// `body`; sorted into `class` unless it is a code example's, which has none.
async fn claim_in(
    ledger: &mut claim_ledger::ClaimLedger,
    name: &str,
    unit: usize,
    (quote, statement): (&str, &str),
    body: &str,
    class: Option<(Class, ClassifiedBy)>,
) -> Result<ClaimId> {
    let id = ClaimId(name.into());
    let extracted = ClaimEvent::Extracted {
        location: ClaimLocation::locate(unit, UnitKind::Block, "body", quote, body),
        statement: statement.into(),
    };
    ledger.append(&id, extracted).await?;
    if let Some((class, by)) = class {
        ledger
            .append(&id, ClaimEvent::Classified { class, by })
            .await?;
    }
    Ok(id)
}

const CODE: &str = "print(1)\n";

/// A failed code example on its last attempt, standing in section `unit`.
async fn failed_example(
    ledger: &mut claim_ledger::ClaimLedger,
    unit: usize,
    body: &str,
) -> Result<ClaimId> {
    failed_example_as(ledger, "example", unit, body).await
}

async fn failed_example_as(
    ledger: &mut claim_ledger::ClaimLedger,
    name: &str,
    unit: usize,
    body: &str,
) -> Result<ClaimId> {
    let id = claim_in(ledger, name, unit, (CODE, CODE), body, None).await?;
    let run = ClaimEvent::CodeExample {
        observation: format!("{name}-0"),
    };
    ledger.append(&id, run).await?;
    let failed = local_check(
        CheckResult::Failed {
            kind: VerdictKind::Unsupported,
        },
        "The example did not complete.",
        vec![],
    );
    ledger.append(&id, ClaimEvent::Checked(failed)).await?;
    let spent = ClaimEvent::RepairAttempted {
        attempt: 1,
        escalated: false,
        before: CODE.into(),
        after: None,
        outcome: RepairOutcome::Rejected {
            reason: "Unusable.".into(),
        },
        goal: None,
    };
    ledger.append(&id, spent).await?;
    Ok(id)
}

const FENCED: &str =
    "Intro text. Try the example.\n\n```python\nprint(1)\n```\n\nThat prints one. It ends here.";

fn section_of(kind: &str, title: &str, body: &str) -> Value {
    json!({"kind":kind,"title":title,"body":body,"rubric":[]})
}

#[tokio::test]
async fn a_batch_with_an_example_beside_statements_publishes_the_statements_not_verified_when_only_their_cuts_would_stop_the_lesson(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let lesson = json!({"blocks":[
        section_of("explanation", "First", SENTENCE),
        section_of("worked_example", "Example", FENCED),
    ],"questions":[]});
    let mut ledger = open().await?;
    // The first explanation is one sentence: its cut would take the section,
    // and with it the lesson's first explanation.
    let first = claim_in(
        &mut ledger,
        "first",
        0,
        (SENTENCE, SENTENCE),
        SENTENCE,
        Some(SOURCED),
    )
    .await?;
    walk(&mut ledger, &first, Class::Sourced, &CUTTING).await?;
    ledger
        .append(
            &first,
            ClaimEvent::Checked(local_check(
                CheckResult::Failed {
                    kind: VerdictKind::Unsupported,
                },
                "Unstated.",
                Vec::new(),
            )),
        )
        .await?;
    let example = failed_example(&mut ledger, 1, FENCED).await?;
    ledger
        .begin_repairs(&policy, &lesson, &[first.clone(), example.clone()])
        .await?;
    let mut context = json!({
        "candidate": lesson, "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [{"unit":0,"claimId":"first","sentence":SENTENCE,"evidence":[],"lastAttempt":true}],
        "exampleClaims": [{"unit":1,"claimId":"example","lastAttempt":true}],
    });

    repair::last_attempts(
        &policy,
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    // The statement keeps its sentence and its section, marked by its last
    // reading; the example's own cut is made, and takes nothing the lesson
    // needs.
    assert_eq!(context["candidate"]["blocks"][0]["body"], SENTENCE);
    assert!(ledger.cuts.going.is_empty(), "{:?}", ledger.cuts.going);
    assert!(
        matches!(
            ledger.entry(&first).unwrap().state(),
            ClaimState::Unverified {
                reason: UnverifiedReason::Unsupported,
                ..
            }
        ),
        "{:?}",
        ledger.entry(&first).unwrap().state()
    );
    let example_text = context["candidate"]["blocks"][1]["body"].as_str().unwrap();
    assert!(!example_text.contains("```"), "{example_text}");
    assert!(ledger.cuts.taken.contains_key(&1));
    assert!(!matches!(
        ledger.entry(&example).unwrap().state(),
        ClaimState::Unverified { .. }
    ));
    Ok(())
}

#[tokio::test]
async fn an_examples_own_cut_that_would_take_the_first_explanation_still_ends_the_batch_as_it_did(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let fence = format!("```python\n{CODE}```\n");
    let rest = "Keep one. Keep two. Cut this one.";
    let lesson = json!({"blocks":[
        section_of("explanation", "First", &fence),
        section_of("explanation", "Second", rest),
    ],"questions":[]});
    let mut ledger = open().await?;
    let example = failed_example(&mut ledger, 0, &fence).await?;
    let cut = claim_in(
        &mut ledger,
        "cut",
        1,
        ("Cut this one.", "Cut this one."),
        rest,
        Some(SOURCED),
    )
    .await?;
    walk(&mut ledger, &cut, Class::Sourced, &CUTTING).await?;
    ledger
        .begin_repairs(&policy, &lesson, &[cut.clone(), example.clone()])
        .await?;
    let mut context = json!({
        "candidate": lesson, "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [{"unit":1,"claimId":"cut","sentence":"Cut this one.","evidence":[],"lastAttempt":true}],
        "exampleClaims": [{"unit":0,"claimId":"example","lastAttempt":true}],
    });

    repair::last_attempts(
        &policy,
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    // An example is never marked: its section goes with it, which is the
    // first explanation, and the statement's cut is made as before.
    assert!(ledger.cuts.going.contains_key(&0));
    assert_eq!(
        context["candidate"]["blocks"][1]["body"],
        "Keep one. Keep two."
    );
    let entry = ledger.entry(&cut).unwrap();
    assert!(
        matches!(
            entry.state(),
            ClaimState::Removed {
                by: Actor::Machine,
                ..
            }
        ),
        "{:?}",
        entry.state()
    );
    assert!(matches!(
        ledger.entry(&example).unwrap().state(),
        ClaimState::Removed { .. }
    ));
    Ok(())
}

/// A ledger holding a target at `s.cut` and, beside it on one sentence, a
/// Reasoning statement that ended Consistent, with the target's last attempt
/// sent.
async fn cut_beside_a_mate() -> Result<(claim_ledger::ClaimLedger, ClaimId, ClaimId)> {
    let mut ledger = open().await?;
    let target = claim(
        &mut ledger,
        "target",
        (SENTENCE, SENTENCE),
        SENTENCE,
        SOURCED,
    )
    .await?;
    walk(&mut ledger, &target, Class::Sourced, &CUTTING).await?;
    let mate = claim(&mut ledger, "mate", (SENTENCE, HEADER), SENTENCE, REASONED).await?;
    walk(&mut ledger, &mate, Class::Reasoning, &CONSISTENT).await?;
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(SENTENCE),
            std::slice::from_ref(&target),
        )
        .await?;
    Ok((ledger, target, mate))
}

#[tokio::test]
async fn a_cut_beside_a_live_statement_on_its_sentence_is_a_narrowing_and_the_statement_rides_on_it(
) -> Result<()> {
    // §1 #8 (a) of the acceptance policy design
    let (ledger, target, mate) = cut_beside_a_mate().await?;

    // The target's repair is a rewrite that keeps the mate's words, so the mate
    // rides on it as on any rewrite.
    let records = &ledger.pending.repairs;
    assert_eq!(records.len(), 2, "{records:?}");
    assert_eq!(records[0].claim_id, target);
    assert_eq!(records[0].goal(), Some(Goal::KeepTheOthers));
    assert_eq!(records[1].claim_id, mate);
    assert_eq!(records[1].rides_on.as_ref(), Some(&target));
    Ok(())
}

#[tokio::test]
async fn a_cut_alone_on_its_sentence_is_no_narrowing_and_nothing_rides_on_it() -> Result<()> {
    let body = format!("{SENTENCE} {HEADER}");
    let (ledger, id, _) = cut_sent(SENTENCE, &body).await?;

    let records = &ledger.pending.repairs;
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].claim_id, id);
    assert_eq!(records[0].goal(), None);
    assert!(records[0].rides_on.is_none());
    Ok(())
}

#[tokio::test]
async fn a_narrowing_whose_answer_was_unusable_is_closed_not_applied_with_its_goal_and_no_step(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let (mut ledger, target, _) = cut_beside_a_mate().await?;
    let from = ledger.entry(&target).unwrap().events().len();

    ledger
        .reject(
            &policy,
            std::slice::from_ref(&target),
            "The edit was unusable.",
        )
        .await?;

    // It stays at its cut with its narrowing spent, and the mate's record is
    // dropped with it.
    assert_eq!(node_of(&ledger, &target), Some("s.cut"));
    assert!(ledger.pending.repairs.is_empty());
    let events = events_of(ledger.entry(&target).unwrap());
    let added = &events[from..];
    assert!(
        matches!(
            added,
            [ClaimEvent::RepairAttempted {
                outcome: RepairOutcome::Rejected { reason },
                goal: Some(Goal::KeepTheOthers),
                ..
            }] if reason == "The edit was unusable."
        ),
        "{added:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_narrowing_that_wrote_the_sentence_without_the_target_ends_it_removed_by_its_cut_and_the_mate_continues_after_the_rewrite(
) -> Result<()> {
    let (mut ledger, target, mate) = cut_beside_a_mate().await?;

    // The narrowing wrote a sentence that states what the mate states.
    let pass = identified(
        &mut ledger,
        &Model::new(),
        &lesson_of(REWRITTEN),
        &[(REWRITTEN, HEADER)],
        &[],
    )
    .await?;

    let entry = ledger.entry(&target).unwrap();
    assert_eq!(
        entry.decided(),
        Some(&Then::End(End::Removed {
            section_goes: false
        }))
    );
    assert!(
        matches!(
            entry.state(),
            ClaimState::Removed {
                by: Actor::Machine,
                ..
            }
        ),
        "{:?}",
        entry.state()
    );
    let events = events_of(entry);
    assert!(
        events.iter().any(|event| matches!(
            event,
            ClaimEvent::RepairAttempted {
                outcome: RepairOutcome::Removed,
                goal: Some(Goal::KeepTheOthers),
                after: Some(after),
                ..
            } if after == REWRITTEN
        )),
        "{events:?}"
    );
    assert!(matches!(
        events.last(),
        Some(ClaimEvent::Stepped {
            step: Step::Cut,
            outcome: Outcome::Cut,
            ..
        })
    ));
    // The statement beside it continues as a machine successor of its class,
    // read from the node after the rewrite.
    let successor = pass.statements[0].id.clone();
    assert_ne!(successor, mate);
    assert!(matches!(
        ledger.entry(&mate).unwrap().state(),
        ClaimState::Edited {
            successor: Some(next),
            by: Actor::Machine,
            ..
        } if *next == successor
    ));
    assert_eq!(node_of(&ledger, &successor), Some("r.final"));
    Ok(())
}

#[tokio::test]
async fn a_narrowing_that_left_the_sentence_as_it_was_is_not_applied_and_the_target_stays_at_its_cut(
) -> Result<()> {
    let (mut ledger, target, mate) = cut_beside_a_mate().await?;
    let from = ledger.entry(&target).unwrap().events().len();

    // The pass after the repair finds the sentence as it was.
    let pass = identified(
        &mut ledger,
        &Model::new(),
        &lesson_of(SENTENCE),
        &[(SENTENCE, SENTENCE), (SENTENCE, HEADER)],
        &[],
    )
    .await?;

    assert_eq!(pass.statements[0].id, target);
    assert_eq!(node_of(&ledger, &target), Some("s.cut"));
    let events = events_of(ledger.entry(&target).unwrap());
    let added = &events[from..];
    assert!(
        matches!(
            added,
            [ClaimEvent::RepairAttempted {
                attempt: 2,
                escalated: true,
                outcome: RepairOutcome::Rejected { reason },
                goal: Some(Goal::KeepTheOthers),
                ..
            }] if reason == "The repair left the sentence as it was."
        ),
        "no step, only the narrowing spent: {added:?}"
    );
    // The sentence stands, so its mate has no successor and its record is dropped.
    assert_eq!(pass.statements[1].id, mate);
    assert!(ledger.pending.repairs.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_narrowing_whose_new_sentence_still_states_the_target_is_not_applied_either() -> Result<()>
{
    let (mut ledger, target, _) = cut_beside_a_mate().await?;
    let from = ledger.entry(&target).unwrap().events().len();

    // The sentence changed, and what it says still holds the target's statement.
    let pass = identified(
        &mut ledger,
        &Model::new(),
        &lesson_of(REWRITTEN),
        &[(REWRITTEN, SENTENCE), (REWRITTEN, HEADER)],
        &[],
    )
    .await?;

    assert_eq!(pass.statements[0].id, target);
    assert_eq!(node_of(&ledger, &target), Some("s.cut"));
    let events = events_of(ledger.entry(&target).unwrap());
    assert!(
        matches!(
            events.last(),
            Some(ClaimEvent::RepairAttempted {
                outcome: RepairOutcome::Rejected { reason },
                goal: Some(Goal::KeepTheOthers),
                ..
            }) if reason == "The narrowed sentence still states it."
        ),
        "{events:?}"
    );
    assert!(
        events[from..]
            .iter()
            .all(|event| !matches!(event, ClaimEvent::Stepped { .. })),
        "no step is taken for it: {events:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_narrowing_whose_sentence_keeps_the_mate_in_other_words_ends_the_target_removed_and_the_mate_continues(
) -> Result<()> {
    let (mut ledger, target, mate) = cut_beside_a_mate().await?;

    // The narrowing kept the mate's statement, worded differently.
    let pass = identified(
        &mut ledger,
        &Model::new(),
        &lesson_of(REWRITTEN),
        &[(REWRITTEN, HEADER_REWORDED)],
        &[],
    )
    .await?;

    let entry = ledger.entry(&target).unwrap();
    assert_eq!(
        entry.decided(),
        Some(&Then::End(End::Removed {
            section_goes: false
        }))
    );
    assert!(
        matches!(
            entry.state(),
            ClaimState::Removed {
                by: Actor::Machine,
                ..
            }
        ),
        "{:?}",
        entry.state()
    );
    assert!(matches!(
        events_of(entry).last(),
        Some(ClaimEvent::Stepped {
            step: Step::Cut,
            outcome: Outcome::Cut,
            ..
        })
    ));
    let successor = pass.statements[0].id.clone();
    assert_ne!(successor, mate);
    assert!(matches!(
        ledger.entry(&mate).unwrap().state(),
        ClaimState::Edited {
            successor: Some(next),
            by: Actor::Machine,
            ..
        } if *next == successor
    ));
    assert_eq!(node_of(&ledger, &successor), Some("r.final"));
    Ok(())
}

#[tokio::test]
async fn a_narrowing_whose_sentence_states_the_mate_twice_in_other_words_ends_the_target_removed(
) -> Result<()> {
    let (mut ledger, target, mate) = cut_beside_a_mate().await?;

    let pass = identified(
        &mut ledger,
        &Model::new(),
        &lesson_of(REWRITTEN),
        &[
            (REWRITTEN, HEADER_REWORDED),
            (REWRITTEN, HEADER_REWORDED_AGAIN),
        ],
        &[],
    )
    .await?;

    let entry = ledger.entry(&target).unwrap();
    assert_eq!(
        entry.decided(),
        Some(&Then::End(End::Removed {
            section_goes: false
        }))
    );
    assert!(
        matches!(
            entry.state(),
            ClaimState::Removed {
                by: Actor::Machine,
                ..
            }
        ),
        "{:?}",
        entry.state()
    );
    assert!(matches!(
        events_of(entry).last(),
        Some(ClaimEvent::Stepped {
            step: Step::Cut,
            outcome: Outcome::Cut,
            ..
        })
    ));
    let successor = pass.statements[0].id.clone();
    assert_ne!(successor, mate);
    assert!(matches!(
        ledger.entry(&mate).unwrap().state(),
        ClaimState::Edited {
            successor: Some(next),
            by: Actor::Machine,
            ..
        } if *next == successor
    ));
    let second = ledger.entry(&pass.statements[1].id).unwrap();
    assert!(
        events_of(second).iter().any(|event| matches!(
            event,
            ClaimEvent::BornOfRepair { parent, .. } if *parent == target
        )),
        "{:?}",
        events_of(second)
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_at_a_rewrite_or_a_cut_is_repaired_and_one_at_any_other_node_is_taken_by_the_next_pass(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let body = format!("{SENTENCE} {HEADER} {REWRITTEN}");
    let rewriting = claim(
        &mut ledger,
        "rewriting",
        (SENTENCE, SENTENCE),
        &body,
        SOURCED,
    )
    .await?;
    walk(&mut ledger, &rewriting, Class::Sourced, &REWRITING).await?;
    let researching = claim(&mut ledger, "researching", (HEADER, HEADER), &body, SOURCED).await?;
    walk(&mut ledger, &researching, Class::Sourced, &RESEARCHING).await?;
    let claims = [
        (SENTENCE, SENTENCE),
        (HEADER, HEADER),
        (REWRITTEN, REWRITTEN),
    ];
    let lesson = lesson_of(&body);
    // A third statement, new to the ledger, waits for its first reading.
    identified(&mut ledger, &Model::new(), &lesson, &claims, &[]).await?;

    // The repair comes first, and takes only the statement at a rewrite.
    match ledger.decide(&policy, &[]) {
        Decision::Repair(targets) => assert_eq!(targets, std::slice::from_ref(&rewriting)),
        _ => panic!("the statement at a rewrite is repaired"),
    }
    assert!(ledger.may_repair(&policy, &rewriting));
    assert!(!ledger.may_repair(&policy, &researching));

    // Once nothing waits to be repaired, the statements at other nodes are
    // taken by the next pass, each named with its node.
    ledger
        .record_step(&policy, &rewriting, REWRITE, Outcome::Rewritten)
        .await?;
    let Decision::Continue(waiting) = ledger.decide(&policy, &[]) else {
        panic!("statements wait for the next pass");
    };
    let nodes: Vec<(&str, Step)> = waiting
        .iter()
        .map(|waits| (waits.node, waits.step))
        .collect();
    assert!(nodes.contains(&("s.read", PARTS)));
    assert!(nodes.contains(&("s.final", PARTS)));
    assert!(nodes.contains(&("s.research", Step::Research)));
    Ok(())
}

#[tokio::test]
async fn a_record_that_does_not_fit_its_graph_stops_the_lesson_naming_the_claim_its_node_and_the_step(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "odd", (SENTENCE, SENTENCE), SENTENCE, SOURCED).await?;
    // A knowledge reading at a part-wise node.
    let stepped = ClaimEvent::Stepped {
        policy: policy.version().into(),
        step: Step::Read(Reading::Knowledge),
        outcome: Outcome::Stands,
        then: Then::End(End::GeneralKnowledge),
        reason: String::new(),
        rewrite_ahead: false,
    };
    ledger.append(&id, stepped).await?;
    identified(
        &mut ledger,
        &Model::new(),
        &lesson_of(SENTENCE),
        &[(SENTENCE, SENTENCE)],
        &[],
    )
    .await?;

    let Decision::Stop(reason) = ledger.decide(&policy, &[]) else {
        panic!("a record that does not fit stops the lesson");
    };

    assert_eq!(reason.code, StopCode::PolicyRecordFault);
    for named in [SENTENCE, "s.read", "Knowledge", "Stands"] {
        assert!(reason.message.contains(named), "{}", reason.message);
    }
    Ok(())
}

#[tokio::test]
async fn a_claim_decided_under_another_policy_stops_the_lesson_with_that_reason() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(&mut ledger, "old", (SENTENCE, SENTENCE), SENTENCE, SOURCED).await?;
    let stepped = ClaimEvent::Stepped {
        policy: "acceptance-v0".into(),
        step: PARTS,
        outcome: Outcome::Unproven,
        then: Then::Node {
            name: "s.research".into(),
            step: Step::Research,
        },
        reason: String::new(),
        rewrite_ahead: true,
    };
    ledger.append(&id, stepped).await?;
    identified(
        &mut ledger,
        &Model::new(),
        &lesson_of(SENTENCE),
        &[(SENTENCE, SENTENCE)],
        &[],
    )
    .await?;

    let Decision::Stop(reason) = ledger.decide(&policy, &[]) else {
        panic!("decisions of another policy stop the lesson");
    };

    assert_eq!(reason.code, StopCode::PolicyChanged);
    assert!(reason.message.contains("earlier checking rules"));
    assert!(reason.message.contains("acceptance-v0"));
    Ok(())
}

#[tokio::test]
async fn a_run_stopped_on_a_second_unreadable_answer_is_resumed_by_the_next_run_which_asks_once_more(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(
        &mut ledger,
        "stuck",
        (SENTENCE, SENTENCE),
        SENTENCE,
        SOURCED,
    )
    .await?;
    let lesson = lesson_of(SENTENCE);
    let claims = [(SENTENCE, SENTENCE)];

    // The first run asks twice and cannot read either answer.
    let pass = identified(&mut ledger, &Model::new(), &lesson, &claims, &[]).await?;
    let first = Says::new(&[Outcome::Unjudged]);
    let stopped = read_pass(&mut ledger, &first, &pass, &lesson).await?;

    assert_eq!(stopped, std::slice::from_ref(&id));
    assert_eq!(first.asked().len(), 2);
    let Decision::Stop(reason) = ledger.decide(&policy, &stopped) else {
        panic!("the run stops");
    };
    assert_eq!(reason.code, StopCode::CheckerUnreadable);
    let entry = ledger.entry(&id).unwrap();
    assert_eq!(entry.unjudged(), 2);
    assert_eq!(entry.state(), &ClaimState::Unchecked);
    // The claim waits at its node with its re-ask spent.
    assert_eq!(
        ledger.position(&policy, &id),
        Some(Walked::At(Position {
            class: Class::Sourced,
            visited: lattice_core::domain::learning::acceptance::Classes::of(Class::Sourced),
            reasked: true,
            node: policy.graph(Class::Sourced).entry(),
            rewrites_left: 2,
        }))
    );

    // The next run, with the same checker or another, asks it once more, and
    // the lesson goes on when it reads.
    let pass = identified(&mut ledger, &Model::new(), &lesson, &claims, &[]).await?;
    assert_eq!(ledger.due(&policy).len(), 1);
    let second = Says::new(&[Outcome::Stands]);
    let stopped = read_pass(&mut ledger, &second, &pass, &lesson).await?;

    assert!(stopped.is_empty());
    assert_eq!(second.asked(), [SENTENCE], "asked once");
    assert!(matches!(
        ledger.entry(&id).unwrap().state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    assert!(matches!(
        ledger.decide(&policy, &stopped),
        Decision::Publish
    ));
    Ok(())
}

#[tokio::test]
async fn a_verified_claim_whose_source_was_replaced_is_reopened_before_anything_is_read_and_the_round_does_not_read_it(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let id = claim(
        &mut ledger,
        "verified",
        (SENTENCE, SENTENCE),
        SENTENCE,
        SOURCED,
    )
    .await?;
    let lesson = lesson_of(SENTENCE);
    let claims = [(SENTENCE, SENTENCE)];
    // The claim was resolved on a version of a source the course has since
    // replaced.
    let pass = identified(&mut ledger, &Model::new(), &lesson, &claims, &[]).await?;
    let reader = Says::new(&[Outcome::Stands]);
    read_pass(&mut ledger, &reader, &pass, &lesson).await?;
    assert!(matches!(
        ledger.entry(&id).unwrap().state(),
        ClaimState::ResolvedByEvidence { source_version_id, .. } if source_version_id == "source-1"
    ));
    assert_eq!(reader.asked().len(), 1);

    // The next pass finds the course without it.
    let current = [LearningSourceDto {
        id: "source-2".into(),
        title: "The replacement".into(),
        url: None,
        excerpt: SENTENCE.into(),
        acquired_at: 0,
    }];
    let pass = identified(&mut ledger, &Model::new(), &lesson, &claims, &current).await?;

    assert!(matches!(
        ledger.entry(&id).unwrap().state(),
        ClaimState::Reopened {
            cause: ReopenCause::SourceDeleted(version),
            ..
        } if version == "source-1"
    ));
    assert!(ledger.due(&policy).is_empty());
    let again = Says::new(&[Outcome::Stands]);
    read_pass(&mut ledger, &again, &pass, &lesson).await?;
    assert!(again.asked().is_empty(), "the round reads nothing");
    Ok(())
}

/// A claim that only waits for the user has no sentence left to show them once
/// that sentence is written over: the words in its place are new, and are read.
/// Carrying it on would put its saved "supported" on words nothing ever read.
#[tokio::test]
async fn a_sentence_written_over_while_its_claim_is_reopened_is_a_new_statement_and_is_read(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let reopened = claim(
        &mut ledger,
        "reopened",
        (SENTENCE, SENTENCE),
        SENTENCE,
        SOURCED,
    )
    .await?;
    let lesson = lesson_of(SENTENCE);
    let claims = [(SENTENCE, SENTENCE)];
    let pass = identified(&mut ledger, &Model::new(), &lesson, &claims, &[]).await?;
    read_pass(&mut ledger, &Says::new(&[Outcome::Stands]), &pass, &lesson).await?;
    let current = [LearningSourceDto {
        id: "source-2".into(),
        title: "The replacement".into(),
        url: None,
        excerpt: SENTENCE.into(),
        acquired_at: 0,
    }];
    identified(&mut ledger, &Model::new(), &lesson, &claims, &current).await?;
    assert!(matches!(
        ledger.entry(&reopened).unwrap().state(),
        ClaimState::Reopened { .. }
    ));

    // Something outside the pipeline writes over the sentence.
    let model = Classes::new(&[]).classed(&[(HEADER, "sourced")]);
    let lesson = lesson_of(HEADER);
    let pass = identified(&mut ledger, &model, &lesson, &[(HEADER, HEADER)], &current).await?;

    let written = pass.statements[0].id.clone();
    assert_ne!(written, reopened, "its end was reached on another sentence");
    assert!(matches!(
        ledger.entry(&reopened).unwrap().state(),
        ClaimState::Removed {
            by: Actor::Machine,
            ..
        }
    ));
    assert_eq!(node_of(&ledger, &written), Some("s.read"));
    assert_eq!(
        ledger
            .due(&policy)
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        [written]
    );
    Ok(())
}

#[tokio::test]
async fn a_confirmed_claim_whose_sentence_or_bound_source_changed_is_reopened_before_anything_is_read(
) -> Result<()> {
    const BOUND: &str = "bound-source";
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let model = Model::new();
    let body = format!("{SENTENCE} {HEADER}");
    let draft = lesson_of(&body).to_string();
    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        lesson_drafts::resume("switch-fixture".into()).await?;
        lesson_drafts::save(&draft).await?;
        let mut ledger = claim_ledger::ClaimLedger::open(&model, &draft).await?;
        let changes = claim(&mut ledger, "changes", (SENTENCE, SENTENCE), &body, SOURCED).await?;
        let stays = claim(&mut ledger, "stays", (HEADER, HEADER), &body, SOURCED).await?;
        for id in [&changes, &stays] {
            walk(&mut ledger, id, Class::Sourced, &REWRITING).await?;
        }
        let store = ClaimReviewRepository::new(db.clone());
        let service = ClaimReviewService::new(db.clone());
        let mut revision = store.load(&lesson_id).await?.revision.0;
        for (id, sources) in [(&changes, vec![]), (&stays, vec![BOUND.to_owned()])] {
            revision = service
                .resolve(
                    &lesson_id,
                    &id.0,
                    ClaimActionRequestDto::Confirm {
                        note: Some("Checked by hand.".into()),
                        source_version_ids: sources,
                    },
                    revision,
                )
                .await?;
        }
        ledger.sync().await?;
        assert!(matches!(
            ledger.entry(&stays).unwrap().state(),
            ClaimState::Confirmed { .. }
        ));

        // The next pass finds the first sentence changed and the bound source
        // gone from the course.
        let rewritten = format!("{REWRITTEN} {HEADER}");
        identified(
            &mut ledger,
            &model,
            &lesson_of(&rewritten),
            &[(REWRITTEN, REWRITTEN), (HEADER, HEADER)],
            &[],
        )
        .await?;

        let cause = |id: &ClaimId| match ledger.entry(id).map(ClaimLedgerEntry::state) {
            Some(ClaimState::Reopened { cause, .. }) => Some(cause.clone()),
            _ => None,
        };
        assert_eq!(cause(&changes), Some(ReopenCause::SentenceChanged));
        assert_eq!(
            cause(&stays),
            Some(ReopenCause::SourceDeleted(BOUND.into()))
        );
        assert!(ledger.due(&StandardPolicy::new()).is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_statement_counts_as_reused_only_when_every_reading_of_it_was_restored() -> Result<()> {
    let mut ledger = open().await?;
    let body = format!("{SENTENCE} {HEADER}");
    claim(&mut ledger, "whole", (SENTENCE, SENTENCE), &body, REASONED).await?;
    claim(&mut ledger, "half", (HEADER, HEADER), &body, REASONED).await?;
    let lesson = lesson_of(&body);
    let claims = [(SENTENCE, SENTENCE), (HEADER, HEADER)];
    let pass = identified(&mut ledger, &Model::new(), &lesson, &claims, &[]).await?;
    // Each statement is read for consistency, then put to the veto. Both
    // readings of the first were restored from saved checks; the second's veto
    // was asked.
    let reader = Says::restoring(&[Outcome::Stands], &[true, true, true, false]);

    let (stopped, trace) =
        crate::lesson_progress::traced(read_pass(&mut ledger, &reader, &pass, &lesson)).await;

    assert!(stopped?.is_empty());
    assert_eq!((trace.checked, trace.reused), (2, 1));
    Ok(())
}

#[tokio::test]
async fn the_reading_phase_says_how_many_statements_are_checked() -> Result<()> {
    let policy = StandardPolicy::new();
    let body = format!("{SENTENCE} {HEADER}");
    let lesson = lesson_of(&body);
    for (claims, words) in [
        (vec![(SENTENCE, SENTENCE)], "Checking 1 statement"),
        (
            vec![(SENTENCE, SENTENCE), (HEADER, HEADER)],
            "Checking 2 statements",
        ),
    ] {
        let mut ledger = open().await?;
        let pass = identified(&mut ledger, &Model::new(), &lesson, &claims, &[]).await?;
        let due = ledger.due(&policy);
        let references = ReferenceCollection::lexical(&[])?;
        let reader = Says::new(&[Outcome::Stands]);
        let checks = ClaimChecks::default();

        let (read, trace) = crate::lesson_progress::traced(passes::read_steps(
            &Model::new(),
            &policy,
            &reader,
            &ledger,
            &checks,
            (&lesson, &references),
            (&pass, &due),
        ))
        .await;

        read?;
        assert_eq!(trace.message, words);
    }
    Ok(())
}

#[tokio::test]
async fn a_pass_that_changed_nothing_stops_naming_what_each_statement_waits_for_in_words(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let body = format!("{SENTENCE} {HEADER}");
    let researching = claim(&mut ledger, "a", (SENTENCE, SENTENCE), &body, SOURCED).await?;
    walk(&mut ledger, &researching, Class::Sourced, &RESEARCHING).await?;
    let reciting = claim(&mut ledger, "b", (HEADER, HEADER), &body, SOURCED).await?;
    walk(&mut ledger, &reciting, Class::Sourced, &RECITING).await?;
    identified(
        &mut ledger,
        &Model::new(),
        &lesson_of(&body),
        &[(SENTENCE, SENTENCE), (HEADER, HEADER)],
        &[],
    )
    .await?;
    let Decision::Continue(waiting) = ledger.decide(&policy, &[]) else {
        panic!("statements wait for the next pass");
    };

    let error = ledger
        .stop_no_progress("{}", &waiting, None)
        .await?
        .to_string();

    assert_eq!(
        error,
        format!(
            "The checks stopped because a whole pass changed nothing: \"{SENTENCE}\" waits for research; \"{HEADER}\" waits for its citation to move."
        )
    );
    // Research that reached nothing is named as the cause of those waiting for it.
    let error = ledger
        .stop_no_progress("{}", &waiting, Some(Unresearched::Unreached))
        .await?
        .to_string();
    assert_eq!(
        error,
        format!(
            "Research could not reach any search engine or web page, so 1 statement still waits for a source: \"{SENTENCE}\"."
        )
    );
    // A plan research could not use is named as the cause instead.
    let error = ledger
        .stop_no_progress("{}", &waiting, Some(Unresearched::Unplanned))
        .await?
        .to_string();
    assert_eq!(
        error,
        format!(
            "Research could not plan its searches: the Learning model gave no plan Lattice could use, so 1 statement still waits for a source: \"{SENTENCE}\"."
        )
    );
    Ok(())
}

/// A check of the fixture model.
pub(super) fn local_check(result: CheckResult, reason: &str, evidence: Vec<EvidenceSeen>) -> Check {
    Check {
        checker: Checker::Local {
            model: Model::new().model_name().into(),
        },
        result,
        reason: reason.into(),
        fingerprint: EvidenceFingerprint::of(POLICY.as_str(), Model::new().model_name(), &evidence),
        evidence,
        window_tokens: 128_000,
    }
}

/// A local check that found `statement` stated elsewhere, in `source`.
fn found_elsewhere(source: &LearningSourceDto, statement: &str, at: usize) -> Check {
    let evidence = vec![EvidenceSeen {
        source_version_id: source.id.clone(),
        start_byte: at,
        end_byte: at + statement.len(),
        text_sha256: text_sha256(statement),
    }];
    let result = CheckResult::SupportedElsewhere {
        cited: vec![],
        source_version_id: source.id.clone(),
        supporting_quote: statement.into(),
    };
    local_check(result, "Stated elsewhere.", evidence)
}

#[tokio::test]
async fn a_claim_at_recite_whose_finding_was_lost_is_recited_from_the_finding_rebuilt_from_the_ledger(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let policy = StandardPolicy::new();
    let at = sources[0].excerpt.find(GOOD).unwrap();
    let mut lesson = json!({"blocks":[{"kind":"explanation","title":"Rows","body":GOOD,"rubric":[],
        "sourceIndex":0,"quote":"An unrelated quotation."}],"questions":[]});

    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        lesson_drafts::resume("rebuild-fixture".into()).await?;
        let mut ledger = open().await?;
        let id = claim(&mut ledger, "recite", (GOOD, GOOD), GOOD, SOURCED).await?;
        walk(&mut ledger, &id, Class::Sourced, &RECITING).await?;
        // What an earlier run of the job left: the check that found the support
        // elsewhere, and the passage the card shows.
        ledger
            .append(
                &id,
                ClaimEvent::Checked(found_elsewhere(&sources[0], GOOD, at)),
            )
            .await?;
        let shown = EvidencePassage {
            source_id: sources[0].id.clone(),
            text: GOOD.into(),
            start_byte: at,
            end_byte: at + GOOD.len(),
            retrieval_kind: "lexical".into(),
            score: 1.0,
        };
        ledger.show_evidence(&id, &[shown]).await?;
        let pass = identified(
            &mut ledger,
            &Model::new(),
            &lesson,
            &[(GOOD, GOOD)],
            &sources,
        )
        .await?;

        // This job saved no finding for the claim.
        let findings = passes::latest_findings(&ledger, &mut ClaimChecks::default(), &pass).await?;

        let [finding] = findings.as_slice() else {
            panic!("one finding: {findings:?}");
        };
        assert_eq!(finding.verdict, ClaimVerdict::Supported);
        assert_eq!(
            finding.two_pass.supported_by.as_ref().map(|found| (
                found.source_id.as_str(),
                found.start_byte,
                found.end_byte
            )),
            Some((sources[0].id.as_str(), at, at + GOOD.len()))
        );
        assert_eq!(
            finding.class.map(|classed| classed.class),
            Some(Class::Sourced)
        );
        let report = pass_report(findings);
        let moved = super::super::repair_loop::recite(
            &mut ledger,
            &policy,
            (&mut lesson, &report),
            (&mut vec![sources[0].clone()], &mut HashSet::new()),
            &references,
        )
        .await?;
        assert!(moved, "the claim is recited");
        assert_eq!(node_of(&ledger, &id), Some("s.citeread"));
        assert_eq!(lesson["blocks"][0]["quote"], "An unrelated quotation.");
        assert_eq!(
            lesson["blocks"][0]["citations"],
            json!([{"sentence":GOOD,"sourceIndex":0,"quote":GOOD}])
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_claim_the_user_resolved_with_evidence_of_their_own_is_reported_with_that_check_whatever_a_reading_left(
) -> Result<()> {
    let mut ledger = open().await?;
    let id = claim(
        &mut ledger,
        "attached",
        (SENTENCE, SENTENCE),
        SENTENCE,
        SOURCED,
    )
    .await?;
    let pass = identified(
        &mut ledger,
        &Model::new(),
        &lesson_of(SENTENCE),
        &[(SENTENCE, SENTENCE)],
        &[],
    )
    .await?;
    let mut checks = ClaimChecks::default();
    // What a reading of an earlier run left, and the check against the page
    // the user attached.
    let mut read = elsewhere(SENTENCE, "source-1", 0..SENTENCE.len());
    read.reason = "A reading of the course.".into();
    checks.latest.insert(id, read);
    let mut attached = elsewhere(SENTENCE, "attached", 0..SENTENCE.len());
    attached.reason = "The page the user attached states it.".into();
    attached.two_pass = Default::default();
    attached.evidence = vec![EvidencePassage {
        retrieval_kind: "user_attached".into(),
        ..passage("attached")
    }];
    checks.settled.insert((0, SENTENCE.to_owned()), attached);

    let findings = passes::latest_findings(&ledger, &mut checks, &pass).await?;

    let [finding] = findings.as_slice() else {
        panic!("one finding: {findings:?}");
    };
    assert_eq!(finding.reason, "The page the user attached states it.");
    assert_eq!(finding.evidence[0].retrieval_kind, "user_attached");
    assert!(finding.two_pass.supported_by.is_none());
    assert_eq!(
        finding.class.map(|classed| classed.class),
        Some(Class::Sourced)
    );
    Ok(())
}

#[tokio::test]
async fn a_claim_at_a_rewrite_whose_finding_was_lost_sends_the_repair_its_reason_and_the_passages_its_card_shows(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let policy = StandardPolicy::new();
    let lesson = lesson_of(SENTENCE);

    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        lesson_drafts::resume("rebuild-fixture".into()).await?;
        let mut ledger = open().await?;
        let id = claim(
            &mut ledger,
            "rewrite",
            (SENTENCE, SENTENCE),
            SENTENCE,
            SOURCED,
        )
        .await?;
        walk(&mut ledger, &id, Class::Sourced, &REWRITING).await?;
        let evidence = vec![EvidenceSeen {
            source_version_id: sources[0].id.clone(),
            start_byte: 0,
            end_byte: GOOD.len(),
            text_sha256: text_sha256(GOOD),
        }];
        let failed = CheckResult::Failed {
            kind: VerdictKind::Unsupported,
        };
        ledger
            .append(
                &id,
                ClaimEvent::Checked(local_check(failed, "No passage states it.", evidence)),
            )
            .await?;
        ledger
            .show_evidence(&id, &[passage(&sources[0].id)])
            .await?;
        let pass = identified(
            &mut ledger,
            &Model::new(),
            &lesson,
            &[(SENTENCE, SENTENCE)],
            &sources,
        )
        .await?;

        let findings = passes::latest_findings(&ledger, &mut ClaimChecks::default(), &pass).await?;

        let report = pass_report(findings);
        let context = super::super::repair_request::repair_context(
            "{}",
            &lesson,
            &report,
            (&ledger, &policy),
            std::slice::from_ref(&id),
            &references,
        )?;
        let [failed] = context["failedClaims"].as_array().unwrap().as_slice() else {
            panic!("one failed claim: {context}");
        };
        assert_eq!(failed["claimId"], "rewrite");
        assert_eq!(
            failed["goal"],
            repair::goal_words(Goal::WhatThePassageStates)
        );
        assert_eq!(failed["reason"], "No passage states it.");
        assert_eq!(failed["evidence"], json!(["evidence-0"]));
        assert_eq!(context["evidence"][0]["text"], SENTENCE);
        Ok(())
    })
    .await
}
