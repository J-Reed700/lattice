#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::super::sections::Gap;
use super::super::tests::repair_script;
use super::*;
use crate::lessons::course_generation_tests::ScriptedModel;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::domain::learning::acceptance::{Outcome, Reading, StandardPolicy, Step};
use lattice_core::domain::learning::claim_review::{
    Actor, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation,
    ClaimState, Class, ClassifiedBy, EvidenceFingerprint, RepairOutcome, UnitKind, VerdictKind,
};
use std::sync::Mutex;

/// What a scripted answer says to make the application refuse it: a sentence replaced by blank text.
const BLANK_TEXT: &str = "blank-text";

/// The task of the section repair a request asks, the last in its conversation.
fn task_in(request: &CompletionRequest) -> Option<Value> {
    messages(request)
        .iter()
        .filter(|(role, _)| *role == "user")
        .filter_map(|(_, text)| repair_script::task_of(text))
        .last()
}

/// A scripted answer in the form the request asks for: flat edits become entries.
fn scripted(request: &CompletionRequest, answer: String) -> String {
    let Some(task) = task_in(request) else {
        return answer;
    };
    if answer == BLANK_TEXT {
        let row = &task["failedClaims"][0];
        return json!({"entries":[{"of":row["entry"],"edits":{
            "sentences":{"replace":[{"at":row["sentence"][0],"with":{"text":"   ","from":[]}}],"insert":[],"remove":[],"move":[]},
            "cite":[],"recite":[],"uncite":[],"keys":[]}}]})
        .to_string();
    }
    match serde_json::from_str::<Value>(&answer) {
        Ok(flat @ Value::Object(_)) => repair_script::answer(&task, &flat),
        _ => answer,
    }
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
        Ok(CompletionResponse::from_text(&scripted(request, answer)))
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

fn recording(answers: Vec<String>) -> Recording {
    Recording {
        answers: Mutex::new(answers.into()),
        sent: Mutex::new(Vec::new()),
    }
}

fn messages(request: &CompletionRequest) -> Vec<(&str, &str)> {
    request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { role, content } => Some((role.as_str(), content.as_str())),
            _ => None,
        })
        .collect()
}

/// A small window that records each request and edits the markers of the
/// claims it is sent.
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
        requests.push((system.to_owned(), user.to_owned()));
        let sent: Value = serde_json::from_str(user).unwrap_or(Value::Null);
        let edits: Vec<Value> = sent["failedClaims"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|claim| claim["statement"].as_str()?.strip_prefix("Claim "))
            .map(|k| json!({"path":"/blocks/0/body","before":format!("[{k}]"),"after":format!("({k})")}))
            .collect();
        Ok(
            lattice_core::application::ports::llm_port::CompletionResponse::from_text(scripted(
                request,
                json!({ "edits": edits }).to_string(),
            )),
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

fn schema() -> Value {
    json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object"}},"questions":{"type":"array","items":{"type":"object"}}}})
}

fn citing_schema() -> Value {
    json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object","properties":{"citations":{"type":"array","items":{"type":"object"}}}}},"questions":{"type":"array","items":{"type":"object"}}}})
}

fn passage_row(id: &str, text: &str) -> Value {
    json!({"id":id,"sourceId":"s","sourceTitle":"The kiln guide","sourceIndex":0,"startByte":0,"endByte":text.len(),"text":text})
}

fn failed_row(id: &str, sentence: &str, evidence: &[&str]) -> Value {
    json!({"unit":0,"claimId":id,"statement":format!("Statement of {id}"),"sentence":sentence,"evidence":evidence})
}

fn context_of(body: &str, failed: Vec<Value>, evidence: Vec<Value>) -> Value {
    json!({"candidate":lesson_of(body),"requirements":{},"evidence":evidence,"executions":[],"repairHistory":[],"failedClaims":failed})
}

/// A ledger whose claims `claims` (id, sentence) wait for a rewrite of `body`.
async fn rewriting(llm: &dyn LLMPort, body: &str, claims: &[(&str, &str)]) -> Result<ClaimLedger> {
    let spent: Vec<(&str, &str, u8)> = claims.iter().map(|(id, said)| (*id, *said, 0)).collect();
    let mut ledger = failed_ledger(llm, body, &spent).await?;
    let ids: Vec<ClaimId> = claims.iter().map(|(id, _)| ClaimId((*id).into())).collect();
    to_the_rewrite(&mut ledger, &ids.iter().collect::<Vec<_>>()).await?;
    ledger
        .begin_repairs(&StandardPolicy::new(), &lesson_of(body), &ids)
        .await?;
    Ok(ledger)
}

async fn repaired(
    llm: &dyn LLMPort,
    ledger: &mut ClaimLedger,
    (context, schema): (Value, Value),
    window: usize,
) -> Result<Value> {
    let references = ReferenceCollection::lexical(&[])?;
    repair_sections(
        llm,
        &StandardPolicy::new(),
        "{}",
        &schema,
        context,
        window,
        &references,
        ledger,
    )
    .await
}

#[tokio::test]
async fn the_failed_claims_of_one_sentence_are_one_entry_and_their_evidence_is_held_once(
) -> Result<()> {
    let body = format!("{FIRST_OLD} {STEADY}");
    let llm = recording(Vec::new());
    let ledger = rewriting(
        &llm,
        &body,
        &[("a", FIRST_OLD), ("b", FIRST_OLD), ("c", STEADY)],
    )
    .await?;
    let mut context = context_of(
        &body,
        vec![
            failed_row("a", FIRST_OLD, &["ev-1", "ev-2"]),
            failed_row("b", FIRST_OLD, &["ev-2"]),
            failed_row("c", STEADY, &["ev-1"]),
        ],
        vec![
            passage_row("ev-1", "Clay loses water below red heat."),
            passage_row("ev-2", "A slow ramp keeps the body whole."),
        ],
    );
    context["candidate"]["blocks"] = json!([
        {"kind":"explanation","title":"T","body":body},
        {"kind":"explanation","title":"Other","body":"Another section entirely."}
    ]);
    context["repairHistory"] = json!([
        {"unit":0,"statement":"Rejected here"},
        {"unit":1,"statement":"Rejected elsewhere"}
    ]);
    let candidate = context["candidate"].clone();

    let input = section_input(
        &llm,
        &ledger,
        (&context, &candidate),
        (0, "/blocks/0"),
        (&Default::default(), false, 4_000),
    )?;

    assert_eq!(input.items.len(), 2);
    assert_eq!(input.items[0].claims.len(), 2);
    assert_eq!(input.items[0].needs(), vec![0, 1]);
    assert_eq!(input.items[1].needs(), vec![0]);
    assert_eq!(input.passages.len(), 2);
    assert_eq!(input.history.0, vec![json!({"unit":0,"statement":"Rejected here"})]);
    assert_eq!(input.section.0["blocks"][1], Value::Null);
    assert_eq!(input.section.0["blocks"][0]["body"], body);
    Ok(())
}

#[test]
fn the_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = Input {
        requirements: marks.told(json!({"deliverable":"Explain the kiln schedule."})),
        sections: marks.told(vec![
            json!({"kind":"explanation","title":"Firing schedule"}),
        ]),
        section: marks.told(json!({"blocks":[{"kind":"explanation","title":"Firing schedule","body":"Water leaves the clay first."}],"questions":[]})),
        lesson: marks.internal(json!({"blocks":[{"kind":"hidden-kind","title":"Hidden lesson","body":"Hidden words stay home."}],"questions":[]})),
        part: marks.internal("/blocks/0".to_owned()),
        question: false,
        cites: false,
        items: vec![Failed {
            id: marks.reference("claim-first-secret".to_owned()),
            claims: vec![Claim {
                row: marks.told(json!({"statement":"The kiln fires to cone 04.","verdict":"unsupported","reason":"No passage states it."})),
                evidence: Internal::of(vec![0]),
            }],
            ids: marks.internal(vec!["claim-first-secret".to_owned()]),
            targets: marks.internal(vec![Protected {
                unit: 100_001,
                field: "body-field".into(),
                quote: "Quote secret words.".into(),
                released: false,
            }]),
            protected: marks.internal(Vec::<Protected>::new()),
            shown: Internal::of(None),
            execution: None,
        }],
        passages: Packed::new(
            vec![Passage {
                id: marks.reference("evidence-secret-1".to_owned()),
                title: Some(marks.told("The kiln guide".to_owned())),
                text: marks.told("Tides turn twice.".to_owned()),
                source_index: Internal::of(Some(0)),
            }],
            |_| 10,
        ),
        history: marks.told(vec![json!({"statement":"An earlier rejected claim."})]),
        wording: Internal::of(Default::default()),
        output_tokens: 500,
    };

    holds::<SectionRepair>(&input, &marks);
}

#[tokio::test]
async fn one_request_corrects_every_failed_sentence_of_a_section_and_shows_shared_evidence_once(
) -> Result<()> {
    let body = format!("{FIRST_OLD} {STEADY} {THIRD_OLD}");
    let edits = json!({"edits":[
        {"path":"/blocks/0/body","before":FIRST_OLD,"after":FIRST_NEW},
        {"path":"/blocks/0/body","before":THIRD_OLD,"after":THIRD_NEW},
    ]});
    let llm = recording(vec![edits.to_string()]);
    let mut ledger = rewriting(&llm, &body, &[("a", FIRST_OLD), ("b", FIRST_OLD), ("c", THIRD_OLD)]).await?;
    let context = context_of(
        &body,
        vec![
            failed_row("a", FIRST_OLD, &["ev-1"]),
            failed_row("b", FIRST_OLD, &["ev-1"]),
            failed_row("c", THIRD_OLD, &["ev-1"]),
        ],
        vec![passage_row("ev-1", "Clay loses water below red heat.")],
    );

    let after = repaired(&llm, &mut ledger, (context, schema()), 4_000).await?;

    let body = after["blocks"][0]["body"].as_str().unwrap();
    assert_eq!(body, format!("{FIRST_NEW} {STEADY} {THIRD_NEW}"));
    let sent = llm.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1, "one request for the section");
    let task = task_in(&sent[0]).unwrap();
    assert_eq!(task["evidence"].as_array().unwrap().len(), 1);
    assert_eq!(task["failedClaims"].as_array().unwrap().len(), 3);
    let said = messages(&sent[0]).iter().map(|(_, text)| *text).collect::<String>();
    for id in ["claimId", "ev-1", "\"unit\""] {
        assert!(!said.contains(id), "{id} reached the request");
    }
    for id in ["a", "b", "c"] {
        assert!(rejections(ledger.entry(&ClaimId(id.into())).unwrap()).is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn an_entry_the_model_leaves_out_costs_only_its_own_claims() -> Result<()> {
    let body = format!("{FIRST_OLD} {STEADY} {THIRD_OLD}");
    let first_only = json!({"only":[0],"edits":[
        {"path":"/blocks/0/body","before":FIRST_OLD,"after":FIRST_NEW},
    ]});
    let llm = recording(vec![first_only.to_string()]);
    let mut ledger = rewriting(&llm, &body, &[("a", FIRST_OLD), ("c", THIRD_OLD)]).await?;
    let context = context_of(
        &body,
        vec![failed_row("a", FIRST_OLD, &[]), failed_row("c", THIRD_OLD, &[])],
        Vec::new(),
    );

    let after = repaired(&llm, &mut ledger, (context, schema()), 4_000).await?;

    let body = after["blocks"][0]["body"].as_str().unwrap();
    assert_eq!(body, format!("{FIRST_NEW} {STEADY} {THIRD_OLD}"));
    assert!(rejections(ledger.entry(&ClaimId("a".into())).unwrap()).is_empty());
    assert_eq!(rejections(ledger.entry(&ClaimId("c".into())).unwrap()).len(), 1);
    assert!(
        llm.sent.lock().unwrap().len() >= 2,
        "the entry left out is asked again alone"
    );
    Ok(())
}

#[tokio::test]
async fn an_entry_the_application_refuses_is_asked_again_alone_in_the_same_conversation(
) -> Result<()> {
    let body = format!("{FIRST_OLD} {STEADY}");
    let corrected = json!({"edits":[{"path":"/blocks/0/body","before":FIRST_OLD,"after":FIRST_NEW}]});
    let llm = recording(vec![BLANK_TEXT.to_owned(), corrected.to_string()]);
    let mut ledger = rewriting(&llm, &body, &[("a", FIRST_OLD)]).await?;
    let context = context_of(&body, vec![failed_row("a", FIRST_OLD, &[])], Vec::new());

    let after = repaired(&llm, &mut ledger, (context, schema()), 4_000).await?;

    assert_eq!(after["blocks"][0]["body"], format!("{FIRST_NEW} {STEADY}"));
    assert!(rejections(ledger.entry(&ClaimId("a".into())).unwrap()).is_empty());
    let sent = llm.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 2, "the entry, then its correction");
    let (first, second) = (messages(&sent[0]), messages(&sent[1]));
    assert_eq!(second[..first.len()], first[..]);
    let (role, asked) = second[first.len() + 1];
    assert_eq!(role, "user");
    assert!(asked.contains("is empty"), "{asked}");
    Ok(())
}

#[tokio::test]
async fn a_change_to_any_sentence_but_the_failed_one_is_discarded() -> Result<()> {
    let body = format!("{FIRST_OLD} {STEADY} {PASSED}");
    let edits = json!({"edits":[
        {"path":"/blocks/0/body","before":FIRST_OLD,"after":FIRST_NEW},
        {"path":"/blocks/0/body","before":STEADY,"after":"A ramp of any kind will do."},
        {"path":"/blocks/0/body","before":PASSED,"after":"The second stage does nothing."},
    ]});
    let llm = recording(vec![edits.to_string()]);
    let mut ledger = rewriting(&llm, &body, &[("a", FIRST_OLD)]).await?;
    passed(&mut ledger, &llm, "kept", (PASSED, PASSED), &body).await?;
    let context = context_of(&body, vec![failed_row("a", FIRST_OLD, &[])], Vec::new());

    let after = repaired(&llm, &mut ledger, (context, schema()), 4_000).await?;

    assert_eq!(
        after["blocks"][0]["body"],
        format!("{FIRST_NEW} {STEADY} {PASSED}")
    );
    assert!(rejections(ledger.entry(&ClaimId("a".into())).unwrap()).is_empty());
    Ok(())
}

#[tokio::test]
async fn an_entry_that_changes_nothing_it_may_change_is_charged() -> Result<()> {
    let body = format!("{FIRST_OLD} {STEADY}");
    let outside = json!({"edits":[
        {"path":"/blocks/0/body","before":STEADY,"after":"A ramp of any kind will do."},
    ]});
    let llm = recording(vec![outside.to_string()]);
    let mut ledger = rewriting(&llm, &body, &[("a", FIRST_OLD)]).await?;
    let context = context_of(&body, vec![failed_row("a", FIRST_OLD, &[])], Vec::new());

    let after = repaired(&llm, &mut ledger, (context, schema()), 4_000).await?;

    assert_eq!(after["blocks"][0]["body"], body);
    let reasons = rejections(ledger.entry(&ClaimId("a".into())).unwrap());
    assert!(
        matches!(reasons.as_slice(), [reason] if reason == NO_CHANGE),
        "{reasons:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_claim_whose_evidence_does_not_fit_the_window_is_charged_and_no_request_is_sent(
) -> Result<()> {
    let body = format!("{FIRST_OLD} {STEADY}");
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let mut ledger = rewriting(&llm, &body, &[("a", FIRST_OLD)]).await?;
    let context = context_of(
        &body,
        vec![failed_row("a", FIRST_OLD, &["ev-big"])],
        vec![passage_row("ev-big", &"x".repeat(24_000))],
    );

    repaired(&llm, &mut ledger, (context, schema()), 500).await?;

    assert_eq!(
        rejections(ledger.entry(&ClaimId("a".into())).unwrap()),
        vec![NO_ROOM.to_owned()]
    );
    assert!(llm.requests.lock().unwrap().is_empty());
    Ok(())
}

#[test]
fn the_system_prompt_gives_its_rules_in_order_and_cites_only_when_the_lesson_does() {
    use text_edits::{NEVER_SAY_WHERE, QUOTED_WORDING, STEPS_IN_ORDER, UNQUOTED_REMARKS};
    let order = |system: &str, rules: &[&str]| -> Vec<usize> {
        rules
            .iter()
            .map(|rule| system.find(rule).unwrap_or_else(|| panic!("{rule}")))
            .collect()
    };
    let citing = words::citing_evidence();
    let rich = system(true, true);
    let at = order(
        &rich,
        &[
            CONFINEMENT,
            words::SENTENCES,
            words::KEYS,
            words::EDITING,
            citing.as_str(),
            NEVER_SAY_WHERE,
            QUOTED_WORDING,
            UNQUOTED_REMARKS,
            STEPS_IN_ORDER,
        ],
    );
    assert!(at.windows(2).all(|pair| pair[0] < pair[1]), "{at:?}");
    assert!(!rich.contains(words::NO_CITATIONS));

    let plain = system(false, false);
    assert!(plain.contains(words::NO_CITATIONS));
    assert!(plain.contains(words::EDITING));
    assert!(!plain.contains(words::KEYS));
    assert!(!plain.contains(&citing));
    assert!(!plain.contains(NEVER_SAY_WHERE));
}

#[tokio::test]
async fn a_section_too_large_for_one_request_is_repaired_in_several_that_fit() -> Result<()> {
    let passages: Vec<_> = (0..12)
        .map(|i| json!({"id":format!("evidence-{i}"),"sourceId":"s","startByte":i * 2000,"endByte":i * 2000 + 1600,"text":format!("passage-{i} {}", "x".repeat(1600))}))
        .collect();
    // Each failed claim is a ledger claim located on its own marker: a
    // repair may change only the failed claims' sentences (claim sources
    // design §10.2), so the edits must land on sentences it targets.
    let body = (0..6)
        .map(|k| format!("Claim [{k}] holds."))
        .collect::<Vec<_>>()
        .join(" ");
    let claims: Vec<_> = (0..6)
        .map(|k| json!({"unit":0,"claimId":format!("claim-{k}"),"statement":format!("Claim {k}"),"sentence":format!("Claim [{k}] holds."),"verdict":"unsupported","reason":"Unstated.","evidence":[format!("evidence-{}", 2 * k), format!("evidence-{}", 2 * k + 1)]}))
        .collect();
    let context = json!({"candidate":{"blocks":[{"kind":"explanation","title":"T","body":body}],"questions":[]},"requirements":{},"failedClaims":claims,"evidence":passages,"executions":[],"repairHistory":[]});
    let output_tokens = 500;
    let llm = SmallWindow {
        window: 4_000,
        requests: Mutex::new(Vec::new()),
    };
    let mut ledger = ClaimLedger::open(&llm, "{}").await?;
    // Open for repair, as a pass leaves its failed claims.
    for k in 0..6 {
        let id = ClaimId(format!("claim-{k}"));
        let location = ClaimLocation::locate(0, UnitKind::Block, "body", format!("[{k}]"), &body);
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
        to_the_rewrite(&mut ledger, &[&id]).await?;
        assert!(ledger.may_repair(&StandardPolicy::new(), &id));
    }

    let after = repaired(&llm, &mut ledger, (context, schema()), output_tokens).await?;

    let requests = llm.requests.lock().unwrap();
    assert!(requests.len() >= 3, "{} requests", requests.len());
    for (system, user) in requests.iter() {
        assert!(llm.count_tokens(system) + llm.count_tokens(user) + output_tokens <= 4_000);
        // Each request carries exactly the evidence its own claims cite.
        let local: Value = serde_json::from_str(user)?;
        let cited: BTreeSet<String> = local["failedClaims"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|claim| claim["evidence"].as_array().unwrap().clone())
            .filter_map(|handle| handle.as_str().map(str::to_owned))
            .collect();
        let carried: BTreeSet<String> = local["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|passage| passage["passage"].as_str().map(str::to_owned))
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
    let body = after["blocks"][0]["body"].as_str().unwrap();
    for k in 0..6 {
        assert!(body.contains(&format!("({k})")), "{body}");
        assert!(!body.contains(&format!("[{k}]")), "{body}");
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
    at_the_cut(&mut ledger, &last).await?;
    to_the_rewrite(&mut ledger, &[&first]).await?;
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
    assert!(entry.events().iter().any(|record| matches!(
        record.event,
        ClaimEvent::Stepped {
            step: Step::Cut,
            outcome: Outcome::Cut,
            ..
        }
    )));
    assert!(!ledger
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
    let run = ClaimEvent::CodeExample {
        observation: "example-0".into(),
    };
    assert!(ledger.append(&example, run).await?);
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
    at_the_cut(&mut ledger, &stating).await?;
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
    let system = system(false, false);

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
    let stating = ClaimId("stating".into());
    let location = ClaimLocation::locate(0, UnitKind::Block, "body", passage, body);
    let extracted = ClaimEvent::Extracted {
        location,
        statement: statement.to_owned(),
    };
    ledger.append(&stating, extracted).await?;
    at_the_cut(&mut ledger, &stating).await?;
    if spent {
        narrowing_spent(&mut ledger, "stating", passage).await?;
    }
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


const GUIDE_SENTENCE: &str =
    "The guide says \"a timeout of 30 seconds applies to each request\" before it fails.";
const GUIDE_QUOTE: &str = "A timeout of 30 seconds applies to each request.";

fn guide_answer() -> Value {
    json!({
        "edits":[{"path":"/blocks/0/body","before":"Fix this.","after":GUIDE_SENTENCE}],
        "citations":[{"path":"/blocks/0","sentence":GUIDE_SENTENCE,"sourceIndex":0,"quote":GUIDE_QUOTE}]
    })
}

async fn guide_repair(llm: &dyn LLMPort) -> Result<Value> {
    let body = "Keep this. Fix this.";
    let mut ledger = rewriting(llm, body, &[("fix", "Fix this.")]).await?;
    passed(&mut ledger, llm, "kept", ("Keep this.", "Keep this."), body).await?;
    let mut context = context_of(
        body,
        vec![failed_row("fix", "Fix this.", &["ev-guide"])],
        vec![passage_row(
            "ev-guide",
            &format!("{GUIDE_QUOTE} The guide ends here."),
        )],
    );
    context["candidate"]["blocks"][0]["citations"] = json!([]);
    repaired(llm, &mut ledger, (context, citing_schema()), 4_000).await
}

/// A rewrite to the wording an evidence passage states cites that passage in
/// the same response, so the final structural pass finds nothing uncited in
/// it; the citation the entry names is stored as the application wrote it.
#[tokio::test]
async fn a_repaired_sentence_cites_the_passage_it_was_given_and_leaves_no_uncited_specific(
) -> Result<()> {
    let model = ScriptedModel {
        outputs: Mutex::new(vec![guide_answer().to_string()].into()),
        prompts: Mutex::new(Vec::new()),
    };

    let after = guide_repair(&model).await?;

    let section = &after["blocks"][0];
    assert_eq!(section["body"], format!("Keep this. {GUIDE_SENTENCE}"));
    assert_eq!(
        section["citations"],
        json!([{"sentence":GUIDE_SENTENCE,"sourceIndex":0,"quote":GUIDE_QUOTE}])
    );
    let uncited = |lesson: &Value| {
        crate::lessons::shape::policy()
            .rules(lesson, crate::lessons::shape::Phase::Draft)
            .into_iter()
            .filter(|finding| finding.kind == crate::review_findings::FindingKind::Quotation)
            .count()
    };
    assert_eq!(uncited(&after), 0);
    // The same rewrite without its citation is what the pass would list.
    let mut bare = after.clone();
    bare["blocks"][0]["citations"] = json!([]);
    assert_eq!(uncited(&bare), 1);
    Ok(())
}

#[tokio::test]
async fn the_citing_request_names_the_evidence_sentences_a_citation_may_quote() -> Result<()> {
    let model = recording(vec![guide_answer().to_string()]);

    guide_repair(&model).await?;

    let sent = model.sent.lock().unwrap().clone();
    let task = task_in(&sent[0]).unwrap();
    let text = task["evidence"][0]["text"].as_str().unwrap();
    assert!(text.contains(&format!("] {GUIDE_QUOTE}")), "{text}");
    assert!(task["failedClaims"][0]["evidence"][0].is_string());
    Ok(())
}

#[tokio::test]
async fn prompt_snapshot_section_repair() -> Result<()> {
    let inner = ScriptedModel {
        outputs: Mutex::new(vec![guide_answer().to_string()].into()),
        prompts: Mutex::new(Vec::new()),
    };
    let model = crate::lessons::course_generation_tests::RequestLog::new(&inner);

    guide_repair(&model).await?;

    model.assert_snapshot::<SectionRepair>();
    Ok(())
}

#[tokio::test]
async fn a_correction_that_changes_a_sentence_a_passed_claim_shares_costs_only_its_own_claim(
) -> Result<()> {
    const THIRD_QUOTE: &str = "The third stage vitrifies the body";
    const KEPT_QUOTE: &str = "the cooling must stay slow";
    const MIXED: &str = "The third stage vitrifies the body, and the cooling must stay slow.";
    const MIXED_NEW: &str = "The third stage turns the body to glass, and the cooling must stay slow.";
    let body = format!("{FIRST_OLD} {STEADY} {MIXED} {CLOSING}");
    // Both targets are rewritten; the third is rewritten with the words of a
    // passed claim that stands in the same sentence.
    let both = json!({"edits":[
        {"path":"/blocks/0/body","before":FIRST_OLD,"after":FIRST_NEW},
        {"path":"/blocks/0/body","before":MIXED,"after":MIXED_NEW},
    ]});
    let llm = recording(vec![both.to_string()]);
    let mut ledger = failed_ledger(
        &llm,
        &body,
        &[("first", FIRST_OLD, 0), ("third", THIRD_QUOTE, 0)],
    )
    .await?;
    passed(&mut ledger, &llm, "passed", (KEPT_QUOTE, KEPT_QUOTE), &body).await?;
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
    let context = context_of(
        &body,
        vec![
            failed_row("first", FIRST_OLD, &[]),
            failed_row("third", THIRD_QUOTE, &[]),
        ],
        Vec::new(),
    );

    let after = repaired(&llm, &mut ledger, (context, schema()), 4_000).await?;

    let after = after["blocks"][0]["body"].as_str().unwrap();
    assert!(
        after.contains(FIRST_NEW) && after.contains(MIXED) && !after.contains(MIXED_NEW),
        "{after}"
    );
    // The put-back correction is charged to its claim alone, which goes on to
    // its retry, told why.
    assert_eq!(node_of(&ledger, &third), Some("s.rewrite.again"));
    let reasons = rejections(ledger.entry(&third).unwrap());
    assert!(
        matches!(reasons.as_slice(), [reason] if reason.contains(KEPT_QUOTE) && reason.contains("put back")),
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
async fn a_claim_repair_whose_answers_cannot_be_read_charges_its_claims_and_the_lesson_goes_on(
) -> Result<()> {
    let body = format!("{FIRST_OLD} {STEADY} {THIRD_OLD} {CLOSING}");
    let llm = recording((0..8).map(|_| "no entries here".to_owned()).collect());
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
    let context = context_of(
        &body,
        vec![
            failed_row("first", FIRST_OLD, &[]),
            failed_row("third", THIRD_OLD, &[]),
        ],
        Vec::new(),
    );

    let after = repaired(&llm, &mut ledger, (context, schema()), 4_000).await;

    assert!(after.is_ok(), "{:?}", after.err());
    for id in [&first, &third] {
        assert_eq!(
            rejections(ledger.entry(id).unwrap()).len(),
            1,
            "one used attempt for the answer that said nothing"
        );
    }
    Ok(())
}

const ALPHA: &str = "Alpha runs first.";
const BETA: &str = "Beta runs second.";
const ALPHA_AND_BETA: &str = "Alpha and beta run together.";

#[tokio::test]
async fn a_correction_that_also_removes_another_failed_sentence_leaves_that_sentence_as_it_was(
) -> Result<()> {
    let body = format!("{ALPHA} {BETA} {STEADY}");
    let merging = json!({"edits":[
        {"path":"/blocks/0/body","before":format!("{ALPHA} {BETA}"),"after":ALPHA_AND_BETA},
    ]});
    let llm = recording(vec![merging.to_string()]);
    let mut ledger = rewriting(&llm, &body, &[("alpha", ALPHA), ("beta", BETA)]).await?;
    let context = context_of(
        &body,
        vec![
            failed_row("alpha", ALPHA, &[]),
            failed_row("beta", BETA, &[]),
        ],
        Vec::new(),
    );

    let after = repaired(&llm, &mut ledger, (context, schema()), 4_000).await?;

    let after = after["blocks"][0]["body"].as_str().unwrap();
    assert_eq!(after, format!("{ALPHA_AND_BETA} {BETA} {STEADY}"));
    assert!(rejections(ledger.entry(&ClaimId("alpha".into())).unwrap()).is_empty());
    let reasons = rejections(ledger.entry(&ClaimId("beta".into())).unwrap());
    assert!(
        matches!(reasons.as_slice(), [reason] if reason == NO_CHANGE),
        "{reasons:?}"
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
