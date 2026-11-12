#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::super::sections::Gap;
use super::*;
use crate::lessons::course_generation_tests::ScriptedModel;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::domain::learning::acceptance::{Outcome, Reading, StandardPolicy, Step};
use lattice_core::domain::learning::claim_review::{
    Actor, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation,
    ClaimState, Class, ClassifiedBy, EvidenceFingerprint, RepairOutcome, UnitKind, VerdictKind,
};
use std::sync::Mutex;
#[test]
fn a_section_patch_keeps_other_content_and_only_receives_its_evidence() -> Result<()> {
    let original = json!({"blocks":[{"kind":"explanation","title":"First","body":"Keep byte-for-byte"},{"kind":"explanation","title":"Second","body":"Fix this"}],"questions":[{"kind":"quiz","prompt":"Keep this question"}]});
    let context = json!({"candidate":original,"requirements":{},"failedClaims":[{"unit":1,"evidence":["evidence-1"]},{"unit":2,"evidence":["evidence-2"]}],"evidence":[{"id":"evidence-1","text":"Relevant"},{"id":"evidence-2","text":"Another section"}],"executions":[],"repairHistory":[{"unit":1,"statement":"Rejected here"},{"unit":2,"statement":"Rejected elsewhere"}]});
    let local = section_context(&context, 1, &section_failures(&context, 1))?;
    assert_eq!(local["candidate"]["blocks"].as_array().unwrap().len(), 1);
    assert_eq!(
        local["evidence"],
        json!([{"id":"evidence-1","text":"Relevant"}])
    );
    assert_eq!(
        local["repairHistory"],
        json!([{"unit":1,"statement":"Rejected here"}])
    );
    // A last attempt is no request: the code takes the sentence out.
    assert!(local.get("repeatedDefects").is_none());
    let patch = json!({"blocks":[{"kind":"explanation","title":"Second","body":"Corrected"}],"questions":[]});
    let mut result = original.clone();
    merge(&mut result, patch, 1)?;
    assert_eq!(result["blocks"][0], original["blocks"][0]);
    assert_eq!(result["questions"], original["questions"]);
    assert_eq!(result["blocks"][1]["body"], "Corrected");
    Ok(())
}

#[test]
fn an_edited_section_of_another_kind_or_number_is_refused() {
    let original = json!({"blocks":[{"kind":"explanation","title":"Keep","body":"Keep this. Fix that."}],"questions":[]});
    let schema = json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object"}},"questions":{"type":"array","items":{"type":"object"}}}});
    let accept = |edit: Value| {
        accept_edits(
            &edit.to_string(),
            (&original, &original, &Default::default()),
            (&schema, "blocks"),
            (&[], &[]),
        )
    };
    let renamed =
        json!({"edits":[{"path":"/blocks/0/kind","before":"explanation","after":"recap"}]});
    assert!(accept(renamed).is_err());
}

/// Answers each request with the next of its answers and keeps every request.
struct Recording {
    answers: Mutex<std::collections::VecDeque<String>>,
    sent: Mutex<Vec<CompletionRequest>>,
}

#[async_trait::async_trait]
impl LLMPort for Recording {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.sent.lock().unwrap().push(request.clone());
        let answer = self.answers.lock().unwrap().pop_front().unwrap_or_default();
        Ok(CompletionResponse::from_text(&answer))
    }
    fn model_name(&self) -> &str {
        "recording"
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

fn messages(request: &CompletionRequest) -> Vec<(&str, &str)> {
    request
        .input
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { role, content } => Some((role.as_str(), content.as_str())),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_section_whose_edits_cannot_be_applied_is_asked_again_in_the_same_conversation(
) -> Result<()> {
    let original = json!({"blocks":[{"kind":"explanation","title":"Keep","body":"Keep this. Keep that."}],"questions":[]});
    let local =
        json!({"candidate":original,"failedClaims":[{"unit":0,"statement":"Correct one fact."}]});
    let ambiguous =
        json!({"edits":[{"path":"/blocks/0/body","before":"Keep","after":"Correct"}]}).to_string();
    let applies =
        json!({"edits":[{"path":"/blocks/0/body","before":"Keep that.","after":"Correct that."}]});
    let model = Recording {
        answers: Mutex::new([ambiguous.clone(), applies.to_string()].into()),
        sent: Mutex::new(Vec::new()),
    };
    let schema = json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object"}},"questions":{"type":"array","items":{"type":"object"}}}});
    let target = Protected {
        unit: 0,
        field: "body".into(),
        quote: "Keep that.".into(),
        released: false,
    };

    let (repaired, put_back) = repair_group(
        &model,
        "Repair the section.",
        (&local, &original, &original, &Default::default()),
        &schema,
        "blocks",
        4_000,
        (&[target], &[]),
    )
    .await?;

    assert_eq!(repaired["blocks"][0]["body"], "Keep this. Correct that.");
    assert!(put_back.is_empty());
    let sent = model.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 2, "the edits, then the correction");
    let (first, second) = (messages(&sent[0]), messages(&sent[1]));
    assert_eq!(second[..first.len()], first[..]);
    assert_eq!(second[first.len()], ("assistant", ambiguous.as_str()));
    let (role, asked) = second[first.len() + 1];
    assert_eq!(role, "user");
    assert!(asked.contains("occurs more than once"), "{asked}");
    Ok(())
}

/// A small window that records each request and edits the next marker.
struct SmallWindow {
    window: usize,
    requests: Mutex<Vec<(String, String)>>,
}

#[async_trait::async_trait]
impl LLMPort for SmallWindow {
    async fn complete(
        &self,
        request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        let (system, user) = crate::lessons::structured_call::task_messages(request);
        let mut requests = self.requests.lock().unwrap();
        let marker = format!("[{}]", requests.len());
        requests.push((system.to_owned(), user.to_owned()));
        Ok(
            lattice_core::application::ports::llm_port::CompletionResponse::from_text(
                json!({"edits":[{"path":"/blocks/0/body","before":marker,"after":marker.replace('[', "(").replace(']', ")")}]})
                    .to_string(),
            ),
        )
    }
    fn model_name(&self) -> &str {
        "small-window"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_section_too_large_for_one_request_is_repaired_in_several_that_fit() -> Result<()> {
    let passages: Vec<_> = (0..12)
        .map(|i| json!({"id":format!("evidence-{i}"),"sourceId":"s","startByte":i * 2000,"endByte":i * 2000 + 1600,"text":format!("passage-{i} {}", "x".repeat(1600))}))
        .collect();
    // Each failed claim is a ledger claim located on its own marker: a
    // repair may change only the failed claims' sentences (claim sources
    // design §10.2), so the edits must land on sentences it targets.
    let body = "[0] [1] [2] [3] [4] [5]";
    let claims: Vec<_> = (0..6)
        .map(|k| json!({"unit":0,"claimId":format!("claim-{k}"),"statement":format!("Claim {k}"),"verdict":"unsupported","reason":"Unstated.","evidence":[format!("evidence-{}", 2 * k), format!("evidence-{}", 2 * k + 1)]}))
        .collect();
    let context = json!({"candidate":{"blocks":[{"kind":"explanation","title":"T","body":body}],"questions":[]},"requirements":{},"failedClaims":claims,"evidence":passages,"executions":[],"repairHistory":[]});
    let schema = json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object"}},"questions":{"type":"array","items":{"type":"object"}}}});
    let output_tokens = 500;
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let references = ReferenceCollection::lexical(&[])?;
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    // Open for repair, as a pass leaves its failed claims.
    for k in 0..6 {
        let id = ClaimId(format!("claim-{k}"));
        let location = ClaimLocation::locate(0, UnitKind::Block, "body", format!("[{k}]"), body);
        ledger
            .append(
                &id,
                ClaimEvent::Extracted {
                    location,
                    statement: format!("Claim {k}"),
                },
            )
            .await?;
        let check = Check {
            checker: Checker::Local {
                model: llm.model_name().into(),
            },
            result: CheckResult::Failed {
                kind: VerdictKind::Unsupported,
            },
            reason: "Unstated.".into(),
            fingerprint: EvidenceFingerprint::of("fixture-v1", llm.model_name(), &[]),
            evidence: Vec::new(),
            window_tokens: 4_000,
        };
        ledger.append(&id, ClaimEvent::Checked(check)).await?;
        assert!(ledger.may_repair(&StandardPolicy::new(), &id));
    }

    let repaired = repair_sections(
        &llm,
        &StandardPolicy::new(),
        "{}",
        &schema,
        context,
        output_tokens,
        &references,
        &mut ledger,
    )
    .await?;

    let requests = llm.requests.lock().unwrap();
    assert!(requests.len() >= 3, "{} requests", requests.len());
    for (system, user) in requests.iter() {
        assert!(llm.count_tokens(system) + llm.count_tokens(user) + output_tokens <= 4_000);
        // Each request carries exactly the evidence its own claims cite.
        let local: Value = serde_json::from_str(user)?;
        let cited: Vec<_> = local["failedClaims"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|claim| claim["evidence"].as_array().unwrap().clone())
            .collect();
        let carried: Vec<_> = local["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .map(|passage| passage["id"].clone())
            .collect();
        assert_eq!(cited, carried);
    }
    for k in 0..6 {
        let statement = format!("\"Claim {k}\"");
        assert_eq!(
            requests
                .iter()
                .filter(|(_, user)| user.contains(&statement))
                .count(),
            1
        );
    }
    // Each request edited the section the previous one left.
    let body = repaired["blocks"][0]["body"].as_str().unwrap();
    for part in 0..requests.len() {
        assert!(body.contains(&format!("({part})")), "{body}");
    }
    Ok(())
}

/// A ledger of failed claims, each with the repair attempts already used on
/// it, as a pass and the repairs before it leave them.
async fn failed_ledger(
    llm: &dyn LLMPort,
    body: &str,
    claims: &[(&str, &str, u8)],
) -> Result<ClaimLedger> {
    let mut ledger = ClaimLedger::open(llm, "{}").await?;
    for &(id, sentence, used) in claims {
        let id = ClaimId(id.to_owned());
        let location = ClaimLocation::locate(0, UnitKind::Block, "body", sentence, body);
        let extracted = ClaimEvent::Extracted {
            location,
            statement: sentence.to_owned(),
        };
        ledger.append(&id, extracted).await?;
        let check = Check {
            checker: Checker::Local {
                model: llm.model_name().into(),
            },
            result: CheckResult::Failed {
                kind: VerdictKind::Unsupported,
            },
            reason: "Unstated.".into(),
            fingerprint: EvidenceFingerprint::of("fixture-v1", llm.model_name(), &[]),
            evidence: Vec::new(),
            window_tokens: 4_000,
        };
        ledger.append(&id, ClaimEvent::Checked(check)).await?;
        for attempt in 1..=used {
            let spent = ClaimEvent::RepairAttempted {
                attempt,
                escalated: false,
                before: sentence.to_owned(),
                after: None,
                outcome: RepairOutcome::Rejected {
                    reason: "Unusable.".into(),
                },
                goal: None,
            };
            ledger.append(&id, spent).await?;
        }
    }
    Ok(ledger)
}

fn lesson_of(body: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"T","body":body}],"questions":[]})
}

#[tokio::test]
async fn the_last_attempt_takes_the_sentence_out_and_leaves_the_rest_to_the_model() -> Result<()> {
    let body = "Keep this. Cut this. Fix this.";
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let mut ledger = failed_ledger(
        &llm,
        body,
        &[("last", "Cut this.", 1), ("first", "Fix this.", 0)],
    )
    .await?;
    let (last, first) = (ClaimId("last".into()), ClaimId("first".into()));
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(body),
            &[last.clone(), first.clone()],
        )
        .await?;
    let mut context = json!({
        "candidate": lesson_of(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"last","sentence":"Cut this.","evidence":[],"lastAttempt":true},
            {"unit":0,"claimId":"first","sentence":"Fix this.","evidence":[]},
        ],
    });

    assert_eq!(
        cut::last_attempts(
            &StandardPolicy::new(),
            crate::lessons::shape::policy(),
            &mut context,
            &mut ledger
        )
        .await?,
        1
    );

    assert_eq!(
        context["candidate"]["blocks"][0]["body"],
        "Keep this. Fix this."
    );
    assert_eq!(claim_ids(&section_failures(&context, 0)), vec![first]);
    assert!(ledger
        .pending
        .repairs
        .iter()
        .any(|record| record.claim_id == last));
    assert!(llm.requests.lock().unwrap().is_empty());
    // The section remembers where the sentence was, for its flow repair.
    let taken = &ledger.cuts.taken[&0];
    assert_eq!(taken.label, "T");
    assert_eq!(
        taken.gaps,
        vec![Gap {
            field: "body".into(),
            before: Some("Keep this.".into()),
            after: Some("Fix this.".into()),
        }]
    );
    assert!(!taken.answer_lost);
    assert!(ledger.cuts.going.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_last_attempt_that_cannot_take_the_sentence_out_ends_the_claim_removed_with_its_section(
) -> Result<()> {
    let body = "Cut this.";
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let mut ledger = failed_ledger(&llm, body, &[("last", "Cut this.", 1)]).await?;
    let last = ClaimId("last".into());
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(body),
            std::slice::from_ref(&last),
        )
        .await?;
    let mut context = json!({
        "candidate": lesson_of(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"last","sentence":"Cut this.","evidence":[],"lastAttempt":true},
        ],
    });

    assert_eq!(
        cut::last_attempts(
            &StandardPolicy::new(),
            crate::lessons::shape::policy(),
            &mut context,
            &mut ledger
        )
        .await?,
        1
    );

    // The sentence stays where it is; its section is the one that goes.
    assert_eq!(context["candidate"]["blocks"][0]["body"], body);
    assert!(section_failures(&context, 0).is_empty());
    assert_eq!(ledger.cuts.going.get(&0).map(String::as_str), Some("T"));
    // The claim is terminal, and nothing waits for the user.
    let entry = ledger.entry(&last).unwrap();
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
    assert!(ledger.entries().iter().all(|entry| {
        !entry.state().awaits_user() && !matches!(entry.state(), ClaimState::Open { .. })
    }));
    assert!(ledger.pending.repairs.is_empty());
    Ok(())
}

#[tokio::test]
async fn the_other_claims_of_a_section_that_goes_are_no_work_for_the_model() -> Result<()> {
    let body = "Cut this.";
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let mut ledger = failed_ledger(
        &llm,
        body,
        &[("last", "Cut this.", 0), ("other", "Cut this.", 0)],
    )
    .await?;
    // The narrowing that would keep what `other` says was asked and not applied.
    narrowing_spent(&mut ledger, "last", "Cut this.").await?;
    let (last, other) = (ClaimId("last".into()), ClaimId("other".into()));
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(body),
            &[last.clone(), other.clone()],
        )
        .await?;
    let mut context = json!({
        "candidate": lesson_of(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"last","sentence":"Cut this.","evidence":[],"lastAttempt":true},
            {"unit":0,"claimId":"other","sentence":"Cut this.","evidence":[]},
        ],
    });

    cut::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    assert!(
        section_failures(&context, 0).is_empty(),
        "a section that goes is not rewritten"
    );
    assert!(targets(&context)?.is_empty());
    assert!(llm.requests.lock().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_failed_example_on_its_last_attempt_is_taken_out_with_its_fences() -> Result<()> {
    let body = "Intro.\n\n```python\nprint(1)\n```\n\nAfter.";
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let mut ledger = failed_ledger(
        &llm,
        body,
        &[("example", "print(1)\n", 1), ("intro", "Intro.", 0)],
    )
    .await?;
    let example = ClaimId("example".into());
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(body),
            std::slice::from_ref(&example),
        )
        .await?;
    let mut context = json!({
        "candidate": lesson_of(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [], "failedClaims": [],
        "exampleClaims": [{"unit":0,"claimId":"example","lastAttempt":true}],
    });

    assert_eq!(
        cut::last_attempts(
            &StandardPolicy::new(),
            crate::lessons::shape::policy(),
            &mut context,
            &mut ledger
        )
        .await?,
        1
    );

    assert_eq!(
        context["candidate"]["blocks"][0]["body"],
        "Intro.\n\nAfter."
    );
    assert!(section_examples(&context, 0).is_empty());
    Ok(())
}

#[tokio::test]
async fn a_claim_on_a_sentence_cut_for_another_is_no_work_for_the_model() -> Result<()> {
    let body = "Keep this. Cut this. Fix this.";
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let mut ledger = failed_ledger(
        &llm,
        body,
        &[
            ("last", "Cut this.", 0),
            ("mate", "Cut this.", 0),
            ("kept", "Keep this.", 0),
        ],
    )
    .await?;
    // The narrowing that would keep what `mate` says was asked and not applied.
    narrowing_spent(&mut ledger, "last", "Cut this.").await?;
    let (last, mate) = (ClaimId("last".into()), ClaimId("mate".into()));
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(body),
            &[last.clone(), mate.clone()],
        )
        .await?;
    let mut context = json!({
        "candidate": lesson_of(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"last","sentence":"Cut this.","evidence":[],"lastAttempt":true},
            {"unit":0,"claimId":"mate","sentence":"Cut this.","evidence":[]},
        ],
    });

    assert_eq!(
        cut::last_attempts(
            &StandardPolicy::new(),
            crate::lessons::shape::policy(),
            &mut context,
            &mut ledger
        )
        .await?,
        1
    );

    assert_eq!(
        context["candidate"]["blocks"][0]["body"],
        "Keep this. Fix this."
    );
    assert!(
        section_failures(&context, 0).is_empty(),
        "the other claim's sentence is gone with the cut"
    );
    // Its record stays: the pass after finds the claim gone and removes it.
    assert!(ledger
        .pending
        .repairs
        .iter()
        .any(|record| record.claim_id == mate));
    Ok(())
}

#[tokio::test]
async fn last_attempts_that_leave_the_lesson_no_claim_stop_it_and_end_every_claim_they_cut_removed(
) -> Result<()> {
    let body = "Keep this. Cut this. Fix this.";
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    // Every claim the ledger holds stands on the sentence the last attempt
    // takes out: the one it was sent for, and one that went with it.
    let mut ledger = failed_ledger(
        &llm,
        body,
        &[("last", "Cut this.", 0), ("mate", "Cut this.", 0)],
    )
    .await?;
    // The narrowing that would keep what `mate` says was asked and not applied.
    narrowing_spent(&mut ledger, "last", "Cut this.").await?;
    let (last, mate) = (ClaimId("last".into()), ClaimId("mate".into()));
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(body),
            &[last.clone(), mate.clone()],
        )
        .await?;
    let mut context = json!({
        "candidate": lesson_of(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"last","sentence":"Cut this.","evidence":[],"lastAttempt":true},
            {"unit":0,"claimId":"mate","sentence":"Cut this.","evidence":[]},
        ],
    });

    let error = cut::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "The sources do not support enough of this lesson: every statement in it would have to be taken out."
    );
    for id in [&last, &mate] {
        let entry = ledger.entry(id).unwrap();
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
    }
    assert!(ledger.pending.repairs.is_empty());
    assert!(llm.requests.lock().unwrap().is_empty());
    Ok(())
}

#[test]
fn the_evidence_table_names_the_lessons_source_index_of_a_numbered_source() {
    let source = |id: &str| LearningSourceDto {
        id: id.into(),
        title: id.into(),
        url: None,
        excerpt: String::new(),
        acquired_at: 0,
    };
    let mut context = json!({"evidence":[
        {"id":"evidence-0","sourceId":"v2"},
        {"id":"evidence-1","sourceId":"found-by-research"}]});

    number_evidence(&mut context, &[source("v1"), source("v2")]);

    assert_eq!(context["evidence"][0]["sourceIndex"], 1);
    assert!(context["evidence"][1].get("sourceIndex").is_none());
}

/// A rewrite to the wording an evidence passage states cites that passage in
/// the same response, so the final structural pass finds nothing uncited in
/// it; confinement, which takes back what changed outside the failed
/// sentence, keeps the citation of the sentence it allowed.
#[tokio::test]
async fn a_repaired_sentence_cites_the_passage_it_was_given_and_leaves_no_uncited_specific(
) -> Result<()> {
    let original = json!({"blocks":[{"kind":"explanation","title":"T","body":"Keep this. Fix this.","citations":[]}],"questions":[]});
    let local = json!({"candidate":original,"failedClaims":[{"unit":0,"statement":"Fix this."}]});
    let sentence =
        "The guide says \"a timeout of 30 seconds applies to each request\" before it fails.";
    let quote = "A timeout of 30 seconds applies to each request.";
    let response = json!({
        "edits":[{"path":"/blocks/0/body","before":"Fix this.","after":sentence}],
        "citations":[{"path":"/blocks/0","sentence":sentence,"sourceIndex":0,"quote":quote}]
    });
    let model = ScriptedModel {
        outputs: Mutex::new(vec![response.to_string()].into()),
        prompts: Mutex::new(Vec::new()),
    };
    let schema = json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object"}},"questions":{"type":"array","items":{"type":"object"}}}});
    let failed = Protected {
        unit: 0,
        field: "body".into(),
        quote: "Fix this.".into(),
        released: false,
    };
    let kept = Protected {
        unit: 0,
        field: "body".into(),
        quote: "Keep this.".into(),
        released: false,
    };

    let (repaired, _) = repair_group(
        &model,
        "system",
        (&local, &original, &original, &Default::default()),
        &schema,
        "blocks",
        4_000,
        (&[failed], &[kept]),
    )
    .await?;

    let section = &repaired["blocks"][0];
    assert_eq!(section["body"], format!("Keep this. {sentence}"));
    assert_eq!(
        section["citations"],
        json!([{"sentence":sentence,"sourceIndex":0,"quote":quote}])
    );
    let uncited = |lesson: &Value| {
        crate::lessons::shape::policy()
            .rules(lesson, crate::lessons::shape::Phase::Draft)
            .into_iter()
            .filter(|finding| finding.kind == crate::review_findings::FindingKind::Quotation)
            .count()
    };
    assert_eq!(uncited(&repaired), 0);
    // The same rewrite without its citation is what the pass would list.
    let mut bare = repaired.clone();
    bare["blocks"][0]["citations"] = json!([]);
    assert_eq!(uncited(&bare), 1);
    Ok(())
}

#[test]
fn a_citation_for_a_sentence_the_confinement_took_back_is_not_kept() {
    let mut confined = json!({"kind":"explanation","body":"Keep this. Fix this.","citations":[]});
    let stray = json!({"sentence":"A sentence the section lacks.","sourceIndex":0,"quote":"x"});
    let kept = json!({"sentence":"Fix this.","sourceIndex":0,"quote":"y"});

    citation::restore_appended(
        &mut confined,
        vec![stray, kept.clone(), kept.clone()],
        "blocks",
    );

    assert_eq!(confined["citations"], json!([kept]));
}

#[test]
fn a_citation_for_a_sentence_of_a_choices_reason_is_kept() {
    let mut confined = json!({"prompt":"Which?","explanation":"A holds.",
        "rationales":["","B fails at 5 attempts.","C fails."],"citations":[]});
    let kept = json!({"sentence":"B fails at 5 attempts.","sourceIndex":0,"quote":"y"});

    citation::restore_appended(&mut confined, vec![kept.clone()], "questions");

    assert_eq!(confined["citations"], json!([kept]));
}

#[test]
fn a_section_given_back_no_citation_gets_no_citations_field() {
    let section = json!({"kind":"explanation","body":"Keep this. Fix this."});
    let stray = json!({"sentence":"A sentence the section lacks.","sourceIndex":0,"quote":"x"});

    for appended in [Vec::new(), vec![stray]] {
        let mut confined = section.clone();
        citation::restore_appended(&mut confined, appended, "blocks");
        assert_eq!(confined, section);
    }
}

/// A claim that passed its check, standing on `quote` of unit 0's `body`, as a
/// pass leaves it.
async fn passed(
    ledger: &mut ClaimLedger,
    llm: &dyn LLMPort,
    id: &str,
    (quote, statement): (&str, &str),
    body: &str,
) -> Result<()> {
    let id = ClaimId(id.to_owned());
    let location = ClaimLocation::locate(0, UnitKind::Block, "body", quote, body);
    let extracted = ClaimEvent::Extracted {
        location,
        statement: statement.to_owned(),
    };
    ledger.append(&id, extracted).await?;
    let check = Check {
        checker: Checker::Local {
            model: llm.model_name().into(),
        },
        result: CheckResult::Supported {
            source_version_id: "source".into(),
            supporting_quote: statement.to_owned(),
        },
        reason: "Stated.".into(),
        fingerprint: EvidenceFingerprint::of("fixture-v1", llm.model_name(), &[]),
        evidence: Vec::new(),
        window_tokens: 4_000,
    };
    ledger.append(&id, ClaimEvent::Checked(check)).await?;
    Ok(())
}

const FIRST_OLD: &str = "The first stage dries the clay.";
const FIRST_NEW: &str = "The first stage drives the water out of the clay.";
const STEADY: &str = "A steady ramp avoids cracks.";
const PASSED: &str = "The second stage burns off the binder.";
const THIRD_OLD: &str = "The third stage vitrifies the body.";
const THIRD_NEW: &str = "The third stage turns the body to glass.";
const MERGED: &str = "The second and third stages burn off the binder and vitrify the body.";
const CLOSING: &str = "Cooling ends the cycle.";

#[tokio::test]
async fn a_hunk_that_mixes_a_target_with_a_passed_sentence_costs_only_that_target_and_the_other_targets_rewrite_stands(
) -> Result<()> {
    // §1 #7 of the acceptance policy design
    let body = format!("{FIRST_OLD} {STEADY} {PASSED} {THIRD_OLD} {CLOSING}");
    // The first target is rewritten cleanly; the third stage is merged with the
    // passed sentence before it, in one hunk. If the refused target is asked
    // again, the answer rewrites the third stage alone.
    let merging = json!({"edits":[
        {"path":"/blocks/0/body","before":FIRST_OLD,"after":FIRST_NEW},
        {"path":"/blocks/0/body","before":format!("{PASSED} {THIRD_OLD}"),"after":MERGED},
    ]});
    let alone = json!({"edits":[{"path":"/blocks/0/body","before":THIRD_OLD,"after":THIRD_NEW}]});
    let llm = Recording {
        answers: Mutex::new([merging.to_string(), alone.to_string()].into()),
        sent: Mutex::new(Vec::new()),
    };
    let mut ledger = failed_ledger(
        &llm,
        &body,
        &[("first", FIRST_OLD, 0), ("third", THIRD_OLD, 0)],
    )
    .await?;
    passed(&mut ledger, &llm, "passed", (PASSED, PASSED), &body).await?;
    let (first, third) = (ClaimId("first".into()), ClaimId("third".into()));
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(&body),
            &[first.clone(), third],
        )
        .await?;
    let context = json!({
        "candidate": lesson_of(&body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"first","sentence":FIRST_OLD,"evidence":[]},
            {"unit":0,"claimId":"third","sentence":THIRD_OLD,"evidence":[]},
        ],
    });
    let schema = json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object"}},"questions":{"type":"array","items":{"type":"object"}}}});
    let references = ReferenceCollection::lexical(&[])?;

    let repaired = repair_sections(
        &llm,
        &StandardPolicy::new(),
        "{}",
        &schema,
        context,
        4_000,
        &references,
        &mut ledger,
    )
    .await?;

    let after = repaired["blocks"][0]["body"].as_str().unwrap();
    assert!(
        after.contains(FIRST_NEW) && !after.contains(FIRST_OLD),
        "the clean rewrite of the first target stands: {after}"
    );
    assert!(
        after.contains(PASSED),
        "the passed sentence stands as it was: {after}"
    );
    assert!(
        !after.contains(MERGED),
        "the merged hunk is not applied: {after}"
    );
    let charged = ledger.entry(&first).unwrap().events().iter().any(|record| {
        matches!(
            &record.event,
            ClaimEvent::RepairAttempted {
                outcome: RepairOutcome::Rejected { .. },
                ..
            }
        )
    });
    assert!(
        !charged,
        "the rewrite that was applied costs its claim no attempt"
    );
    Ok(())
}

/// The node of the statement `id` in its class's graph.
fn node_of(ledger: &ClaimLedger, id: &ClaimId) -> Option<&'static str> {
    let policy = StandardPolicy::new();
    let (at, _) = ledger.waits_at(&policy, id)?;
    policy.graph(at.class).node(at.node).map(|node| node.name)
}

/// The reasons the corrections of a claim could not be applied, in order.
fn rejections(entry: &ClaimLedgerEntry) -> Vec<String> {
    entry
        .events()
        .iter()
        .filter_map(|record| match &record.event {
            ClaimEvent::RepairAttempted {
                outcome: RepairOutcome::Rejected { reason },
                ..
            } => Some(reason.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_target_whose_correction_was_put_back_goes_to_its_retry_and_the_groups_other_rewrite_stands(
) -> Result<()> {
    let body = format!("{FIRST_OLD} {STEADY} {PASSED} {THIRD_OLD} {CLOSING}");
    let merging = json!({"edits":[
        {"path":"/blocks/0/body","before":FIRST_OLD,"after":FIRST_NEW},
        {"path":"/blocks/0/body","before":format!("{PASSED} {THIRD_OLD}"),"after":MERGED},
    ]});
    let llm = Recording {
        answers: Mutex::new([merging.to_string()].into()),
        sent: Mutex::new(Vec::new()),
    };
    let mut ledger = failed_ledger(
        &llm,
        &body,
        &[("first", FIRST_OLD, 0), ("third", THIRD_OLD, 0)],
    )
    .await?;
    passed(&mut ledger, &llm, "passed", (PASSED, PASSED), &body).await?;
    let policy = StandardPolicy::new();
    let (first, third) = (ClaimId("first".into()), ClaimId("third".into()));
    // Both are Sourced statements the policy has brought to a rewrite.
    for id in [&first, &third] {
        let sorted = ClaimEvent::Classified {
            class: Class::Sourced,
            by: ClassifiedBy::Writer,
        };
        ledger.append(id, sorted).await?;
        for (step, outcome) in [
            (Step::Read(Reading::Parts), Outcome::Unproven),
            (Step::Research, Outcome::FoundNothing),
            (Step::Read(Reading::Page), Outcome::Refuted),
        ] {
            assert!(ledger.record_step(&policy, id, step, outcome).await?);
        }
        assert_eq!(node_of(&ledger, id), Some("s.rewrite"));
    }
    ledger
        .begin_repairs(&policy, &lesson_of(&body), &[first.clone(), third.clone()])
        .await?;
    let context = json!({
        "candidate": lesson_of(&body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"first","sentence":FIRST_OLD,"evidence":[]},
            {"unit":0,"claimId":"third","sentence":THIRD_OLD,"evidence":[]},
        ],
    });
    let schema = json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object"}},"questions":{"type":"array","items":{"type":"object"}}}});
    let references = ReferenceCollection::lexical(&[])?;

    let repaired = repair_sections(
        &llm,
        &policy,
        "{}",
        &schema,
        context,
        4_000,
        &references,
        &mut ledger,
    )
    .await?;

    let after = repaired["blocks"][0]["body"].as_str().unwrap();
    assert!(
        after.contains(FIRST_NEW) && after.contains(PASSED) && !after.contains(MERGED),
        "{after}"
    );
    // The put-back correction is charged to its claim alone, which goes on to
    // its retry, told why.
    assert_eq!(node_of(&ledger, &third), Some("s.rewrite.again"));
    let reasons = rejections(ledger.entry(&third).unwrap());
    assert!(
        matches!(reasons.as_slice(), [reason] if reason.contains(PASSED) && reason.contains("put back")),
        "{reasons:?}"
    );
    // The other target's rewrite stands: nothing is charged to it, and the
    // pass that reads the sentence decides it.
    assert_eq!(node_of(&ledger, &first), Some("s.rewrite"));
    assert!(rejections(ledger.entry(&first).unwrap()).is_empty());
    assert!(ledger
        .pending
        .repairs
        .iter()
        .any(|record| record.claim_id == first));
    Ok(())
}

#[tokio::test]
async fn the_last_attempt_on_a_sentence_that_also_states_a_passed_claim_leaves_what_the_passed_claim_says(
) -> Result<()> {
    // §1 #8 (a) of the acceptance policy design
    const SHARED: &str = "Glaze needs a clean surface, so a bisque firing comes first.";
    const PASSED_STATEMENT: &str = "Glaze needs a clean surface";
    let body = format!("Intro line. {SHARED} Closing line.");
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let mut ledger = failed_ledger(
        &llm,
        &body,
        &[("failed", SHARED, 1), ("intro", "Intro line.", 0)],
    )
    .await?;
    passed(
        &mut ledger,
        &llm,
        "passed",
        (SHARED, PASSED_STATEMENT),
        &body,
    )
    .await?;
    let failed = ClaimId("failed".into());
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(&body),
            std::slice::from_ref(&failed),
        )
        .await?;
    let mut context = json!({
        "candidate": lesson_of(&body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"failed","sentence":SHARED,"evidence":[],"lastAttempt":true,"alsoStates":[PASSED_STATEMENT]},
        ],
    });

    cut::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    let after = context["candidate"]["blocks"][0]["body"].as_str().unwrap();
    assert!(
        after.contains(PASSED_STATEMENT),
        "the words of the claim that passed stay: {after}"
    );
    Ok(())
}

#[tokio::test]
async fn the_last_attempt_takes_out_the_sentence_that_states_the_claim_not_the_rest_of_a_passage_wide_quote(
) -> Result<()> {
    // §1 #8 (b) of the acceptance policy design
    const OTHER: &str = "The kiln is loaded while cold.";
    const STATING: &str = "A bisque firing reaches cone 04.";
    const STATEMENT: &str = "A bisque firing reaches cone 04";
    let passage = format!("{OTHER} {STATING}");
    let body = format!("{passage} Closing line.");
    // The claim's quote is the passage of both sentences, as an older pass
    // stored it; no claim stands on the first.
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let mut ledger = failed_ledger(
        &llm,
        &body,
        &[
            ("stating", passage.as_str(), 1),
            ("kept", "Closing line.", 0),
        ],
    )
    .await?;
    let stating = ClaimId("stating".into());
    let location = ClaimLocation::locate(0, UnitKind::Block, "body", passage.as_str(), &body);
    let extracted = ClaimEvent::Extracted {
        location,
        statement: STATEMENT.to_owned(),
    };
    ledger.append(&stating, extracted).await?;
    // The locator reads the statement as the passage's second sentence.
    let located = lattice_core::shared::text::claim_sentence(&body, (0, passage.len()), STATEMENT);
    assert_eq!(
        located.and_then(|(start, end)| body.get(start..end)),
        Some(STATING)
    );
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(&body),
            std::slice::from_ref(&stating),
        )
        .await?;
    let mut context = json!({
        "candidate": lesson_of(&body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"stating","sentence":passage,"evidence":[],"lastAttempt":true},
        ],
    });

    cut::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    assert_eq!(
        context["candidate"]["blocks"][0]["body"],
        format!("{OTHER} Closing line.")
    );
    Ok(())
}

#[test]
fn each_goal_of_a_rewrite_has_the_words_the_request_gives_it() {
    assert_eq!(
        goal_words(Goal::WhatThePassageStates),
        "Say only what the claim's evidence passages state. Where no passage states a detail, keep the sentence's point and leave that detail out."
    );
    assert_eq!(
        goal_words(Goal::AgreeWithThePassages),
        "Make the sentence agree with its evidence passages, correcting what the reason names."
    );
    assert_eq!(
        goal_words(Goal::AsTheFieldHasIt),
        "State it as the field generally has it, correcting what the reason names."
    );
    assert_eq!(
        goal_words(Goal::KeepTheOthers),
        "Take this claim out of the sentence and keep each statement in alsoStates, said as it is now."
    );
}

#[test]
fn the_system_prompt_says_what_the_goal_the_retry_and_the_history_are() {
    let system = repair_system(&json!({}));

    let history = "repairHistory lists sentences earlier corrections replaced or removed because a check failed them; do not reintroduce what they asserted, or paraphrases of it.";
    let goal = "Each failed claim's goal says what its corrected sentence must do; the check that follows tests exactly that. A failed claim with notApplied was sent before and its correction could not be applied, for the reason given: correct it so that it can be.";
    assert!(system.contains(history), "{system}");
    assert!(system.contains(goal), "{system}");
    assert!(!system.contains("claim variants rejected"), "{system}");
    // The goal is said after the sentence on `alsoStates`.
    let also = system.find("A failed claim's alsoStates lists").unwrap();
    let said = system.find(goal).unwrap();
    assert!(also < said && said < system.find("When evidence cannot support").unwrap());
}

const COLD: &str = "The kiln is loaded while cold.";
const BISQUE: &str = "A bisque firing reaches cone 04.";

/// A claim "stating", located by `passage` as an older pass stored it and
/// stating `statement`, sent on its last attempt in a lesson of `body`, with a
/// claim "kept" on `Closing line.`.
async fn passage_wide(
    llm: &dyn LLMPort,
    body: &str,
    stated: (&str, &str),
) -> Result<(ClaimLedger, Value)> {
    passage_wide_with(llm, body, stated, false).await
}

/// [`passage_wide`], the last attempt being the one after a narrowing of the
/// claim was asked and not applied when `spent`.
async fn passage_wide_with(
    llm: &dyn LLMPort,
    body: &str,
    (passage, statement): (&str, &str),
    spent: bool,
) -> Result<(ClaimLedger, Value)> {
    let mut ledger = failed_ledger(
        llm,
        body,
        &[
            ("stating", passage, u8::from(!spent)),
            ("kept", "Closing line.", 0),
        ],
    )
    .await?;
    if spent {
        narrowing_spent(&mut ledger, "stating", passage).await?;
    }
    let stating = ClaimId("stating".into());
    let location = ClaimLocation::locate(0, UnitKind::Block, "body", passage, body);
    let extracted = ClaimEvent::Extracted {
        location,
        statement: statement.to_owned(),
    };
    ledger.append(&stating, extracted).await?;
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(body),
            std::slice::from_ref(&stating),
        )
        .await?;
    let context = json!({
        "candidate": lesson_of(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"stating","sentence":passage,"evidence":[],"lastAttempt":true},
        ],
    });
    Ok((ledger, context))
}

#[tokio::test]
async fn a_passage_wide_quote_is_cut_to_the_sentence_that_states_the_claim_in_its_first_place(
) -> Result<()> {
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let body = format!("{COLD} {BISQUE} Middle line. {BISQUE} Closing line.");
    let passage = format!("{COLD} {BISQUE}");
    let (mut ledger, mut context) = passage_wide(
        &llm,
        &body,
        (passage.as_str(), "A bisque firing reaches cone 04"),
    )
    .await?;

    let handled = cut::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    // The first sentence of the passage stays; the stating one goes where the
    // quote first stands, and its second copy is kept.
    assert_eq!(handled, 1);
    assert_eq!(
        context["candidate"]["blocks"][0]["body"],
        format!("{COLD} Middle line. {BISQUE} Closing line.")
    );
    Ok(())
}

#[tokio::test]
async fn a_cut_whose_sentence_another_live_statement_stands_on_takes_everything_its_quote_touches(
) -> Result<()> {
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let body = format!("{COLD} {BISQUE} Closing line.");
    let passage = format!("{COLD} {BISQUE}");
    let (mut ledger, mut context) = passage_wide_with(
        &llm,
        &body,
        (passage.as_str(), "A bisque firing reaches cone 04"),
        true,
    )
    .await?;
    // A statement of the ledger's states the same sentence.
    let mate = ClaimId("mate".into());
    let extracted = ClaimEvent::Extracted {
        location: ClaimLocation::locate(0, UnitKind::Block, "body", BISQUE, &body),
        statement: "A bisque firing is taken to cone 04".to_owned(),
    };
    ledger.append(&mate, extracted).await?;
    let sorted = ClaimEvent::Classified {
        class: Class::Sourced,
        by: ClassifiedBy::Writer,
    };
    ledger.append(&mate, sorted).await?;

    cut::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    // Once its narrowing was not applied, the quote is cut whole.
    assert_eq!(context["candidate"]["blocks"][0]["body"], "Closing line.");
    Ok(())
}

#[tokio::test]
async fn a_claim_that_draws_on_several_sentences_of_its_quote_is_cut_with_everything_the_quote_touches(
) -> Result<()> {
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    // No sentence of this passage holds the greater part of the statement.
    let passage = "The Peace of Westphalia was signed in 1648 in the cities of Osnabrück and Münster. It ended the Thirty Years' War in the Holy Roman Empire. It also closed the Eighty Years' War between Spain and the Dutch Republic. Historians often date the modern order of sovereign states from these treaties.";
    let statement = "Signed in 1648, the Peace of Westphalia ended the Thirty Years' War and is often seen by historians as the start of the order of sovereign states.";
    let body = format!("{passage} Closing line.");
    let (mut ledger, mut context) =
        passage_wide_with(&llm, &body, (passage, statement), true).await?;

    cut::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    // Once its narrowing was not applied, the whole quote goes.
    assert_eq!(context["candidate"]["blocks"][0]["body"], "Closing line.");
    Ok(())
}

/// Brings the Sourced statement `id` to `s.cut`: its readings and its rewrite
/// came to nothing.
async fn at_the_cut(ledger: &mut ClaimLedger, id: &ClaimId) -> Result<()> {
    let policy = StandardPolicy::new();
    let sorted = ClaimEvent::Classified {
        class: Class::Sourced,
        by: ClassifiedBy::Writer,
    };
    ledger.append(id, sorted).await?;
    for (step, outcome) in [
        (Step::Read(Reading::Parts), Outcome::Unproven),
        (Step::Research, Outcome::FoundNothing),
        (Step::Read(Reading::Page), Outcome::Refuted),
        (
            Step::Rewrite(Goal::WhatThePassageStates),
            Outcome::Rewritten,
        ),
        (Step::Read(Reading::Parts), Outcome::Refuted),
    ] {
        assert!(ledger.record_step(&policy, id, step, outcome).await?);
    }
    assert_eq!(node_of(ledger, id), Some("s.cut"));
    Ok(())
}

/// The narrowing rewrite of claim `id`, which stands on `sentence`, was asked
/// and not applied: the first attempt of a claim that has none yet.
async fn narrowing_spent(ledger: &mut ClaimLedger, id: &str, sentence: &str) -> Result<()> {
    let spent = ClaimEvent::RepairAttempted {
        attempt: 1,
        escalated: false,
        before: sentence.to_owned(),
        after: None,
        outcome: RepairOutcome::Rejected {
            reason: "The repair left the sentence as it was.".into(),
        },
        goal: Some(Goal::KeepTheOthers),
    };
    assert!(ledger.append(&ClaimId(id.to_owned()), spent).await?);
    Ok(())
}

const GLAZE: &str = "Glaze needs a clean surface, so a bisque firing comes first.";
const GLAZE_STATES: &str = "Glaze needs a clean surface";

#[tokio::test]
async fn a_statement_no_single_sentence_states_is_narrowed_and_sent_not_cut() -> Result<()> {
    // §1 #8 (a) of the acceptance policy design
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    // No sentence of this passage holds the greater part of the statement, and a
    // live statement stands on its first sentence and on its last.
    let first =
        "The Peace of Westphalia was signed in 1648 in the cities of Osnabrück and Münster.";
    let last = "Historians often date the modern order of sovereign states from these treaties.";
    let passage = format!("{first} It ended the Thirty Years' War in the Holy Roman Empire. It also closed the Eighty Years' War between Spain and the Dutch Republic. {last}");
    let statement = "Signed in 1648, the Peace of Westphalia ended the Thirty Years' War and is often seen by historians as the start of the order of sovereign states.";
    let body = format!("{passage} Closing line.");
    let (mut ledger, mut context) =
        passage_wide(&llm, &body, (passage.as_str(), statement)).await?;
    passed(&mut ledger, &llm, "first", (first, first), &body).await?;
    passed(&mut ledger, &llm, "last", (last, last), &body).await?;

    cut::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    // Nothing is cut: the statement is sent, told to take its claim out and
    // leave the others as they are.
    assert_eq!(context["candidate"]["blocks"][0]["body"], body);
    let failed = section_failures(&context, 0);
    assert_eq!(failed.len(), 1);
    assert!(failed[0].get("lastAttempt").is_none(), "{}", failed[0]);
    assert_eq!(failed[0]["goal"], goal_words(Goal::KeepTheOthers));
    Ok(())
}

#[tokio::test]
async fn a_statement_narrowed_is_still_sent_and_nothing_is_decided_for_it_yet() -> Result<()> {
    // §1 #8 (a) of the acceptance policy design
    let body = format!("Intro line. {GLAZE} Closing line.");
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let mut ledger = failed_ledger(
        &llm,
        &body,
        &[("cut", GLAZE, 0), ("intro", "Intro line.", 0)],
    )
    .await?;
    passed(&mut ledger, &llm, "beside", (GLAZE, GLAZE_STATES), &body).await?;
    let cut = ClaimId("cut".into());
    at_the_cut(&mut ledger, &cut).await?;
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(&body),
            std::slice::from_ref(&cut),
        )
        .await?;
    let mut context = json!({
        "candidate": lesson_of(&body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"cut","sentence":GLAZE,"evidence":[],"lastAttempt":true,"alsoStates":[GLAZE_STATES]},
        ],
    });
    let recorded = ledger.entry(&cut).unwrap().events().len();

    let handled = cut::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    // Nothing is cut; the model is asked to take the claim out of the sentence.
    assert_eq!(handled, 0);
    assert_eq!(context["candidate"]["blocks"][0]["body"], body);
    let failed = section_failures(&context, 0);
    assert_eq!(claim_ids(&failed), vec![cut.clone()]);
    assert!(failed[0].get("lastAttempt").is_none(), "{}", failed[0]);
    assert_eq!(failed[0]["goal"], goal_words(Goal::KeepTheOthers));
    // Neither a step nor a repair is recorded for it until the pass that reads
    // the sentence, and its repair is the narrowing.
    assert_eq!(ledger.entry(&cut).unwrap().events().len(), recorded);
    let record = ledger
        .pending
        .repairs
        .iter()
        .find(|record| record.claim_id == cut)
        .unwrap();
    assert_eq!(record.goal(), Some(Goal::KeepTheOthers));
    Ok(())
}

#[tokio::test]
async fn a_statement_whose_narrowing_was_not_applied_keeps_its_sentence_and_is_published_not_verified(
) -> Result<()> {
    let body = format!("Intro line. {GLAZE} Closing line.");
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let mut ledger = failed_ledger(
        &llm,
        &body,
        &[("cut", GLAZE, 0), ("intro", "Intro line.", 0)],
    )
    .await?;
    passed(&mut ledger, &llm, "beside", (GLAZE, GLAZE_STATES), &body).await?;
    let cut = ClaimId("cut".into());
    at_the_cut(&mut ledger, &cut).await?;
    narrowing_spent(&mut ledger, "cut", GLAZE).await?;
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &lesson_of(&body),
            std::slice::from_ref(&cut),
        )
        .await?;
    // The narrowing is not asked again, and nothing rides on the cut.
    assert!(ledger
        .pending
        .repairs
        .iter()
        .all(|record| record.goal().is_none() && record.rides_on.is_none()));
    let mut context = json!({
        "candidate": lesson_of(&body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"cut","sentence":GLAZE,"evidence":[],"lastAttempt":true},
        ],
    });

    cut::last_attempts(
        &StandardPolicy::new(),
        crate::lessons::shape::policy(),
        &mut context,
        &mut ledger,
    )
    .await?;

    // The sentence stays with its neighbour's words; the statement is published
    // not verified.
    assert_eq!(context["candidate"]["blocks"][0]["body"], body);
    let entry = ledger.entry(&cut).unwrap();
    assert!(
        matches!(entry.state(), ClaimState::Unverified { .. }),
        "{:?}",
        entry.state()
    );
    let events = entry.events();
    assert!(
        events.iter().any(|record| matches!(
            &record.event,
            ClaimEvent::Stepped {
                step: Step::Cut,
                outcome: Outcome::CannotCut,
                ..
            }
        )),
        "{events:?}"
    );
    Ok(())
}

/// Brings the Sourced statements `ids` to `s.rewrite`: their readings found
/// nothing.
async fn to_the_rewrite(ledger: &mut ClaimLedger, ids: &[&ClaimId]) -> Result<()> {
    let policy = StandardPolicy::new();
    for id in ids {
        let sorted = ClaimEvent::Classified {
            class: Class::Sourced,
            by: ClassifiedBy::Writer,
        };
        ledger.append(id, sorted).await?;
        for (step, outcome) in [
            (Step::Read(Reading::Parts), Outcome::Unproven),
            (Step::Research, Outcome::FoundNothing),
            (Step::Read(Reading::Page), Outcome::Refuted),
        ] {
            assert!(ledger.record_step(&policy, id, step, outcome).await?);
        }
        assert_eq!(node_of(ledger, id), Some("s.rewrite"));
    }
    Ok(())
}

/// The `(attempt, escalated)` of a claim's repairs, in order.
fn attempts(entry: &ClaimLedgerEntry) -> Vec<(u8, bool)> {
    entry
        .events()
        .iter()
        .filter_map(|record| match &record.event {
            ClaimEvent::RepairAttempted {
                attempt, escalated, ..
            } => Some((*attempt, *escalated)),
            _ => None,
        })
        .collect()
}

fn missing_edit() -> String {
    json!({"edits":[{"path":"/blocks/0/body","before":"Words the section never had.","after":"Anything."}]}).to_string()
}

#[tokio::test]
async fn a_claim_repair_whose_edits_never_apply_charges_its_claims_and_the_lesson_goes_on(
) -> Result<()> {
    let body = format!("{FIRST_OLD} {STEADY} {THIRD_OLD} {CLOSING}");
    let llm = Recording {
        answers: Mutex::new((0..4).map(|_| missing_edit()).collect()),
        sent: Mutex::new(Vec::new()),
    };
    let mut ledger = failed_ledger(
        &llm,
        &body,
        &[("first", FIRST_OLD, 0), ("third", THIRD_OLD, 0)],
    )
    .await?;
    let policy = StandardPolicy::new();
    let (first, third) = (ClaimId("first".into()), ClaimId("third".into()));
    to_the_rewrite(&mut ledger, &[&first, &third]).await?;
    ledger
        .begin_repairs(&policy, &lesson_of(&body), &[first.clone(), third.clone()])
        .await?;
    let context = json!({
        "candidate": lesson_of(&body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"first","sentence":FIRST_OLD,"evidence":[]},
            {"unit":0,"claimId":"third","sentence":THIRD_OLD,"evidence":[]},
        ],
    });
    let schema = json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object"}},"questions":{"type":"array","items":{"type":"object"}}}});
    let references = ReferenceCollection::lexical(&[])?;

    let repaired = repair_sections(
        &llm,
        &policy,
        "{}",
        &schema,
        context,
        4_000,
        &references,
        &mut ledger,
    )
    .await;

    assert!(repaired.is_ok(), "{:?}", repaired.err());
    for id in [&first, &third] {
        let reasons = rejections(ledger.entry(id).unwrap());
        assert!(
            matches!(reasons.as_slice(), [reason] if reason.contains("does not occur")),
            "one used attempt that quotes why the edits could not be applied: {reasons:?}"
        );
    }
    let sent = llm.sent.lock().unwrap().clone();
    assert!(sent.len() >= 2, "{} requests", sent.len());
    let second = &sent[1];
    assert!(
        second
            .json_schema
            .as_ref()
            .is_some_and(|schema| schema.pointer("/properties/edits").is_some()),
        "the correction turn asks for edits again: {:?}",
        second.json_schema
    );
    assert!(
        messages(second).contains(&("assistant", missing_edit().as_str())),
        "the correction turn carries the first answer: {:?}",
        messages(second)
    );
    Ok(())
}

/// `Recording` with a window of its own.
struct SmallWindowRecording(Recording, usize);

#[async_trait::async_trait]
impl LLMPort for SmallWindowRecording {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.0.complete(request).await
    }
    fn model_name(&self) -> &str {
        "recording"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        self.1
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

const ALPHA: &str = "Alpha runs first.";
const BETA: &str = "Beta runs second.";
const ALPHA_AND_BETA: &str = "Alpha and beta run together.";

#[tokio::test]
async fn a_correction_that_changes_another_groups_failed_sentence_is_put_back() -> Result<()> {
    let body = format!("{ALPHA} {BETA} {STEADY}");
    let merging = json!({"edits":[
        {"path":"/blocks/0/body","before":format!("{ALPHA} {BETA}"),"after":ALPHA_AND_BETA},
    ]});
    let llm = SmallWindowRecording(
        Recording {
            answers: Mutex::new([merging.to_string(), json!({"edits":[]}).to_string()].into()),
            sent: Mutex::new(Vec::new()),
        },
        4_000,
    );
    let mut ledger = failed_ledger(&llm, &body, &[("alpha", ALPHA, 0), ("beta", BETA, 0)]).await?;
    let policy = StandardPolicy::new();
    let (alpha, beta) = (ClaimId("alpha".into()), ClaimId("beta".into()));
    to_the_rewrite(&mut ledger, &[&alpha, &beta]).await?;
    ledger
        .begin_repairs(&policy, &lesson_of(&body), &[alpha.clone(), beta.clone()])
        .await?;
    // Each claim's passage fills the request budget, so the groups are apart.
    let passage = |id: &str| json!({"id":id,"sourceId":"s","startByte":0,"endByte":6000,"text":"x".repeat(6000)});
    let context = json!({
        "candidate": lesson_of(&body), "requirements": {},
        "evidence": [passage("ev-a"), passage("ev-b")],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"alpha","statement":"Alpha statement","evidence":["ev-a"]},
            {"unit":0,"claimId":"beta","statement":"Beta statement","evidence":["ev-b"]},
        ],
    });
    let schema = json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object"}},"questions":{"type":"array","items":{"type":"object"}}}});
    let references = ReferenceCollection::lexical(&[])?;

    let repaired = repair_sections(
        &llm,
        &policy,
        "{}",
        &schema,
        context,
        500,
        &references,
        &mut ledger,
    )
    .await?;

    let after = repaired["blocks"][0]["body"].as_str().unwrap();
    assert!(after.contains(BETA), "{after}");
    assert!(!after.contains(ALPHA_AND_BETA), "{after}");
    let reasons = rejections(ledger.entry(&alpha).unwrap());
    assert!(
        matches!(reasons.as_slice(), [reason] if reason.contains("a sentence it was not asked to change")),
        "{reasons:?}"
    );
    let sent = llm.0.sent.lock().unwrap().clone();
    assert!(sent.len() >= 2, "{} requests", sent.len());
    assert!(
        messages(&sent[1])
            .iter()
            .any(|(_, text)| text.contains(BETA)),
        "the second group's request holds its sentence as it was"
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_cut_after_two_rewrites_that_were_not_applied_records_its_third_repair_as_the_last(
) -> Result<()> {
    let body = format!("{FIRST_OLD} {STEADY}");
    let llm = Recording {
        answers: Mutex::new(Default::default()),
        sent: Mutex::new(Vec::new()),
    };
    let mut ledger = failed_ledger(&llm, &body, &[("only", FIRST_OLD, 0)]).await?;
    let policy = StandardPolicy::new();
    let only = ClaimId("only".into());
    to_the_rewrite(&mut ledger, &[&only]).await?;
    let lesson = lesson_of(&body);

    for next in ["s.rewrite.again", "s.cut"] {
        ledger
            .begin_repairs(&policy, &lesson, std::slice::from_ref(&only))
            .await?;
        ledger
            .reject(&policy, std::slice::from_ref(&only), "Nothing usable.")
            .await?;
        assert_eq!(node_of(&ledger, &only), Some(next));
    }
    ledger
        .begin_repairs(&policy, &lesson, std::slice::from_ref(&only))
        .await?;
    ledger
        .cut(&policy, std::slice::from_ref(&only), Outcome::Cut)
        .await?;

    assert_eq!(
        attempts(ledger.entry(&only).unwrap()),
        [(1, false), (2, false), (3, true)]
    );
    Ok(())
}
