use super::*;
use crate::application::ports::llm_port::CompletionResponse;
use crate::shared::error::AppError;
use async_trait::async_trait;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::Mutex;

/// What a scripted model does with its next request.
pub(super) enum Reply {
    Text(String),
    /// A reply, and the output tokens the backend reports for it.
    Counted(String, u64),
    Cut(String),
    Outage,
}

/// Answers each request as scripted and records every one it was sent.
pub(super) struct Scripted {
    replies: Mutex<VecDeque<Reply>>,
    sent: Mutex<Vec<CompletionRequest>>,
}

impl Scripted {
    pub(super) fn new(replies: Vec<Reply>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            sent: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn sent(&self) -> Vec<CompletionRequest> {
        self.sent.lock().unwrap().clone()
    }
}

#[async_trait]
impl LLMPort for Scripted {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.sent.lock().unwrap().push(request.clone());
        match self.replies.lock().unwrap().pop_front() {
            Some(Reply::Text(text)) => Ok(CompletionResponse::from_text(text)),
            Some(Reply::Counted(text, tokens)) => Ok(CompletionResponse {
                output_tokens: tokens,
                ..CompletionResponse::from_text(text)
            }),
            Some(Reply::Cut(text)) => Ok(CompletionResponse {
                finish_reason: "length".into(),
                ..CompletionResponse::from_text(text)
            }),
            Some(Reply::Outage) | None => Err(AppError::ServiceNotAvailable(
                "the model is not running".into(),
            )),
        }
    }
    fn model_name(&self) -> &str {
        "scripted-9b"
    }
    fn max_context_tokens(&self) -> usize {
        8_192
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

pub(super) fn checker(llm: &Scripted, policy: CheckPolicy) -> ClaimChecker<'_> {
    ClaimChecker::new(llm, SamplingOverride::deterministic(), 512, policy)
}

fn passages() -> Vec<String> {
    vec!["Passage zero.".into(), "Passage one.".into()]
}

fn answer(verdict: &str) -> Reply {
    Reply::Text(format!(
        r#"{{"verdict": "{verdict}", "reason": "No passage says otherwise."}}"#
    ))
}

fn judged<T: std::fmt::Debug>(judgment: ClassJudgment<T>) -> T {
    match judgment {
        ClassJudgment::Judged(value) => value,
        other => unreachable!("{other:?}"),
    }
}

/// The system and the user message of a request.
pub(super) fn messages(request: &CompletionRequest) -> (String, String) {
    let content = |wanted: &str| {
        request
            .input
            .iter()
            .find_map(|item| match item {
                CompletionInput::Message { role, content } if role == wanted => {
                    Some(content.clone())
                }
                _ => None,
            })
            .unwrap_or_default()
    };
    (content("system"), content("user"))
}

#[tokio::test]
async fn a_consistency_check_sends_the_prompt_of_record_and_reads_each_verdict() {
    for (word, expected) in [
        ("consistent", ConsistencyVerdict::Consistent),
        ("contradicted", ConsistencyVerdict::Contradicted),
        ("unfounded_specific", ConsistencyVerdict::UnfoundedSpecific),
    ] {
        let llm = Scripted::new(vec![answer(word)]);
        let outcome = judged(
            checker(&llm, CheckPolicy::Consistency)
                .check_consistency("Errors travel forward.", &passages())
                .await,
        );
        assert_eq!(
            outcome,
            ConsistencyOutcome {
                verdict: expected,
                reason: "No passage says otherwise.".into()
            }
        );
        let sent = llm.sent();
        assert_eq!(sent.len(), 1);
        let (system, user) = messages(&sent[0]);
        assert_eq!(system, CONSISTENCY_SYSTEM);
        assert_eq!(
            user,
            "Source passages:\n[passage-0]\nPassage zero.\n\n[passage-1]\nPassage one.\n\nExplanatory statement: Errors travel forward."
        );
        assert_eq!(sent[0].sampling, Some(SamplingOverride::deterministic()));
        assert_eq!(
            sent[0].json_schema.as_ref().unwrap()["properties"]["verdict"]["enum"],
            json!(["consistent", "contradicted", "unfounded_specific"])
        );
    }
}

#[tokio::test]
async fn an_unreadable_answer_is_asked_for_once_more_and_then_left_unchecked() {
    let llm = Scripted::new(vec![Reply::Text("not json".into()), answer("consistent")]);
    let outcome = judged(
        checker(&llm, CheckPolicy::Consistency)
            .check_consistency("S.", &passages())
            .await,
    );
    assert_eq!(outcome.verdict, ConsistencyVerdict::Consistent);
    let sent = llm.sent();
    assert_eq!(sent.len(), 2);
    let (_, again) = messages(&sent[1]);
    assert!(again.contains("could not be read") && again.contains("not json"));

    let unreadable = || Reply::Text("{}".into());
    let unknown = || answer("maybe");
    let cut = || Reply::Cut(r#"{"verdict": "consistent", "reason": "Fine."}"#.into());
    for replies in [
        vec![unreadable(), unreadable()],
        vec![unknown(), unknown()],
        vec![cut(), cut()],
    ] {
        let llm = Scripted::new(replies);
        let judgment = checker(&llm, CheckPolicy::Consistency)
            .check_consistency("S.", &passages())
            .await;
        assert!(matches!(judgment, ClassJudgment::Unusable), "{judgment:?}");
        assert_eq!(llm.sent().len(), 2);
    }
}

#[tokio::test]
async fn an_outage_stays_an_error_and_another_policy_makes_no_call() {
    let llm = Scripted::new(vec![Reply::Outage]);
    let judgment = checker(&llm, CheckPolicy::Consistency)
        .check_consistency("S.", &passages())
        .await;
    assert!(matches!(judgment, ClassJudgment::Failed(_)), "{judgment:?}");
    assert_eq!(llm.sent().len(), 1);

    let llm = Scripted::new(vec![answer("consistent")]);
    let wrong = checker(&llm, CheckPolicy::Strict)
        .check_consistency("S.", &passages())
        .await;
    assert!(matches!(wrong, ClassJudgment::Unusable));
    let wrong = checker(&llm, CheckPolicy::Consistency)
        .check_knowledge("S.")
        .await;
    assert!(matches!(wrong, ClassJudgment::Unusable));
    assert!(llm.sent().is_empty());
}

#[tokio::test]
async fn the_knowledge_judge_asks_three_times_at_temperature_and_keeps_every_sample() {
    let llm = Scripted::new(vec![
        answer("true_common"),
        answer("true_common"),
        answer("needs_source"),
    ]);
    let outcome = judged(
        checker(&llm, CheckPolicy::Knowledge)
            .check_knowledge("OCR reads pixels into text.")
            .await,
    );
    let verdicts: Vec<KnowledgeVerdict> = outcome.samples.iter().map(|s| s.verdict).collect();
    assert_eq!(
        verdicts,
        vec![
            KnowledgeVerdict::TrueCommon,
            KnowledgeVerdict::TrueCommon,
            KnowledgeVerdict::NeedsSource
        ]
    );
    let sent = llm.sent();
    assert_eq!(sent.len(), KNOWLEDGE_SAMPLES);
    for request in &sent {
        let (system, user) = messages(request);
        assert_eq!(system, KNOWLEDGE_SYSTEM);
        assert_eq!(user, "Statement: OCR reads pixels into text.");
        assert_eq!(request.sampling.and_then(|s| s.temperature), Some(0.7));
    }
}

#[tokio::test]
async fn a_sample_that_cannot_be_read_leaves_the_statement_unchecked() {
    let llm = Scripted::new(vec![
        answer("true_common"),
        Reply::Text("{}".into()),
        Reply::Text("{}".into()),
    ]);
    let judgment = checker(&llm, CheckPolicy::Knowledge)
        .check_knowledge("OCR reads pixels into text.")
        .await;
    assert!(matches!(judgment, ClassJudgment::Unusable), "{judgment:?}");
    assert_eq!(llm.sent().len(), 3, "no further samples after a bad one");
}

#[test]
fn the_samples_decide_a_pass_a_falsity_veto_and_a_need_for_a_source() {
    use KnowledgeVerdict::{False, NeedsSource, TrueCommon};
    let outcome = |verdicts: [KnowledgeVerdict; 3]| KnowledgeOutcome {
        samples: verdicts
            .into_iter()
            .map(|verdict| KnowledgeSample {
                verdict,
                reason: "Because.".into(),
            })
            .collect(),
    };
    let unanimous = outcome([TrueCommon; 3]);
    assert!(unanimous.is_common() && !unanimous.is_false() && !unanimous.needs_source());

    // A lone dissent forfeits the pass but is no majority, so it never vetoes.
    let lone = outcome([TrueCommon, False, TrueCommon]);
    assert!(!lone.is_common() && !lone.is_false());

    let majority = outcome([False, TrueCommon, False]);
    assert!(majority.is_false() && !majority.is_common());

    let doubtful = outcome([TrueCommon, NeedsSource, TrueCommon]);
    assert!(doubtful.needs_source() && !doubtful.is_common() && !doubtful.is_false());

    assert!(!KnowledgeOutcome { samples: vec![] }.is_common());
}

fn samples_of(verdicts: [KnowledgeVerdict; 3]) -> KnowledgeOutcome {
    KnowledgeOutcome {
        samples: verdicts
            .into_iter()
            .map(|verdict| KnowledgeSample {
                verdict,
                reason: "Because.".into(),
            })
            .collect(),
    }
}

#[test]
fn every_mix_of_three_samples_has_the_outcome_a_common_statement_and_a_veto_give_it() {
    use KnowledgeVerdict::{False, NeedsSource as Needs, TrueCommon as Common};
    use Outcome::{NeedsSource as Source, Refuted, Stands};
    let rows = [
        ([Common, Common, Common], Stands, Stands),
        ([Common, Common, Needs], Source, Stands),
        ([Common, Common, False], Source, Stands),
        ([Common, Needs, Needs], Source, Stands),
        ([Common, Needs, False], Source, Stands),
        ([Common, False, False], Refuted, Refuted),
        ([Needs, Needs, Needs], Source, Stands),
        ([Needs, Needs, False], Source, Stands),
        ([Needs, False, False], Refuted, Refuted),
        ([False, False, False], Refuted, Refuted),
    ];
    for (verdicts, knowledge, veto) in rows {
        let samples = samples_of(verdicts);
        assert_eq!(samples.knowledge_outcome(), knowledge, "{verdicts:?}");
        assert_eq!(samples.veto_outcome(), veto, "{verdicts:?}");
    }
}

#[test]
fn no_samples_never_pass_a_statement_and_never_veto_it() {
    let none = KnowledgeOutcome { samples: vec![] };
    assert_eq!(none.knowledge_outcome(), Outcome::NeedsSource);
    assert_eq!(none.veto_outcome(), Outcome::Stands);
}

#[test]
fn each_consistency_verdict_has_its_outcome() {
    for (verdict, expected) in [
        (ConsistencyVerdict::Consistent, Outcome::Stands),
        (ConsistencyVerdict::Contradicted, Outcome::Refuted),
        (ConsistencyVerdict::UnfoundedSpecific, Outcome::NeedsSource),
    ] {
        let outcome = ConsistencyOutcome {
            verdict,
            reason: "Because.".into(),
        };
        assert_eq!(outcome.outcome(), expected, "{verdict:?}");
    }
}

#[test]
fn a_consistency_check_fits_the_window_only_beside_its_answer() {
    let llm = Scripted::new(vec![]);
    let checker = checker(&llm, CheckPolicy::Consistency);
    assert!(checker.consistency_fits("S.", &passages()));
    assert!(!checker.consistency_fits("S.", &["word ".repeat(40_000)]));
    let room = checker.consistency_evidence_room("S.");
    assert!(room > 0 && room < 8_192, "{room}");
}

#[tokio::test]
async fn a_reason_past_600_characters_is_kept_whole() {
    let reason = "No passage says otherwise. ".repeat(30).trim().to_string();
    assert!(reason.chars().count() > 600);
    let llm = Scripted::new(vec![Reply::Text(
        json!({"verdict": "consistent", "reason": reason}).to_string(),
    )]);
    let outcome = judged(
        checker(&llm, CheckPolicy::Consistency)
            .check_consistency("OCR reads pixels into text.", &passages())
            .await,
    );
    assert_eq!(outcome.reason, reason);
}
