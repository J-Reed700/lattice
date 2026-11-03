#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::super::sections::Gap;
use super::*;
use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::domain::learning::claim_review::{
    Actor, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLocation, ClaimState,
    EvidenceFingerprint, RepairOutcome, UnitKind, VerdictKind,
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
    assert!(merge(&mut result, original.clone(), 1).is_err());
    assert!(merge(
        &mut result,
        json!({"blocks":[{"kind":"recap"}],"questions":[]}),
        1
    )
    .is_err());
    Ok(())
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
async fn a_section_whose_edits_cannot_be_applied_is_asked_for_in_the_same_conversation(
) -> Result<()> {
    let original = json!({"blocks":[{"kind":"explanation","title":"Keep","body":"Keep this. Keep that."}],"questions":[]});
    let local =
        json!({"candidate":original,"failedClaims":[{"unit":0,"statement":"Correct one fact."}]});
    let replacement = json!({"blocks":[{"kind":"explanation","title":"Keep","body":"Keep this. Correct that."}],"questions":[]});
    let ambiguous =
        json!({"edits":[{"path":"/blocks/0/body","before":"Keep","after":"Correct"}]}).to_string();
    let model = Recording {
        answers: Mutex::new([ambiguous.clone(), replacement.to_string()].into()),
        sent: Mutex::new(Vec::new()),
    };
    let schema = json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object"}},"questions":{"type":"array","items":{"type":"object"}}}});
    let target = Protected {
        unit: 0,
        field: "body".into(),
        quote: "Keep that.".into(),
    };

    let repaired = repair_group(
        &model,
        "Repair the section.",
        (&local, &original),
        &schema,
        "blocks",
        4_000,
        (&[target], &[]),
    )
    .await?;

    assert_eq!(repaired, replacement);
    let sent = model.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 2, "the edits, then the section");
    let (edits, section) = (messages(&sent[0]), messages(&sent[1]));
    // The server has just read the edit request, so the section request
    // continues it rather than opening another conversation.
    assert_eq!(section.len(), edits.len() + 2, "{section:?}");
    assert_eq!(section[..edits.len()], edits[..]);
    assert_eq!(section[edits.len()], ("assistant", ambiguous.as_str()));
    let (role, asked) = section[edits.len() + 1];
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
        let (system, user) =
            crate::features::learning::lessons::structured_call::task_messages(request);
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
        assert!(ledger.may_repair(&id));
    }

    let repaired = repair_sections(
        &llm,
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
        .begin_repairs(&lesson_of(body), &[last.clone(), first.clone()])
        .await?;
    let mut context = json!({
        "candidate": lesson_of(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"last","sentence":"Cut this.","evidence":[],"lastAttempt":true},
            {"unit":0,"claimId":"first","sentence":"Fix this.","evidence":[]},
        ],
    });

    assert_eq!(cut::last_attempts(&mut context, &mut ledger).await?, 1);

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
        .begin_repairs(&lesson_of(body), &[last.clone()])
        .await?;
    let mut context = json!({
        "candidate": lesson_of(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"last","sentence":"Cut this.","evidence":[],"lastAttempt":true},
        ],
    });

    assert_eq!(cut::last_attempts(&mut context, &mut ledger).await?, 1);

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
        &[("last", "Cut this.", 1), ("other", "Cut this.", 0)],
    )
    .await?;
    let (last, other) = (ClaimId("last".into()), ClaimId("other".into()));
    ledger
        .begin_repairs(&lesson_of(body), &[last.clone(), other.clone()])
        .await?;
    let mut context = json!({
        "candidate": lesson_of(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"last","sentence":"Cut this.","evidence":[],"lastAttempt":true},
            {"unit":0,"claimId":"other","sentence":"Cut this.","evidence":[]},
        ],
    });

    cut::last_attempts(&mut context, &mut ledger).await?;

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
        .begin_repairs(&lesson_of(body), &[example.clone()])
        .await?;
    let mut context = json!({
        "candidate": lesson_of(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [], "failedClaims": [],
        "exampleClaims": [{"unit":0,"claimId":"example","lastAttempt":true}],
    });

    assert_eq!(cut::last_attempts(&mut context, &mut ledger).await?, 1);

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
            ("last", "Cut this.", 1),
            ("mate", "Cut this.", 0),
            ("kept", "Keep this.", 0),
        ],
    )
    .await?;
    let (last, mate) = (ClaimId("last".into()), ClaimId("mate".into()));
    ledger
        .begin_repairs(&lesson_of(body), &[last.clone(), mate.clone()])
        .await?;
    let mut context = json!({
        "candidate": lesson_of(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"last","sentence":"Cut this.","evidence":[],"lastAttempt":true},
            {"unit":0,"claimId":"mate","sentence":"Cut this.","evidence":[]},
        ],
    });

    assert_eq!(cut::last_attempts(&mut context, &mut ledger).await?, 1);

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
        &[("last", "Cut this.", 1), ("mate", "Cut this.", 0)],
    )
    .await?;
    let (last, mate) = (ClaimId("last".into()), ClaimId("mate".into()));
    ledger
        .begin_repairs(&lesson_of(body), &[last.clone(), mate.clone()])
        .await?;
    let mut context = json!({
        "candidate": lesson_of(body), "requirements": {}, "evidence": [],
        "executions": [], "repairHistory": [],
        "failedClaims": [
            {"unit":0,"claimId":"last","sentence":"Cut this.","evidence":[],"lastAttempt":true},
            {"unit":0,"claimId":"mate","sentence":"Cut this.","evidence":[]},
        ],
    });

    let error = cut::last_attempts(&mut context, &mut ledger)
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

/// A rewrite to the figure an evidence passage states cites that passage in
/// the same response, so the final structural pass finds nothing uncited in
/// it; confinement, which takes back what changed outside the failed
/// sentence, keeps the citation of the sentence it allowed.
#[tokio::test]
async fn a_repaired_sentence_cites_the_passage_it_was_given_and_leaves_no_uncited_specific(
) -> Result<()> {
    let original = json!({"blocks":[{"kind":"explanation","title":"T","body":"Keep this. Fix this.","citations":[]}],"questions":[]});
    let local = json!({"candidate":original,"failedClaims":[{"unit":0,"statement":"Fix this."}]});
    let sentence = "The default timeout is 30 seconds.";
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
    };
    let kept = Protected {
        unit: 0,
        field: "body".into(),
        quote: "Keep this.".into(),
    };

    let repaired = repair_group(
        &model,
        "system",
        (&local, &original),
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
        crate::features::learning::generation::lesson_rules(lesson)
            .into_iter()
            .filter(|finding| {
                finding.kind == crate::features::learning::review_findings::FindingKind::Quotation
            })
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
fn a_section_given_back_no_citation_gets_no_citations_field() {
    let section = json!({"kind":"explanation","body":"Keep this. Fix this."});
    let stray = json!({"sentence":"A sentence the section lacks.","sourceIndex":0,"quote":"x"});

    for appended in [Vec::new(), vec![stray]] {
        let mut confined = section.clone();
        citation::restore_appended(&mut confined, appended, "blocks");
        assert_eq!(confined, section);
    }
}
