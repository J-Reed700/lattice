//! The reading driver over scripted readers: the readings it makes along each
//! class's graph, the decision it gives each, and the faults it will not
//! guess past.
use super::super::class_checks::finding_of;
use super::super::readings::driver::{read_until_remedy, settled, ReadSteps};
use super::super::readings::{Read, ReadContext, Reader, ReaderSet, Statement};
use super::*;
use lattice_core::domain::learning::acceptance::{
    start, walk_from, AcceptancePolicy, End, NodeId, Outcome, Position, Reading, Step, Taken, Then,
    Walked,
};
use lattice_core::domain::learning::claim_review::FaultStage;
use std::collections::VecDeque;
use std::sync::Arc;

const EVERY_READING: [Reading; 6] = [
    Reading::Parts,
    Reading::Ground,
    Reading::Page,
    Reading::Consistency,
    Reading::Knowledge,
    Reading::Veto,
];

/// The readings asked, in order, each with the class its statements were
/// read as.
type Log = Arc<Mutex<Vec<(Reading, Vec<Classed>)>>>;

/// A reader that answers with the outcomes it is given, one for each
/// statement it reads, and keeps what it was asked.
struct Scripted {
    reading: Reading,
    outcomes: Mutex<VecDeque<Outcome>>,
    log: Log,
    down: bool,
    /// Answers for one statement fewer than it is given.
    short: bool,
    /// Never answers, as a model that does not reply: before it has read,
    /// or after it has told of its readings when it tells.
    held: bool,
    /// Tells the context of each reading it makes before it returns.
    tells: bool,
}

fn verdict_of(outcome: Outcome) -> ClaimVerdict {
    match outcome {
        Outcome::Stands | Outcome::StandsElsewhere => ClaimVerdict::Supported,
        Outcome::Refuted => ClaimVerdict::Contradicted,
        Outcome::Unjudged => ClaimVerdict::Unverified,
        _ => ClaimVerdict::Unsupported,
    }
}

#[async_trait::async_trait]
impl Reader for Scripted {
    fn reading(&self) -> Reading {
        self.reading
    }

    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>> {
        if self.down {
            return Err(AppError::ServiceNotAvailable("the model is down".into()));
        }
        let classes = statements.iter().map(|statement| statement.classed);
        self.log
            .lock()
            .unwrap()
            .push((self.reading, classes.collect()));
        if self.held && !self.tells {
            std::future::pending::<()>().await;
        }
        let mut reads: Vec<Read> = {
            let mut outcomes = self.outcomes.lock().unwrap();
            statements
                .iter()
                .map(|statement| {
                    let outcome = outcomes.pop_front().expect("an outcome for each statement");
                    Read::new(
                        outcome,
                        finding_of(
                            (statement.unit, statement.claim, statement.classed),
                            verdict_of(outcome),
                            format!("{outcome:?}"),
                            &[],
                            None,
                        ),
                        false,
                    )
                })
                .collect()
        };
        if self.short {
            reads.pop();
        }
        if self.tells {
            for (ordinal, read) in reads.iter().enumerate() {
                at.report(ordinal, read);
            }
            if self.held {
                std::future::pending::<()>().await;
            }
        }
        Ok(reads)
    }
}

/// A reader of each reading, scripted.
struct Script {
    readers: Vec<Scripted>,
    log: Log,
}

impl Script {
    fn new() -> Self {
        let log = Log::default();
        let readers = EVERY_READING
            .into_iter()
            .map(|reading| Scripted {
                reading,
                outcomes: Mutex::new(VecDeque::new()),
                log: log.clone(),
                down: false,
                short: false,
                held: false,
                tells: false,
            })
            .collect();
        Self { readers, log }
    }

    fn reader(&mut self, reading: Reading) -> &mut Scripted {
        self.readers
            .iter_mut()
            .find(|reader| reader.reading == reading)
            .expect("a reader of each reading")
    }

    /// `reading` answers with `outcomes`, one for each statement it reads.
    fn says(mut self, reading: Reading, outcomes: &[Outcome]) -> Self {
        let queue = self.reader(reading).outcomes.get_mut().unwrap();
        queue.extend(outcomes.iter().copied());
        self
    }

    fn down(mut self, reading: Reading) -> Self {
        self.reader(reading).down = true;
        self
    }

    fn short(mut self, reading: Reading) -> Self {
        self.reader(reading).short = true;
        self
    }

    fn held(mut self, reading: Reading) -> Self {
        self.reader(reading).held = true;
        self
    }

    fn tells(mut self, reading: Reading) -> Self {
        self.reader(reading).tells = true;
        self
    }

    /// The readings asked, in order, with how many statements each call held.
    fn calls(&self) -> Vec<(Reading, usize)> {
        let log = self.log.lock().unwrap();
        log.iter()
            .map(|(reading, batch)| (*reading, batch.len()))
            .collect()
    }

    /// The class each statement of each call to `reading` was read as.
    fn seen(&self, reading: Reading) -> Vec<Vec<Classed>> {
        let log = self.log.lock().unwrap();
        log.iter()
            .filter(|(asked, _)| *asked == reading)
            .map(|(_, batch)| batch.clone())
            .collect()
    }
}

impl ReaderSet for Script {
    fn get(&self, reading: Reading) -> &dyn Reader {
        self.readers
            .iter()
            .find(|reader| reader.reading == reading)
            .expect("a reader of each reading")
    }
}

fn claim(text: &str) -> Claim {
    Claim {
        quote: text.into(),
        statement: text.into(),
    }
}

fn statement(claim: &Claim, class: Class, by: ClassifiedBy) -> Statement<'_> {
    Statement {
        unit: 0,
        claim,
        classed: Classed { class, by },
    }
}

/// Where a statement of `class` begins.
fn entry(policy: &dyn AcceptancePolicy, class: Class) -> Position {
    start(policy, class)
}

/// Where a statement of `class` stands at `node`, having changed class or not.
fn at(policy: &dyn AcceptancePolicy, class: Class, changed_class: bool, node: &str) -> Position {
    Position {
        class,
        changed_class,
        reasked: false,
        node: policy.graph(class).id_of(node).unwrap(),
        rewrites_left: 2,
    }
}

fn node(name: &str, step: Step) -> Then {
    Then::Node {
        name: name.into(),
        step,
    }
}

/// Where the ledger's replay of `read`'s steps leaves a statement that began
/// at `start`: the driver's own account of it.
fn replayed(policy: &dyn AcceptancePolicy, start: Position, read: &ReadSteps) -> Walked {
    let taken: Vec<Taken> = read
        .steps
        .iter()
        .map(|step| Taken {
            step: Step::Read(step.reading),
            outcome: step.read.outcome(),
        })
        .collect();
    walk_from(policy, start, &taken)
}

/// The driver over `script`, for `statements` in a lesson with no sources.
async fn drive(
    script: &Script,
    retained: &ClaimChecks,
    statements: &[(Statement<'_>, Position)],
) -> Result<Vec<ReadSteps>> {
    let references = ReferenceCollection::lexical(&[])?;
    let lesson = json!({"blocks": [], "questions": []});
    let citations = Citations::default();
    let model = Model::new();
    let at = ReadContext::new(&model, &lesson, &references, &citations, &[], retained).await?;
    read_until_remedy(&StandardPolicy::new(), script, &at, statements).await
}

/// How many statements the progress counts, and how many of them as reused,
/// when the reader of `held` is asked and never answers.
async fn counted_while_held(
    script: &Script,
    held: Reading,
    statements: &[(Statement<'_>, Position)],
) -> (u32, u32) {
    let retained = ClaimChecks::default();
    let ((), trace) = crate::lesson_progress::traced(async {
        crate::lesson_progress::begin_checks(statements.len());
        let work = drive(script, &retained, statements);
        tokio::select! {
            result = work => panic!("The reader of {held:?} should hold the driver: {result:?}"),
            _ = async {
                while !script.calls().iter().any(|(asked, _)| *asked == held) {
                    tokio::task::yield_now().await;
                }
            } => {}
        }
    })
    .await;
    (trace.checked, trace.reused)
}

#[tokio::test]
async fn a_sourced_statement_its_citation_states_is_resolved_in_one_reading() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new().says(Reading::Parts, &[Outcome::Stands]);
    let cited = claim("Quillet is a lexer.");
    let given = [(
        statement(&cited, Class::Sourced, ClassifiedBy::Writer),
        entry(&policy, Class::Sourced),
    )];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    assert_eq!(read.len(), 1);
    assert_eq!(read[0].walked, Walked::Ended(End::Resolved));
    assert_eq!(
        replayed(&policy, entry(&policy, Class::Sourced), &read[0]),
        read[0].walked
    );
    let [step] = read[0].steps.as_slice() else {
        panic!("one reading: {:?}", read[0].steps);
    };
    assert_eq!(step.reading, Reading::Parts);
    assert_eq!(step.read.outcome(), Outcome::Stands);
    assert_eq!(step.then, Then::End(End::Resolved));
    assert!(!step.rewrite_ahead);
    assert_eq!(step.class, None);
    assert_eq!(script.calls(), vec![(Reading::Parts, 1)]);
    Ok(())
}

#[tokio::test]
async fn a_reasoning_statement_that_stands_is_put_to_the_veto_in_the_same_call() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new()
        .says(Reading::Consistency, &[Outcome::Stands])
        .says(Reading::Veto, &[Outcome::Stands]);
    let reasoned = claim("A tokenizer splits text.");
    let given = [(
        statement(&reasoned, Class::Reasoning, ClassifiedBy::Model),
        entry(&policy, Class::Reasoning),
    )];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    assert_eq!(read[0].walked, Walked::Ended(End::Consistent));
    assert_eq!(
        replayed(&policy, entry(&policy, Class::Reasoning), &read[0]),
        read[0].walked
    );
    let steps: Vec<_> = read[0]
        .steps
        .iter()
        .map(|step| {
            (
                step.reading,
                step.read.outcome(),
                step.then.clone(),
                step.rewrite_ahead,
            )
        })
        .collect();
    assert_eq!(
        steps,
        vec![
            (
                Reading::Consistency,
                Outcome::Stands,
                node("r.veto", Step::Read(Reading::Veto)),
                true
            ),
            (
                Reading::Veto,
                Outcome::Stands,
                Then::End(End::Consistent),
                false
            ),
        ]
    );
    assert_eq!(
        script.calls(),
        vec![(Reading::Consistency, 1), (Reading::Veto, 1)]
    );
    Ok(())
}

#[tokio::test]
async fn a_vetoed_reasoning_statement_the_passages_ground_ends_consistent_after_three_readings(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new()
        .says(Reading::Consistency, &[Outcome::Stands])
        .says(Reading::Veto, &[Outcome::Refuted])
        .says(Reading::Ground, &[Outcome::Stands]);
    let reasoned = claim("A tokenizer splits text.");
    let given = [(
        statement(&reasoned, Class::Reasoning, ClassifiedBy::Model),
        entry(&policy, Class::Reasoning),
    )];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    assert_eq!(read[0].walked, Walked::Ended(End::Consistent));
    assert_eq!(
        replayed(&policy, entry(&policy, Class::Reasoning), &read[0]),
        read[0].walked
    );
    let readings: Vec<_> = read[0].steps.iter().map(|step| step.reading).collect();
    assert_eq!(
        readings,
        vec![Reading::Consistency, Reading::Veto, Reading::Ground]
    );
    assert_eq!(
        read[0].steps[1].then,
        node("r.ground", Step::Read(Reading::Ground))
    );
    assert_eq!(read[0].steps[2].then, Then::End(End::Consistent));
    Ok(())
}

#[tokio::test]
async fn a_common_statement_that_needs_a_source_is_read_as_sourced_by_evidence_in_the_same_call(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new()
        .says(Reading::Knowledge, &[Outcome::NeedsSource])
        .says(Reading::Parts, &[Outcome::Stands]);
    let common = claim("Quillet was released in 2019.");
    let given = [(
        statement(&common, Class::Common, ClassifiedBy::Model),
        entry(&policy, Class::Common),
    )];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    assert_eq!(read[0].walked, Walked::Ended(End::Resolved));
    assert_eq!(
        replayed(&policy, entry(&policy, Class::Common), &read[0]),
        read[0].walked
    );
    let [first, second] = read[0].steps.as_slice() else {
        panic!("two readings: {:?}", read[0].steps);
    };
    assert_eq!(first.reading, Reading::Knowledge);
    assert_eq!(first.class, Some(Class::Sourced));
    assert_eq!(first.then, node("s.read", Step::Read(Reading::Parts)));
    assert!(first.rewrite_ahead);
    assert_eq!(second.reading, Reading::Parts);
    assert_eq!(second.class, None);
    let by_evidence = Classed {
        class: Class::Sourced,
        by: ClassifiedBy::Evidence,
    };
    assert_eq!(script.seen(Reading::Parts), vec![vec![by_evidence]]);
    let by_model = Classed {
        class: Class::Common,
        by: ClassifiedBy::Model,
    };
    assert_eq!(script.seen(Reading::Knowledge), vec![vec![by_model]]);
    Ok(())
}

#[tokio::test]
async fn a_statement_from_reasoning_still_unproven_on_its_page_goes_to_uncite() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new().says(Reading::Page, &[Outcome::Unproven]);
    let reasoned = claim("Quillet's commands change nothing.");
    let given = [(
        statement(&reasoned, Class::Reasoning, ClassifiedBy::Model),
        at(&policy, Class::Sourced, true, "s.reread"),
    )];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    let [step] = read[0].steps.as_slice() else {
        panic!("one reading: {:?}", read[0].steps);
    };
    assert_eq!(step.reading, Reading::Page);
    assert_eq!(step.then, node("s.uncite", Step::Uncite));
    assert_eq!(
        read[0].walked,
        Walked::At(at(&policy, Class::Sourced, true, "s.uncite"))
    );
    Ok(())
}

#[tokio::test]
async fn an_unreadable_answer_is_read_once_more_at_the_same_node() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new().says(Reading::Parts, &[Outcome::Unjudged, Outcome::Stands]);
    let cited = claim("Quillet is a lexer.");
    let given = [(
        statement(&cited, Class::Sourced, ClassifiedBy::Writer),
        entry(&policy, Class::Sourced),
    )];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    assert_eq!(read[0].walked, Walked::Ended(End::Resolved));
    assert_eq!(
        replayed(&policy, entry(&policy, Class::Sourced), &read[0]),
        read[0].walked
    );
    let steps: Vec<_> = read[0]
        .steps
        .iter()
        .map(|step| (step.reading, step.read.outcome(), step.then.clone()))
        .collect();
    assert_eq!(
        steps,
        vec![
            (
                Reading::Parts,
                Outcome::Unjudged,
                node("s.read", Step::Read(Reading::Parts))
            ),
            (Reading::Parts, Outcome::Stands, Then::End(End::Resolved)),
        ]
    );
    assert_eq!(
        script.calls(),
        vec![(Reading::Parts, 1), (Reading::Parts, 1)]
    );
    Ok(())
}

#[tokio::test]
async fn a_second_unreadable_answer_stops_the_statement() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new().says(Reading::Parts, &[Outcome::Unjudged, Outcome::Unjudged]);
    let cited = claim("Quillet is a lexer.");
    let given = [(
        statement(&cited, Class::Sourced, ClassifiedBy::Writer),
        entry(&policy, Class::Sourced),
    )];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    assert_eq!(read[0].walked, Walked::Stopped);
    assert_eq!(
        replayed(&policy, entry(&policy, Class::Sourced), &read[0]),
        read[0].walked
    );
    assert_eq!(read[0].steps.len(), 2);
    assert_eq!(read[0].steps[1].then, Then::Stop);
    assert!(!read[0].steps[1].rewrite_ahead);
    assert_eq!(script.calls().len(), 2, "no third reading");
    Ok(())
}

#[tokio::test]
async fn a_statement_whose_specific_nothing_states_waits_at_research() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new().says(Reading::Parts, &[Outcome::Unproven]);
    let cited = claim("Quillet was written in 2004.");
    let given = [(
        statement(&cited, Class::Sourced, ClassifiedBy::Writer),
        entry(&policy, Class::Sourced),
    )];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    assert_eq!(
        read[0].walked,
        Walked::At(at(&policy, Class::Sourced, false, "s.research"))
    );
    assert_eq!(
        replayed(&policy, entry(&policy, Class::Sourced), &read[0]),
        read[0].walked
    );
    assert_eq!(read[0].steps.len(), 1);
    assert_eq!(read[0].steps[0].then, node("s.research", Step::Research));
    assert!(read[0].steps[0].rewrite_ahead);
    Ok(())
}

#[tokio::test]
async fn a_statement_handed_in_at_a_remedy_is_not_read() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new();
    let cited = claim("Quillet was written in 2004.");
    let position = at(&policy, Class::Sourced, false, "s.research");
    let given = [(
        statement(&cited, Class::Sourced, ClassifiedBy::Writer),
        position,
    )];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    assert!(read[0].steps.is_empty());
    assert_eq!(read[0].walked, Walked::At(position));
    assert!(script.calls().is_empty());
    Ok(())
}

#[tokio::test]
async fn statements_at_one_reading_are_read_in_one_call() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new().says(Reading::Parts, &[Outcome::Stands, Outcome::Stands]);
    let (lexer, cli) = (claim("Quillet is a lexer."), claim("Quillet has a CLI."));
    let sourced = |claim| {
        (
            statement(claim, Class::Sourced, ClassifiedBy::Writer),
            entry(&policy, Class::Sourced),
        )
    };
    let given = [sourced(&lexer), sourced(&cli)];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    assert_eq!(script.calls(), vec![(Reading::Parts, 2)]);
    assert!(read
        .iter()
        .all(|steps| steps.walked == Walked::Ended(End::Resolved)));
    Ok(())
}

#[tokio::test]
async fn statements_at_two_readings_are_one_call_each_in_the_fixed_order() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new()
        .says(Reading::Parts, &[Outcome::Stands])
        .says(Reading::Consistency, &[Outcome::Refuted]);
    let (reasoned, cited) = (
        claim("A tokenizer splits text."),
        claim("Quillet is a lexer."),
    );
    // The reasoning statement is handed in first; its reading comes second.
    let given = [
        (
            statement(&reasoned, Class::Reasoning, ClassifiedBy::Model),
            entry(&policy, Class::Reasoning),
        ),
        (
            statement(&cited, Class::Sourced, ClassifiedBy::Writer),
            entry(&policy, Class::Sourced),
        ),
    ];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    assert_eq!(
        script.calls(),
        vec![(Reading::Parts, 1), (Reading::Consistency, 1)]
    );
    assert_eq!(
        read[0].walked,
        Walked::At(at(&policy, Class::Reasoning, false, "r.rewrite"))
    );
    assert_eq!(read[1].walked, Walked::Ended(End::Resolved));
    Ok(())
}

#[tokio::test]
async fn an_outcome_the_node_does_not_answer_is_a_fault() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new().says(Reading::Ground, &[Outcome::AllCommon]);
    let common = claim("Quillet is a lexer.");
    let given = [(
        statement(&common, Class::Common, ClassifiedBy::Model),
        at(&policy, Class::Common, false, "c.ground"),
    )];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    let held = read[0].fault.as_ref().expect("a fault");
    assert_eq!(held.stage, FaultStage::Take);
    assert!(held.message.contains("c.ground"), "{}", held.message);
    assert!(held.message.contains("AllCommon"), "{}", held.message);
    assert!(read[0].steps.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_position_the_graph_does_not_have_is_a_fault() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new();
    let cited = claim("Quillet is a lexer.");
    let nowhere = Position {
        node: NodeId("s.nowhere"),
        ..entry(&policy, Class::Sourced)
    };
    let given = [(
        statement(&cited, Class::Sourced, ClassifiedBy::Writer),
        nowhere,
    )];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    let held = read[0].fault.as_ref().expect("a fault");
    assert_eq!(held.stage, FaultStage::Read);
    assert!(held.message.contains("s.nowhere"), "{}", held.message);
    Ok(())
}

#[tokio::test]
async fn a_reader_that_answers_for_fewer_statements_than_it_was_given_is_a_fault() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new()
        .says(Reading::Parts, &[Outcome::Stands, Outcome::Stands])
        .short(Reading::Parts);
    let (lexer, cli) = (claim("Quillet is a lexer."), claim("Quillet has a CLI."));
    let sourced = |claim| {
        (
            statement(claim, Class::Sourced, ClassifiedBy::Writer),
            entry(&policy, Class::Sourced),
        )
    };
    let given = [sourced(&lexer), sourced(&cli)];

    let read = drive(&script, &ClaimChecks::default(), &given).await?;

    assert!(read
        .iter()
        .all(|walk| matches!(&walk.fault, Some(held) if held.stage == FaultStage::Ask)));
    Ok(())
}

#[tokio::test]
async fn a_reader_that_cannot_be_reached_fails_the_whole_call() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new()
        .says(Reading::Parts, &[Outcome::Stands])
        .down(Reading::Consistency);
    let (cited, reasoned) = (
        claim("Quillet is a lexer."),
        claim("A tokenizer splits text."),
    );
    let given = [
        (
            statement(&cited, Class::Sourced, ClassifiedBy::Writer),
            entry(&policy, Class::Sourced),
        ),
        (
            statement(&reasoned, Class::Reasoning, ClassifiedBy::Model),
            entry(&policy, Class::Reasoning),
        ),
    ];

    let outage = drive(&script, &ClaimChecks::default(), &given).await;

    assert!(
        matches!(outage, Err(AppError::ServiceNotAvailable(_))),
        "{outage:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_quotation_or_the_users_evidence_settles_a_statement_the_gate_does_not_weigh(
) -> Result<()> {
    let sources = vec![LearningSourceDto {
        id: "book".into(),
        title: "The Quillet Book".into(),
        url: None,
        excerpt: "Quillet ships with a toolchain. The lexer reads characters and emits tokens one at a time."
            .into(),
        acquired_at: 1,
    }];
    let references = ReferenceCollection::lexical(&sources)?;
    let citations = Citations::new(
        HashMap::from([(
            0,
            references.cited_passages(&["Quillet ships with a toolchain."])?,
        )]),
        HashMap::new(),
    );
    let (plain, quoted, brought) = (
        claim("Quillet is a lexer."),
        claim(
            "The Quillet Book states: \"The lexer reads characters and emits tokens one at a time.\"",
        ),
        claim("Quillet has a CLI."),
    );
    let classed = Classed {
        class: Class::Sourced,
        by: ClassifiedBy::Writer,
    };
    let retained = ClaimChecks {
        settled: HashMap::from([(
            (0, brought.statement.clone()),
            finding_of(
                (0, &brought, classed),
                ClaimVerdict::Supported,
                "The user's passage states it.".into(),
                &[],
                None,
            ),
        )]),
        ..ClaimChecks::default()
    };
    let lesson = json!({"blocks": [], "questions": []});
    let model = Model::new();
    let at = ReadContext::new(&model, &lesson, &references, &citations, &[], &retained).await?;
    let statements: Vec<Statement<'_>> = [&plain, &quoted, &brought]
        .into_iter()
        .map(|claim| Statement {
            unit: 0,
            claim,
            classed,
        })
        .collect();

    assert_eq!(settled(&at, &statements)?, HashSet::from([1, 2]));
    Ok(())
}

#[tokio::test]
async fn a_statement_that_stands_at_parts_is_counted_when_parts_returns() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new()
        .says(Reading::Parts, &[Outcome::Stands])
        .held(Reading::Consistency);
    let (cited, reasoned) = (
        claim("Quillet is a lexer."),
        claim("A tokenizer splits text."),
    );
    // Parts is read before Consistency, whatever the order the statements are
    // handed in.
    let given = [
        (
            statement(&reasoned, Class::Reasoning, ClassifiedBy::Model),
            entry(&policy, Class::Reasoning),
        ),
        (
            statement(&cited, Class::Sourced, ClassifiedBy::Writer),
            entry(&policy, Class::Sourced),
        ),
    ];

    let counted = counted_while_held(&script, Reading::Consistency, &given).await;

    assert_eq!(
        counted,
        (1, 0),
        "The statement that stood at Parts is counted before the next reading is asked"
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_that_goes_to_a_remedy_is_counted_when_it_gets_there() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new()
        .says(Reading::Parts, &[Outcome::Unproven])
        .held(Reading::Consistency);
    let (cited, reasoned) = (
        claim("Quillet was written in 2004."),
        claim("A tokenizer splits text."),
    );
    let given = [
        (
            statement(&reasoned, Class::Reasoning, ClassifiedBy::Model),
            entry(&policy, Class::Reasoning),
        ),
        (
            statement(&cited, Class::Sourced, ClassifiedBy::Writer),
            entry(&policy, Class::Sourced),
        ),
    ];

    let counted = counted_while_held(&script, Reading::Consistency, &given).await;

    assert_eq!(
        counted,
        (1, 0),
        "The statement that went to research is counted when it gets there"
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_handed_in_at_a_remedy_is_counted_once_when_the_walk_starts() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new();
    let cited = claim("Quillet was written in 2004.");
    let given = [(
        statement(&cited, Class::Sourced, ClassifiedBy::Writer),
        at(&policy, Class::Sourced, false, "s.research"),
    )];
    let retained = ClaimChecks::default();

    let (read, trace) = crate::lesson_progress::traced(async {
        crate::lesson_progress::begin_checks(given.len());
        drive(&script, &retained, &given).await
    })
    .await;

    read?;
    assert_eq!((trace.checked, trace.reused), (1, 0));
    assert!(script.calls().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_statement_a_reader_has_told_of_is_counted_before_the_reader_returns() -> Result<()> {
    let policy = StandardPolicy::new();
    let script = Script::new()
        .says(Reading::Parts, &[Outcome::Stands])
        .tells(Reading::Parts)
        .held(Reading::Parts);
    let cited = claim("Quillet is a lexer.");
    let given = [(
        statement(&cited, Class::Sourced, ClassifiedBy::Writer),
        entry(&policy, Class::Sourced),
    )];

    let counted = counted_while_held(&script, Reading::Parts, &given).await;

    assert_eq!(
        counted,
        (1, 0),
        "A reading the reader told of counts before the reader returns"
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_is_counted_once_whether_or_not_its_reader_told_of_it() -> Result<()> {
    let policy = StandardPolicy::new();
    let (lexer, cli) = (claim("Quillet is a lexer."), claim("Quillet has a CLI."));
    let sourced = |claim| {
        (
            statement(claim, Class::Sourced, ClassifiedBy::Writer),
            entry(&policy, Class::Sourced),
        )
    };
    let given = [sourced(&lexer), sourced(&cli)];
    let retained = ClaimChecks::default();
    for script in [
        Script::new().says(Reading::Parts, &[Outcome::Stands, Outcome::Stands]),
        Script::new()
            .says(Reading::Parts, &[Outcome::Stands, Outcome::Stands])
            .tells(Reading::Parts),
    ] {
        let (read, trace) = crate::lesson_progress::traced(async {
            crate::lesson_progress::begin_checks(given.len());
            drive(&script, &retained, &given).await
        })
        .await;

        read?;
        assert_eq!((trace.checked, trace.reused), (2, 0));
    }
    Ok(())
}
