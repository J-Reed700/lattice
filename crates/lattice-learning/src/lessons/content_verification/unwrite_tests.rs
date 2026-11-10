//! No repair writes a new statement (acceptance design §2.6): a hunk a pending
//! repair changed whose new text holds a statement none of those standing on
//! its old sentences continues is put back before the pass records anything,
//! and only the targets in it are charged.
use super::acceptance_switch_tests::{events_of, lesson_of};
use super::claim_ledger_tests::{prepare, section};
use super::class_checks_tests::Classes;
use super::*;
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::acceptance::{End, Goal, Step, Taken, Then};
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

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }

    fn sorted(&self) -> Vec<String> {
        self.sorted.lock().unwrap().clone()
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Extract lesson claims.") {
            let data = context(prompt);
            let units: Vec<Value> = data["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|unit| self.claims_of(unit))
                .collect();
            return Ok(inventory::fixture_response(units).to_string());
        }
        if prompt.starts_with("You sort statements extracted from a lesson") {
            let statements: serde_json::Map<String, Value> =
                serde_json::from_str(prompt.rsplit("\n\n").next().unwrap())?;
            self.sorted.lock().unwrap().extend(
                statements
                    .values()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
        }
        if prompt.starts_with("Repair verified lesson defects.") {
            return self.repair(context(prompt));
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
        for passage in unit["passages"].as_array().into_iter().flatten() {
            let text = passage["text"].as_str().unwrap_or_default();
            for (sentence, statements) in &self.sentences {
                if text.contains(*sentence) {
                    claims.extend(
                        statements.iter().map(
                            |statement| json!({"passageId":passage["id"],"statement":statement}),
                        ),
                    );
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
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let raw = lesson_of(body).to_string();
    let ended = prepare(
        &Database::from(pool.clone()),
        &repo,
        &job,
        &lesson_id,
        model,
        Some(raw),
        &[source()],
    )
    .await;
    let claims = ClaimReviewRepository::new(pool.clone())
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

/// Why a correction that wrote `statement`, which its sentence did not make,
/// was not applied.
fn wrote(statement: &str) -> String {
    format!("The correction wrote a statement its sentence did not make (\"{statement}\"), so it was put back; correct only what was asked and add nothing.")
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

/// Asserts that no claim of `claims` was ever extracted as `statement`.
fn never_extracted(claims: &[ClaimLedgerEntry], statement: &str) {
    let taken: Vec<&ClaimId> = claims
        .iter()
        .filter(|claim| {
            events_of(claim).iter().any(|event| {
                matches!(event, ClaimEvent::Extracted { statement: said, .. } if said == statement)
            })
        })
        .map(ClaimLedgerEntry::claim_id)
        .collect();
    assert!(
        taken.is_empty(),
        "the ledger took {statement:?} in as {taken:?}"
    );
}

/// Asserts that the one claim first extracted as `target` waits at the retry
/// of its rewrite, its correction not applied for writing `written`.
fn at_its_retry_for_writing(claims: &[ClaimLedgerEntry], target: &str, written: &str) {
    let found = first_stated(claims, target);
    let [entry] = found.as_slice() else {
        panic!("one claim first stated as {target:?}: {}", found.len());
    };
    let events = events_of(entry);
    assert_eq!(
        entry.decided(),
        Some(&Then::Node {
            name: "s.rewrite.again".into(),
            step: Step::Rewrite(Goal::WhatThePassageStates),
        }),
        "{events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            ClaimEvent::RepairAttempted {
                outcome: RepairOutcome::Rejected { reason },
                goal: Some(Goal::WhatThePassageStates),
                ..
            } if *reason == wrote(written)
        )),
        "{events:?}"
    );
}

#[tokio::test]
async fn a_rewrite_that_wrote_a_statement_its_sentence_did_not_make_is_put_back_and_its_target_is_asked_again_told_why(
) -> Result<()> {
    // §2.6 of the acceptance policy design: the correction keeps the
    // neighbour's words, says the figure's part anew, and writes a statement
    // the sentence did not make.
    let model = Script::new(&[
        (SENTENCE, &[BUFFERS, MISSING]),
        (READS, &[READS_ONE, MISSING]),
        (NAMES, &[NAMES]),
    ])
    .failing(&[BUFFERS])
    .repairs(vec![vec![(SENTENCE, format!("{READS} {NAMES}"))]]);

    let run = prepared(&model, &section(SENTENCE)).await?;

    never_extracted(&run.claims, NAMES);
    at_its_retry_for_writing(&run.claims, BUFFERS, NAMES);
    // The neighbour on the sentence keeps its claim, with no successor.
    let mates = first_stated(&run.claims, MISSING);
    let [mate] = mates.as_slice() else {
        panic!("the neighbour's claim and no other: {}", mates.len());
    };
    assert!(
        events_of(mate)
            .iter()
            .all(|event| !matches!(event, ClaimEvent::Superseded { .. })),
        "{:?}",
        events_of(mate)
    );
    assert!(
        matches!(mate.state(), ClaimState::Consistent { .. }),
        "{:?}",
        mate.state()
    );
    // The draft holds the sentence as it was, and the retry is asked of it,
    // told why.
    assert_eq!(run.saved.as_deref(), Some(section(SENTENCE).as_str()));
    let requests = model.requests();
    assert_eq!(requests.len(), 2, "the correction, then its retry");
    assert_eq!(
        requests[1]["candidate"]["blocks"][0]["body"],
        section(SENTENCE)
    );
    assert_eq!(requests[1]["failedClaims"][0]["notApplied"], wrote(NAMES));
    assert!(run.ended.is_err(), "the run ends at the retry's request");
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
async fn only_the_hunk_that_wrote_a_statement_is_put_back_and_only_its_target_is_charged(
) -> Result<()> {
    // Two failed sentences of one field, with a sentence that stands between
    // them, are corrected in one request: one correction is clean, the other
    // writes a statement its sentence did not make.
    let model = Script::new(&[
        (CACHES, &[CACHES]),
        (BUFFERS, &[BUFFERS]),
        (CACHED, &[CACHED]),
        (READS_ONE, &[READS_ONE]),
        (NAMES, &[NAMES]),
    ])
    .failing(&[CACHES, BUFFERS])
    .repairs(vec![vec![
        (CACHES, CACHED.to_owned()),
        (BUFFERS, format!("{READS_ONE} {NAMES}")),
    ]]);

    let run = prepared(&model, &section(&format!("{CACHES} {BETWEEN} {BUFFERS}"))).await?;

    never_extracted(&run.claims, NAMES);
    at_its_retry_for_writing(&run.claims, BUFFERS, NAMES);
    // The clean correction stands, recorded as made, and nothing is charged
    // to its target.
    let others = first_stated(&run.claims, CACHES);
    let [other] = others.as_slice() else {
        panic!("one claim first stated as {CACHES:?}: {}", others.len());
    };
    let steps = other.steps();
    assert!(
        steps.iter().any(|(taken, _)| *taken
            == Taken {
                step: Step::Rewrite(Goal::WhatThePassageStates),
                outcome: Outcome::Rewritten,
            }),
        "{steps:?}"
    );
    assert!(
        events_of(other).iter().all(|event| !matches!(
            event,
            ClaimEvent::RepairAttempted {
                outcome: RepairOutcome::Rejected { .. },
                ..
            }
        )),
        "{:?}",
        events_of(other)
    );
    let kept = section(&format!("{CACHED} {BETWEEN} {BUFFERS}"));
    assert_eq!(run.saved.as_deref(), Some(kept.as_str()));
    let requests = model.requests();
    assert_eq!(requests.len(), 2, "the corrections, then one retry");
    assert_eq!(requests[1]["candidate"]["blocks"][0]["body"], kept);
    let retried = requests[1]["failedClaims"].as_array().unwrap();
    assert_eq!(retried.len(), 1, "{retried:?}");
    assert_eq!(retried[0]["statement"], BUFFERS);
    assert_eq!(retried[0]["notApplied"], wrote(NAMES));
    Ok(())
}

#[tokio::test]
async fn a_narrowing_that_wrote_a_statement_is_put_back_and_spent_and_the_next_round_keeps_its_sentence_and_publishes_it_not_verified(
) -> Result<()> {
    // The figure's correction is as unsupported as the figure, so the
    // statement comes to its cut; the neighbour stands on its sentence, so the
    // cut asks the model to take the statement out (step 7b-1), and the
    // narrowed sentence comes with a statement the sentence did not make.
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

    never_extracted(&run.claims, NAMES);
    let targets = first_stated(&run.claims, BUFFERS);
    let [target] = targets.as_slice() else {
        panic!("one target: {}", targets.len());
    };
    let events = events_of(target);
    let Some(spent) = events.iter().position(|event| {
        matches!(
            event,
            ClaimEvent::RepairAttempted {
                outcome: RepairOutcome::Rejected { .. },
                goal: Some(Goal::KeepTheOthers),
                ..
            }
        )
    }) else {
        panic!("the narrowing was never spent: {events:?}");
    };
    assert!(
        matches!(
            events[spent],
            ClaimEvent::RepairAttempted {
                outcome: RepairOutcome::Rejected { reason },
                ..
            } if *reason == wrote(NAMES)
        ),
        "{:?}",
        events[spent]
    );
    // The narrowing takes no step: the next the statement takes is its cut.
    let after: Vec<(Step, Outcome)> = events[spent + 1..]
        .iter()
        .filter_map(|event| match event {
            ClaimEvent::Stepped { step, outcome, .. } => Some((*step, *outcome)),
            _ => None,
        })
        .collect();
    assert_eq!(after, [(Step::Cut, Outcome::CannotCut)], "{events:?}");
    assert_eq!(target.decided(), Some(&Then::End(End::Unverified)));
    // The sentence stays, the neighbour's words with it.
    assert_eq!(
        run.published().as_deref(),
        Some(section(&format!("{STILL_FAILING} {GOOD}")).as_str())
    );
    assert_eq!(
        model.requests().len(),
        2,
        "the correction and the narrowing; the code refuses the cut"
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_a_split_rewrite_wrote_beside_its_targets_successor_is_no_child_it_is_taken_back(
) -> Result<()> {
    // The correction puts the neighbour's statement first, at more length
    // than the whole sentence, so nothing it wrote stands where the figure's
    // part stood: the figure's part said anew continues the claim as the
    // repair's successor, and the statement after it would be the repair's
    // child.
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

    let born: Vec<&ClaimId> = run
        .claims
        .iter()
        .filter(|claim| {
            events_of(claim)
                .iter()
                .any(|event| matches!(event, ClaimEvent::BornOfRepair { .. }))
        })
        .map(ClaimLedgerEntry::claim_id)
        .collect();
    assert!(born.is_empty(), "born of a repair: {born:?}");
    never_extracted(&run.claims, NAMES);
    let sorted = model.sorted();
    assert!(
        !sorted.iter().any(|statement| statement == NAMES),
        "{sorted:?}"
    );
    at_its_retry_for_writing(&run.claims, BUFFERS, NAMES);
    assert_eq!(run.saved.as_deref(), Some(section(SENTENCE).as_str()));
    Ok(())
}
