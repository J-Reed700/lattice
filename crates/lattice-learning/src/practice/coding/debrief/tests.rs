use super::*;
use crate::lessons::structured_call::task_messages;
use crate::practice::coding::problem::{
    CaseCompare, CaseStatus, CaseVisibility, Misreading, ProblemCase, ProblemDifficulty,
    ProblemSignature, ProblemSpec, SignatureParam,
};
use crate::practice::kinds::program::dto::ExampleVerdict;
use crate::practice::practical_dto::LearningSimulationSpeaker;
use crate::practice::session::debrief::{DebriefAnswer, DebriefStatements};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use lattice_core::application::services::model_contract::Contract;
use lattice_core::domain::learning::practice::work::Rung;
use lattice_core::domain::learning::practice::ScoreStatus;
use std::collections::BTreeMap;

mod golden;

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

fn hint(ordinal: u32, rung: Rung) -> StoredHint {
    StoredHint {
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


fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

/// A model that answers the nth request of each contract with the nth of its texts, and with no entries after them.
fn scripted(
    rows: impl Fn(&Sent, usize) -> String + Send + Sync + 'static,
    statements: impl Fn(&Sent, usize) -> String + Send + Sync + 'static,
) -> ScriptedModel {
    ScriptedModel::new()
        .on_request::<DebriefAnswer>(move |sent, nth| reply(rows(sent, nth)))
        .on_request::<DebriefStatements>(move |sent, nth| reply(statements(sent, nth)))
}

fn in_turn(texts: Vec<String>) -> impl Fn(&Sent, usize) -> String + Send + Sync + 'static {
    move |_, nth| {
        texts
            .get(nth)
            .cloned()
            .unwrap_or_else(|| json!({"entries": []}).to_string())
    }
}

fn said_in(sent: &Sent) -> String {
    let (system, user) = task_messages(&sent.request);
    format!("{system}\n{user}")
}

/// The handle the request gives the thing listed under `label`.
fn handle_of(sent: &Sent, label: &str) -> String {
    let text = said_in(sent);
    let rest = text
        .split_once(&format!("{label} ["))
        .map(|(_, rest)| rest)
        .unwrap_or_default();
    rest.split(']').next().unwrap_or_default().to_owned()
}

fn sent_to<C: Contract>(model: &ScriptedModel) -> Vec<Sent> {
    model
        .sent()
        .into_iter()
        .filter(|sent| sent.call.id == C::ID)
        .collect()
}

fn entry(of: &str, score: &str, cited: Value) -> Value {
    let mut entry = json!({
        "of": of,
        "score": score,
        "quotes": [],
        "parts": [],
        "checks": [],
        "cases": [],
        "hints": [],
        "feedback": "What this session showed.",
        "nextStep": "Do one thing next.",
    });
    if let (Some(fields), Some(cited)) = (entry.as_object_mut(), cited.as_object()) {
        for (key, handles) in cited {
            fields.insert(key.clone(), handles.clone());
        }
    }
    entry
}

fn good_entries() -> Vec<Value> {
    vec![
        entry("c1", "3", json!({"quotes": ["q1"]})),
        entry("c2", "4", json!({"checks": ["r1"]})),
        entry("c3", "2", json!({"cases": ["e1"]})),
        entry("c4", "3", json!({"hints": ["h1"]})),
    ]
}

fn answer(entries: Vec<Value>) -> String {
    json!({"entries": entries}).to_string()
}

fn statements() -> String {
    json!({"entries": [
        {"of": "k1", "text": "Remember what you have seen in a dictionary."},
        {"of": "k2", "text": "O(n) time, O(n) space"},
    ]})
    .to_string()
}

async fn debrief_with(
    rows: Vec<String>,
    evidence: &SessionEvidence,
) -> (Result<Debrief>, ScriptedModel) {
    let model = scripted(in_turn(rows), in_turn(vec![statements()]));
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
    let (result, model) = debrief_with(vec![answer(good_entries())], &evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.count::<DebriefAnswer>(), 1);
    assert_eq!(model.count::<DebriefStatements>(), 1);
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
            text: "I would keep a dictionary of the numbers I have seen so far".into()
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
async fn the_second_learner_turn_is_a_quote_and_the_counterpart_is_not() {
    let mut entries = good_entries();
    entries[0] = entry("c1", "3", json!({"quotes": ["q2"]}));
    entries[3] = entry("c4", "3", json!({"quotes": ["q3"]}));
    let (result, _) = debrief_with(vec![answer(entries)], &evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(
        row_of(&debrief, "problem_solving").evidence,
        [ScorecardEvidence::Quote {
            turn_id: "t3".into(),
            text: "Linear time, since each number is visited once".into()
        }]
    );
    let communication = row_of(&debrief, "communication");
    assert_eq!(communication.status, ScoreStatus::InsufficientEvidence);
    assert_eq!(communication.points, None);
    assert!(communication.evidence.is_empty());
}

#[tokio::test]
async fn a_row_left_with_no_valid_evidence_becomes_insufficient_evidence() {
    let mut entries = good_entries();
    entries[1] = entry(
        "c2",
        "4",
        json!({"checks": ["r9"], "cases": ["e7"], "hints": ["h5"]}),
    );
    let (result, model) = debrief_with(vec![answer(entries)], &evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.count::<DebriefAnswer>(), 1);
    let code = row_of(&debrief, "code");
    assert_eq!(code.status, ScoreStatus::InsufficientEvidence);
    assert_eq!(code.points, None);
    assert!(code.evidence.is_empty());
    assert_eq!(code.feedback, "What this session showed.");
    assert_eq!(row_of(&debrief, "testing").points, Some(2));
    debrief.scorecard.validate().expect("a valid scorecard");
}

#[tokio::test]
async fn points_without_any_evidence_are_never_kept() {
    let mut entries = good_entries();
    entries[2] = entry("c3", "1", json!({}));
    let (result, _) = debrief_with(vec![answer(entries)], &evidence()).await;
    let debrief = result.expect("a debrief");

    let testing = row_of(&debrief, "testing");
    assert_eq!(testing.status, ScoreStatus::InsufficientEvidence);
    assert_eq!(testing.points, None);
}

#[tokio::test]
async fn a_row_without_enough_evidence_keeps_neither_points_nor_evidence() {
    let mut entries = good_entries();
    entries[3] = entry(
        "c4",
        "insufficient_evidence",
        json!({"hints": ["h1"], "quotes": ["q1"]}),
    );
    let (result, _) = debrief_with(vec![answer(entries)], &evidence()).await;
    let debrief = result.expect("a debrief");

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
    let mut entries = good_entries();
    entries[0] = entry("c1", "insufficient_evidence", json!({}));
    entries[3] = entry("c4", "insufficient_evidence", json!({}));
    let (result, model) = debrief_with(vec![answer(entries)], &quiet).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.count::<DebriefAnswer>(), 1);
    for id in ["problem_solving", "communication"] {
        let silent = row_of(&debrief, id);
        assert_eq!(silent.status, ScoreStatus::InsufficientEvidence);
        assert_eq!(silent.points, None, "silence is never zero");
    }
}

#[tokio::test]
async fn consecutive_lines_of_one_run_are_one_item_and_the_others_stand_apart() {
    let mut entries = good_entries();
    entries[1] = entry("c2", "4", json!({"parts": ["l5", "l2", "l3", "l9"]}));
    let (result, _) = debrief_with(vec![answer(entries)], &evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(
        row_of(&debrief, "code").evidence,
        [
            ScorecardEvidence::Lines {
                run_id: "r1".into(),
                path: "solution.py".into(),
                start: 2,
                end: 3
            },
            ScorecardEvidence::Lines {
                run_id: "r1".into(),
                path: "solution.py".into(),
                start: 5,
                end: 5
            },
        ]
    );
}

#[tokio::test]
async fn a_run_an_example_and_a_hint_that_exist_are_kept() {
    let mut entries = good_entries();
    entries[1] = entry(
        "c2",
        "4",
        json!({"checks": ["r1"], "cases": ["e1"], "hints": ["h1"]}),
    );
    let (result, model) = debrief_with(vec![answer(entries)], &evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.count::<DebriefAnswer>(), 1);
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
async fn a_missing_criterion_is_asked_for_again_alone() {
    let model = scripted(
        |sent, nth| match nth {
            0 => answer(good_entries()[..3].to_vec()),
            _ => answer(vec![entry(
                &handle_of(sent, "Communication"),
                "3",
                json!({"hints": ["h1"]}),
            )]),
        },
        in_turn(vec![statements()]),
    );
    let result = CodingDebriefer { llm: &model }.debrief(&evidence()).await;
    let debrief = result.expect("a debrief");

    let asked = sent_to::<DebriefAnswer>(&model);
    assert_eq!(asked.len(), 2);
    assert!(said_in(&asked[1]).contains("- Communication ["));
    assert!(!said_in(&asked[1]).contains("- Code ["));
    assert_eq!(debrief.scorecard.rows.len(), 4);
    assert_eq!(row_of(&debrief, "communication").points, Some(3));
}

#[tokio::test]
async fn what_the_debrief_states_is_asked_for_again_when_it_comes_back_blank() {
    let model = scripted(
        in_turn(vec![answer(good_entries())]),
        |sent, nth| match nth {
            0 => json!({"entries": [
                {"of": "k1", "text": " "},
                {"of": "k2", "text": "O(n) time, O(n) space"},
            ]})
            .to_string(),
            _ => json!({"entries": [{
                "of": handle_of(sent, "approach"),
                "text": "Remember what you have seen in a dictionary.",
            }]})
            .to_string(),
        },
    );
    let result = CodingDebriefer { llm: &model }.debrief(&evidence()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.count::<DebriefStatements>(), 2);
    assert_eq!(
        debrief.approach,
        "Remember what you have seen in a dictionary."
    );
    assert_eq!(debrief.complexity, "O(n) time, O(n) space");
}

#[tokio::test]
async fn the_request_shows_the_session_and_the_criteria() {
    let (result, model) = debrief_with(vec![answer(good_entries())], &evidence()).await;
    result.expect("a debrief");

    let said = said_in(&sent_to::<DebriefAnswer>(&model)[0]);
    for expected in [
        "I would keep a dictionary of the numbers",
        "O(n) time, O(n) space",
        "1: def two_sum(numbers, target):",
        "- Problem solving [c1]",
        "- Communication [c4]",
    ] {
        assert!(said.contains(expected), "the request holds {expected}");
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

    let facts = facts(&session).unwrap();

    assert_eq!(facts.checks, 4);
    assert_eq!(facts.submits, 2);
    assert_eq!(facts.hints, [(Rung::Nudge, 2), (Rung::Plan, 1)]);
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
    facts(session).unwrap().stage_ms
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
    assert!(facts(&evidence()).unwrap().stage_ms.is_empty());
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
