//! The counterpart's reply: asked for, checked, asked again once with the reason, or refused.

use std::future::Future;

use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::drills::runner::CodeRunner;
use crate::lessons::structured_call::{complete_checked_request, reask};
use crate::model_call;
use crate::practice::coding::harness::CaseHarness;
use crate::practice::coding::observation::WorkspaceObservation;
use crate::practice::coding::policy::{CodingCue, CodingPolicy};
use crate::practice::coding::problem::Problem;
use crate::practice::coding::solution_check::{code_blocks, passes_every_case};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::problem_patterns::named_in;
use lattice_core::shared::error::{AppError, Result};

/// What the model is asked to reserve for one reply, schema fields included.
const REPLY_OUTPUT_TOKENS: usize = 2048;

/// A reply that passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterpartReply {
    pub content: String,
    pub lines: Vec<u32>,
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawReply {
    content: String,
    #[serde(default)]
    lines: Vec<u32>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    clarification: Option<String>,
    #[serde(default)]
    speak: Option<bool>,
}

/// What a reply says when it answers with the problem's own "your call" line.
const YOUR_CALL: &str = "Your call: state your assumption";

fn refuse(reason: impl Into<String>) -> AppError {
    AppError::InvalidInput(reason.into())
}

/// The schema of a reply on `cue`.
fn reply_schema(problem: &Problem, cue: &CodingCue) -> Value {
    let mut options: Vec<String> = (0..problem.spec.clarifications.len())
        .map(|index| index.to_string())
        .collect();
    options.push("your_call".to_owned());
    options.push("none".to_owned());
    let mut properties = json!({
        "content": {"type": "string"},
        "lines": {"type": "array", "items": {"type": "integer"}},
        "path": {"type": ["string", "null"]},
        "clarification": {"type": "string", "enum": options},
    });
    let mut required = vec!["content", "lines", "path", "clarification"];
    if *cue == CodingCue::Checkpoint {
        if let Some(properties) = properties.as_object_mut() {
            properties.insert("speak".to_owned(), json!({"type": "boolean"}));
        }
        required.push("speak");
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": properties,
    })
}

/// Reads and checks a reply, apart from running the code in it. None is silence.
fn accept(
    raw: &str,
    problem: &Problem,
    policy: &CodingPolicy,
    cue: &CodingCue,
    observation: &WorkspaceObservation,
) -> Result<Option<CounterpartReply>> {
    let reply: RawReply = crate::generation::parse_json(raw)?;
    if reply.speak == Some(false) {
        return if *cue == CodingCue::Checkpoint {
            Ok(None)
        } else {
            Err(refuse(
                "Only a checkpoint may stay silent. Answer the learner.",
            ))
        };
    }
    let content = reply.content;
    if content.trim().is_empty() {
        return Err(refuse("The reply is blank. Say something."));
    }

    let path = if reply.lines.is_empty() {
        None
    } else {
        Some(
            reply
                .path
                .unwrap_or_else(|| crate::practice::coding::problem::SOLUTION_PATH.to_owned()),
        )
    };
    if let Some(path) = &path {
        let count = observation
            .line_count(path)
            .ok_or_else(|| refuse(format!("{path} is not one of the learner's files.")))?;
        for line in &reply.lines {
            let exists = *line >= 1 && usize::try_from(*line).is_ok_and(|line| line <= count);
            if !exists {
                return Err(refuse(format!(
                    "Line {line} is not in {path}, which has {count} lines. Refer only to lines that exist."
                )));
            }
        }
    }

    let internal = problem
        .cases
        .iter()
        .map(|case| case.id.clone())
        .chain(
            problem
                .misreadings
                .iter()
                .map(|misreading| misreading.id.clone()),
        )
        .chain(observation.internal_ids());
    for id in internal.filter(|id| !id.is_empty()) {
        if content.contains(&id) {
            return Err(refuse(format!(
                "The reply contains the internal id {id}. Never write an id; describe a case by what it is about."
            )));
        }
    }

    if let Some(pattern) = named_in(&content, &problem.spec.patterns) {
        return Err(refuse(format!(
            "The reply names the technique \"{}\". Do not name it; ask about the idea instead.",
            pattern.name
        )));
    }

    match reply.clarification.as_deref() {
        None | Some("none") => {}
        Some("your_call") => {
            if !content.contains(YOUR_CALL) {
                return Err(refuse(format!(
                    "With clarification your_call the reply must contain \"{YOUR_CALL}\"."
                )));
            }
        }
        Some(index) => {
            let clarification = index
                .parse::<usize>()
                .ok()
                .and_then(|index| problem.spec.clarifications.get(index));
            match clarification {
                None => {
                    return Err(refuse(format!(
                        "{index} is not a clarification of this problem. Use a clarification's number, your_call or none."
                    )))
                }
                Some(clarification) if !content.contains(&clarification.answer) => {
                    return Err(refuse(format!(
                        "The reply must contain the answer to clarification {index} word for word: {}",
                        clarification.answer
                    )))
                }
                Some(_) => {}
            }
        }
    }

    policy.check(&content, cue, observation)?;
    Ok(Some(CounterpartReply {
        content,
        lines: reply.lines,
        path,
    }))
}

/// Refuses `content` when a fenced block in it passes every case. `passes` runs one block.
async fn refuse_passing_blocks<F, Fut>(content: &str, passes: F) -> Result<()>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<bool>>,
{
    for block in code_blocks(content) {
        if passes(block).await? {
            return Err(refuse(
                "A code block in the reply passes every case, so it solves the problem. Do not write a working solution; describe the step in words, or show a fragment that leaves the problem unsolved.",
            ));
        }
    }
    Ok(())
}

/// The counterpart's checked reply on `cue`, or None when it chose silence (only on Checkpoint).
pub async fn respond(
    llm: &dyn LLMPort,
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    problem: &Problem,
    policy: &CodingPolicy,
    cue: &CodingCue,
    observation: &WorkspaceObservation,
    cancel: CancellationToken,
) -> Result<Option<CounterpartReply>> {
    let mut request = model_call::structured(
        &policy.system(cue, observation),
        observation.render(),
        reply_schema(problem, cue),
        REPLY_OUTPUT_TOKENS,
        "low",
    );
    let mut again = false;
    loop {
        let (raw, reply) = complete_checked_request(llm, request.clone(), None, None, |raw| {
            accept(raw, problem, policy, cue, observation).map(|reply| (raw.to_owned(), reply))
        })
        .await?;
        let Some(reply) = reply else {
            return Ok(None);
        };
        let ran = refuse_passing_blocks(&reply.content, |code| {
            let cancel = cancel.clone();
            async move { passes_every_case(runner, harness, problem, &code, cancel).await }
        })
        .await;
        let why = match ran {
            Ok(()) => return Ok(Some(reply)),
            Err(AppError::InvalidInput(why)) => why,
            Err(error) => return Err(error),
        };
        if again {
            return Err(AppError::ModelAnswerUnusable(why));
        }
        request = reask(request, &raw, &why);
        again = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::practice::coding::dto::CodingStage;
    use crate::practice::coding::observation::observe;
    use crate::practice::coding::observation::test_support::{
        failing_run, observation, session, MISREADING_ID,
    };
    use crate::practice::coding::policy::policy_for;
    use crate::practice::coding::problem::{CodingSetting, CodingTask};
    use lattice_core::domain::learning::practice::problem_patterns::PROBLEM_PATTERNS;

    fn json(content: &str) -> String {
        serde_json::json!({"content": content, "lines": [], "path": null, "clarification": "none"})
            .to_string()
    }

    fn with(fields: Value) -> String {
        let mut base = serde_json::json!({"content": "Look at it again.", "lines": [], "path": null, "clarification": "none"});
        if let (Some(base), Some(fields)) = (base.as_object_mut(), fields.as_object()) {
            for (key, value) in fields {
                base.insert(key.clone(), value.clone());
            }
        }
        base.to_string()
    }

    fn accepted(
        setting: CodingSetting,
        cue: &CodingCue,
        raw: &str,
    ) -> Result<Option<CounterpartReply>> {
        let (problem, observation) = observation(setting, CodingStage::Code);
        let policy = policy_for(setting, CodingTask::Problem);
        accept(raw, &problem, policy.as_ref(), cue, &observation)
    }

    fn reason(result: Result<Option<CounterpartReply>>) -> String {
        match result {
            Err(AppError::InvalidInput(reason)) => reason,
            other => format!("not a refusal: {other:?}"),
        }
    }

    #[test]
    fn a_clean_reply_is_accepted_with_its_lines() {
        let raw = with(
            serde_json::json!({"content": "Check line 2.", "lines": [2], "path": "solution.py"}),
        );
        let reply = accepted(CodingSetting::Coached, &CodingCue::Reply, &raw);
        assert_eq!(
            reply.ok().flatten(),
            Some(CounterpartReply {
                content: "Check line 2.".to_owned(),
                lines: vec![2],
                path: Some("solution.py".to_owned()),
            })
        );
    }

    #[test]
    fn a_blank_reply_is_refused() {
        let result = accepted(CodingSetting::Coached, &CodingCue::Reply, &json("  \n "));
        assert!(
            matches!(result, Err(AppError::InvalidInput(_))),
            "{result:?}"
        );
    }

    #[test]
    fn a_line_past_the_end_of_the_file_is_refused() {
        let raw = with(serde_json::json!({"lines": [4], "path": "solution.py"}));
        let why = reason(accepted(CodingSetting::Coached, &CodingCue::Reply, &raw));
        assert!(why.contains('4'), "{why}");
    }

    #[test]
    fn line_zero_and_a_file_the_draft_lacks_are_refused() {
        let zero = with(serde_json::json!({"lines": [0], "path": "solution.py"}));
        assert!(accepted(CodingSetting::Coached, &CodingCue::Reply, &zero).is_err());
        let elsewhere = with(serde_json::json!({"lines": [1], "path": "other.py"}));
        assert!(accepted(CodingSetting::Coached, &CodingCue::Reply, &elsewhere).is_err());
    }

    #[test]
    fn lines_with_no_path_mean_the_solution_file() {
        let last = with(serde_json::json!({"lines": [3]}));
        let reply = accepted(CodingSetting::Coached, &CodingCue::Reply, &last)
            .ok()
            .flatten();
        assert_eq!(
            reply.and_then(|reply| reply.path),
            Some("solution.py".to_owned())
        );
        let past = with(serde_json::json!({"lines": [4]}));
        assert!(accepted(CodingSetting::Coached, &CodingCue::Reply, &past).is_err());
    }

    #[test]
    fn a_reply_naming_the_pattern_is_refused() {
        let name = PROBLEM_PATTERNS
            .first()
            .map(|pattern| pattern.name)
            .unwrap_or_default();
        let raw = json(&format!(
            "This is a classic {} problem.",
            name.to_lowercase()
        ));
        let why = reason(accepted(CodingSetting::Coached, &CodingCue::Reply, &raw));
        assert!(why.contains(name), "{why}");
    }

    #[test]
    fn a_clarification_answer_not_verbatim_is_refused() {
        let raw =
            with(serde_json::json!({"content": "Yes, it can be empty.", "clarification": "0"}));
        assert!(accepted(CodingSetting::Mock, &CodingCue::Reply, &raw).is_err());
    }

    #[test]
    fn a_clarification_answer_verbatim_is_accepted() {
        let raw = with(
            serde_json::json!({"content": "Yes, and its total is 0. Anything else?", "clarification": "0"}),
        );
        assert!(accepted(CodingSetting::Mock, &CodingCue::Reply, &raw)
            .ok()
            .flatten()
            .is_some());
    }

    #[test]
    fn a_clarification_the_problem_lacks_is_refused() {
        let raw = with(serde_json::json!({"clarification": "7"}));
        assert!(accepted(CodingSetting::Coached, &CodingCue::Reply, &raw).is_err());
    }

    #[test]
    fn your_call_needs_its_sentence() {
        let missing =
            with(serde_json::json!({"content": "It is up to you.", "clarification": "your_call"}));
        assert!(accepted(CodingSetting::Mock, &CodingCue::Reply, &missing).is_err());
        let said = with(
            serde_json::json!({"content": "Your call: state your assumption.", "clarification": "your_call"}),
        );
        assert!(accepted(CodingSetting::Mock, &CodingCue::Reply, &said)
            .ok()
            .flatten()
            .is_some());
    }

    #[test]
    fn an_internal_id_is_refused() {
        for id in ["case-hidden-1", MISREADING_ID] {
            let raw = json(&format!("Your code fails {id}."));
            assert!(
                accepted(CodingSetting::Coached, &CodingCue::Reply, &raw).is_err(),
                "{id}"
            );
        }
    }

    #[test]
    fn a_session_run_or_turn_id_is_refused() {
        let problem = crate::practice::coding::observation::test_support::problem();
        let mut session = session(&problem, CodingSetting::Coached, CodingStage::Test);
        session.runs.push(failing_run());
        let observation = observe(&problem, &session, None);
        let policy = policy_for(CodingSetting::Coached, CodingTask::Problem);
        for id in ["run-77777777-aaaa", "5b1f0c2e-1111-4222-8333-444455556666"] {
            let raw = json(&format!("See {id}."));
            let result = accept(
                &raw,
                &problem,
                policy.as_ref(),
                &CodingCue::Reply,
                &observation,
            );
            assert!(result.is_err(), "{id}");
        }
    }

    #[test]
    fn two_questions_from_an_interviewer_are_refused_and_from_the_coach_are_not() {
        let raw = json("What is the cost? And why that structure?");
        assert!(accepted(CodingSetting::Mock, &CodingCue::Reply, &raw).is_err());
        assert!(accepted(CodingSetting::Watched, &CodingCue::Reply, &raw).is_err());
        assert!(accepted(CodingSetting::Coached, &CodingCue::Reply, &raw)
            .ok()
            .flatten()
            .is_some());
    }

    #[test]
    fn silence_is_accepted_only_on_a_checkpoint() {
        let silent = with(serde_json::json!({"content": "", "speak": false}));
        let cues = [
            CodingCue::Opening,
            CodingCue::Reply,
            CodingCue::Submitted,
            CodingCue::Idle,
            CodingCue::StageOpened(CodingStage::Plan),
        ];
        for cue in cues {
            assert!(
                accepted(CodingSetting::Watched, &cue, &silent).is_err(),
                "{cue:?}"
            );
        }
        assert_eq!(
            accepted(CodingSetting::Watched, &CodingCue::Checkpoint, &silent).ok(),
            Some(None)
        );
    }

    #[test]
    fn speaking_on_a_checkpoint_is_checked_like_any_reply() {
        let spoken = with(serde_json::json!({"content": "Why a loop here?", "speak": true}));
        assert!(
            accepted(CodingSetting::Watched, &CodingCue::Checkpoint, &spoken)
                .ok()
                .flatten()
                .is_some()
        );
        let blank = with(serde_json::json!({"content": " ", "speak": true}));
        assert!(accepted(CodingSetting::Watched, &CodingCue::Checkpoint, &blank).is_err());
    }

    #[test]
    fn a_reply_wrapped_in_a_json_fence_is_accepted() {
        let fenced = format!("```json\n{}\n```", json("Check line 2."));
        let reply = accepted(CodingSetting::Coached, &CodingCue::Reply, &fenced);
        assert_eq!(
            reply.ok().flatten().map(|reply| reply.content),
            Some("Check line 2.".to_owned())
        );
    }

    #[test]
    fn an_answer_that_is_not_the_json_asked_for_is_refused() {
        let result = accepted(CodingSetting::Coached, &CodingCue::Reply, "plain words");
        assert!(
            matches!(result, Err(AppError::InvalidInput(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn a_code_block_that_passes_every_case_is_refused() {
        let content = "Try this:\n```python\ndef total(values):\n    return sum(values)\n```\n";
        let result = refuse_passing_blocks(content, |_code| async { Ok(true) }).await;
        assert!(
            matches!(result, Err(AppError::InvalidInput(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn a_code_block_that_fails_a_case_is_allowed() {
        let content = "Think about this:\n```python\nfor value in values:\n    pass\n```\n";
        let result = refuse_passing_blocks(content, |_code| async { Ok(false) }).await;
        assert!(result.is_ok(), "{result:?}");
    }

    #[tokio::test]
    async fn every_block_is_run_and_any_passing_one_refuses() {
        let content = "```python\nfirst\n```\ntext\n```python\nsecond\n```\n";
        let result =
            refuse_passing_blocks(content, |code| async move { Ok(code.contains("second")) }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn a_reply_without_code_runs_nothing() {
        let result = refuse_passing_blocks("No code here.", |_code| async {
            Err::<bool, _>(AppError::InvalidState("ran".into()))
        })
        .await;
        assert!(result.is_ok(), "{result:?}");
    }

    #[tokio::test]
    async fn a_run_that_cannot_happen_is_not_a_refusal() {
        let content = "```python\nx\n```";
        let result = refuse_passing_blocks(content, |_code| async {
            Err::<bool, _>(AppError::ServiceNotAvailable("no runtime".into()))
        })
        .await;
        assert!(
            matches!(result, Err(AppError::ServiceNotAvailable(_))),
            "{result:?}"
        );
    }

    #[test]
    fn the_schema_offers_the_clarifications_and_speak_only_on_a_checkpoint() {
        let (problem, _) = observation(CodingSetting::Watched, CodingStage::Code);
        let plain = reply_schema(&problem, &CodingCue::Reply);
        let checkpoint = reply_schema(&problem, &CodingCue::Checkpoint);
        let options = plain["properties"]["clarification"]["enum"].clone();
        assert_eq!(options, serde_json::json!(["0", "1", "your_call", "none"]));
        assert!(plain["properties"].get("speak").is_none());
        assert!(checkpoint["properties"].get("speak").is_some());
        assert!(plain["properties"].get("content").is_some());
        assert!(plain["properties"].get("lines").is_some());
        assert!(plain["properties"].get("path").is_some());
    }
}
