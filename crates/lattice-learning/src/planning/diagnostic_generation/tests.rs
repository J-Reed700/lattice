#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::evaluation::{DiagnosticEvaluation, Input as EvaluationInput, Key, Prompt};
use super::tasks::{DiagnosticTasks, Input as TasksInput, Outcome, Passage};
use super::*;
use crate::dto::{LearningOutcomeDefinitionDto, LearningProgramDto, LearningSourceDto};
use crate::lessons::course_generation_tests::Model;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::shape::Pointers;
use lattice_core::application::services::model_contract::shown::Told;
use lattice_core::application::services::model_contract::test_support::samples::schema_of;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::ScriptedModel;
use lattice_core::application::services::model_contract::Contract;
use serde_json::{json, Value};
use std::sync::Mutex;

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

/// Every message of the request, system and user, one after another.
fn prompt_of(request: &CompletionRequest) -> String {
    request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The task a prompt carries, the one that has a `key` field: a re-ask restates it.
fn task_of(prompt: &str, key: &str) -> Value {
    prompt
        .rsplit("\n\n")
        .find_map(|chunk| {
            serde_json::from_str::<Value>(chunk)
                .ok()
                .filter(|task| task.get(key).is_some())
        })
        .unwrap()
}

/// Answers each starting-point tasks request with `tasks(prompt, number of earlier asks)` and every
/// other request, the review of what was written, as the fixture model does.
struct Writer<F> {
    tasks: F,
    rest: Model,
    asked: Mutex<Vec<String>>,
}

impl<F: Fn(&str, usize) -> String + Send + Sync> Writer<F> {
    fn new(tasks: F) -> Self {
        Self {
            tasks,
            rest: Model("{}".into()),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl<F: Fn(&str, usize) -> String + Send + Sync> LLMPort for Writer<F> {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if !request
            .contract()
            .is_some_and(|call| call.id == DiagnosticTasks::ID)
        {
            return self.rest.complete(request).await;
        }
        let prompt = prompt_of(request);
        let earlier = {
            let mut asked = self.asked.lock().unwrap();
            asked.push(prompt.clone());
            asked.len() - 1
        };
        Ok(CompletionResponse::from_text((self.tasks)(
            &prompt, earlier,
        )))
    }
    fn model_name(&self) -> &str {
        self.rest.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.rest.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.rest.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        self.rest.is_ready().await
    }
}

fn program_and_outcomes() -> (LearningProgramDto, Vec<LearningOutcomeDefinitionDto>) {
    let mut program = crate::tests::fixture();
    for (index, module) in program.modules.iter_mut().enumerate() {
        module.id = format!("module-{index}");
    }
    let outcomes = program
        .modules
        .iter()
        .enumerate()
        .map(|(index, module)| LearningOutcomeDefinitionDto {
            id: format!("outcome-{index}"),
            module_id: Some(module.id.clone()),
            lesson_id: None,
            title: format!("Apply skill {index}"),
            description: String::new(),
            ordinal: index as i64,
            created_at: 0,
        })
        .collect();
    (program, outcomes)
}

fn sources() -> Vec<LearningSourceDto> {
    vec![LearningSourceDto {
        id: "guidance".into(),
        title: "Skill guidance".into(),
        url: None,
        excerpt: "Skill guidance for every module.".into(),
        acquired_at: 1,
    }]
}

/// The handle and title of each outcome a tasks request shows.
fn shown_outcomes(prompt: &str) -> Vec<(String, String)> {
    task_of(prompt, "outcomes")["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|outcome| {
            (
                outcome["id"].as_str().unwrap().to_owned(),
                outcome["title"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn task_fields(title: &str) -> Value {
    json!({
        "prompt": format!("Predict the result of the case for {title} and explain why."),
        "expectedAnswer": format!("The result for {title} follows from the stated rule."),
        "criteria": format!("Names the rule for {title}, applies it, and states the result."),
    })
}

/// An entry for the outcome shown under `handle`.
fn task_entry(handle: &str, fields: Value) -> Value {
    let mut entry = json!({ "of": handle });
    if let (Some(entry), Some(fields)) = (entry.as_object_mut(), fields.as_object()) {
        entry.extend(fields.clone());
    }
    entry
}

/// An answer that writes `fields(title)` for each outcome the prompt shows, last shown first.
fn written(prompt: &str, fields: impl Fn(&str) -> Value) -> String {
    let mut entries: Vec<Value> = shown_outcomes(prompt)
        .iter()
        .map(|(handle, title)| task_entry(handle, fields(title)))
        .collect();
    entries.reverse();
    json!({ "entries": entries }).to_string()
}

#[tokio::test]
async fn the_app_names_the_outcome_of_each_task() -> Result<()> {
    let (program, outcomes) = program_and_outcomes();
    let model = Writer::new(|prompt, _| {
        written(prompt, |title| {
            let mut fields = task_fields(title);
            fields["outcomeId"] = json!("outcome-invented");
            fields
        })
    });

    let tasks = author(&model, &program, &outcomes, &sources()).await?;

    assert_eq!(tasks.len(), outcomes.len());
    for (outcome, task) in outcomes.iter().zip(&tasks) {
        assert_eq!(task.outcome_id, outcome.id);
        assert!(task.prompt.contains(&outcome.title), "{task:?}");
    }
    Ok(())
}

#[tokio::test]
async fn an_outcome_missing_from_the_answer_is_asked_for_again_alone() -> Result<()> {
    let (program, outcomes) = program_and_outcomes();
    assert!(outcomes.len() >= 2);
    let model = Writer::new(|prompt, earlier| {
        let skipped = "Apply skill 1";
        let mut entries: Vec<Value> = Vec::new();
        for (handle, title) in shown_outcomes(prompt) {
            if earlier > 0 || title != skipped {
                entries.push(task_entry(&handle, task_fields(&title)));
            }
        }
        json!({ "entries": entries }).to_string()
    });

    let tasks = author(&model, &program, &outcomes, &sources()).await?;

    assert_eq!(tasks.len(), outcomes.len());
    let asked = model.asked();
    assert_eq!(asked.len(), 2);
    assert_eq!(
        shown_outcomes(&asked[1]),
        [("o2".to_owned(), "Apply skill 1".to_owned())]
    );
    Ok(())
}

#[tokio::test]
async fn a_blank_field_is_corrected_for_that_task_alone() -> Result<()> {
    let (program, outcomes) = program_and_outcomes();
    let model = Writer::new(|prompt, earlier| {
        written(prompt, |title| {
            let mut fields = task_fields(title);
            if earlier == 0 && title == "Apply skill 1" {
                fields["criteria"] = json!("  ");
            }
            fields
        })
    });

    let tasks = author(&model, &program, &outcomes, &sources()).await?;

    assert!(tasks.iter().all(|task| !task.criteria.trim().is_empty()));
    let asked = model.asked();
    assert_eq!(asked.len(), 2);
    assert!(asked[1].contains("each with text in it"), "{}", asked[1]);
    Ok(())
}

#[tokio::test]
async fn an_outcome_that_stays_unanswered_fails_the_starting_point() {
    let (program, outcomes) = program_and_outcomes();
    let model = Writer::new(|_, _| "{}".to_owned());

    let authored = author(&model, &program, &outcomes, &sources()).await;

    assert!(authored.is_err());
}

#[test]
fn the_tasks_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = TasksInput {
        goal: marks.told("Read the harbour tide tables.".to_owned()),
        prior_knowledge: marks.told("Has watched the tide come in.".to_owned()),
        outcomes: vec![Outcome {
            id: marks.reference("outcome-7d41c0ae".to_owned()),
            title: marks.told("Find high water in a table".to_owned()),
            description: marks.told("Read the hour of high water from the table.".to_owned()),
            ordinal: marks.told(0_i64),
            created_at: marks.told(0_i64),
        }],
        passages: Packed::new(
            vec![Passage {
                title: Some(marks.told("Tide guide".to_owned())),
                text: marks.told("High water follows the moon across the bay.".to_owned()),
            }],
            |_| 5,
        ),
    };

    holds::<DiagnosticTasks>(&input, &marks);
}

#[test]
fn the_tasks_schema_bounds_no_text_but_a_blank_one() {
    let input = TasksInput {
        goal: Told("Read the harbour tide tables.".into()),
        prior_knowledge: Told("None.".into()),
        outcomes: vec![Outcome {
            id: "outcome-0".into(),
            title: Told("Find high water".into()),
            description: Told(String::new()),
            ordinal: Told(0),
            created_at: Told(0),
        }],
        passages: Packed::default(),
    };
    let schema = schema_of::<DiagnosticTasks>(&input, Pointers::Enum).unwrap();
    let caps = crate::content_caps_tests::text_caps(schema.as_json());
    assert!(caps.is_empty(), "{caps:?}");
}

fn harbour_prompt() -> Prompt {
    Prompt {
        id: "prompt-0".into(),
        prompt: Told("Which join column would you index first?".into()),
        outcome_title: Told("Index a join".into()),
        key: Some(Key {
            prompt: Told("Which join column would you index first?".into()),
            expected_answer: Told("The column the join compares.".into()),
            criteria: Told("Names the join column and says why.".into()),
        }),
        response: Told(
            "I would index the join column first. It lets the planner skip a scan.".into(),
        ),
    }
}

#[test]
fn the_evaluation_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = EvaluationInput {
        prompts: vec![Prompt {
            id: marks.reference("prompt-3b9e12cd".to_owned()),
            prompt: marks.told("Which tide table would you read first?".to_owned()),
            outcome_title: marks.told("Find high water in a table".to_owned()),
            key: Some(Key {
                prompt: marks.told("Which tide table would you read first?".to_owned()),
                expected_answer: marks
                    .told("The table for the harbour mouth, since it sets the gate.".to_owned()),
                criteria: marks.told("Names the harbour mouth table and says why.".to_owned()),
            }),
            response: marks.told("I would read the harbour mouth table first.".to_owned()),
        }],
    };

    holds::<DiagnosticEvaluation>(&input, &marks);
}

#[test]
fn the_evaluator_writes_its_grounds_before_its_signal() {
    let input = EvaluationInput {
        prompts: vec![harbour_prompt()],
    };
    let schema = schema_of::<DiagnosticEvaluation>(&input, Pointers::Enum).unwrap();
    let items = &schema.as_json()["properties"]["entries"]["items"];
    assert_eq!(
        items["required"],
        json!(["of", "evidence", "feedback", "signal"])
    );
    assert_eq!(items["properties"]["of"]["enum"], json!(["p1"]));
    assert_eq!(
        items["properties"]["evidence"]["items"]["enum"],
        json!(["u1", "u2"])
    );
}

#[test]
fn the_evaluator_bounds_no_text_but_a_blank_one() {
    let input = EvaluationInput {
        prompts: vec![harbour_prompt()],
    };
    let schema = schema_of::<DiagnosticEvaluation>(&input, Pointers::Enum).unwrap();
    let caps = crate::content_caps_tests::text_caps(schema.as_json());
    assert!(caps.is_empty(), "{caps:?}");
}

fn attempt() -> LearningDiagnosticAttemptDto {
    LearningDiagnosticAttemptDto {
        id: "a1".into(),
        program_id: "program".into(),
        status: LearningDiagnosticStatus::Active,
        revision: 0,
        prompts: vec![LearningDiagnosticPromptDto {
            id: "p1".into(),
            prompt: "Which join column would you index first?".into(),
            outcome_id: "o1".into(),
            outcome_title: "Index a join".into(),
            source_version_ids: vec![],
        }],
        responses: vec![],
        source_coverage_gaps: vec![],
        interpretation: String::new(),
        findings: vec![],
        created_at: 0,
        submitted_at: None,
    }
}

fn answer() -> String {
    "I would index the join column first, because it lets the planner skip a scan. ".repeat(12)
}

#[test]
fn feedback_and_a_quote_of_any_length_are_read_not_refused() {
    let answer = answer();
    let responses = [LearningDiagnosticResponseDto {
        prompt_id: "p1".into(),
        response: answer.clone(),
    }];
    let finding = |feedback: String, quote: &str| LearningDiagnosticFindingDto {
        prompt_id: "p1".into(),
        outcome_id: "o1".into(),
        signal: LearningDiagnosticSignal::ReadyForChallenge,
        feedback,
        evidence_quote: Some(quote.into()),
    };
    for (what, feedback, quote) in [
        (
            "a quote past 700 characters",
            "You chose the right first step and said why.".to_owned(),
            answer.as_str(),
        ),
        (
            "feedback past 1,200 characters",
            "You chose the right first step and said why. ".repeat(30),
            "index the join column",
        ),
        (
            "feedback under 20 characters",
            "Right idea.".to_owned(),
            "index the join column",
        ),
    ] {
        let checked = validate_findings(&attempt(), &responses, &[finding(feedback, quote)]);
        assert!(checked.is_ok(), "{what}: {checked:?}");
    }
}

#[test]
fn a_quote_with_a_curly_apostrophe_stands_in_a_response_with_a_straight_one() {
    let responses = [LearningDiagnosticResponseDto {
        prompt_id: "p1".into(),
        response: "I'd index the join column first because it skips a scan.".into(),
    }];
    let finding = LearningDiagnosticFindingDto {
        prompt_id: "p1".into(),
        outcome_id: "o1".into(),
        signal: LearningDiagnosticSignal::ReadyForChallenge,
        feedback: "You chose the right first step and said why.".into(),
        evidence_quote: Some("I\u{2019}d index the join column first".into()),
    };
    assert!(validate_findings(&attempt(), &responses, std::slice::from_ref(&finding)).is_ok());
}

fn two_prompt_attempt() -> LearningDiagnosticAttemptDto {
    let prompt = |id: &str, outcome: &str| LearningDiagnosticPromptDto {
        id: id.into(),
        prompt: "Which join column would you index first?".into(),
        outcome_id: outcome.into(),
        outcome_title: format!("Index a join for {outcome}"),
        source_version_ids: vec![],
    };
    LearningDiagnosticAttemptDto {
        prompts: vec![prompt("p1", "o1"), prompt("p2", "o2")],
        ..attempt()
    }
}

fn task(outcome: &str) -> DiagnosticTask {
    DiagnosticTask {
        outcome_id: outcome.into(),
        prompt: "Which join column would you index first?".into(),
        expected_answer: "The column the join compares, so the planner skips a scan.".into(),
        criteria: "Names the join column and says why an index helps.".into(),
    }
}

const HARBOUR: &str =
    "The harbour gate opens at dawn. The pilot boards at six. The tug follows the tide.";

fn responses_of(first: &str, second: &str) -> Vec<LearningDiagnosticResponseDto> {
    vec![
        LearningDiagnosticResponseDto {
            prompt_id: "p1".into(),
            response: first.into(),
        },
        LearningDiagnosticResponseDto {
            prompt_id: "p2".into(),
            response: second.into(),
        },
    ]
}

/// The handles of the sentences a request shows for the response under `prompt`, in order.
fn sentence_handles(prompt: &str, shown: &str) -> Vec<String> {
    let task = task_of(prompt, "responses");
    task["responses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|response| response["promptId"] == shown)
        .map(|response| {
            response["response"]
                .as_str()
                .unwrap()
                .lines()
                .map(|row| {
                    row.trim_start_matches('[')
                        .split(']')
                        .next()
                        .unwrap()
                        .to_owned()
                })
                .collect()
        })
        .unwrap()
}

fn shown_prompts(prompt: &str) -> Vec<String> {
    task_of(prompt, "prompts")["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|shown| shown["id"].as_str().unwrap().to_owned())
        .collect()
}

fn finding_entry(of: &str, evidence: &[&str], signal: &str) -> Value {
    json!({
        "of": of,
        "evidence": evidence,
        "feedback": "You named the first step and said why. The join column is the right first index. Next, read a plan.",
        "signal": signal,
    })
}

/// A model that answers each evaluation request with `entries(prompt, number of earlier asks)`.
fn judging(entries: impl Fn(&str, usize) -> Vec<Value> + Send + Sync + 'static) -> ScriptedModel {
    ScriptedModel::new().on_request::<DiagnosticEvaluation>(move |sent, earlier| {
        let entries = entries(&prompt_of(&sent.request), earlier);
        reply(json!({ "entries": entries }).to_string())
    })
}

#[tokio::test]
async fn the_app_names_the_prompt_and_outcome_of_each_finding() -> Result<()> {
    let responses = responses_of(HARBOUR, HARBOUR);
    let model = judging(|prompt, _| {
        let mut entries: Vec<Value> = shown_prompts(prompt)
            .iter()
            .map(|handle| {
                let first = sentence_handles(prompt, handle).remove(0);
                let mut entry = finding_entry(handle, &[&first], "ready_for_challenge");
                entry["promptId"] = json!("invented");
                entry["outcomeId"] = json!("invented");
                entry
            })
            .collect();
        entries.reverse();
        entries
    });

    let findings = evaluate(
        &model,
        &two_prompt_attempt(),
        &[task("o1"), task("o2")],
        &responses,
    )
    .await?;

    let pairs: Vec<(&str, &str)> = findings
        .iter()
        .map(|f| (f.prompt_id.as_str(), f.outcome_id.as_str()))
        .collect();
    assert_eq!(pairs, [("p1", "o1"), ("p2", "o2")]);
    Ok(())
}

#[tokio::test]
async fn the_cited_sentences_are_the_quote() -> Result<()> {
    let attempt = two_prompt_attempt();
    let responses = responses_of(HARBOUR, HARBOUR);
    let model = judging(|prompt, _| {
        let handles = shown_prompts(prompt);
        let first = sentence_handles(prompt, &handles[0]);
        let second = sentence_handles(prompt, &handles[1]);
        vec![
            finding_entry(&handles[0], &[&first[1]], "ready_for_challenge"),
            finding_entry(&handles[1], &[&second[2], &second[0]], "needs_practice"),
        ]
    });

    let findings = evaluate(&model, &attempt, &[task("o1"), task("o2")], &responses).await?;

    assert_eq!(
        findings[0].evidence_quote.as_deref(),
        Some("The pilot boards at six.")
    );
    assert_eq!(findings[1].evidence_quote.as_deref(), Some(HARBOUR));
    assert!(validate_findings(&attempt, &responses, &findings).is_ok());
    Ok(())
}

#[tokio::test]
async fn a_cite_that_names_no_shown_sentence_costs_only_that_cite() -> Result<()> {
    let attempt = two_prompt_attempt();
    let responses = responses_of(HARBOUR, HARBOUR);
    let model = judging(|prompt, _| {
        shown_prompts(prompt)
            .iter()
            .map(|handle| {
                let second = sentence_handles(prompt, handle).remove(1);
                finding_entry(handle, &["u99", second.as_str()], "ready_for_challenge")
            })
            .collect()
    });

    let findings = evaluate(&model, &attempt, &[task("o1"), task("o2")], &responses).await?;

    assert!(findings
        .iter()
        .all(|f| f.evidence_quote.as_deref() == Some("The pilot boards at six.")));
    Ok(())
}

#[tokio::test]
async fn a_signal_other_than_uncertain_needs_evidence() -> Result<()> {
    let attempt = two_prompt_attempt();
    let responses = responses_of(HARBOUR, HARBOUR);
    let model = judging(|prompt, earlier| {
        shown_prompts(prompt)
            .iter()
            .map(|handle| {
                if earlier == 0 && handle == "p2" {
                    finding_entry(handle, &[], "needs_practice")
                } else {
                    let first = sentence_handles(prompt, handle).remove(0);
                    finding_entry(handle, &[&first], "needs_practice")
                }
            })
            .collect()
    });

    let findings = evaluate(&model, &attempt, &[task("o1"), task("o2")], &responses).await?;

    assert!(findings.iter().all(|f| f.evidence_quote.is_some()));
    assert_eq!(model.count::<DiagnosticEvaluation>(), 2);
    Ok(())
}

#[tokio::test]
async fn an_uncertain_finding_needs_no_evidence() -> Result<()> {
    let attempt = two_prompt_attempt();
    let responses = responses_of(HARBOUR, "I am not sure which column matters here.");
    let model = judging(|prompt, _| {
        shown_prompts(prompt)
            .iter()
            .map(|handle| finding_entry(handle, &[], "uncertain"))
            .collect()
    });

    let findings = evaluate(&model, &attempt, &[task("o1"), task("o2")], &responses).await?;

    assert!(findings
        .iter()
        .all(|f| f.evidence_quote.is_none() && f.signal == LearningDiagnosticSignal::Uncertain));
    assert_eq!(model.count::<DiagnosticEvaluation>(), 1);
    Ok(())
}

#[tokio::test]
async fn a_prompt_missing_from_the_answer_is_asked_for_again_alone() -> Result<()> {
    let attempt = two_prompt_attempt();
    let responses = responses_of(HARBOUR, HARBOUR);
    let model = judging(|prompt, earlier| {
        shown_prompts(prompt)
            .iter()
            .filter(|handle| earlier > 0 || handle.as_str() != "p2")
            .map(|handle| {
                let first = sentence_handles(prompt, handle).remove(0);
                finding_entry(handle, &[&first], "ready_for_challenge")
            })
            .collect()
    });

    let findings = evaluate(&model, &attempt, &[task("o1"), task("o2")], &responses).await?;

    assert_eq!(findings.len(), 2);
    let asked: Vec<Vec<String>> = model
        .sent()
        .iter()
        .map(|sent| shown_prompts(&prompt_of(&sent.request)))
        .collect();
    assert_eq!(asked, [vec!["p1", "p2"], vec!["p2"]]);
    Ok(())
}

#[tokio::test]
async fn a_prompt_that_stays_unanswered_is_not_submitted() {
    let responses = responses_of(HARBOUR, HARBOUR);
    let model = judging(|_, _| Vec::new());

    let failed = evaluate(
        &model,
        &two_prompt_attempt(),
        &[task("o1"), task("o2")],
        &responses,
    )
    .await;

    let message = failed.unwrap_err().to_string();
    assert!(
        message.contains("could not be tied to your actual answers"),
        "{message}"
    );
}

#[tokio::test]
async fn prompt_snapshot_diagnostic_tasks() -> Result<()> {
    use crate::planning::outline_reference_tests::Capture;
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let (program, outcomes) = program_and_outcomes();
    let entries: Vec<_> = (0..outcomes.len())
        .map(|index| {
            json!({
                "of": format!("o{}", index + 1),
                "prompt": "Predict the result of this concrete case and explain why.",
                "expectedAnswer": "The result follows from the stated rule applied to the case.",
                "criteria": "Names the rule, applies it, and states the predicted result."
            })
        })
        .collect();
    let llm = Model(json!({ "entries": entries }).to_string());
    let capture = Capture::of::<DiagnosticTasks>(&llm);

    author(&capture, &program, &outcomes, &sources()).await?;

    snapshot::assert_requests(
        env!("CARGO_MANIFEST_DIR"),
        "diagnostic-tasks",
        &capture.requests(),
    );
    Ok(())
}

#[tokio::test]
async fn prompt_snapshot_diagnostic_evaluation() -> Result<()> {
    use crate::planning::outline_reference_tests::Capture;
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let responses = responses_of(
        "I would index the join column first, because it lets the planner skip a scan.",
        "I am not sure which column matters here.",
    );
    let reply = json!({"entries": [
        {"of": "p1", "evidence": ["u1"], "feedback": "You chose the join column and said why. The join column is the right first index, since the planner can then skip a scan. Next, check the plan.", "signal": "ready_for_challenge"},
        {"of": "p2", "evidence": [], "feedback": "You were unsure. The join column is the right first index, since the planner can then skip a scan. Next, read a plan.", "signal": "uncertain"}
    ]});
    let llm = Model(reply.to_string());
    let capture = Capture::of::<DiagnosticEvaluation>(&llm);

    evaluate(
        &capture,
        &two_prompt_attempt(),
        &[task("o1"), task("o2")],
        &responses,
    )
    .await?;

    snapshot::assert_requests(
        env!("CARGO_MANIFEST_DIR"),
        "diagnostic-evaluation",
        &capture.requests(),
    );
    Ok(())
}
