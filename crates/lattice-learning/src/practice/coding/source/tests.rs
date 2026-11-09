#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::persistence::hash_text;
use crate::practice::coding::proof::fakes::{Behaviour, FakeHarness, FakeRunner, ScriptedModel};
use std::sync::Mutex;

struct RecordingSteps {
    steps: Mutex<Vec<(u32, String)>>,
    stop_after: Option<usize>,
}

impl RecordingSteps {
    fn new() -> Self {
        Self {
            steps: Mutex::new(Vec::new()),
            stop_after: None,
        }
    }

    fn stopping_after(steps: usize) -> Self {
        Self {
            stop_after: Some(steps),
            ..Self::new()
        }
    }

    fn recorded(&self) -> Vec<(u32, String)> {
        self.steps.lock().unwrap().clone()
    }
}

#[async_trait]
impl BuildSteps for RecordingSteps {
    async fn step(&self, current: u32, message: &str) -> Result<bool> {
        let mut steps = self.steps.lock().unwrap();
        steps.push((current, message.to_owned()));
        Ok(self.stop_after.is_none_or(|limit| steps.len() <= limit))
    }
}

/// A runner whose code fails the listed cases when it contains the text, and passes the rest.
fn runner(fails: Vec<(&'static str, Vec<&'static str>)>) -> FakeRunner {
    FakeRunner::new(move |code, case| {
        let failing = fails
            .iter()
            .any(|(text, ids)| code.contains(text) && ids.contains(&case.id.as_str()));
        if failing {
            Behaviour::Fails
        } else {
            Behaviour::Passes { ms: 5 }
        }
    })
}

fn killers() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("BAD", vec!["hidden-1"]),
        ("LATE", vec!["hidden-1", "hidden-3"]),
        ("EARLY", vec!["example-2"]),
    ]
}

fn literal_case(category: &str) -> Value {
    json!({"category": category, "args": ["[1, 2, 3]", "5"], "expected": "[1, 2]"})
}

/// An answer that proves against `killers()` with the reference `REF`.
fn answer() -> Value {
    json!({
        "title": "Pair with a given sum",
        "statement": "Return the positions of the two numbers that add up to the target.",
        "function": "pair_positions",
        "params": [
            {"name": "numbers", "annotation": "list[int]"},
            {"name": "target", "annotation": "int"}
        ],
        "returns": "list[int]",
        "constraints": ["There is exactly one pair."],
        "clarifications": [],
        "targetComplexity": "O(n) time, O(n) space",
        "patterns": ["hash_map_lookup"],
        "compare": "exact",
        "tolerance": 0,
        "reference": "REF",
        "misreadings": [
            {"description": "Uses a number twice.", "code": "LATE"},
            {"description": "Stops at the first sum.", "code": "EARLY"},
            {"description": "Same as the reference.", "code": "SAME"}
        ],
        "examples": [literal_case("a pair at the start"), literal_case("a pair in the middle")],
        "hidden": [
            literal_case("duplicate numbers"),
            literal_case("negative numbers"),
            literal_case("the pair at the end")
        ],
        "scale": [{"category": "a long list", "args": ["list(range(50000))", "99997"]}]
    })
}

fn with(mut answer: Value, field: &str, value: Value) -> Value {
    answer[field] = value;
    answer
}

fn request(task: CodingTask) -> ProblemRequest {
    ProblemRequest {
        program_id: "program".into(),
        lesson: Some(LessonBrief {
            title: "Lookups".into(),
            objective: "Use a table to avoid repeated scans.".into(),
        }),
        task,
        language: "python".into(),
        difficulty: ProblemDifficulty::Easy,
        patterns: vec!["hash_map_lookup".into()],
        focus: None,
        details: None,
    }
}

async fn build(
    answers: Vec<Value>,
    runner: &FakeRunner,
    request: &ProblemRequest,
) -> (Result<Problem>, ScriptedModel) {
    let model = ScriptedModel::new(answers.iter().map(Value::to_string));
    let source = GeneratedProblems {
        llm: &model,
        runner,
        harness: &FakeHarness,
    };
    let problem = source
        .problem(request, &RecordingSteps::new(), CancellationToken::new())
        .await;
    (problem, model)
}

fn built(result: (Result<Problem>, ScriptedModel)) -> (Problem, ScriptedModel) {
    match result {
        (Ok(problem), model) => (problem, model),
        (Err(error), _) => panic!("expected a proven problem, got {error:?}"),
    }
}

#[tokio::test]
async fn a_reference_that_fails_a_case_is_asked_for_again_with_the_failure() {
    let failing = with(answer(), "reference", json!("REF BAD"));
    let runner = runner(killers());
    let (_, model) = built(
        build(
            vec![failing, answer()],
            &runner,
            &request(CodingTask::Problem),
        )
        .await,
    );
    assert_eq!(model.calls(), 2);
    let second = &model.texts()[1];
    assert!(second.contains("did not pass every case"), "{second}");
    assert!(second.contains("hidden-1"), "{second}");
}

#[tokio::test]
async fn a_misreading_that_passes_everything_is_dropped() {
    let runner = runner(killers());
    let (problem, model) = built(
        build(vec![answer()], &runner, &request(CodingTask::Problem)).await,
    );
    assert_eq!(model.calls(), 1);
    let ids: Vec<&str> = problem
        .misreadings
        .iter()
        .map(|misreading| misreading.id.as_str())
        .collect();
    assert_eq!(ids, ["misreading-1", "misreading-2"]);
}

#[tokio::test]
async fn fewer_than_two_killed_misreadings_asks_again() {
    let one_killed = with(
        answer(),
        "misreadings",
        json!([
            {"description": "Uses a number twice.", "code": "LATE"},
            {"description": "Same as the reference.", "code": "SAME"},
            {"description": "Also the same.", "code": "ALSO SAME"}
        ]),
    );
    let runner = runner(killers());
    let (problem, model) = built(
        build(
            vec![one_killed, answer()],
            &runner,
            &request(CodingTask::Problem),
        )
        .await,
    );
    assert_eq!(model.calls(), 2);
    assert!(model.texts()[1].contains("misreading-2"));
    assert_eq!(problem.misreadings.len(), 2);
}

#[tokio::test]
async fn a_statement_that_names_its_pattern_is_refused_and_asked_again() {
    let naming = with(
        answer(),
        "statement",
        json!("Use a Hash Map Lookup to find the positions of the two numbers that add up to the target."),
    );
    let runner = runner(killers());
    let (problem, model) = built(
        build(
            vec![naming, answer()],
            &runner,
            &request(CodingTask::Problem),
        )
        .await,
    );
    assert_eq!(model.calls(), 2);
    assert!(model.texts()[1].contains("names the pattern"));
    assert!(!problem.spec.statement.contains("Hash Map"));
    assert!(runner.codes().iter().all(|code| !code.is_empty()));
}

#[tokio::test]
async fn a_title_that_names_an_alias_of_its_pattern_is_refused() {
    let alias = PROBLEM_PATTERNS
        .iter()
        .find(|pattern| pattern.id == "hash_map_lookup")
        .and_then(|pattern| pattern.aliases.first())
        .copied()
        .unwrap();
    let naming = with(answer(), "title", json!(format!("A {alias} problem")));
    let runner = runner(killers());
    let (_, model) = built(
        build(
            vec![naming, answer()],
            &runner,
            &request(CodingTask::Problem),
        )
        .await,
    );
    assert_eq!(model.calls(), 2);
}

#[tokio::test]
async fn a_problem_that_is_never_proven_fails_with_the_last_reason_after_two_more_asks() {
    let failing = with(answer(), "reference", json!("REF BAD"));
    let runner = runner(killers());
    let (result, model) = build(
        vec![failing.clone(), failing.clone(), failing],
        &runner,
        &request(CodingTask::Problem),
    )
    .await;
    assert_eq!(model.calls(), MAX_PROOF_REASKS + 1);
    match result {
        Err(AppError::ModelAnswerUnusable(reason)) => {
            assert!(reason.contains("hidden-1"), "{reason}");
        }
        other => panic!("expected an unusable answer, got {other:?}"),
    }
}

#[tokio::test]
async fn killed_by_records_the_cases_that_kill_each_misreading() {
    let runner = runner(killers());
    let (problem, _) = built(
        build(vec![answer()], &runner, &request(CodingTask::Problem)).await,
    );
    let killed = |id: &str| {
        problem
            .misreadings
            .iter()
            .find(|misreading| misreading.id == id)
            .map(|misreading| misreading.killed_by.clone())
    };
    assert_eq!(
        killed("misreading-1"),
        Some(vec!["hidden-1".to_owned(), "hidden-3".to_owned()])
    );
    assert_eq!(killed("misreading-2"), Some(vec!["example-2".to_owned()]));
}

#[tokio::test]
async fn a_bug_fix_starts_from_the_misreading_killed_by_the_most_cases() {
    let runner = runner(killers());
    let (problem, _) = built(
        build(vec![answer()], &runner, &request(CodingTask::BugFix)).await,
    );
    assert_eq!(problem.bug.as_deref(), Some("misreading-1"));
    assert_eq!(problem.spec.starter, "LATE");
}

#[tokio::test]
async fn a_bug_fix_tie_goes_to_the_first_misreading() {
    let tied = vec![("LATE", vec!["hidden-1"]), ("EARLY", vec!["example-2"])];
    let runner = runner(tied);
    let (problem, _) = built(
        build(vec![answer()], &runner, &request(CodingTask::BugFix)).await,
    );
    assert_eq!(problem.bug.as_deref(), Some("misreading-1"));
}

#[tokio::test]
async fn a_problem_starts_from_its_signature_and_names_no_bug() {
    let runner = runner(killers());
    let (problem, _) = built(
        build(vec![answer()], &runner, &request(CodingTask::Problem)).await,
    );
    assert_eq!(
        problem.spec.starter,
        "def pair_positions(numbers: list[int], target: int) -> list[int]:\n    pass\n"
    );
    assert_eq!(problem.bug, None);
}

#[tokio::test]
async fn cases_are_examples_then_hidden_then_scale_with_the_scale_digest_and_time() {
    let runner = runner(killers());
    let (problem, _) = built(
        build(vec![answer()], &runner, &request(CodingTask::Problem)).await,
    );
    let ids: Vec<&str> = problem.cases.iter().map(|case| case.id.as_str()).collect();
    assert_eq!(
        ids,
        ["example-1", "example-2", "hidden-1", "hidden-2", "hidden-3", "scale-1"]
    );
    assert_eq!(problem.examples().count(), 2);
    let scale = problem.cases.last().unwrap();
    assert!(scale.scale);
    assert_eq!(scale.expected, None);
    assert_eq!(scale.digest, Some(hash_text("'big'")));
    assert_eq!(problem.reference_ms.get("scale-1"), Some(&5));
    assert!(problem.cases.iter().filter(|case| !case.scale).all(|case| case.digest.is_none()));
}

#[tokio::test]
async fn the_build_reports_writing_then_proving_and_stops_when_told_to() {
    let runner = runner(killers());
    let model = ScriptedModel::new([answer().to_string()]);
    let source = GeneratedProblems {
        llm: &model,
        runner: &runner,
        harness: &FakeHarness,
    };
    let steps = RecordingSteps::new();
    source
        .problem(
            &request(CodingTask::Problem),
            &steps,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        steps.recorded(),
        [
            (0, STEP_WRITING.to_owned()),
            (1, STEP_PROVING.to_owned())
        ]
    );

    let model = ScriptedModel::new([answer().to_string()]);
    let source = GeneratedProblems {
        llm: &model,
        runner: &runner,
        harness: &FakeHarness,
    };
    let stopped = RecordingSteps::stopping_after(0);
    let result = source
        .problem(
            &request(CodingTask::Problem),
            &stopped,
            CancellationToken::new(),
        )
        .await;
    assert!(matches!(result, Err(AppError::InvalidState(_))), "{result:?}");
    assert_eq!(model.calls(), 0);
}

#[tokio::test]
async fn a_draft_asked_for_again_is_reported_at_the_proving_step() {
    let failing = with(answer(), "reference", json!("REF BAD"));
    let runner = runner(killers());
    let model = ScriptedModel::new([failing.to_string(), answer().to_string()]);
    let source = GeneratedProblems {
        llm: &model,
        runner: &runner,
        harness: &FakeHarness,
    };
    let steps = RecordingSteps::new();
    source
        .problem(
            &request(CodingTask::Problem),
            &steps,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let indexes: Vec<u32> = steps.recorded().iter().map(|(index, _)| *index).collect();
    assert!(indexes.windows(2).all(|pair| pair[0] <= pair[1]), "{indexes:?}");
}

#[tokio::test]
async fn a_case_with_the_wrong_number_of_arguments_is_asked_for_again() {
    let short = with(
        answer(),
        "examples",
        json!([
            {"category": "one argument", "args": ["[1, 2]"], "expected": "[0, 1]"},
            literal_case("a pair in the middle")
        ]),
    );
    let runner = runner(killers());
    let (_, model) = built(
        build(vec![short, answer()], &runner, &request(CodingTask::Problem)).await,
    );
    assert_eq!(model.calls(), 2);
    assert!(model.texts()[1].contains("example-1"));
}

#[tokio::test]
async fn every_requested_pattern_must_be_listed() {
    let mut wanted = request(CodingTask::Problem);
    wanted.patterns = vec!["hash_map_lookup".into(), "binary_search".into()];
    let both = with(answer(), "patterns", json!(["hash_map_lookup", "binary_search"]));
    let runner = runner(killers());
    let (problem, model) = built(build(vec![answer(), both], &runner, &wanted).await);
    assert_eq!(model.calls(), 2);
    assert!(model.texts()[1].contains("binary_search"));
    assert_eq!(problem.spec.patterns, ["hash_map_lookup", "binary_search"]);
}

#[tokio::test]
async fn a_request_for_an_unknown_pattern_is_refused_before_asking() {
    let mut unknown = request(CodingTask::Problem);
    unknown.patterns = vec!["no_such_pattern".into()];
    let runner = runner(killers());
    let (result, model) = build(vec![answer()], &runner, &unknown).await;
    assert!(matches!(result, Err(AppError::InvalidInput(_))), "{result:?}");
    assert_eq!(model.calls(), 0);
}

#[test]
fn the_schema_keeps_every_item_to_six_properties_and_offers_the_catalogue_as_an_enum() {
    fn items(schema: &Value, found: &mut Vec<usize>) {
        match schema {
            Value::Object(map) => {
                if let Some(Value::Object(item)) = map.get("items") {
                    if let Some(Value::Object(properties)) = item.get("properties") {
                        found.push(properties.len());
                    }
                }
                map.values().for_each(|value| items(value, found));
            }
            Value::Array(list) => list.iter().for_each(|value| items(value, found)),
            _ => {}
        }
    }
    let schema = schema();
    let mut sizes = Vec::new();
    items(&schema, &mut sizes);
    assert!(!sizes.is_empty());
    assert!(sizes.iter().all(|size| *size <= 6), "{sizes:?}");
    let offered: Vec<&str> = schema["properties"]["patterns"]["items"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let catalogue: Vec<&str> = PROBLEM_PATTERNS.iter().map(|pattern| pattern.id).collect();
    assert_eq!(offered, catalogue);
}
