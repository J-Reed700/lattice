use super::*;
use crate::practice::coding::dto::ExampleVerdict;
use crate::practice::coding::problem::{
    CaseCompare, CaseStatus, CaseVisibility, Misreading, ProblemCase, ProblemDifficulty,
    ProblemSignature, ProblemSpec, SignatureParam,
};
use crate::practice::practical_dto::LearningSimulationSpeaker;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::domain::learning::practice::work::Rung;
use lattice_core::domain::learning::practice::ScoreStatus;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

struct ScriptedModel {
    answers: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl ScriptedModel {
    fn new(answers: Vec<String>) -> Self {
        Self {
            answers: Mutex::new(answers.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    /// What each request said, in order.
    fn said(&self) -> Vec<String> {
        self.requests
            .lock()
            .map(|requests| {
                requests
                    .iter()
                    .map(|request| {
                        request
                            .input
                            .iter()
                            .filter_map(|input| match input {
                                CompletionInput::Message { content, .. } => Some(content.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl LLMPort for ScriptedModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(request.clone());
        }
        let next = self
            .answers
            .lock()
            .ok()
            .and_then(|mut answers| answers.pop_front());
        next.map(CompletionResponse::from_text)
            .ok_or_else(|| AppError::InternalError("Unexpected extra model call".into()))
    }
    fn model_name(&self) -> &str {
        "scripted-debrief-model"
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

const SOLUTION: &str = "def two_sum(numbers, target):\n    seen = {}\n    for i, n in enumerate(numbers):\n        if target - n in seen:\n            return [seen[target - n], i]\n        seen[n] = i\n";

fn problem() -> Problem {
    Problem {
        spec: ProblemSpec {
            title: "Pair with a sum".into(),
            statement: "Return the indices of two numbers that add up to the target.".into(),
            signature: ProblemSignature {
                function: "two_sum".into(),
                params: vec![
                    SignatureParam {
                        name: "numbers".into(),
                        annotation: "list[int]".into(),
                    },
                    SignatureParam {
                        name: "target".into(),
                        annotation: "int".into(),
                    },
                ],
                returns: "list[int]".into(),
            },
            constraints: vec!["2 <= len(numbers)".into()],
            clarifications: vec![],
            language: "python".into(),
            difficulty: ProblemDifficulty::Easy,
            target_complexity: "O(n) time, O(n) space".into(),
            patterns: vec![],
            starter: "def two_sum(numbers, target):\n    pass\n".into(),
        },
        reference: SOLUTION.into(),
        misreadings: vec![Misreading {
            id: "reuse".into(),
            description: "Uses one number twice".into(),
            code: "def two_sum(numbers, target):\n    return [0, 0]\n".into(),
            killed_by: vec!["case-1".into()],
        }],
        cases: vec![ProblemCase {
            id: "case-1".into(),
            category: "a basic pair".into(),
            args: vec!["[2, 7]".into(), "9".into()],
            expected: Some("[0, 1]".into()),
            compare: CaseCompare::Exact,
            visibility: CaseVisibility::Example,
            scale: false,
            digest: None,
        }],
        reference_ms: BTreeMap::new(),
        bug: None,
    }
}

fn turn(id: &str, speaker: LearningSimulationSpeaker, content: &str) -> LearningSimulationTurnDto {
    LearningSimulationTurnDto {
        id: id.into(),
        ordinal: 0,
        speaker,
        content: content.into(),
        citations: vec![],
        model_name: None,
        created_at: 0,
    }
}

fn file(content: &str) -> Vec<LearningLabFile> {
    vec![LearningLabFile {
        path: "solution.py".into(),
        content: content.into(),
    }]
}

fn example(ordinal: u32) -> CodingExampleDto {
    CodingExampleDto {
        ordinal,
        args: vec!["[2, 7]".into(), "9".into()],
        expected: "[0, 1]".into(),
        verdict: ExampleVerdict::Right,
        expects: None,
        why_invalid: None,
    }
}

fn hint(ordinal: u32, rung: Rung) -> PracticeHintDto {
    PracticeHintDto {
        ordinal,
        rung,
        text: "Think about what you have already seen.".into(),
        reason: "The learner re-scans the list.".into(),
        lines: vec![],
        path: None,
        stage: Stage::Make,
        run_id: None,
        in_role: false,
        at: 0,
    }
}

fn run(run_id: &str, scope: CodingRunScope) -> EvidenceRun {
    EvidenceRun {
        run_id: run_id.into(),
        scope,
        files: file(SOLUTION),
        results: vec![CaseResult {
            case_id: "case-1".into(),
            status: CaseStatus::Passed,
            actual: Some("[0, 1]".into()),
            printed: String::new(),
            error: None,
            elapsed_ms: Some(1),
        }],
        created_at: 0,
    }
}

fn event(ordinal: u32, kind: &str, payload: Value, at: i64) -> CodingEventRow {
    CodingEventRow {
        ordinal,
        kind: kind.into(),
        payload,
        at,
    }
}

fn evidence() -> SessionEvidence {
    SessionEvidence {
        problem: problem(),
        form: Form::Build,
        settings: vec![Setting::Coached],
        turns: vec![
            turn(
                "t1",
                LearningSimulationSpeaker::Learner,
                "I would keep a dictionary of the numbers I have seen so far",
            ),
            turn(
                "t2",
                LearningSimulationSpeaker::Counterpart,
                "What does that cost?",
            ),
            turn(
                "t3",
                LearningSimulationSpeaker::Learner,
                "Linear time, since each number is visited once",
            ),
        ],
        runs: vec![run("r1", CodingRunScope::All)],
        examples: vec![example(0)],
        hints: vec![hint(0, Rung::Nudge)],
        events: vec![],
        draft: file(SOLUTION),
    }
}

fn quote(turn_id: &str, text: &str) -> Value {
    json!({"kind": "quote", "ref": turn_id, "text": text, "path": "", "start": 0, "end": 0})
}

fn run_item(run_id: &str) -> Value {
    json!({"kind": "run", "ref": run_id, "text": "", "path": "", "start": 0, "end": 0})
}

fn lines_item(run_id: &str, path: &str, start: i64, end: i64) -> Value {
    json!({"kind": "lines", "ref": run_id, "text": "", "path": path, "start": start, "end": end})
}

fn example_item(ordinal: &str) -> Value {
    json!({"kind": "example", "ref": ordinal, "text": "", "path": "", "start": 0, "end": 0})
}

fn hint_item(ordinal: &str) -> Value {
    json!({"kind": "hint", "ref": ordinal, "text": "", "path": "", "start": 0, "end": 0})
}

fn row(id: &str, points: Option<i64>, evidence: Vec<Value>) -> Value {
    json!({
        "criterionId": id,
        "status": if points.is_some() { "observed" } else { "insufficient_evidence" },
        "points": points,
        "evidence": evidence,
        "feedback": "What this session showed.",
        "nextStep": "Do one thing next.",
    })
}

fn good_rows() -> Vec<Value> {
    vec![
        row(
            "problem_solving",
            Some(3),
            vec![quote("t1", "a dictionary of the numbers")],
        ),
        row("code", Some(4), vec![run_item("r1")]),
        row("testing", Some(2), vec![example_item("0")]),
        row("communication", Some(3), vec![hint_item("0")]),
    ]
}

fn answer(rows: Vec<Value>) -> String {
    json!({
        "rows": rows,
        "approach": "Remember what you have seen in a dictionary.",
        "complexity": "O(n) time, O(n) space",
    })
    .to_string()
}

async fn debrief_with(
    answers: Vec<String>,
    evidence: &SessionEvidence,
) -> (Result<Debrief>, ScriptedModel) {
    let model = ScriptedModel::new(answers);
    let result = CodingDebriefer { llm: &model }.debrief(evidence).await;
    (result, model)
}

fn row_of<'a>(debrief: &'a Debrief, id: &str) -> &'a ScorecardRow {
    debrief
        .scorecard
        .rows
        .iter()
        .find(|row| row.criterion_id == id)
        .expect("a row for the criterion")
}

#[tokio::test]
async fn a_clean_answer_is_asked_for_once_and_becomes_the_scorecard() {
    let (result, model) = debrief_with(vec![answer(good_rows())], &evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.said().len(), 1);
    let ids: Vec<&str> = debrief
        .scorecard
        .rows
        .iter()
        .map(|row| row.criterion_id.as_str())
        .collect();
    assert_eq!(ids, ["problem_solving", "code", "testing", "communication"]);
    let titles: Vec<&str> = debrief
        .scorecard
        .rows
        .iter()
        .map(|row| row.title.as_str())
        .collect();
    assert_eq!(
        titles,
        ["Problem solving", "Code", "Testing", "Communication"]
    );
    assert!(debrief.scorecard.rows.iter().all(|row| row.max_points == 4));
    let solving = row_of(&debrief, "problem_solving");
    assert_eq!(solving.status, ScoreStatus::Observed);
    assert_eq!(solving.points, Some(3));
    assert_eq!(
        solving.evidence,
        [ScorecardEvidence::Quote {
            turn_id: "t1".into(),
            text: "a dictionary of the numbers".into()
        }]
    );
    assert_eq!(
        debrief.approach,
        "Remember what you have seen in a dictionary."
    );
    assert_eq!(debrief.complexity, "O(n) time, O(n) space");
    debrief.scorecard.validate().expect("a valid scorecard");
}

#[tokio::test]
async fn a_quote_not_in_a_learner_turn_is_refused_then_dropped() {
    let mut rows = good_rows();
    rows[0] = row(
        "problem_solving",
        Some(3),
        vec![
            quote("t1", "a sorted array and two pointers"),
            quote("t3", "each number is visited once"),
        ],
    );
    let (result, model) = debrief_with(vec![answer(rows.clone()), answer(rows)], &evidence()).await;
    let debrief = result.expect("a debrief");

    let said = model.said();
    assert_eq!(said.len(), 2);
    assert!(
        said[1].contains("a sorted array and two pointers"),
        "the re-ask names the quote that is not there"
    );
    let solving = row_of(&debrief, "problem_solving");
    assert_eq!(solving.status, ScoreStatus::Observed);
    assert_eq!(
        solving.evidence,
        [ScorecardEvidence::Quote {
            turn_id: "t3".into(),
            text: "each number is visited once".into()
        }]
    );
}

#[tokio::test]
async fn a_quote_fixed_on_the_re_ask_is_kept() {
    let mut bad = good_rows();
    bad[0] = row(
        "problem_solving",
        Some(3),
        vec![quote("t1", "a sorted array")],
    );
    let (result, model) = debrief_with(vec![answer(bad), answer(good_rows())], &evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.said().len(), 2);
    assert_eq!(
        row_of(&debrief, "problem_solving").evidence,
        [ScorecardEvidence::Quote {
            turn_id: "t1".into(),
            text: "a dictionary of the numbers".into()
        }]
    );
}

#[tokio::test]
async fn words_the_counterpart_said_are_not_the_learners_evidence() {
    let mut rows = good_rows();
    rows[3] = row(
        "communication",
        Some(3),
        vec![quote("t2", "What does that cost?")],
    );
    let (result, model) = debrief_with(vec![answer(rows.clone()), answer(rows)], &evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.said().len(), 2);
    let communication = row_of(&debrief, "communication");
    assert_eq!(communication.status, ScoreStatus::InsufficientEvidence);
    assert_eq!(communication.points, None);
    assert!(communication.evidence.is_empty());
}

#[tokio::test]
async fn a_quote_must_come_from_the_turn_it_names() {
    let mut rows = good_rows();
    rows[0] = row(
        "problem_solving",
        Some(3),
        vec![quote("t3", "a dictionary of the numbers")],
    );
    let (result, _) = debrief_with(vec![answer(rows.clone()), answer(rows)], &evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(
        row_of(&debrief, "problem_solving").status,
        ScoreStatus::InsufficientEvidence
    );
}

#[tokio::test]
async fn a_row_left_with_no_valid_evidence_becomes_insufficient_evidence() {
    let mut rows = good_rows();
    rows[1] = row(
        "code",
        Some(4),
        vec![run_item("r9"), example_item("7"), hint_item("5")],
    );
    let (result, model) = debrief_with(vec![answer(rows.clone()), answer(rows)], &evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.said().len(), 2);
    let code = row_of(&debrief, "code");
    assert_eq!(code.status, ScoreStatus::InsufficientEvidence);
    assert_eq!(code.points, None);
    assert!(code.evidence.is_empty());
    assert_eq!(row_of(&debrief, "testing").points, Some(2));
    debrief.scorecard.validate().expect("a valid scorecard");
}

#[tokio::test]
async fn points_without_evidence_are_refused_and_never_kept() {
    let mut rows = good_rows();
    rows[2] = row("testing", Some(1), vec![]);
    let (result, model) = debrief_with(vec![answer(rows.clone()), answer(rows)], &evidence()).await;
    let debrief = result.expect("a debrief");

    let said = model.said();
    assert_eq!(said.len(), 2);
    assert!(said[1].contains("testing: an observed row needs evidence"));
    let testing = row_of(&debrief, "testing");
    assert_eq!(testing.status, ScoreStatus::InsufficientEvidence);
    assert_eq!(testing.points, None);
}

#[tokio::test]
async fn a_row_without_enough_evidence_keeps_neither_points_nor_evidence() {
    let mut rows = good_rows();
    rows[3] = json!({
        "criterionId": "communication",
        "status": "insufficient_evidence",
        "points": 0,
        "evidence": [hint_item("0")],
        "feedback": "The learner said little.",
        "nextStep": "Say the plan aloud.",
    });
    let (result, model) = debrief_with(vec![answer(rows.clone()), answer(rows)], &evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.said().len(), 2);
    let communication = row_of(&debrief, "communication");
    assert_eq!(communication.status, ScoreStatus::InsufficientEvidence);
    assert_eq!(communication.points, None);
    assert!(communication.evidence.is_empty());
}

#[tokio::test]
async fn silence_is_insufficient_evidence_and_asks_for_nothing_again() {
    let mut quiet = evidence();
    quiet.turns.clear();
    quiet.hints.clear();
    let mut rows = good_rows();
    rows[0] = row("problem_solving", None, vec![]);
    rows[3] = row("communication", None, vec![]);
    let (result, model) = debrief_with(vec![answer(rows)], &quiet).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.said().len(), 1);
    for id in ["problem_solving", "communication"] {
        let silent = row_of(&debrief, id);
        assert_eq!(silent.status, ScoreStatus::InsufficientEvidence);
        assert_eq!(silent.points, None, "silence is never zero");
    }
}

#[tokio::test]
async fn every_failure_goes_back_in_the_one_re_ask() {
    let mut rows = good_rows();
    rows[0] = row(
        "problem_solving",
        Some(3),
        vec![quote("t1", "words nobody said")],
    );
    rows[1] = row(
        "code",
        Some(4),
        vec![lines_item("r1", "solution.py", 1, 99)],
    );
    rows[2] = row("testing", Some(2), vec![example_item("42")]);
    let (result, model) = debrief_with(vec![answer(rows), answer(good_rows())], &evidence()).await;
    result.expect("a debrief");

    let said = model.said();
    assert_eq!(said.len(), 2);
    assert!(said[1].contains("the words are not in turn t1"));
    assert!(said[1].contains("code, evidence 1"));
    assert!(said[1].contains("no example 42"));
}

#[tokio::test]
async fn it_asks_at_most_twice() {
    let mut rows = good_rows();
    rows[0] = row(
        "problem_solving",
        Some(3),
        vec![quote("t1", "nothing like it")],
    );
    let (result, model) = debrief_with(
        vec![answer(rows.clone()), answer(rows.clone()), answer(rows)],
        &evidence(),
    )
    .await;
    result.expect("a debrief");

    assert_eq!(model.said().len(), 2);
}

#[tokio::test]
async fn lines_must_exist_in_the_file_of_that_run() {
    let mut rows = good_rows();
    rows[1] = row(
        "code",
        Some(4),
        vec![
            lines_item("r1", "solution.py", 2, 3),
            lines_item("r1", "solution.py", 5, 7),
            lines_item("r1", "other.py", 1, 1),
            lines_item("r1", "solution.py", 0, 2),
            lines_item("r1", "solution.py", 4, 2),
            lines_item("r2", "solution.py", 1, 1),
        ],
    );
    let (result, _) = debrief_with(vec![answer(rows.clone()), answer(rows)], &evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(
        row_of(&debrief, "code").evidence,
        [ScorecardEvidence::Lines {
            run_id: "r1".into(),
            path: "solution.py".into(),
            start: 2,
            end: 3
        }]
    );
}

#[tokio::test]
async fn a_run_an_example_and_a_hint_that_exist_are_kept() {
    let mut rows = good_rows();
    rows[1] = row(
        "code",
        Some(4),
        vec![run_item("r1"), example_item("0"), hint_item("0")],
    );
    let (result, model) = debrief_with(vec![answer(rows)], &evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.said().len(), 1);
    assert_eq!(
        row_of(&debrief, "code").evidence,
        [
            ScorecardEvidence::Run {
                run_id: "r1".into()
            },
            ScorecardEvidence::Example { ordinal: 0 },
            ScorecardEvidence::Hint { ordinal: 0 },
        ]
    );
}

#[tokio::test]
async fn a_missing_criterion_is_asked_for_again() {
    let mut short = good_rows();
    short.pop();
    let (result, model) = debrief_with(vec![answer(short), answer(good_rows())], &evidence()).await;
    let debrief = result.expect("a debrief");

    let said = model.said();
    assert_eq!(said.len(), 2);
    assert!(said[1].contains("communication"));
    assert_eq!(debrief.scorecard.rows.len(), 4);
}

#[tokio::test]
async fn a_criterion_written_twice_is_asked_for_again() {
    let mut twice = good_rows();
    twice.push(row("code", Some(1), vec![run_item("r1")]));
    let (result, model) = debrief_with(vec![answer(twice), answer(good_rows())], &evidence()).await;
    result.expect("a debrief");

    assert_eq!(model.said().len(), 2);
}

#[tokio::test]
async fn the_prompt_shows_the_session_and_the_criteria() {
    let (result, model) = debrief_with(vec![answer(good_rows())], &evidence()).await;
    result.expect("a debrief");

    let said = model.said();
    let prompt = said.first().expect("a request");
    for expected in [
        "t1",
        "a dictionary of the numbers",
        "O(n) time, O(n) space",
        "1: def two_sum(numbers, target):",
        "problem_solving",
        "communication",
    ] {
        assert!(prompt.contains(expected), "the prompt holds {expected}");
    }
}

#[test]
fn facts_count_runs_submits_hints_and_settings() {
    let mut session = evidence();
    session.runs = vec![
        run("r1", CodingRunScope::Shown),
        run("r2", CodingRunScope::Input),
        run("r3", CodingRunScope::All),
        run("r4", CodingRunScope::All),
    ];
    session.hints = vec![
        hint(0, Rung::Nudge),
        hint(1, Rung::Nudge),
        hint(2, Rung::Plan),
    ];
    session.settings = vec![Setting::Coached, Setting::Mock];

    let facts = facts(&session);

    assert_eq!(facts.runs, 4);
    assert_eq!(facts.submits, 2);
    assert_eq!(
        facts
            .hints
            .iter()
            .map(|hint| (hint.rung, hint.count))
            .collect::<Vec<_>>(),
        [(Rung::Nudge, 2), (Rung::Plan, 1)]
    );
    assert_eq!(facts.settings, [Setting::Coached, Setting::Mock]);
}

fn timeline() -> Vec<CodingEventRow> {
    vec![
        event(0, "stage", json!({"to": "understand"}), 0),
        event(1, "opened", json!({}), 1_000),
        event(
            2,
            "stage",
            json!({"from": "understand", "to": "examples", "skipped": []}),
            4_000,
        ),
        event(3, "closed", json!({}), 6_000),
        event(4, "opened", json!({}), 100_000),
        event(
            5,
            "stage",
            json!({"from": "examples", "to": "plan", "skipped": []}),
            101_000,
        ),
        event(
            6,
            "stage",
            json!({"from": "plan", "to": "make", "skipped": []}),
            105_000,
        ),
        event(7, "check", json!({"runId": "r1"}), 106_000),
        event(8, "opened", json!({}), 200_000),
        event(9, "closed", json!({}), 203_000),
    ]
}

fn times(session: &SessionEvidence) -> Vec<(Stage, u64)> {
    facts(session)
        .stage_ms
        .iter()
        .map(|time| (time.stage, time.active_ms))
        .collect()
}

#[test]
fn stage_time_is_the_open_time_across_stage_moves() {
    let mut session = evidence();
    session.events = timeline();

    assert_eq!(
        times(&session),
        [
            (Stage::Understand, 3_000),
            (Stage::Examples, 3_000),
            (Stage::Plan, 4_000),
            (Stage::Make, 4_000),
        ]
    );
}

#[test]
fn a_span_the_app_never_closed_ends_at_the_last_event_before_the_next_open() {
    let mut session = evidence();
    session.events = vec![
        event(0, "stage", json!({"to": "understand"}), 0),
        event(1, "opened", json!({}), 1_000),
        event(2, "check", json!({}), 9_000),
        event(3, "opened", json!({}), 500_000),
        event(4, "closed", json!({}), 502_000),
    ];

    assert_eq!(times(&session), [(Stage::Understand, 10_000)]);
}

#[test]
fn a_span_still_open_at_the_end_ends_at_the_last_event() {
    let mut session = evidence();
    session.events = vec![
        event(0, "stage", json!({"to": "understand"}), 0),
        event(1, "opened", json!({}), 1_000),
        event(2, "stage", json!({"to": "make"}), 3_000),
        event(3, "check", json!({}), 8_000),
    ];

    assert_eq!(
        times(&session),
        [(Stage::Understand, 2_000), (Stage::Make, 5_000)]
    );
}

#[test]
fn time_while_the_clock_is_closed_is_not_counted() {
    let mut session = evidence();
    session.events = vec![
        event(0, "stage", json!({"to": "understand"}), 0),
        event(1, "opened", json!({}), 1_000),
        event(2, "closed", json!({}), 2_000),
        event(3, "stage", json!({"to": "plan"}), 50_000),
        event(4, "opened", json!({}), 60_000),
        event(5, "closed", json!({}), 61_000),
    ];

    assert_eq!(
        times(&session),
        [(Stage::Understand, 1_000), (Stage::Plan, 1_000)]
    );
}

#[test]
fn events_are_read_in_ordinal_order() {
    let mut session = evidence();
    let mut shuffled = timeline();
    shuffled.reverse();
    session.events = shuffled;

    assert_eq!(
        times(&session),
        [
            (Stage::Understand, 3_000),
            (Stage::Examples, 3_000),
            (Stage::Plan, 4_000),
            (Stage::Make, 4_000),
        ]
    );
}

#[test]
fn a_session_with_no_events_has_no_stage_time() {
    assert!(facts(&evidence()).stage_ms.is_empty());
}

#[test]
fn a_bug_fix_lists_its_own_stages() {
    let mut session = evidence();
    session.form = Form::Repair;
    session.events = vec![
        event(0, "stage", json!({"to": "reproduce"}), 0),
        event(1, "opened", json!({}), 0),
        event(2, "stage", json!({"to": "locate"}), 2_000),
        event(3, "closed", json!({}), 5_000),
    ];

    assert_eq!(
        times(&session),
        [(Stage::Reproduce, 2_000), (Stage::Locate, 3_000)]
    );
}

mod golden;
