//! What a repair request tells the model (acceptance design §2.4, §2.6): the
//! goal of each correction, why a retry's first correction could not be
//! applied, and only the sentences an earlier correction replaced or removed.
use super::super::repair::goal_words;
use super::acceptance_switch_tests::{
    claim, identified, lesson_of, local_check, open, walk, walk_because, CUTTING, REASONED,
    RESEARCHING, SOURCED,
};
use super::claim_ledger_tests::{pass_report, prepare, section};
use super::class_checks_tests::Classes;
use super::*;
use crate::lesson_drafts;
use lattice_core::domain::learning::acceptance::{Goal, Step};
use lattice_core::domain::learning::claim_review::{
    CheckResult, ClaimEvent, ClaimLocation, RepairOutcome, UnitKind, VerdictKind,
};
use lattice_core::shared::persistence::Database;

/// The draft's input hash in these fixtures, as `prepare` resumes it.
const INPUT: &str = "ledger-fixture";
const REWRITING_SENTENCE: &str = "The reader hands over one row at a time.";
const WAITING_SENTENCE: &str = "A header row names the columns of the file.";
/// A second sentence the fixture sources contradict, beside `BAD`.
const SECOND: &str = "DictReader removes all whitespace from CSV values by default.";
/// What a Sourced statement's rewrite is asked: its node's goal.
const WHAT_THE_PASSAGES_STATE: &str = "Say only what the claim's evidence passages state. Where no passage states a detail, keep the sentence's point and leave that detail out.";
/// Why a rewrite that left the sentence as it was could not be applied.
const LEFT_AS_IT_WAS: &str = "The repair left the sentence as it was.";

/// The steps that leave a Sourced statement at `s.rewrite`.
const AT_REWRITE: [(Step, Outcome); 3] = [
    (Step::Read(Reading::Parts), Outcome::Unproven),
    (Step::Research, Outcome::FoundNothing),
    (Step::Read(Reading::Page), Outcome::Refuted),
];

/// A model that extracts the statements it is given, as `Classes` does, and
/// keeps the context of each repair request, which it answers with an outage:
/// the run ends there.
struct Stopping {
    inner: Classes,
    requests: Mutex<Vec<Value>>,
}

impl Stopping {
    fn new(statements: &[&'static str]) -> Self {
        Self {
            inner: Classes::new(statements),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Repair verified lesson defects.") {
            self.requests
                .lock()
                .unwrap()
                .push(fixture_task(prompt).unwrap());
            return Err(AppError::Network(
                "the run ends at the first repair request".into(),
            ));
        }
        self.inner.respond(prompt).await
    }
}

#[async_trait::async_trait]
impl LLMPort for Stopping {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "stopping-fixture"
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

/// A model that keeps the context of each repair request. Its first correction
/// takes `BAD` out and leaves `SECOND` as it was, so `SECOND` comes back to be
/// asked again; the second request ends the run.
struct Corrections {
    inner: Model,
    requests: Mutex<Vec<Value>>,
}

impl Corrections {
    fn new() -> Self {
        Self {
            inner: Model::new(),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Extract lesson claims.") {
            let data = fixture_task(prompt).unwrap();
            let passage = data["units"][0]["passages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["field"] == "/body")
                .unwrap();
            let text = passage_text(passage);
            let claims: Vec<_> = [BAD, SECOND, GOOD]
                .into_iter()
                .filter(|s| text.contains(s))
                .map(|statement| json!({"sentenceId":sentence_id(passage, statement),"statement":statement}))
                .collect();
            return Ok(inventory::fixture_response(vec![
                json!({"index":0,"claims":claims,"nonFactualReason":""}),
            ])
            .to_string());
        }
        if prompt.starts_with("Repair verified lesson defects.") {
            let mut asked = self.requests.lock().unwrap();
            asked.push(fixture_task(prompt).unwrap());
            if asked.len() > 1 {
                return Err(AppError::Network(
                    "the run ends at the second repair request".into(),
                ));
            }
            let body = fixture_task(prompt).unwrap()["candidate"]["blocks"][0]["body"]
                .as_str()
                .unwrap()
                .replace(&format!("{BAD} "), "");
            return Ok(repair_body(prompt, json!(body)));
        }
        if prompt.starts_with("Source passages:")
            && prompt
                .split("\n\nClaim: ")
                .nth(1)
                .unwrap()
                .starts_with(SECOND)
        {
            return Ok("contradicted\nReason: The fixture reference preserves whitespace rather than removing it.\nSource passage: passage-0".into());
        }
        self.inner.respond(prompt).await
    }
}

#[async_trait::async_trait]
impl LLMPort for Corrections {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "corrections-fixture"
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

/// The repair requests a lesson with two sentences its sources contradict is
/// sent: one for both, and one for the sentence the first correction left.
async fn requests_for_two_contradicted_sentences() -> Result<Vec<Value>> {
    requests_after_the_writer_asked("{}").await
}

/// The same requests, the writer's request being `writer_request`.
async fn requests_after_the_writer_asked(writer_request: &str) -> Result<Vec<Value>> {
    let model = Corrections::new();
    let body = format!("{}{BAD} {SECOND}", format!("{GOOD} ").repeat(12));
    let lesson = json!({"blocks":[{"kind":"explanation","title":"CSV header names","body":body,"rubric":[]}],"questions":[]});

    // The second request ends the run.
    let _ = verify_and_repair(
        &model,
        writer_request,
        &json!({}),
        lesson.to_string(),
        &[source()],
        3000,
    )
    .await;

    let requests = model.requests();
    assert_eq!(
        requests.len(),
        2,
        "a request for both sentences, then one for what the first left"
    );
    Ok(requests)
}

#[tokio::test]
async fn a_sourced_statement_at_a_rewrite_is_sent_with_the_goal_of_its_node() -> Result<()> {
    let requests = requests_for_two_contradicted_sentences().await?;

    let failed = requests[0]["failedClaims"].as_array().unwrap();
    assert!(!failed.is_empty());
    for statement in failed {
        assert_eq!(statement["goal"], WHAT_THE_PASSAGES_STATE, "{statement}");
    }
    Ok(())
}

#[tokio::test]
async fn a_statement_at_the_retry_of_a_rewrite_is_told_why_its_first_correction_was_not_applied(
) -> Result<()> {
    let requests = requests_for_two_contradicted_sentences().await?;

    let retried = &requests[1]["failedClaims"][0];
    assert_eq!(retried["statement"], SECOND);
    assert_eq!(retried["notApplied"], LEFT_AS_IT_WAS, "{retried}");
    Ok(())
}

#[tokio::test]
async fn the_repair_history_does_not_carry_a_statement_that_waits_at_a_node_that_is_no_repair(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let body = section(&format!("{REWRITING_SENTENCE} {WAITING_SENTENCE}"));
    let raw = lesson_of(&body).to_string();
    // One statement stands at `s.rewrite`; the other failed and waits at
    // `s.research`.
    lesson_drafts::run(
        &Database::from(pool.clone()),
        &repo,
        &job,
        &lesson_id,
        async {
            lesson_drafts::resume(INPUT.into()).await?;
            lesson_drafts::save(&raw).await?;
            let mut ledger = open().await?;
            let rewriting = (REWRITING_SENTENCE, REWRITING_SENTENCE);
            let id = claim(&mut ledger, "rewriting", rewriting, &body, SOURCED).await?;
            walk(&mut ledger, &id, Class::Sourced, &AT_REWRITE).await?;
            let waiting = (WAITING_SENTENCE, WAITING_SENTENCE);
            let id = claim(&mut ledger, "waiting", waiting, &body, SOURCED).await?;
            walk(&mut ledger, &id, Class::Sourced, &RESEARCHING).await?;
            Ok(())
        },
    )
    .await?;
    let program_id = repo.job(&job).await?.program_id;
    let model = Stopping::new(&[REWRITING_SENTENCE, WAITING_SENTENCE]);

    // Research reaches nothing, so the second statement stays where it is,
    // and the first is repaired.
    let ended = research::run(
        Database::from(pool.clone()),
        program_id,
        "Research methods".into(),
        Some(research::tests::web(false)),
        prepare(
            &Database::from(pool.clone()),
            &repo,
            &job,
            &lesson_id,
            &model,
            Some(raw),
            &[source()],
        ),
    )
    .await
    .err()
    .map(|error| error.to_string())
    .unwrap_or_default();

    assert!(ended.contains("first repair request"), "{ended}");
    let requests = model.requests();
    let [request] = requests.as_slice() else {
        panic!("one repair request: {requests:?}");
    };
    let history = request["repairHistory"].to_string();
    assert!(!history.contains(WAITING_SENTENCE), "{history}");
    Ok(())
}

const DRAFT_QUESTIONS: &str =
    "Create exactly two distinct multiple-choice items of each kind: practice, quiz, and test.";

#[tokio::test]
async fn a_verification_repair_is_told_the_shape_the_lesson_is_kept_with() -> Result<()> {
    let writer_request = json!({"lessonShape":[DRAFT_QUESTIONS]}).to_string();
    let requests = requests_after_the_writer_asked(&writer_request).await?;

    let lines = requests[0]["requirements"]["lessonShape"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !lines.contains(&json!(DRAFT_QUESTIONS)),
        "the repair was told the draft's questions: {lines:?}"
    );
    let words: Vec<&str> = lines.iter().filter_map(Value::as_str).collect();
    assert!(
        words
            .join("\n")
            .contains("at least one question of each kind"),
        "{words:?}"
    );
    Ok(())
}

/// A finding that no passage supports `statement` of section 0.
fn unsupported(statement: &str) -> Finding {
    Finding {
        unit: 0,
        quote: statement.into(),
        statement: statement.into(),
        verdict: ClaimVerdict::Unsupported,
        reason: "No passage states it.".into(),
        evidence: vec![],
        supporting_quote: None,
        resolution: None,
        trust: None,
        label: None,
        class: None,
        claim_id: None,
        two_pass: Default::default(),
    }
}

#[tokio::test]
async fn the_request_after_a_correction_lists_the_sentence_it_removed_and_why() -> Result<()> {
    let requests = requests_for_two_contradicted_sentences().await?;

    // Nothing was rewritten or cut before the first request.
    assert_eq!(requests[0]["repairHistory"], json!([]));
    // The first correction took `BAD` out; `SECOND` was sent but not rewritten.
    let history = requests[1]["repairHistory"].as_array().unwrap();
    let [removed] = history.as_slice() else {
        panic!("one sentence was removed: {history:?}");
    };
    assert_eq!(removed["unit"], 0);
    assert_eq!(removed["sentence"], BAD);
    let reason = removed["reason"].as_str().unwrap_or_default();
    assert!(!reason.is_empty(), "{removed}");
    Ok(())
}

/// What a reasoning statement is asked once a veto refuted it and its passages said nothing.
const CORRECT_WHAT_WAS_FOUND: &str = "Correct what the reason found wrong in the sentence, and keep the rest of the sentence as it is.";

#[tokio::test]
async fn the_rewrite_of_a_vetoed_statement_its_passages_do_not_address_carries_the_veto_reason(
) -> Result<()> {
    const VETO: &str = "The sentence says the reader keeps every space, which the sources deny.";
    const SILENT: &str = "The passages do not address this.";
    let policy = StandardPolicy::new();
    let sentence = "The reader keeps every space of a field, so headers match by position.";
    let mut ledger = open().await?;
    let id = claim(
        &mut ledger,
        "vetoed",
        (sentence, sentence),
        sentence,
        REASONED,
    )
    .await?;
    walk_because(
        &mut ledger,
        &id,
        Class::Reasoning,
        &[
            (
                Step::Read(Reading::Consistency),
                Outcome::Stands,
                "It follows.",
            ),
            (Step::Read(Reading::Veto), Outcome::Refuted, VETO),
            (Step::Read(Reading::Ground), Outcome::Refuted, SILENT),
        ],
    )
    .await?;
    let lesson = lesson_of(sentence);
    identified(
        &mut ledger,
        &Model::new(),
        &lesson,
        &[(sentence, sentence)],
        &[],
    )
    .await?;
    let mut finding = unsupported(sentence);
    finding.reason = SILENT.into();
    let report = pass_report(vec![finding]);

    let context = repair_context(
        "{}",
        &lesson,
        &report,
        (&ledger, &policy),
        &[id],
        &ReferenceCollection::lexical(&[])?,
    )?;

    let failed = context["failedClaims"].as_array().unwrap();
    let [sent] = failed.as_slice() else {
        panic!("one failed claim: {failed:?}");
    };
    assert_eq!(sent["goal"], CORRECT_WHAT_WAS_FOUND);
    assert_eq!(sent["reason"], VETO);
    assert_eq!(sent["groundReason"], SILENT);
    Ok(())
}

#[tokio::test]
async fn a_claim_carries_the_goal_of_its_rewrite_and_a_retry_why_the_latest_correction_was_not_applied(
) -> Result<()> {
    const THIRD_SENTENCE: &str = "Each field keeps the spaces it was written with.";
    let policy = StandardPolicy::new();
    let body = format!("{REWRITING_SENTENCE} {WAITING_SENTENCE} {THIRD_SENTENCE}");
    let mut ledger = open().await?;
    let not_applied = (
        Step::Rewrite(Goal::WhatThePassageStates),
        Outcome::NotApplied,
    );
    let to_retry: Vec<(Step, Outcome)> = AT_REWRITE.into_iter().chain([not_applied]).collect();
    let to_cut: Vec<(Step, Outcome)> = to_retry.iter().copied().chain([not_applied]).collect();
    let mut ids = Vec::new();
    for (name, sentence, steps) in [
        ("first", REWRITING_SENTENCE, AT_REWRITE.to_vec()),
        ("retry", WAITING_SENTENCE, to_retry),
        ("last", THIRD_SENTENCE, to_cut),
    ] {
        let id = claim(&mut ledger, name, (sentence, sentence), &body, SOURCED).await?;
        walk(&mut ledger, &id, Class::Sourced, &steps).await?;
        ids.push(id);
    }
    // The retry's two corrections that could not be applied, the latest last.
    for reason in [
        "The edit named a sentence twice.",
        "The repair left the sentence as it was.",
    ] {
        let attempt = ClaimEvent::RepairAttempted {
            attempt: 1,
            escalated: false,
            before: WAITING_SENTENCE.into(),
            after: None,
            outcome: RepairOutcome::Rejected {
                reason: reason.into(),
            },
            goal: Some(Goal::WhatThePassageStates),
        };
        assert!(ledger.append(&ids[1], attempt).await?);
    }
    let lesson = lesson_of(&body);
    identified(
        &mut ledger,
        &Model::new(),
        &lesson,
        &[
            (REWRITING_SENTENCE, REWRITING_SENTENCE),
            (WAITING_SENTENCE, WAITING_SENTENCE),
            (THIRD_SENTENCE, THIRD_SENTENCE),
        ],
        &[],
    )
    .await?;
    let report = pass_report(
        [REWRITING_SENTENCE, WAITING_SENTENCE, THIRD_SENTENCE]
            .map(unsupported)
            .to_vec(),
    );

    let context = repair_context(
        "{}",
        &lesson,
        &report,
        (&ledger, &policy),
        &ids,
        &ReferenceCollection::lexical(&[])?,
    )?;

    let failed = context["failedClaims"].as_array().unwrap();
    let sent = |name: &str| failed.iter().find(|item| item["claimId"] == name).unwrap();
    // A statement at a rewrite is asked toward the goal of its node.
    assert_eq!(sent("first")["goal"], WHAT_THE_PASSAGES_STATE);
    assert!(sent("first").get("notApplied").is_none());
    // At the retry it is told why the latest correction could not be applied.
    assert_eq!(sent("retry")["goal"], WHAT_THE_PASSAGES_STATE);
    assert_eq!(sent("retry")["notApplied"], LEFT_AS_IT_WAS);
    // The cut is made in code: no goal, and no reason, are sent for it.
    assert_eq!(sent("last")["lastAttempt"], true);
    assert!(sent("last").get("goal").is_none());
    assert!(sent("last").get("notApplied").is_none());
    Ok(())
}

#[tokio::test]
async fn a_cut_beside_a_live_statement_on_its_sentence_is_asked_of_the_model_not_marked_for_the_code(
) -> Result<()> {
    // §1 #8 (a) of the acceptance policy design
    let policy = StandardPolicy::new();
    let sentence = REWRITING_SENTENCE;
    let mut ledger = open().await?;
    let cut = claim(&mut ledger, "cut", (sentence, sentence), sentence, SOURCED).await?;
    walk(&mut ledger, &cut, Class::Sourced, &CUTTING).await?;
    // The statement beside it is what the sentence also states.
    claim(
        &mut ledger,
        "beside",
        (sentence, WAITING_SENTENCE),
        sentence,
        SOURCED,
    )
    .await?;
    let lesson = lesson_of(sentence);
    identified(
        &mut ledger,
        &Model::new(),
        &lesson,
        &[(sentence, sentence), (sentence, WAITING_SENTENCE)],
        &[],
    )
    .await?;
    let report = pass_report(vec![unsupported(sentence), unsupported(WAITING_SENTENCE)]);

    let context = repair_context(
        "{}",
        &lesson,
        &report,
        (&ledger, &policy),
        std::slice::from_ref(&cut),
        &ReferenceCollection::lexical(&[])?,
    )?;

    let failed = context["failedClaims"].as_array().unwrap();
    assert_eq!(failed.len(), 1);
    // No last attempt for the code to take; the narrowing goal, and what the
    // sentence also states, which the rewrite keeps.
    assert!(failed[0].get("lastAttempt").is_none(), "{}", failed[0]);
    assert_eq!(failed[0]["goal"], goal_words(Goal::KeepTheOthers));
    assert_eq!(failed[0]["alsoStates"], json!([WAITING_SENTENCE]));
    Ok(())
}

#[tokio::test]
async fn the_history_is_the_statements_that_failed_a_reading_and_were_rewritten_or_cut_each_with_its_reason(
) -> Result<()> {
    const CUT: &str = "Each field keeps the spaces it was written with.";
    const NEVER_REPAIRED: &str = "The sample file ends with a blank line.";
    const PASSED: &str = "A row of the file holds one value for each column.";
    const EXAMPLE: &str = "print(1)";
    let body = format!(
        "{REWRITING_SENTENCE} {WAITING_SENTENCE} {CUT} {NEVER_REPAIRED} {PASSED} {EXAMPLE}"
    );
    let mut ledger = open().await?;
    let failed = CheckResult::Failed {
        kind: VerdictKind::Unsupported,
    };
    let repair = |before: &str, outcome: RepairOutcome| ClaimEvent::RepairAttempted {
        attempt: 1,
        escalated: false,
        before: before.into(),
        after: None,
        outcome,
        goal: Some(Goal::WhatThePassageStates),
    };
    // Two statements whose check failed them: one rewritten, one cut.
    for (name, sentence, reason, outcome) in [
        (
            "rewritten",
            REWRITING_SENTENCE,
            "No passage states it.",
            RepairOutcome::Rewritten,
        ),
        (
            "cut",
            CUT,
            "The passages say otherwise.",
            RepairOutcome::Removed,
        ),
    ] {
        let id = claim(&mut ledger, name, (sentence, sentence), &body, SOURCED).await?;
        walk(&mut ledger, &id, Class::Sourced, &AT_REWRITE).await?;
        let check = local_check(failed.clone(), reason, vec![]);
        assert!(ledger.append(&id, ClaimEvent::Checked(check)).await?);
        assert!(ledger.append(&id, repair(sentence, outcome)).await?);
    }
    // One that failed its check and was never repaired.
    let waiting = claim(
        &mut ledger,
        "waiting",
        (NEVER_REPAIRED, NEVER_REPAIRED),
        &body,
        SOURCED,
    )
    .await?;
    walk(&mut ledger, &waiting, Class::Sourced, &RESEARCHING).await?;
    let check = local_check(failed.clone(), "No passage states it.", vec![]);
    assert!(ledger.append(&waiting, ClaimEvent::Checked(check)).await?);
    // One that passed its check and went with a sentence another's repair
    // changed.
    let passed = claim(&mut ledger, "passed", (PASSED, PASSED), &body, SOURCED).await?;
    let stated = CheckResult::Supported {
        source_version_id: "source".into(),
        supporting_quote: PASSED.into(),
    };
    let check = local_check(stated, "A passage states it.", vec![]);
    assert!(ledger.append(&passed, ClaimEvent::Checked(check)).await?);
    let rewritten = repair(PASSED, RepairOutcome::Rewritten);
    assert!(ledger.append(&passed, rewritten).await?);
    // A failed example, which keeps its own path.
    let example = ClaimId("example".into());
    let extracted = ClaimEvent::Extracted {
        location: ClaimLocation::locate(0, UnitKind::Block, "body", EXAMPLE, &body),
        statement: "The code example example-0 runs to completion with the output it shows.".into(),
    };
    assert!(ledger.append(&example, extracted).await?);
    let run = ClaimEvent::CodeExample {
        observation: "example-0".into(),
    };
    assert!(ledger.append(&example, run).await?);
    let check = local_check(failed, "It did not run.", vec![]);
    assert!(ledger.append(&example, ClaimEvent::Checked(check)).await?);
    let removed = repair(EXAMPLE, RepairOutcome::Removed);
    assert!(ledger.append(&example, removed).await?);

    assert_eq!(
        ledger.repair_history(),
        vec![
            json!({"unit":0,"sentence":REWRITING_SENTENCE,"reason":"No passage states it."}),
            json!({"unit":0,"sentence":CUT,"reason":"The passages say otherwise."}),
        ]
    );
    Ok(())
}
