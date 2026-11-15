#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::proof::fakes::{Behaviour, FakeRunner, ScriptedModel, SCALE_DIGEST};
use crate::practice::kinds::program::shape::solution_piece;
use crate::practice::kinds::program::toolchain::FakeToolchains;
use crate::practice::kinds::program::{kind, KIND};
use crate::practice::session::author::Failed;
use crate::practice::session::builder::{BuildSteps, MAX_PROOF_REASKS, STEP_PROVING, STEP_WRITING};
use lattice_core::application::services::model_contract::ask::Answered;
use lattice_core::application::services::model_contract::shape::{Pointers, Shape};
use lattice_core::application::services::model_contract::shown::Scope;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::domain::learning::practice::work::{Draft as ExerciseDraft, Form};
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
fn runner(fails: Vec<(&'static str, Vec<&'static str>)>) -> Arc<FakeRunner> {
    Arc::new(FakeRunner::new(move |code, case| {
        let failing = fails
            .iter()
            .any(|(text, ids)| code.contains(text) && ids.contains(&case.id.as_str()));
        if failing {
            Behaviour::Fails
        } else {
            Behaviour::Passes { ms: 5 }
        }
    }))
}

/// The Program kind running its code on `runner`.
fn program(runner: &Arc<FakeRunner>) -> Kind {
    kind(Arc::new(FakeToolchains::sharing(runner.clone())))
}

fn killers() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("BAD", vec!["hidden-1"]),
        ("LATE", vec!["hidden-1", "hidden-3"]),
        ("EARLY", vec!["example-2"]),
    ]
}

fn literal_case(category: &str) -> Value {
    json!({"category": category, "args": [{"text": "[1, 2, 3]"}, {"text": "5"}], "expected": "[1, 2]"})
}

/// An answer that proves against `killers()` with the reference `REF`.
fn answer() -> Value {
    json!({
        "title": "Pair with a given sum",
        "statement": "Return the positions of the two numbers that add up to the target.",
        "functionName": "pair_positions",
        "params": [
            {"name": "numbers", "annotation": "list[int]"},
            {"name": "target", "annotation": "int"}
        ],
        "returns": "list[int]",
        "constraints": [{"text": "There is exactly one pair."}],
        "clarifications": [],
        "targetComplexity": "O(n) time, O(n) space",
        "patterns": [],
        "compare": "exact",
        "tolerance": "0",
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
        "scale": [{"category": "a long list", "args": [{"text": "list(range(50000))"}, {"text": "99997"}]}]
    })
}

fn with(mut answer: Value, field: &str, value: Value) -> Value {
    answer[field] = value;
    answer
}

fn request(task: Form) -> ProblemRequest {
    ProblemRequest {
        program_id: "program".into(),
        lesson: Some(LessonBrief {
            title: "Lookups".into(),
            objective: "Use a table to avoid repeated scans.".into(),
        }),
        form: task,
        language: "python".into(),
        difficulty: ProblemDifficulty::Easy,
        patterns: vec!["hash_map_lookup".into()],
        focus: None,
        details: None,
    }
}

async fn build(
    answers: Vec<Value>,
    runner: &Arc<FakeRunner>,
    request: &ProblemRequest,
) -> (Result<Problem>, ScriptedModel) {
    let model = ScriptedModel::new(answers.iter().map(Value::to_string));
    let problem = build_problem(
        &program(runner),
        request,
        &model,
        &RecordingSteps::new(),
        CancellationToken::new(),
    )
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
    let (_, model) = built(build(vec![failing, answer()], &runner, &request(Form::Build)).await);
    assert_eq!(model.calls(), 2);
    let second = &model.texts()[1];
    assert!(second.contains("did not pass every case"), "{second}");
    assert!(second.contains("hidden-1"), "{second}");
}

#[tokio::test]
async fn a_misreading_that_passes_everything_is_dropped() {
    let runner = runner(killers());
    let (problem, model) = built(build(vec![answer()], &runner, &request(Form::Build)).await);
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
    let (problem, model) =
        built(build(vec![one_killed, answer()], &runner, &request(Form::Build)).await);
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
    let (problem, model) =
        built(build(vec![naming, answer()], &runner, &request(Form::Build)).await);
    assert_eq!(model.calls(), 2);
    assert!(model.texts()[1].contains("names the pattern"));
    assert!(!problem.spec.statement.contains("Hash Map"));
    assert!(runner.codes().iter().all(|code| !code.is_empty()));
}

#[tokio::test]
async fn the_rules_state_what_a_scale_input_may_be_built_from() {
    let runner = runner(killers());
    let (_, model) = built(build(vec![answer()], &runner, &request(Form::Build)).await);
    assert!(model.texts()[0].contains(
        "- A scale case gives an argument expression, not a literal, built only from calls of range, list, tuple, dict, set, str, int, len, sum, min, max, sorted, reversed, zip and enumerate, literals, arithmetic, comparisons, conditional expressions, indexing and comprehensions, such as list(range(50000)), [3] * 100000 or [i * 7919 % 100003 for i in range(100000)]."
    ));
}

#[tokio::test]
async fn the_function_is_read_from_function_name_and_the_schema_asks_for_that_key() {
    let runner = runner(killers());
    let (problem, _) = built(build(vec![answer()], &runner, &request(Form::Build)).await);
    assert_eq!(problem.spec.signature.function, "pair_positions");
    let schema = answer_schema(&input_of(&request(Form::Build)));
    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(required.contains(&"functionName"), "{required:?}");
    assert!(!required.contains(&"function"), "{required:?}");
    assert!(schema["properties"].get("functionName").is_some());
    assert!(schema["properties"].get("function").is_none());
}

#[tokio::test]
async fn source_in_the_function_name_is_refused_and_asked_again() {
    let whole = with(
        answer(),
        "functionName",
        json!("def pair_positions(numbers, target):\n    return []"),
    );
    let runner = runner(killers());
    let (problem, model) =
        built(build(vec![whole, answer()], &runner, &request(Form::Build)).await);
    assert_eq!(model.calls(), 2);
    assert!(model.texts()[1].contains("is not a Python function name."));
    assert_eq!(problem.spec.signature.function, "pair_positions");
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
    let (_, model) = built(build(vec![naming, answer()], &runner, &request(Form::Build)).await);
    assert_eq!(model.calls(), 2);
}

#[tokio::test]
async fn a_problem_that_is_never_proven_fails_with_the_last_reason_after_two_more_asks() {
    let failing = with(answer(), "reference", json!("REF BAD"));
    let runner = runner(killers());
    let (result, model) = build(
        vec![failing.clone(), failing.clone(), failing],
        &runner,
        &request(Form::Build),
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
    let (problem, _) = built(build(vec![answer()], &runner, &request(Form::Build)).await);
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
    let (problem, _) = built(build(vec![answer()], &runner, &request(Form::Repair)).await);
    assert_eq!(problem.bug.as_deref(), Some("misreading-1"));
    assert_eq!(problem.spec.starter, "LATE");
}

#[tokio::test]
async fn a_bug_fix_tie_goes_to_the_first_misreading() {
    let tied = vec![("LATE", vec!["hidden-1"]), ("EARLY", vec!["example-2"])];
    let runner = runner(tied);
    let (problem, _) = built(build(vec![answer()], &runner, &request(Form::Repair)).await);
    assert_eq!(problem.bug.as_deref(), Some("misreading-1"));
}

#[tokio::test]
async fn a_problem_starts_from_its_signature_and_names_no_bug() {
    let runner = runner(killers());
    let (problem, _) = built(build(vec![answer()], &runner, &request(Form::Build)).await);
    assert_eq!(
        problem.spec.starter,
        "def pair_positions(numbers: list[int], target: int) -> list[int]:\n    pass\n"
    );
    assert_eq!(problem.bug, None);
}

#[tokio::test]
async fn cases_are_examples_then_hidden_then_scale_with_the_scale_digest_and_time() {
    let runner = runner(killers());
    let (problem, _) = built(build(vec![answer()], &runner, &request(Form::Build)).await);
    let ids: Vec<&str> = problem.cases.iter().map(|case| case.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "example-1",
            "example-2",
            "hidden-1",
            "hidden-2",
            "hidden-3",
            "scale-1"
        ]
    );
    assert_eq!(problem.examples().count(), 2);
    let scale = problem.cases.last().unwrap();
    assert!(scale.scale);
    assert_eq!(scale.expected, None);
    assert_eq!(scale.digest.as_deref(), Some(SCALE_DIGEST));
    assert_eq!(problem.reference_ms.get("scale-1"), Some(&5));
    assert!(problem
        .cases
        .iter()
        .filter(|case| !case.scale)
        .all(|case| case.digest.is_none()));
}

#[tokio::test]
async fn the_build_reports_writing_then_proving_and_stops_when_told_to() {
    let runner = runner(killers());
    let model = ScriptedModel::new([answer().to_string()]);
    let steps = RecordingSteps::new();
    build_problem(
        &program(&runner),
        &request(Form::Build),
        &model,
        &steps,
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        steps.recorded(),
        [(0, STEP_WRITING.to_owned()), (1, STEP_PROVING.to_owned())]
    );

    let model = ScriptedModel::new([answer().to_string()]);
    let stopped = RecordingSteps::stopping_after(0);
    let result = build_problem(
        &program(&runner),
        &request(Form::Build),
        &model,
        &stopped,
        CancellationToken::new(),
    )
    .await;
    assert!(
        matches!(result, Err(AppError::InvalidState(_))),
        "{result:?}"
    );
    assert_eq!(model.calls(), 0);
}

#[tokio::test]
async fn a_draft_asked_for_again_is_reported_at_the_proving_step() {
    let failing = with(answer(), "reference", json!("REF BAD"));
    let runner = runner(killers());
    let model = ScriptedModel::new([failing.to_string(), answer().to_string()]);
    let steps = RecordingSteps::new();
    build_problem(
        &program(&runner),
        &request(Form::Build),
        &model,
        &steps,
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let indexes: Vec<u32> = steps.recorded().iter().map(|(index, _)| *index).collect();
    assert!(
        indexes.windows(2).all(|pair| pair[0] <= pair[1]),
        "{indexes:?}"
    );
}

#[tokio::test]
async fn a_case_with_the_wrong_number_of_arguments_is_asked_for_again() {
    let short = with(
        answer(),
        "examples",
        json!([
            {"category": "one argument", "args": [{"text": "[1, 2]"}], "expected": "[0, 1]"},
            literal_case("a pair in the middle")
        ]),
    );
    let runner = runner(killers());
    let (_, model) = built(build(vec![short, answer()], &runner, &request(Form::Build)).await);
    assert_eq!(model.calls(), 2);
    assert!(model.texts()[1].contains("example-1"));
}

#[tokio::test]
async fn a_requested_pattern_is_recorded_without_the_model_listing_it() {
    let mut wanted = request(Form::Build);
    wanted.patterns = vec!["hash_map_lookup".into(), "binary_search".into()];
    let runner = runner(killers());
    let (problem, model) = built(build(vec![answer()], &runner, &wanted).await);
    assert_eq!(model.calls(), 1);
    assert_eq!(problem.spec.patterns, ["hash_map_lookup", "binary_search"]);
}

#[tokio::test]
async fn a_pattern_the_model_adds_by_its_handle_follows_the_requested_ones() {
    let extra = other_than(&["hash_map_lookup"])[0];
    let adding = with(answer(), "patterns", json!(["t1"]));
    let runner = runner(killers());
    let (problem, model) = built(build(vec![adding], &runner, &request(Form::Build)).await);
    assert_eq!(model.calls(), 1);
    assert_eq!(problem.spec.patterns, ["hash_map_lookup", extra]);
}

#[tokio::test]
async fn a_pattern_handle_that_names_nothing_costs_only_that_pattern() {
    let adding = with(answer(), "patterns", json!(["t1", "t99", "x3"]));
    let runner = runner(killers());
    let (problem, model) = built(build(vec![adding], &runner, &request(Form::Build)).await);
    assert_eq!(model.calls(), 1);
    assert_eq!(problem.spec.patterns.len(), 2);
}

#[tokio::test]
async fn a_problem_asked_for_no_pattern_must_name_one_and_is_asked_again_alone() {
    let mut open = request(Form::Build);
    open.patterns = Vec::new();
    let first = PROBLEM_PATTERNS[0].id;
    let named = with(answer(), "patterns", json!(["t1"]));
    let runner = runner(killers());
    let (problem, model) = built(build(vec![answer(), named], &runner, &open).await);
    assert_eq!(model.calls(), 2);
    assert!(model.texts()[1].contains("at least one pattern"));
    assert_eq!(problem.spec.patterns, [first]);
}

#[tokio::test]
async fn the_request_names_the_requested_pattern_and_offers_the_others_by_handle() {
    let runner = runner(killers());
    let (_, model) = built(build(vec![answer()], &runner, &request(Form::Build)).await);
    let sent = &model.texts()[0];
    let asked = &PROBLEM_PATTERNS[0];
    assert!(
        sent.contains(&format!("{}: {}", asked.name, asked.cue)),
        "{sent}"
    );
    let offered = other_than(&["hash_map_lookup"]);
    assert!(sent.contains("[t1]"), "{sent}");
    assert!(sent.contains(&format!("[t{}]", offered.len())), "{sent}");
    assert!(
        !sent.contains(&format!("[t{}]", offered.len() + 1)),
        "{sent}"
    );
    assert!(
        PROBLEM_PATTERNS
            .iter()
            .filter(|pattern| pattern.id.contains('_'))
            .all(|pattern| !sent.contains(pattern.id)),
        "{sent}"
    );
}

#[tokio::test]
async fn a_float_comparison_needs_a_tolerance_above_zero_and_is_asked_again() {
    let loose = with(
        with(answer(), "compare", json!("float_within")),
        "tolerance",
        json!("0"),
    );
    let fine = with(
        with(answer(), "compare", json!("float_within")),
        "tolerance",
        json!("0.001"),
    );
    let runner = runner(killers());
    let (problem, model) = built(build(vec![loose, fine], &runner, &request(Form::Build)).await);
    assert_eq!(model.calls(), 2);
    assert!(model.texts()[1].contains("needs a tolerance greater than zero"));
    assert!(problem
        .cases
        .iter()
        .filter(|case| !case.scale)
        .all(|case| case.compare == CaseCompare::FloatWithin { tolerance: 0.001 }));
}

#[tokio::test]
async fn an_answer_missing_a_field_is_asked_for_again() {
    let mut partial = answer();
    partial.as_object_mut().unwrap().remove("reference");
    let runner = runner(killers());
    let (_, model) = built(build(vec![partial, answer()], &runner, &request(Form::Build)).await);
    assert_eq!(model.calls(), 2);
}

#[test]
fn the_problem_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = Input {
        difficulty: Told(ProblemDifficulty::Medium),
        language: Told("python".into()),
        runtime: marks.told("Python 3.12, standard library only.".to_owned()),
        lesson: Some(Brief {
            title: marks.told("Reading the tide tables".to_owned()),
            objective: marks.told("Find the gate opening in a harbour almanac.".to_owned()),
        }),
        focus: Some(marks.told("Questions about kiln schedules".to_owned())),
        details: Some(marks.told("A harbour logistics interview".to_owned())),
        wanted: vec![Wanted {
            id: marks.internal("pattern-key-0001".to_owned()),
            name: marks.told("Tide windows".to_owned()),
            cue: marks.told("Something repeats on a cycle of the moon.".to_owned()),
        }],
        others: vec![Pattern {
            id: marks.reference("pattern-key-0002".to_owned()),
            name: marks.told("Mooring order".to_owned()),
            cue: marks.told("Boats are taken in the order they arrive.".to_owned()),
        }],
        repair: true,
        earlier: vec![Earlier {
            answer: marks.told("the first draft of the harbour problem".to_owned()),
            reason: marks.told("The reference failed the case hidden-1.".to_owned()),
        }],
    };
    holds::<AuthorProblem>(&input, &marks);
}

/// The ids of the catalogue other than `asked`, in catalogue order.
fn other_than(asked: &[&str]) -> Vec<&'static str> {
    PROBLEM_PATTERNS
        .iter()
        .map(|pattern| pattern.id)
        .filter(|id| !asked.contains(id))
        .collect()
}

/// What the author's request shows for `request`, without a transport.
fn input_of(request: &ProblemRequest) -> Input {
    Input {
        difficulty: Told(request.difficulty),
        language: Told(request.language.clone()),
        runtime: Told("Python.".into()),
        lesson: None,
        focus: None,
        details: None,
        wanted: request
            .patterns
            .iter()
            .filter_map(|id| problem_pattern(id))
            .map(|pattern| Wanted {
                id: Internal::of(pattern.id.to_owned()),
                name: Told(pattern.name.to_owned()),
                cue: Told(pattern.cue.to_owned()),
            })
            .collect(),
        others: PROBLEM_PATTERNS
            .iter()
            .filter(|pattern| !request.patterns.iter().any(|id| id == pattern.id))
            .map(|pattern| Pattern {
                id: pattern.id.to_owned(),
                name: Told(pattern.name.to_owned()),
                cue: Told(pattern.cue.to_owned()),
            })
            .collect(),
        repair: false,
        earlier: Vec::new(),
    }
}

/// The schema of the author's answer, with the patterns shown by handle.
fn answer_schema(input: &Input) -> Value {
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    showing.show(&input.others).unwrap();
    <<AuthorProblem as Answered>::Answer as Shape>::schema(&showing, Pointers::Plain)
        .unwrap()
        .as_json()
        .clone()
}

#[tokio::test]
async fn a_request_for_an_unknown_pattern_is_refused_before_asking() {
    let mut unknown = request(Form::Build);
    unknown.patterns = vec!["no_such_pattern".into()];
    let runner = runner(killers());
    let (result, model) = build(vec![answer()], &runner, &unknown).await;
    assert!(
        matches!(result, Err(AppError::InvalidInput(_))),
        "{result:?}"
    );
    assert_eq!(model.calls(), 0);
}

#[test]
fn the_schema_keeps_every_item_to_six_properties_and_offers_the_other_patterns_by_handle() {
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
    let input = input_of(&request(Form::Build));
    let schema = answer_schema(&input);
    let mut sizes = Vec::new();
    items(&schema, &mut sizes);
    assert!(!sizes.is_empty());
    assert!(sizes.iter().all(|size| *size <= 6), "{sizes:?}");
    let text = schema.to_string();
    assert!(
        PROBLEM_PATTERNS
            .iter()
            .filter(|pattern| pattern.id.contains('_'))
            .all(|pattern| !text.contains(pattern.id)),
        "{text}"
    );
    assert!(schema["properties"].get("tolerance").is_some());
}

async fn authored(
    form: Form,
    failed: &[Failed],
    request: &ProblemRequest,
    answers: Vec<Value>,
) -> (Result<Authored>, ScriptedModel) {
    let model = ScriptedModel::new(answers.iter().map(Value::to_string));
    let author = ProgramAuthor {
        tools: Arc::new(FakeToolchains::sharing(runner(killers()))),
    };
    let parts = parts_of(request).unwrap();
    let drafted = author
        .draft(&Commission {
            program_id: &request.program_id,
            lesson: parts.lesson.as_ref(),
            form,
            options: &parts.options,
            facts: &parts.facts,
            llm: &model,
            failed,
        })
        .await;
    (drafted, model)
}

fn draft_of(authored: Result<Authored>) -> (ExerciseDraft, String) {
    match authored {
        Ok(Authored::Draft { draft, raw }) => (*draft, raw),
        other => panic!("expected a draft, got {other:?}"),
    }
}

#[tokio::test]
async fn a_build_draft_is_the_answer_in_the_core_vocabulary_with_the_signature_as_its_starter() {
    let (drafted, model) = authored(Form::Build, &[], &request(Form::Build), vec![answer()]).await;
    let (draft, raw) = draft_of(drafted);
    assert_eq!(model.calls(), 1);
    assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), answer());
    assert_eq!(draft.kind.as_str(), KIND);
    assert_eq!(draft.form, Form::Build);
    assert_eq!(draft.reference, solution_piece("REF").unwrap());
    assert_eq!(
        draft.starter,
        Some(
            solution_piece(
                "def pair_positions(numbers: list[int], target: int) -> list[int]:
    pass
"
            )
            .unwrap()
        )
    );
    assert_eq!(draft.cases.len(), 6);
    assert_eq!(draft.misreadings.len(), 3);
    assert!(draft.asks.is_empty());
}

#[tokio::test]
async fn a_repair_draft_has_no_starter_of_its_own() {
    let (drafted, _) = authored(Form::Repair, &[], &request(Form::Repair), vec![answer()]).await;
    let (draft, _) = draft_of(drafted);
    assert_eq!(draft.form, Form::Repair);
    assert_eq!(draft.starter, None);
}

#[tokio::test]
async fn a_form_a_program_does_not_have_is_refused_before_asking() {
    let (drafted, model) =
        authored(Form::Complete, &[], &request(Form::Build), vec![answer()]).await;
    assert!(
        matches!(drafted, Err(AppError::InvalidState(_))),
        "{drafted:?}"
    );
    assert_eq!(model.calls(), 0);
}

#[tokio::test]
async fn the_drafts_refused_before_are_answered_in_the_conversation() {
    let failed = [Failed {
        raw: "the first answer".to_owned(),
        reason: "The reference is wrong.".to_owned(),
    }];
    let (drafted, model) =
        authored(Form::Build, &failed, &request(Form::Build), vec![answer()]).await;
    draft_of(drafted);
    let sent = &model.texts()[0];
    assert!(sent.contains("the first answer"), "{sent}");
    assert!(sent.contains("The reference is wrong."), "{sent}");
}

mod golden;
