//! A statement a repair writes beside its target's successor is born of the
//! repair (acceptance design §2.6): it is read once from the post-rewrite node
//! of its class with the successor's rewrites, and nothing is put back or
//! charged.
use super::acceptance_switch_tests::{events_of, lesson_of, open};
use super::claim_ledger_tests::{prepare, section};
use super::class_checks_tests::Classes;
use super::*;
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::acceptance::{
    rewrites_given, AcceptancePolicy, End, Goal, Position, StandardPolicy, Step, Taken, Then,
    Walked,
};
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimState, RepairOutcome,
};

/// A sentence that states a figure no source gives and, beside it, what the
/// course states.
const SENTENCE: &str =
    "DictReader buffers 100 rows before parsing, and missing values default to None in nonblank rows.";
const BUFFERS: &str = "DictReader buffers 100 rows before parsing.";
const MISSING: &str = "Missing values default to None in nonblank rows.";
/// The figure's part said anew, beside the same neighbour.
const READS: &str =
    "DictReader reads one row at a time, and missing values default to None in nonblank rows.";
const READS_ONE: &str = "DictReader reads one row at a time.";
/// A statement no sentence of these lessons makes.
const NAMES: &str = "DictReader names each field after its header.";
/// The neighbour's statement alone, at more length than the whole sentence it
/// stood on.
const MISSING_ALONE: &str = "Missing values default to None in nonblank rows, whatever DictReader does with the rows before them.";
/// The figure's part said anew as a sentence of its own.
const BUFFERS_ONE: &str = "DictReader buffers rows one at a time before parsing.";
/// The sentence said anew with another figure no source gives.
const STILL_FAILING: &str =
    "DictReader buffers 200 rows before parsing, and missing values default to None in nonblank rows.";
const BUFFERS_200: &str = "DictReader buffers 200 rows before parsing.";
/// A statement that shares more words with `CACHES` than with the figure's
/// sentence.
const KEEPS: &str = "DictReader keeps each file's headers.";
/// A second failed sentence, its correction, and a sentence that states
/// nothing to keep the two apart.
const CACHES: &str = "DictReader caches 200 headers per file.";
const CACHED: &str = "DictReader caches the header row once per file.";
const BETWEEN: &str = "Open the sample file before you read on.";

/// A model that extracts the statements each scripted sentence makes from a
/// passage that holds it, finds the `failing` ones contradicted and the others
/// supported, and answers its repair requests in turn, each by writing its
/// replacements into the section. It keeps every repair request and every
/// statement it is asked to sort; a repair request past the script ends the
/// run. It sorts, judges consistency and answers for knowledge as `Classes`
/// does, the neighbour's statement being Reasoning.
struct Script {
    inner: Classes,
    sentences: Vec<(&'static str, Vec<&'static str>)>,
    failing: Vec<&'static str>,
    repairs: Vec<Vec<(&'static str, String)>>,
    /// Sentences its extraction finds only once a repair was requested.
    late: Vec<&'static str>,
    /// Reports another model identity once a repair was requested.
    renamed_after_repair: bool,
    requests: Mutex<Vec<Value>>,
    sorted: Mutex<Vec<String>>,
}

impl Script {
    fn new(sentences: &[(&'static str, &[&'static str])]) -> Self {
        Self {
            inner: Classes::new(&[]).classed(&[(MISSING, "reasoning")]),
            sentences: sentences
                .iter()
                .map(|(sentence, statements)| (*sentence, statements.to_vec()))
                .collect(),
            failing: Vec::new(),
            repairs: Vec::new(),
            late: Vec::new(),
            renamed_after_repair: false,
            requests: Mutex::new(Vec::new()),
            sorted: Mutex::new(Vec::new()),
        }
    }

    fn failing(mut self, statements: &[&'static str]) -> Self {
        self.failing = statements.to_vec();
        self
    }

    /// The n-th repair request writes each `to` of `repairs[n]` where the
    /// section says its `from`.
    fn repairs(mut self, repairs: Vec<Vec<(&'static str, String)>>) -> Self {
        self.repairs = repairs;
        self
    }

    fn late(mut self, sentences: &[&'static str]) -> Self {
        self.late = sentences.to_vec();
        self
    }

    fn renamed_after_repair(mut self) -> Self {
        self.renamed_after_repair = true;
        self
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }

    fn sorted(&self) -> Vec<String> {
        self.sorted.lock().unwrap().clone()
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Extract lesson claims.") {
            let data = fixture_task(prompt).unwrap();
            let units: Vec<Value> = data["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|unit| self.claims_of(unit))
                .collect();
            return Ok(inventory::fixture_response(units).to_string());
        }
        if prompt.starts_with("You sort statements extracted from a lesson") {
            let Some(Value::Object(statements)) = fixture_task(prompt) else {
                panic!("a JSON task");
            };
            self.sorted.lock().unwrap().extend(
                statements
                    .values()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
        }
        if prompt.starts_with("Repair verified lesson defects.") {
            return self.repair(fixture_task(prompt).unwrap());
        }
        if prompt.starts_with("Source passages:") {
            let claim = prompt
                .split("\n\nClaim: ")
                .nth(1)
                .and_then(|rest| rest.lines().next())
                .unwrap_or_default();
            if self
                .failing
                .iter()
                .any(|failing| claim.starts_with(*failing))
            {
                return Ok(format!(
                    "contradicted\nReason: The reference gives another figure.\nSource quote: {GOOD}\nSource passage: passage-0"
                ));
            }
        }
        self.inner.respond(prompt).await
    }

    /// One unit of an extraction request's inventory: the statements of each
    /// scripted sentence its passages hold.
    fn claims_of(&self, unit: &Value) -> Value {
        let mut claims = Vec::new();
        let asked = self.requests.lock().unwrap().len();
        for passage in unit["passages"].as_array().into_iter().flatten() {
            let text = passage_text(passage);
            for (sentence, statements) in &self.sentences {
                if text.contains(*sentence) && (asked > 0 || !self.late.contains(sentence)) {
                    claims.extend(statements.iter().map(|statement| {
                        json!({"sentenceId":sentence_id(passage, sentence),"statement":statement})
                    }));
                }
            }
        }
        let reason = if claims.is_empty() {
            "The fixture's section holds only practice."
        } else {
            ""
        };
        json!({"index":unit["index"],"claims":claims,"nonFactualReason":reason})
    }

    /// The next scripted correction of the section `request` sends, or the end
    /// of the run.
    fn repair(&self, request: Value) -> Result<String> {
        let mut requests = self.requests.lock().unwrap();
        let body = request["candidate"]["blocks"][0]["body"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let script = self.repairs.get(requests.len()).cloned();
        requests.push(request);
        let Some(replacements) = script else {
            return Err(AppError::Network(
                "the run ends at a repair request past the script".into(),
            ));
        };
        let after = replacements
            .iter()
            .fold(body.clone(), |text, (from, to)| text.replace(*from, to));
        Ok(json!({"edits":[{"path":"/blocks/0/body","before":body,"after":after}]}).to_string())
    }
}

#[async_trait::async_trait]
impl LLMPort for Script {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        if self.renamed_after_repair && !self.requests.lock().unwrap().is_empty() {
            return "script-fixture-after-repair";
        }
        "script-fixture"
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

/// How preparing a lesson of one section ended, the claims its ledger holds
/// after, and the section's body in the draft the run saved last.
struct Prepared {
    ended: Result<(String, LessonVerificationReport)>,
    claims: Vec<ClaimLedgerEntry>,
    saved: Option<String>,
}

impl Prepared {
    /// The section's body in the lesson the run published.
    fn published(&self) -> Option<String> {
        let (raw, _) = self.ended.as_ref().ok()?;
        body_of(raw)
    }
}

/// The body of the first section of `raw`, a lesson.
fn body_of(raw: &str) -> Option<String> {
    let lesson: Value = serde_json::from_str(raw).ok()?;
    lesson["blocks"][0]["body"].as_str().map(str::to_owned)
}

/// Prepares a lesson whose one section's body is `body`, with `model`.
async fn prepared(model: &Script, body: &str) -> Result<Prepared> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let raw = lesson_of(body).to_string();
    let ended = prepare(&db, &repo, &job, &lesson_id, model, Some(raw), &[source()]).await;
    let claims = ClaimReviewRepository::new(db.clone())
        .load(&lesson_id)
        .await?
        .claims;
    let saved = repo
        .lesson_draft(&job, &lesson_id)
        .await?
        .and_then(|draft| body_of(&draft.candidate));
    Ok(Prepared {
        ended,
        claims,
        saved,
    })
}

/// The claims of `claims` first extracted as `statement`.
fn first_stated<'a>(claims: &'a [ClaimLedgerEntry], statement: &str) -> Vec<&'a ClaimLedgerEntry> {
    claims
        .iter()
        .filter(|claim| {
            let first = claim
                .events()
                .iter()
                .find_map(|record| match &record.event {
                    ClaimEvent::Extracted {
                        statement: said, ..
                    } => Some(said.as_str()),
                    _ => None,
                });
            first == Some(statement)
        })
        .collect()
}

/// The claims of `claims` that hold a `BornOfRepair`, each with its parent and
/// the attempts it was given.
fn born(claims: &[ClaimLedgerEntry]) -> Vec<(&ClaimLedgerEntry, ClaimId, u8)> {
    claims
        .iter()
        .filter_map(|claim| {
            events_of(claim).into_iter().find_map(|event| match event {
                ClaimEvent::BornOfRepair {
                    parent,
                    attempts_left,
                } => Some((claim, parent.clone(), *attempts_left)),
                _ => None,
            })
        })
        .collect()
}

/// The parent and the attempts of the one claim of `claims` born of a repair,
/// which states `statement`.
fn born_stating(claims: &[ClaimLedgerEntry], statement: &str) -> (ClaimId, u8) {
    let found = born(claims);
    let [(claim, parent, attempts_left)] = found.as_slice() else {
        panic!("one claim born of a repair: {}", found.len());
    };
    assert_eq!(claim.statement(), statement);
    (parent.clone(), *attempts_left)
}

/// Asserts that no rejection recorded for a claim of `claims` says a
/// correction wrote a statement its sentence did not make.
fn none_charged_for_writing(claims: &[ClaimLedgerEntry]) {
    let reasons: Vec<&str> = claims
        .iter()
        .flat_map(events_of)
        .filter_map(|event| match event {
            ClaimEvent::RepairAttempted {
                outcome: RepairOutcome::Rejected { reason },
                ..
            } => Some(reason.as_str()),
            _ => None,
        })
        .filter(|reason| reason.contains("did not make"))
        .collect();
    assert!(reasons.is_empty(), "{reasons:?}");
}

/// Where a ledger that holds the placing events of `entry` (its extraction,
/// class, steps and birth) up to and including the first that `last` picks
/// stands it.
async fn standing_after(
    entry: &ClaimLedgerEntry,
    last: impl Fn(&ClaimEvent) -> bool,
) -> Result<Option<Position>> {
    let mut ledger = open().await?;
    for event in events_of(entry) {
        let placing = matches!(
            event,
            ClaimEvent::Extracted { .. }
                | ClaimEvent::Classified { .. }
                | ClaimEvent::Stepped { .. }
                | ClaimEvent::BornOfRepair { .. }
        );
        if placing {
            ledger.append(entry.claim_id(), event.clone()).await?;
        }
        if last(event) {
            break;
        }
    }
    Ok(
        match ledger.position(&StandardPolicy::new(), entry.claim_id()) {
            Some(Walked::At(at)) => Some(at),
            _ => None,
        },
    )
}

/// Asserts that a claim born of a repair, sorted Sourced, stands at the node
/// after a rewrite of its class with `attempts_left` rewrites, before any step
/// of its own.
async fn assert_begins_after_the_rewrite(
    claim: &ClaimLedgerEntry,
    attempts_left: u8,
) -> Result<()> {
    let policy = StandardPolicy::new();
    let at = standing_after(claim, |event| {
        matches!(event, ClaimEvent::BornOfRepair { .. })
    })
    .await?;
    let expected = Position {
        class: Class::Sourced,
        visited: lattice_core::domain::learning::acceptance::Classes::of(Class::Sourced),
        reasked: false,
        node: policy.graph(Class::Sourced).after_rewrite(),
        rewrites_left: attempts_left,
    };
    assert_eq!(at, Some(expected), "{:?}", events_of(claim));
    Ok(())
}

#[tokio::test]
async fn a_correction_that_reads_as_two_statements_stands_and_the_second_is_born_of_the_repair(
) -> Result<()> {
    // §2.6 of the acceptance policy design: the correction keeps the
    // neighbour's words, says the figure's part anew, and writes a statement
    // beside it.
    let model = Script::new(&[
        (SENTENCE, &[BUFFERS, MISSING]),
        (READS, &[READS_ONE, MISSING]),
        (NAMES, &[NAMES]),
    ])
    .failing(&[BUFFERS])
    .repairs(vec![vec![(SENTENCE, format!("{READS} {NAMES}"))]]);

    let run = prepared(&model, &section(SENTENCE)).await?;

    let corrected = section(&format!("{READS} {NAMES}"));
    assert_eq!(run.published().as_deref(), Some(corrected.as_str()));
    if let Some(saved) = &run.saved {
        assert_eq!(saved, &corrected);
    }
    assert_eq!(model.requests().len(), 1, "the correction alone");
    none_charged_for_writing(&run.claims);
    // The target continues as what its rewrite wrote.
    let targets = first_stated(&run.claims, BUFFERS);
    let [target] = targets.as_slice() else {
        panic!("one target: {}", targets.len());
    };
    assert!(
        events_of(target).iter().any(|event| matches!(
            event,
            ClaimEvent::RepairAttempted {
                outcome: RepairOutcome::Rewritten,
                goal: Some(Goal::WhatThePassageStates),
                ..
            }
        )),
        "{:?}",
        events_of(target)
    );
    // The other statement is born of the repair with the successor's rewrites,
    // and is read from the node after a rewrite, not from its class's entry.
    let (parent, attempts_left) = born_stating(&run.claims, NAMES);
    assert_eq!(&parent, target.claim_id());
    let heir = standing_after(target, |event| {
        matches!(
            event,
            ClaimEvent::Stepped {
                step: Step::Rewrite(_),
                ..
            }
        )
    })
    .await?;
    assert_eq!(heir.map(|at| at.rewrites_left), Some(attempts_left));
    let names = first_stated(&run.claims, NAMES);
    let [names] = names.as_slice() else {
        panic!("one claim first stated as {NAMES:?}: {}", names.len());
    };
    assert_begins_after_the_rewrite(names, attempts_left).await?;
    assert_eq!(
        names.steps().first().map(|(taken, _)| *taken),
        Some(Taken {
            step: Step::Read(Reading::Parts),
            outcome: Outcome::Stands,
        })
    );
    Ok(())
}

#[tokio::test]
async fn a_rewrite_with_one_successor_for_each_statement_it_touched_stands_and_each_is_read_from_after_the_rewrite(
) -> Result<()> {
    // What stays: the correction says the figure's part anew and keeps the
    // neighbour's words, so each statement it touched has one successor.
    let model = Script::new(&[
        (SENTENCE, &[BUFFERS, MISSING]),
        (READS, &[READS_ONE, MISSING]),
    ])
    .failing(&[BUFFERS])
    .repairs(vec![vec![(SENTENCE, READS.to_owned())]]);

    let run = prepared(&model, &section(SENTENCE)).await?;

    assert_eq!(run.published().as_deref(), Some(section(READS).as_str()));
    assert_eq!(model.requests().len(), 1);
    // The target continues as what its rewrite wrote, read from the node
    // after its rewrite.
    let targets = first_stated(&run.claims, BUFFERS);
    let [target] = targets.as_slice() else {
        panic!("one target: {}", targets.len());
    };
    let steps = target.steps();
    let rewritten = Taken {
        step: Step::Rewrite(Goal::WhatThePassageStates),
        outcome: Outcome::Rewritten,
    };
    let Some(at) = steps.iter().position(|(taken, _)| *taken == rewritten) else {
        panic!("the rewrite is not recorded: {steps:?}");
    };
    assert_eq!(
        steps[at].1,
        Then::Node {
            name: "s.final".into(),
            step: Step::Read(Reading::Parts),
        }
    );
    assert_eq!(
        steps.get(at + 1).map(|(taken, _)| *taken),
        Some(Taken {
            step: Step::Read(Reading::Parts),
            outcome: Outcome::Stands,
        })
    );
    // The neighbour that rode on the rewrite continues as its successor, of
    // its own class, read from its own graph's node after a rewrite.
    let mates = first_stated(&run.claims, MISSING);
    let (Some(mate), Some(next)) = (
        mates
            .iter()
            .find(|claim| matches!(claim.state(), ClaimState::Edited { .. })),
        mates
            .iter()
            .find(|claim| !matches!(claim.state(), ClaimState::Edited { .. })),
    ) else {
        panic!("the neighbour and its successor: {}", mates.len());
    };
    assert!(
        matches!(
            mate.state(),
            ClaimState::Edited {
                successor: Some(successor),
                by: Actor::Machine,
                ..
            } if successor == next.claim_id()
        ),
        "{:?}",
        mate.state()
    );
    assert_eq!(
        next.steps().first(),
        Some(&(
            Taken {
                step: Step::Read(Reading::Consistency),
                outcome: Outcome::Stands,
            },
            Then::Node {
                name: "r.reveto".into(),
                step: Step::Read(Reading::Veto),
            }
        ))
    );
    Ok(())
}

#[tokio::test]
async fn both_hunks_of_a_request_stand_and_the_extra_statement_is_born_of_the_repair_whose_sentence_shares_most_words_with_it(
) -> Result<()> {
    // Two failed sentences of one field, with a sentence that stands between
    // them, are corrected in one request; the second correction writes a
    // statement beside its own that is nearer in words to the first sentence.
    let model = Script::new(&[
        (CACHES, &[CACHES]),
        (BUFFERS, &[BUFFERS]),
        (CACHED, &[CACHED]),
        (READS_ONE, &[READS_ONE]),
        (KEEPS, &[KEEPS]),
    ])
    .failing(&[CACHES, BUFFERS])
    .repairs(vec![vec![
        (CACHES, CACHED.to_owned()),
        (BUFFERS, format!("{READS_ONE} {KEEPS}")),
    ]]);

    let run = prepared(&model, &section(&format!("{CACHES} {BETWEEN} {BUFFERS}"))).await?;

    let corrected = section(&format!("{CACHED} {BETWEEN} {READS_ONE} {KEEPS}"));
    assert_eq!(run.published().as_deref(), Some(corrected.as_str()));
    if let Some(saved) = &run.saved {
        assert_eq!(saved, &corrected);
    }
    assert_eq!(model.requests().len(), 1, "the corrections alone");
    none_charged_for_writing(&run.claims);
    for target in [CACHES, BUFFERS] {
        let found = first_stated(&run.claims, target);
        let [entry] = found.as_slice() else {
            panic!("one claim first stated as {target:?}: {}", found.len());
        };
        assert!(
            events_of(entry).iter().any(|event| matches!(
                event,
                ClaimEvent::RepairAttempted {
                    outcome: RepairOutcome::Rewritten,
                    ..
                }
            )),
            "{target}: {:?}",
            events_of(entry)
        );
    }
    let caches = first_stated(&run.claims, CACHES);
    let (parent, _) = born_stating(&run.claims, KEEPS);
    assert_eq!(Some(&parent), caches.first().map(|entry| entry.claim_id()));
    Ok(())
}

#[tokio::test]
async fn a_narrowing_stands_and_the_statement_it_wrote_is_born_of_the_narrowed_claims_repair(
) -> Result<()> {
    // The figure's correction is as unsupported as the figure, so the
    // statement comes to its cut; the neighbour stands on its sentence, so the
    // cut asks the model to take the statement out (step 7b-1), and the
    // narrowed sentence comes with a statement beside the neighbour's.
    let model = Script::new(&[
        (SENTENCE, &[BUFFERS, MISSING]),
        (GOOD, &[GOOD]),
        (STILL_FAILING, &[BUFFERS_200, MISSING]),
        (MISSING_ALONE, &[MISSING]),
        (NAMES, &[NAMES]),
    ])
    .failing(&[BUFFERS, BUFFERS_200])
    .repairs(vec![
        vec![(SENTENCE, STILL_FAILING.to_owned())],
        vec![(STILL_FAILING, format!("{MISSING_ALONE} {NAMES}"))],
    ]);

    let run = prepared(&model, &section(&format!("{SENTENCE} {GOOD}"))).await?;

    let corrected = section(&format!("{MISSING_ALONE} {NAMES} {GOOD}"));
    assert_eq!(run.published().as_deref(), Some(corrected.as_str()));
    assert_eq!(
        model.requests().len(),
        2,
        "the correction and the narrowing"
    );
    none_charged_for_writing(&run.claims);
    let targets = first_stated(&run.claims, BUFFERS);
    let [target] = targets.as_slice() else {
        panic!("one target: {}", targets.len());
    };
    let events = events_of(target);
    assert!(
        events.iter().any(|event| matches!(
            event,
            ClaimEvent::RepairAttempted {
                outcome: RepairOutcome::Removed,
                goal: Some(Goal::KeepTheOthers),
                ..
            }
        )),
        "{events:?}"
    );
    assert_eq!(
        target.decided(),
        Some(&Then::End(End::Removed {
            section_goes: false
        }))
    );
    let (parent, _) = born_stating(&run.claims, NAMES);
    assert_eq!(&parent, target.claim_id());
    Ok(())
}

#[tokio::test]
async fn a_statement_a_split_rewrite_wrote_beside_its_targets_successor_is_born_of_the_repair_and_sorted(
) -> Result<()> {
    // The correction puts the neighbour's statement first, at more length
    // than the whole sentence, so nothing it wrote stands where the figure's
    // part stood: the figure's part said anew continues the claim as the
    // repair's successor, and the statement after it is born of the repair.
    let model = Script::new(&[
        (SENTENCE, &[BUFFERS, MISSING]),
        (MISSING_ALONE, &[MISSING]),
        (BUFFERS_ONE, &[BUFFERS_ONE]),
        (NAMES, &[NAMES]),
    ])
    .failing(&[BUFFERS])
    .repairs(vec![vec![(
        SENTENCE,
        format!("{MISSING_ALONE} {BUFFERS_ONE} {NAMES}"),
    )]]);

    let run = prepared(&model, &section(SENTENCE)).await?;

    let corrected = section(&format!("{MISSING_ALONE} {BUFFERS_ONE} {NAMES}"));
    assert_eq!(run.published().as_deref(), Some(corrected.as_str()));
    if let Some(saved) = &run.saved {
        assert_eq!(saved, &corrected);
    }
    assert_eq!(model.requests().len(), 1, "the correction alone");
    none_charged_for_writing(&run.claims);
    let targets = first_stated(&run.claims, BUFFERS);
    let [target] = targets.as_slice() else {
        panic!("one target: {}", targets.len());
    };
    let (parent, _) = born_stating(&run.claims, NAMES);
    assert_eq!(&parent, target.claim_id());
    let sorted = model.sorted();
    assert!(
        sorted.iter().any(|statement| statement == NAMES),
        "{sorted:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_born_statement_whose_reading_does_not_stand_goes_to_its_cut_with_no_rewrite_asked_for_it(
) -> Result<()> {
    // The correction adds a statement of its own that the passages
    // contradict, in a sentence of its own.
    let model = Script::new(&[
        (SENTENCE, &[BUFFERS, MISSING]),
        (READS, &[READS_ONE, MISSING]),
        (NAMES, &[NAMES]),
        (GOOD, &[GOOD]),
    ])
    .failing(&[BUFFERS, NAMES])
    .repairs(vec![vec![(SENTENCE, format!("{READS} {NAMES}"))]]);

    let run = prepared(&model, &section(&format!("{SENTENCE} {GOOD}"))).await?;

    assert_eq!(
        model.requests().len(),
        1,
        "no rewrite for the born statement"
    );
    let (_, attempts_left) = born_stating(&run.claims, NAMES);
    let names = first_stated(&run.claims, NAMES);
    let [names] = names.as_slice() else {
        panic!("one claim first stated as {NAMES:?}: {}", names.len());
    };
    assert_begins_after_the_rewrite(names, attempts_left).await?;
    assert_eq!(
        names.steps(),
        vec![
            (
                Taken {
                    step: Step::Read(Reading::Parts),
                    outcome: Outcome::Refuted,
                },
                Then::Node {
                    name: "s.cut".into(),
                    step: Step::Cut,
                }
            ),
            (
                Taken {
                    step: Step::Cut,
                    outcome: Outcome::Cut,
                },
                Then::End(End::Removed {
                    section_goes: false
                })
            ),
        ]
    );
    let published = run.published().unwrap_or_default();
    assert!(!published.contains(NAMES), "{published:?}");
    Ok(())
}

#[tokio::test]
async fn a_statement_new_to_the_ledger_outside_any_repaired_region_begins_at_its_class_entry_born_of_nothing(
) -> Result<()> {
    // The model finds the second sentence's statement only on the pass after
    // the correction, which reports another identity, has no basis, and
    // extracts the section whole: it is new, but the correction did not write it.
    let model = Script::new(&[
        (SENTENCE, &[BUFFERS, MISSING]),
        (READS, &[READS_ONE, MISSING]),
        (GOOD, &[GOOD]),
    ])
    .late(&[GOOD])
    .renamed_after_repair()
    .failing(&[BUFFERS])
    .repairs(vec![vec![(SENTENCE, READS.to_owned())]]);

    let run = prepared(&model, &section(&format!("{SENTENCE} {GOOD}"))).await?;

    let corrected = section(&format!("{READS} {GOOD}"));
    assert_eq!(run.published().as_deref(), Some(corrected.as_str()));
    let found = born(&run.claims);
    assert!(found.is_empty(), "born of a repair: {}", found.len());
    let fresh = first_stated(&run.claims, GOOD);
    let [fresh] = fresh.as_slice() else {
        panic!("one claim first stated as {GOOD:?}: {}", fresh.len());
    };
    let policy = StandardPolicy::new();
    let at = standing_after(fresh, |event| {
        matches!(event, ClaimEvent::Classified { .. })
    })
    .await?;
    assert_eq!(
        at,
        Some(Position {
            class: Class::Sourced,
            visited: lattice_core::domain::learning::acceptance::Classes::of(Class::Sourced),
            reasked: false,
            node: policy.graph(Class::Sourced).entry(),
            rewrites_left: rewrites_given(&policy, Class::Sourced),
        })
    );
    Ok(())
}
