//! The §14.6 failure-mode matrix: batching from the measured window, a claim
//! no window holds, outages and rate limits (deferred, resumed from the
//! claims still unaudited), repeated failures, the audit switched off or the
//! provider changed mid-run, and the stop messages for no Verifier, no key
//! and missing consent. No Verifier set and no API key saved are refused
//! before any audit runs (`resolve_verifier`); here they are the stop the
//! lesson shows.

use super::*;
use crate::features::learning::lessons::verifier_audit::batches::{Context, Item, Sizer};
use crate::features::learning::lessons::verifier_audit::calls::WindowSizer;

/// The smallest window, in steps of 64 tokens, whose request holds the
/// first `claims` of the harness's claims, measured as the audit measures.
fn window_holding(harness: &Harness, claims: usize) -> usize {
    let ledger = harness.store.ledger.lock().unwrap().clone();
    let due = ledger::due(&ledger, &harness.passages, &verifier_name());
    let items = |count: usize| -> Vec<Item> {
        (0..count)
            .map(|claim| Item {
                claim,
                passages: (0..due[claim].passages.len()).collect(),
                context: Context::Unit,
                part: None,
            })
            .collect()
    };
    let fits = |window: usize, count: usize| {
        let probe = ScriptedVerifier::answering(window, supports);
        WindowSizer::new(&probe, &due, &harness.units, false).fits(&items(count))
    };
    let window = (1..)
        .map(|step| step * 64)
        .find(|window| fits(*window, claims))
        .unwrap();
    if claims < due.len() {
        assert!(
            !fits(window, claims + 1),
            "the window holds exactly {claims}"
        );
    }
    window
}

fn three_claims() -> Harness {
    Harness::new(vec![
        verified("a", "Water boils at 100 °C at sea level."),
        verified("b", "Salt raises the boiling point."),
        verified("c", "Water is everywhere."),
    ])
}

#[tokio::test]
async fn batches_follow_the_measured_window_and_each_records_its_tokens() {
    let harness = three_claims();
    let llm = ScriptedVerifier::answering(window_holding(&harness, 1), supports);

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert_eq!(llm.calls(), 3);
    assert_eq!(outcome.summary().batches_planned, 3);
    assert_eq!(
        harness.progress.lock().unwrap().as_slice(),
        [
            "Audit batch 1 of 3",
            "Audit batch 2 of 3",
            "Audit batch 3 of 3"
        ]
    );
    let run = harness.runs.only();
    assert_eq!(run.batches.len(), 3);
    assert!(run
        .batches
        .iter()
        .all(|batch| batch.claim_ids.len() == 1 && batch.input_tokens == 1_000));
    assert_eq!((run.input_tokens, run.output_tokens), (3_000, 150));
    assert_eq!(
        (
            outcome.summary().input_tokens,
            outcome.summary().output_tokens
        ),
        (3_000, 150)
    );
}

#[tokio::test]
async fn a_larger_window_packs_more_claims_into_a_batch() {
    let harness = three_claims();
    let llm = ScriptedVerifier::answering(window_holding(&harness, 2), supports);

    harness.audit(&llm, None).await.unwrap();

    assert_eq!(llm.calls(), 2, "two claims, then one");
}

#[tokio::test]
async fn a_claim_no_request_can_hold_is_could_not_verify_and_never_sent() {
    let mut harness = three_claims();
    let window = window_holding(&harness, 1);
    let huge = "Boiling depends on pressure. ".repeat(window);
    harness
        .passages
        .insert(ClaimId("b".into()), vec![passage("v9", &huge)]);
    let llm = ScriptedVerifier::answering(window, supports);

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert!(llm.user_texts().iter().all(|text| !text.contains(&huge)));
    let check = harness.store.verifier_checks("b").pop().unwrap();
    assert_eq!(
        check.result,
        CheckResult::Failed {
            kind: VerdictKind::CouldNotJudge
        }
    );
    assert!(
        check.reason.contains(&format!(
            "The Verifier's window of {window} tokens cannot hold"
        )),
        "{}",
        check.reason
    );
    assert_eq!(outcome.summary().disputes, 1, "it queues for the user");
    assert!(matches!(
        harness.store.state("a"),
        ClaimState::ResolvedByEvidence { .. }
    ));
}

/// Whether the audit's measure fits `items` of the harness's claims in
/// `window`.
fn fits_at(harness: &Harness, window: usize, items: &[Item]) -> bool {
    let ledger = harness.store.ledger.lock().unwrap().clone();
    let due = ledger::due(&ledger, &harness.passages, &verifier_name());
    let probe = ScriptedVerifier::answering(window, supports);
    WindowSizer::new(&probe, &due, &harness.units, false).fits(items)
}

#[tokio::test]
async fn a_claim_too_large_for_one_request_is_judged_in_parts_by_passage() {
    let mut harness = Harness::new(vec![verified("a", "Water boils at 100 °C at sea level.")]);
    let filler = "Pressure matters for boiling. ".repeat(70);
    harness.passages.insert(
        ClaimId("a".into()),
        vec![
            passage("v1", &format!("Nothing here. {filler}")),
            passage(
                "v2",
                &format!("Water boils at 100 °C at sea level. {filler}"),
            ),
        ],
    );
    let part = |passages: Vec<usize>| Item {
        claim: 0,
        passages,
        context: Context::Unit,
        part: None,
    };
    let window = (1..)
        .map(|step| step * 64)
        .find(|window| {
            fits_at(&harness, *window, &[part(vec![0])])
                && fits_at(&harness, *window, &[part(vec![1])])
        })
        .unwrap();
    assert!(!fits_at(&harness, window, &[part(vec![0, 1])]));
    let llm = ScriptedVerifier::answering(window, |_, passages| {
        let supporting = passages
            .iter()
            .find(|(_, text)| text.starts_with("Water boils"));
        match supporting {
            Some((label, _)) => json!({"verdict": "supported",
                "passagesUsed": [{"passage": label, "quote": "Water boils at 100 °C at sea level."}]}),
            None => json!({"verdict": "unsupported"}),
        }
    });

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert_eq!(llm.calls(), 2, "one request per part");
    assert!(llm.user_texts()[0].contains("Part 1 of 2 of this claim's passages."));
    let checks = harness.store.verifier_checks("a");
    assert_eq!(checks.len(), 1, "the parts combine into one verdict");
    assert!(matches!(
        checks[0].result,
        CheckResult::Supported { ref source_version_id, .. } if source_version_id == "v2"
    ));
    assert_eq!(
        checks[0].evidence.len(),
        2,
        "every passage compared is recorded"
    );
    assert!(outcome.publishable());
}

#[tokio::test]
async fn an_outage_defers_and_the_next_audit_resumes_with_the_unaudited_claims() {
    let harness = three_claims();
    let window = window_holding(&harness, 1);
    let down = ScriptedVerifier::new(window, |request, call| {
        if call == 2 {
            return Err(AppError::Network(
                "Anthropic API returned HTTP 503 Service Unavailable".into(),
            ));
        }
        Ok(usage(CompletionResponse::from_text(answer_each(
            request, &supports,
        ))))
    });

    let outcome = harness.audit(&down, None).await.unwrap();

    assert!(matches!(&outcome, AuditOutcome::Deferred { .. }));
    assert!(
        !outcome.publishable(),
        "an unfinished audit holds the lesson"
    );
    assert_eq!(outcome.summary().batches_done, 1);
    let run = harness.runs.only();
    assert_eq!(run.state, RunState::Stopped);
    assert_eq!(run.last_failure(), Some("NETWORK_ERROR"));
    // Claims are packed in lesson order, and "Water is everywhere." (c)
    // opens the unit.
    assert_eq!(
        harness.store.verifier_checks("c").len(),
        1,
        "batch 1 is kept"
    );

    let up = ScriptedVerifier::answering(window, supports);
    let resumed = harness.audit(&up, None).await.unwrap();

    assert!(matches!(resumed, AuditOutcome::Completed(_)));
    let texts = up.user_texts();
    assert_eq!(texts.len(), 2, "only the two claims still unaudited");
    assert!(texts
        .iter()
        .all(|text| sent_claims(text) != ["Water is everywhere."]));
    assert_eq!(harness.runs.only().state, RunState::Completed);
}

#[tokio::test]
async fn a_rate_limit_defers_however_often_it_recurs() {
    let harness = Harness::new(vec![verified("a", "Water boils at 100 °C at sea level.")]);
    let limited = ScriptedVerifier::new(BIG, |_, _| Err(AppError::rate_limited(Some(30))));

    for _ in 0..3 {
        let outcome = harness.audit(&limited, None).await.unwrap();
        assert!(matches!(
            outcome,
            AuditOutcome::Deferred {
                error: AppError::RateLimitExceeded(_),
                ..
            }
        ));
    }
}

#[tokio::test]
async fn the_same_failure_twice_with_no_batch_done_between_is_rejected() {
    let harness = Harness::new(vec![verified("a", "Water boils at 100 °C at sea level.")]);
    let broken = ScriptedVerifier::new(BIG, |_, _| {
        Err(AppError::InternalError(
            "request 7f3a: schema not accepted".into(),
        ))
    });

    let first = harness.audit(&broken, None).await.unwrap();
    assert!(matches!(first, AuditOutcome::Deferred { .. }));

    let second = harness.audit(&broken, None).await;
    assert!(
        matches!(&second, Err(AppError::ModelRejected(message)) if message.contains("twice")),
        "{second:?}"
    );
}

#[tokio::test]
async fn different_failures_are_not_the_same_failure() {
    let harness = Harness::new(vec![verified("a", "Water boils at 100 °C at sea level.")]);
    let first = ScriptedVerifier::new(BIG, |_, _| Err(AppError::InternalError("one".into())));
    let second = ScriptedVerifier::new(BIG, |_, _| Err(AppError::InvalidInput("two".into())));

    assert!(matches!(
        harness.audit(&first, None).await.unwrap(),
        AuditOutcome::Deferred { .. }
    ));
    assert!(matches!(
        harness.audit(&second, None).await.unwrap(),
        AuditOutcome::Deferred { .. }
    ));
}

#[tokio::test]
async fn a_rejection_from_the_verifier_is_final_at_once() {
    let harness = Harness::new(vec![verified("a", "Water boils at 100 °C at sea level.")]);
    let rejecting = ScriptedVerifier::new(BIG, |_, _| {
        Err(AppError::ModelRejected("400 invalid_request_error".into()))
    });

    let outcome = harness.audit(&rejecting, None).await;

    assert!(matches!(outcome, Err(AppError::ModelRejected(_))));
    assert_eq!(rejecting.calls(), 1);
}

#[tokio::test]
async fn turning_the_audit_off_mid_run_stops_at_the_next_batch_and_keeps_its_verdicts() {
    let harness = three_claims();
    let window = window_holding(&harness, 1);
    harness
        .gate
        .stops
        .lock()
        .unwrap()
        .extend([None, None, Some(AuditStop::TurnedOff)]);
    let llm = ScriptedVerifier::answering(window, supports);

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert_eq!(llm.calls(), 1);
    let AuditOutcome::Stopped { stop, summary } = &outcome else {
        panic!("not stopped: {outcome:?}");
    };
    assert_eq!(*stop, AuditStop::TurnedOff);
    assert_eq!(
        stop.message(Some((summary.batches_done, summary.batches_planned))),
        "Audit stopped: Verifier turned off after batch 1 of 3"
    );
    assert!(outcome.publishable(), "opt-in off is no audit gate");
    // Batch 1 is the lesson's first claim, "Water is everywhere." (c).
    assert_eq!(harness.store.verifier_checks("c").len(), 1);
    assert!(harness.store.verifier_checks("a").is_empty());
    assert!(harness.store.verifier_checks("b").is_empty());
    assert_eq!(harness.runs.only().state, RunState::Stopped);
}

#[tokio::test]
async fn a_dispute_found_before_the_audit_was_turned_off_still_holds_the_lesson() {
    let harness = three_claims();
    let window = window_holding(&harness, 1);
    harness
        .gate
        .stops
        .lock()
        .unwrap()
        .extend([None, None, Some(AuditStop::TurnedOff)]);
    let llm = ScriptedVerifier::answering(
        window,
        |_, _| json!({"verdict": "unsupported", "reason": "nothing says so"}),
    );

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert_eq!(outcome.summary().disputes, 1);
    assert!(!outcome.publishable());
}

#[tokio::test]
async fn a_provider_changed_since_consent_is_refused_before_anything_is_sent() {
    let harness = three_claims();
    harness.gate.stops.lock().unwrap().push_back(Some(
        AuditStop::from_consent(&policy::consent(
            &ModelRef::Cloud {
                provider: CloudProvider::Openai,
                model: "gpt-x".into(),
            },
            Some("anthropic"),
        ))
        .unwrap(),
    ));
    let llm = ScriptedVerifier::answering(BIG, supports);

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert_eq!(llm.calls(), 0);
    assert!(harness.runs.runs.lock().unwrap().is_empty());
    let AuditOutcome::Stopped { stop, .. } = &outcome else {
        panic!("not refused: {outcome:?}");
    };
    assert_eq!(
        stop.message(None),
        "Consent was for Anthropic; confirm again for OpenAI"
    );
    assert_eq!(stop.code(), "verifier_consent_changed");
    assert!(
        !outcome.publishable(),
        "an audit that is on and cannot run holds the lesson"
    );
}

#[test]
fn every_stop_says_what_is_wrong_and_the_fix() {
    let unset = AuditStop::NoVerifier.stop_reason(None, 7);
    assert_eq!(unset.code, "verifier_unset");
    assert_eq!(unset.message, "Audit cannot run: no Verifier model is set.");
    assert!(unset.fix.contains("Settings → Models under Verifier"));
    assert!(unset.fix.contains("turn the audit off"));
    assert_eq!(unset.at, 7);

    let no_key = AuditStop::Unavailable {
        message: "Your Verifier model Anthropic/claude-x has no API key saved.".into(),
    };
    assert_eq!(
        no_key.message(None),
        "Audit cannot run: Your Verifier model Anthropic/claude-x has no API key saved."
    );
    assert_eq!(no_key.code(), "verifier_unavailable");

    let missing = AuditStop::ConsentMissing {
        provider: CloudProvider::Google,
    };
    assert_eq!(
        missing.message(None),
        "Audit cannot run: sending lesson text to Google was not confirmed for this course."
    );
    assert!(missing
        .fix()
        .contains("confirm sending lesson text to Google"));

    assert_eq!(
        AuditStop::TurnedOff.message(None),
        "Audit stopped: you turned the Verifier off"
    );
}

#[test]
fn an_audit_waiting_to_retry_or_unable_to_run_holds_the_lesson() {
    let summary = AuditSummary::default();
    assert!(!AuditOutcome::Deferred {
        error: AppError::Network("down".into()),
        summary: summary.clone(),
    }
    .publishable());
    assert!(!AuditOutcome::Stopped {
        stop: AuditStop::NoVerifier,
        summary: summary.clone(),
    }
    .publishable());
    assert!(AuditOutcome::Completed(summary).publishable());
}
