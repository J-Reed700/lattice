#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::practical_dto::{LearningSimulationSpeaker, LearningSimulationTurnDto};
use crate::practice::session::fake_kind::{fake_kind, letters_exercise, ScriptedModel};
use crate::practice::session::kind::Kind;
use crate::practice::session::record::{
    DebriefSession, EventKind, SessionCheck, SessionEvent, SessionHint, SessionLearnerCase,
};
use lattice_core::domain::learning::practice::work::{
    CaseResult, CaseStatus, CheckReport, CheckScope, Form, LearnerCaseVerdict, PartRef, Piece,
    Rung, Setting, Stage,
};
use serde_json::{json, Value};

const T1: &str = "I would start with the first slot";
const T2: &str = "What does it hold?";

fn kind() -> Kind {
    fake_kind(&["a", "b", "c"]).with_learner_cases().build()
}

fn piece(slots: &[&str]) -> Piece {
    Piece::new(json!(slots))
}

fn turn(id: &str, speaker: LearningSimulationSpeaker, content: &str) -> LearningSimulationTurnDto {
    LearningSimulationTurnDto {
        id: id.to_owned(),
        ordinal: 0,
        speaker,
        content: content.to_owned(),
        citations: Vec::new(),
        model_name: None,
        created_at: 0,
    }
}

fn check(id: &str, scope: CheckScope, slots: &[&str], status: CaseStatus) -> SessionCheck {
    SessionCheck {
        id: id.to_owned(),
        scope,
        piece: piece(slots),
        report: CheckReport {
            results: vec![CaseResult {
                case_id: "case-1".to_owned(),
                status,
                detail: Value::Null,
            }],
            marks: Vec::new(),
            detail: Value::Null,
        },
        at: 0,
    }
}

fn hint(ordinal: u32, rung: Rung) -> SessionHint {
    SessionHint {
        ordinal,
        rung,
        text: "Look at the second slot.".to_owned(),
        stage: Stage::Make,
    }
}

fn event(ordinal: u32, kind: EventKind, at: i64) -> SessionEvent {
    SessionEvent { ordinal, kind, at }
}

/// Turns t1 (learner), t2 (counterpart); checks c1 (all, a x c, failed) and c2 (shown, a b c, passed); the learner
/// case 0; the hint 0; the draft a b c.
fn session() -> DebriefSession {
    DebriefSession {
        exercise: letters_exercise(),
        form: Form::Build,
        settings: vec![Setting::Coached],
        turns: vec![
            turn("t1", LearningSimulationSpeaker::Learner, T1),
            turn("t2", LearningSimulationSpeaker::Counterpart, T2),
        ],
        checks: vec![
            check("c1", CheckScope::All, &["a", "x", "c"], CaseStatus::Failed),
            check(
                "c2",
                CheckScope::Shown,
                &["a", "b", "c"],
                CaseStatus::Passed,
            ),
        ],
        learner_cases: vec![SessionLearnerCase {
            ordinal: 0,
            body: json!({"letter": "a"}),
            verdict: LearnerCaseVerdict::Right,
        }],
        hints: vec![hint(0, Rung::Nudge)],
        events: Vec::new(),
        draft: piece(&["a", "b", "c"]),
    }
}

fn quote(turn: &str, text: &str) -> Value {
    json!({"kind": "quote", "ref": turn, "text": text, "index": 0})
}

fn slot(check: &str, index: u32) -> Value {
    json!({"kind": "slot", "ref": check, "text": "", "index": index})
}

fn check_item(check: &str) -> Value {
    json!({"kind": "check", "ref": check, "text": "", "index": 0})
}

fn case_item(ordinal: &str) -> Value {
    json!({"kind": "case", "ref": ordinal, "text": "", "index": 0})
}

fn hint_item(ordinal: &str) -> Value {
    json!({"kind": "hint", "ref": ordinal, "text": "", "index": 0})
}

fn row(id: &str, points: Option<i64>, evidence: Vec<Value>) -> Value {
    json!({
        "criterionId": id,
        "status": if points.is_some() { "observed" } else { "insufficient_evidence" },
        "points": points,
        "evidence": evidence,
        "feedback": "You did this.",
        "nextStep": "Do that next time.",
    })
}

fn good_rows() -> Vec<Value> {
    vec![
        row("accuracy", Some(3), vec![quote("t1", "the first slot")]),
        row("process", Some(2), vec![check_item("c1")]),
        row("clarity", None, Vec::new()),
    ]
}

fn answer(rows: Vec<Value>) -> String {
    json!({"rows": rows}).to_string()
}

fn answer_stating(rows: Vec<Value>) -> String {
    json!({"rows": rows, "model": "Fill a, b, c.", "pitfall": "Do not reverse them."}).to_string()
}

async fn ask(
    answers: &[String],
    kind: &Kind,
    session: &DebriefSession,
) -> (Result<Debrief>, ScriptedModel) {
    let model = ScriptedModel::new(answers.iter().map(String::as_str));
    let result = debrief(&model, kind, session).await;
    (result, model)
}

fn row_of<'a>(debrief: &'a Debrief, id: &str) -> &'a DebriefRow {
    debrief
        .rows
        .iter()
        .find(|row| row.criterion_id == id)
        .expect("a row for the criterion")
}

#[tokio::test]
async fn a_clean_answer_is_asked_for_once_and_becomes_the_rows() {
    let (result, model) = ask(&[answer(good_rows())], &kind(), &session()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.requests().len(), 1);
    let ids: Vec<&str> = debrief
        .rows
        .iter()
        .map(|row| row.criterion_id.as_str())
        .collect();
    assert_eq!(ids, ["accuracy", "process", "clarity"]);
    let titles: Vec<&str> = debrief.rows.iter().map(|row| row.title.as_str()).collect();
    assert_eq!(titles, ["Accuracy", "Process", "Clarity"]);
    assert!(debrief.rows.iter().all(|row| row.max_points == MAX_POINTS));
    let accuracy = row_of(&debrief, "accuracy");
    assert_eq!(accuracy.status, ScoreStatus::Observed);
    assert_eq!(accuracy.points, Some(3));
    assert_eq!(
        accuracy.evidence,
        [Evidence::Quote {
            turn_id: "t1".into(),
            text: "the first slot".into()
        }]
    );
    assert_eq!(accuracy.feedback, "You did this.");
    assert_eq!(accuracy.next_step, "Do that next time.");
    let clarity = row_of(&debrief, "clarity");
    assert_eq!(clarity.status, ScoreStatus::InsufficientEvidence);
    assert_eq!(clarity.points, None);
    assert!(debrief.stated.is_empty());
}

#[tokio::test]
async fn every_kind_of_evidence_the_session_has_is_kept() {
    let everything = vec![
        quote("t1", "start with"),
        slot("c1", 1),
        check_item("c2"),
        case_item("0"),
        hint_item("0"),
    ];
    let mut rows = good_rows();
    rows[0] = row("accuracy", Some(4), everything);

    let (result, model) = ask(&[answer(rows)], &kind(), &session()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.requests().len(), 1, "nothing to ask again");
    assert_eq!(
        row_of(&debrief, "accuracy").evidence,
        [
            Evidence::Quote {
                turn_id: "t1".into(),
                text: "start with".into()
            },
            Evidence::Part {
                check_id: "c1".into(),
                parts: vec![PartRef::new("1")]
            },
            Evidence::Check {
                check_id: "c2".into()
            },
            Evidence::LearnerCase { ordinal: 0 },
            Evidence::Hint { ordinal: 0 },
        ]
    );
}

#[tokio::test]
async fn evidence_the_session_does_not_have_is_refused_with_the_reason() {
    let refused = [
        (
            quote("t9", "start with"),
            "quote: there is no turn t9.".to_owned(),
        ),
        (
            quote("t2", "What"),
            "quote: turn t2 is not the learner's.".to_owned(),
        ),
        (
            quote("t1", "words never said"),
            format!("quote: the words are not in turn t1, which says: {T1}"),
        ),
        (
            quote("t1", ""),
            format!("quote: the words are not in turn t1, which says: {T1}"),
        ),
        (slot("c9", 0), "slot: there is no check c9.".to_owned()),
        (slot("c1", 7), "slot: 7 is not in check c1.".to_owned()),
        (check_item("c9"), "check: there is no check c9.".to_owned()),
        (case_item("5"), "case: there is no case 5.".to_owned()),
        (
            case_item("first"),
            "case: there is no case first.".to_owned(),
        ),
        (hint_item("4"), "hint: there is no hint 4.".to_owned()),
    ];
    for (item, reason) in refused {
        let mut bad = good_rows();
        bad[0] = row("accuracy", Some(3), vec![item.clone()]);
        let (result, model) = ask(&[answer(bad), answer(good_rows())], &kind(), &session()).await;

        let debrief = result.expect("a debrief");
        let texts = model.texts();
        assert_eq!(texts.len(), 2, "{item} is asked again once");
        assert!(
            texts[1].contains(&format!("accuracy, evidence 1: {reason}")),
            "{item} is refused with: {reason}"
        );
        assert_eq!(row_of(&debrief, "accuracy").evidence.len(), 1);
    }
}

#[tokio::test]
async fn an_item_of_a_kind_the_kind_does_not_name_is_refused() {
    let mut bad = good_rows();
    bad[0] = row(
        "accuracy",
        Some(3),
        vec![json!({"kind": "lines", "ref": "c1", "text": "", "index": 0})],
    );
    let (result, model) = ask(&[answer(bad), answer(good_rows())], &kind(), &session()).await;

    result.expect("a debrief");
    assert_eq!(model.requests().len(), 2);
}

#[tokio::test]
async fn a_kind_without_learner_cases_does_not_take_them_as_evidence() {
    let plain = fake_kind(&["a", "b", "c"]).build();
    let mut session = session();
    session.learner_cases.clear();
    let mut bad = good_rows();
    bad[0] = row("accuracy", Some(3), vec![case_item("0")]);

    let (result, model) = ask(&[answer(bad), answer(good_rows())], &plain, &session).await;

    result.expect("a debrief");
    assert_eq!(model.requests().len(), 2);
}

#[tokio::test]
async fn one_re_ask_carries_every_fault() {
    let mut bad = good_rows();
    bad[0] = row(
        "accuracy",
        Some(3),
        vec![quote("t9", "start with"), check_item("c9")],
    );
    bad[1] = row("process", Some(2), vec![hint_item("4")]);
    let (result, model) = ask(&[answer(bad), answer(good_rows())], &kind(), &session()).await;

    result.expect("a debrief");
    let texts = model.texts();
    assert_eq!(texts.len(), 2);
    for fault in [
        "accuracy, evidence 1: quote: there is no turn t9.",
        "accuracy, evidence 2: check: there is no check c9.",
        "process, evidence 1: hint: there is no hint 4.",
    ] {
        assert!(texts[1].contains(fault), "the re-ask carries: {fault}");
    }
    assert!(texts[1]
        .contains("The app checked the evidence against the session and found these faults:"));
}

#[tokio::test]
async fn evidence_still_wrong_after_the_re_ask_is_dropped() {
    let mut bad = good_rows();
    bad[0] = row(
        "accuracy",
        Some(3),
        vec![quote("t1", "the first slot"), check_item("c9")],
    );
    let (result, model) = ask(&[answer(bad.clone()), answer(bad)], &kind(), &session()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.requests().len(), 2, "asked at most twice");
    let accuracy = row_of(&debrief, "accuracy");
    assert_eq!(accuracy.status, ScoreStatus::Observed);
    assert_eq!(accuracy.points, Some(3));
    assert_eq!(
        accuracy.evidence,
        [Evidence::Quote {
            turn_id: "t1".into(),
            text: "the first slot".into()
        }]
    );
}

#[tokio::test]
async fn a_row_left_without_evidence_becomes_insufficient_evidence() {
    let mut bad = good_rows();
    bad[0] = row("accuracy", Some(3), vec![check_item("c9")]);
    let (result, _) = ask(&[answer(bad.clone()), answer(bad)], &kind(), &session()).await;
    let debrief = result.expect("a debrief");

    let accuracy = row_of(&debrief, "accuracy");
    assert_eq!(accuracy.status, ScoreStatus::InsufficientEvidence);
    assert_eq!(accuracy.points, None);
    assert!(accuracy.evidence.is_empty());
    assert_eq!(accuracy.feedback, "You did this.");
}

#[tokio::test]
async fn points_that_break_the_rules_are_refused_with_the_reason() {
    let mut bad = good_rows();
    bad[0] = row("accuracy", Some(9), vec![check_item("c1")]);
    bad[1] = json!({
        "criterionId": "process", "status": "observed", "points": null,
        "evidence": [check_item("c1")], "feedback": "f", "nextStep": "n",
    });
    bad[2] = json!({
        "criterionId": "clarity", "status": "insufficient_evidence", "points": 1,
        "evidence": [check_item("c1")], "feedback": "f", "nextStep": "n",
    });
    let (result, model) = ask(&[answer(bad), answer(good_rows())], &kind(), &session()).await;

    result.expect("a debrief");
    let texts = model.texts();
    for fault in [
        "accuracy: points must be from 0 to 4, not 9.",
        "process: an observed row needs points.",
        "clarity: a row without enough evidence has no points.",
        "clarity: a row without enough evidence cites none.",
    ] {
        assert!(texts[1].contains(fault), "the re-ask carries: {fault}");
    }
}

#[tokio::test]
async fn an_observed_row_without_evidence_is_refused() {
    let mut bad = good_rows();
    bad[0] = row("accuracy", Some(3), Vec::new());
    let (result, model) = ask(&[answer(bad), answer(good_rows())], &kind(), &session()).await;

    result.expect("a debrief");
    assert!(model.texts()[1].contains("accuracy: an observed row needs evidence."));
}

#[tokio::test]
async fn a_row_for_an_unknown_criterion_refuses_the_answer() {
    let mut extra = good_rows();
    extra.push(row("speed", Some(1), vec![check_item("c1")]));
    let (result, model) = ask(&[answer(extra), answer(good_rows())], &kind(), &session()).await;

    result.expect("a debrief");
    assert!(model.texts()[1].contains("A row names a criterion that is not one of the three."));
}

#[tokio::test]
async fn a_missing_criterion_refuses_the_answer() {
    let mut short = good_rows();
    short.pop();
    let (result, model) = ask(&[answer(short), answer(good_rows())], &kind(), &session()).await;

    result.expect("a debrief");
    assert!(model.texts()[1].contains("Criterion clarity must have exactly one row; it has 0."));
}

#[tokio::test]
async fn a_criterion_written_twice_refuses_the_answer() {
    let mut twice = good_rows();
    twice.push(row("process", Some(1), vec![check_item("c1")]));
    let (result, model) = ask(&[answer(twice), answer(good_rows())], &kind(), &session()).await;

    result.expect("a debrief");
    assert!(model.texts()[1].contains("Criterion process must have exactly one row; it has 2."));
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_twice_is_an_error() {
    let mut short = good_rows();
    short.pop();
    let (result, _) = ask(&[answer(short.clone()), answer(short)], &kind(), &session()).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn silence_is_asked_for_nothing_again() {
    let silent = vec![
        row("accuracy", None, Vec::new()),
        row("process", None, Vec::new()),
        row("clarity", None, Vec::new()),
    ];
    let (result, model) = ask(&[answer(silent)], &kind(), &session()).await;
    let debrief = result.expect("a debrief");

    assert_eq!(model.requests().len(), 1);
    assert!(debrief
        .rows
        .iter()
        .all(|row| row.status == ScoreStatus::InsufficientEvidence));
}

#[test]
fn a_kind_with_three_criteria_and_nothing_stated_has_a_system_text_of_three() {
    let kind = kind();

    let system = system(&kind, Form::Build);

    assert!(system.starts_with(
        "You write the debrief of one practice session: a scorecard with one row for each of three criteria. The session below is the learner's work and the transcript of the session."
    ));
    assert!(system.contains(
        "Each criterion is scored from 0 to 4 and earns points as follows.\n- Accuracy (accuracy): Got the letters right.\n- Process (process): Worked in small steps.\n- Clarity (clarity): Said what they were doing."
    ));
    assert!(
        system.contains("Time per stage, checks, hints and settings are shown beside your scores;")
    );
    assert!(system.ends_with(
        "Each row gets feedback about what this session demonstrated, and one next step: a concrete thing to do next time. Write for the learner, in the second person."
    ));
}

#[test]
fn a_kind_with_three_criteria_and_nothing_stated_has_a_schema_of_three_and_no_stated_keys() {
    let schema = schema(&kind(), Form::Build);

    assert_eq!(schema["required"], json!(["rows"]));
    let keys: Vec<&str> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["rows"]);
    let row = &schema["properties"]["rows"]["items"];
    assert_eq!(
        row["properties"]["criterionId"]["enum"],
        json!(["accuracy", "process", "clarity"])
    );
    assert_eq!(row["properties"]["points"]["maximum"], json!(MAX_POINTS));
    assert_eq!(
        row["required"],
        json!([
            "criterionId",
            "status",
            "points",
            "evidence",
            "feedback",
            "nextStep"
        ])
    );
}

#[test]
fn a_kind_that_states_something_says_so_in_the_opening_and_the_closing() {
    let kind = fake_kind(&["a", "b", "c"])
        .stating_a_model_and_a_pitfall()
        .build();

    let system = system(&kind, Form::Build);

    assert!(system.starts_with(
        "You write the debrief of one practice session: a scorecard with one row for each of three criteria, and the model answer and the pitfall to avoid. The session below"
    ));
    assert!(system.ends_with(
        "a concrete thing to do next time. The model is how a strong learner fills the slots, and the pitfall is the mistake to avoid. Write for the learner, in the second person."
    ));
    let schema = schema(&kind, Form::Build);
    assert_eq!(schema["required"], json!(["rows", "model", "pitfall"]));
    assert_eq!(schema["properties"]["model"], json!({"type": "string"}));
    assert_eq!(schema["properties"]["pitfall"], json!({"type": "string"}));
}

#[test]
fn the_system_text_says_how_each_kind_of_evidence_is_cited() {
    let with_cases = system(&kind(), Form::Build);
    let without = system(&fake_kind(&["a", "b", "c"]).build(), Form::Build);

    assert!(with_cases.contains(
        "Every scored row rests on evidence from the session, each item of one of these kinds. A quote: ref is the id of a learner turn, text is words copied exactly from that turn. A slot: ref is a check id, index is a slot of that check, counted from 0. A check: ref is a check id. A case: ref is the case's ordinal. A hint: ref is the hint's ordinal. Leave index 0 when a slot is not cited. Cite only what is in the session; the app checks every item and drops one that is wrong."
    ));
    assert!(without.contains(
        "A check: ref is a check id. A hint: ref is the hint's ordinal. Leave index 0 when a slot is not cited."
    ));
}

#[test]
fn the_schema_lists_the_kinds_of_evidence_and_the_fields_of_an_item() {
    let item = |schema: &Value| {
        schema["properties"]["rows"]["items"]["properties"]["evidence"]["items"].clone()
    };

    let with_cases = item(&schema(&kind(), Form::Build));
    let without = item(&schema(&fake_kind(&["a", "b", "c"]).build(), Form::Build));

    assert_eq!(
        with_cases["properties"]["kind"],
        json!({"enum": ["quote", "slot", "check", "case", "hint"]})
    );
    assert_eq!(
        without["properties"]["kind"],
        json!({"enum": ["quote", "slot", "check", "hint"]})
    );
    assert_eq!(
        with_cases["required"],
        json!(["kind", "ref", "text", "index"])
    );
    assert_eq!(with_cases["additionalProperties"], json!(false));
    let keys: Vec<&str> = with_cases["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["kind", "ref", "text", "index"]);
    assert_eq!(
        with_cases["properties"]["index"],
        json!({"type": "integer"})
    );
}

#[tokio::test]
async fn a_kind_with_stated_keys_must_have_them_written() {
    let stating = fake_kind(&["a", "b", "c"])
        .stating_a_model_and_a_pitfall()
        .build();
    let without_pitfall = json!({"rows": good_rows(), "model": "Fill a, b, c."}).to_string();
    let blank_model =
        json!({"rows": good_rows(), "model": "  ", "pitfall": "Do not reverse them."}).to_string();
    for unwritten in [without_pitfall, blank_model] {
        let (result, model) = ask(
            &[unwritten, answer_stating(good_rows())],
            &stating,
            &session(),
        )
        .await;

        let debrief = result.expect("a debrief");
        assert!(
            model.texts()[1].contains("The model answer and the pitfall to avoid must be written.")
        );
        assert_eq!(
            debrief.stated,
            [
                ("model".to_owned(), "Fill a, b, c.".to_owned()),
                ("pitfall".to_owned(), "Do not reverse them.".to_owned()),
            ]
        );
    }
}

#[tokio::test]
async fn a_kind_that_states_nothing_ignores_what_the_model_adds() {
    let extra = json!({"rows": good_rows(), "model": "Fill a, b, c."}).to_string();
    let (result, model) = ask(&[extra], &kind(), &session()).await;

    let debrief = result.expect("a debrief");
    assert_eq!(model.requests().len(), 1);
    assert!(debrief.stated.is_empty());
}

#[test]
fn the_prompt_tells_the_session_in_the_order_the_kind_names_it() {
    let prompt = prompt(&kind(), &session()).expect("a prompt");
    let told: Value = serde_json::from_str(&prompt).unwrap();

    let keys: Vec<&str> = told
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "exercise",
            "form",
            "settings",
            "transcript",
            "cases",
            "checks",
            "finalDraft",
            "hints"
        ]
    );
    assert_eq!(
        told["exercise"],
        json!({"title": "Spell it", "reference": ["a", "b", "c"]})
    );
    assert_eq!(told["form"], json!("build"));
    assert_eq!(told["settings"], json!(["coached"]));
    assert_eq!(
        told["transcript"],
        json!([
            {"turnId": "t1", "speaker": "learner", "text": T1},
            {"turnId": "t2", "speaker": "counterpart", "text": T2},
        ])
    );
    assert_eq!(
        told["cases"],
        json!([{"ordinal": 0, "letter": "a", "verdict": "right"}])
    );
    assert_eq!(told["checks"][0]["id"], json!("c1"));
    assert_eq!(told["checks"][1]["id"], json!("c2"));
    assert_eq!(
        told["hints"],
        json!([{"ordinal": 0, "rung": "nudge", "text": "Look at the second slot.", "stage": "make"}])
    );
    assert!(prompt.starts_with("{\n  \"exercise\": {"), "pretty printed");
}

#[test]
fn a_kind_without_learner_cases_has_no_list_of_them_in_the_prompt() {
    let plain = fake_kind(&["a", "b", "c"]).build();
    let mut session = session();
    session.learner_cases.clear();

    let told: Value = serde_json::from_str(&prompt(&plain, &session).unwrap()).unwrap();

    assert!(told.get("cases").is_none());
}

#[test]
fn a_check_of_the_same_piece_as_the_one_before_says_so() {
    let mut session = session();
    session.checks = vec![
        check("c1", CheckScope::All, &["a", "b", "c"], CaseStatus::Passed),
        check(
            "c2",
            CheckScope::Shown,
            &["a", "b", "c"],
            CaseStatus::Passed,
        ),
    ];

    let told: Value = serde_json::from_str(&prompt(&kind(), &session).unwrap()).unwrap();

    assert_eq!(told["checks"][0]["slots"], json!(["a", "b", "c"]));
    assert_eq!(told["checks"][1]["slots"], json!("the same as check c1"));
}

#[test]
fn the_final_draft_is_told_only_when_no_check_holds_it() {
    let mut held = session();
    held.draft = piece(&["a", "b", "c"]);
    let mut changed = session();
    changed.draft = piece(&["a", "b", "z"]);
    let mut unchecked = session();
    unchecked.checks.clear();

    let held: Value = serde_json::from_str(&prompt(&kind(), &held).unwrap()).unwrap();
    let changed: Value = serde_json::from_str(&prompt(&kind(), &changed).unwrap()).unwrap();
    let unchecked: Value = serde_json::from_str(&prompt(&kind(), &unchecked).unwrap()).unwrap();

    assert_eq!(held["finalDraft"], Value::Null);
    assert_eq!(changed["finalDraft"], json!(["a", "b", "z"]));
    assert_eq!(unchecked["finalDraft"], json!(["a", "b", "c"]));
}

#[test]
fn a_session_with_learner_cases_for_a_kind_without_them_is_a_fault() {
    let plain = fake_kind(&["a", "b", "c"]).build();

    let result = prompt(&plain, &session());

    assert!(matches!(
        result,
        Err(lattice_core::shared::error::AppError::InvalidState(_))
    ));
}

#[test]
fn facts_count_checks_submits_hints_and_settings() {
    let mut session = session();
    session.checks = vec![
        check("c1", CheckScope::Shown, &["a"], CaseStatus::Passed),
        check("c2", CheckScope::Own, &["a"], CaseStatus::Ran),
        check("c3", CheckScope::All, &["a"], CaseStatus::Passed),
        check("c4", CheckScope::All, &["a"], CaseStatus::Passed),
    ];
    session.hints = vec![
        hint(0, Rung::Nudge),
        hint(1, Rung::Nudge),
        hint(2, Rung::Plan),
    ];
    session.settings = vec![Setting::Coached, Setting::Mock];

    let facts = facts(&kind(), &session.counted());

    assert_eq!(facts.checks, 4);
    assert_eq!(facts.submits, 2);
    assert_eq!(facts.hints, [(Rung::Nudge, 2), (Rung::Plan, 1)]);
    assert_eq!(facts.settings, [Setting::Coached, Setting::Mock]);
    assert!(facts.stage_ms.is_empty());
}

fn times(events: Vec<SessionEvent>) -> Vec<(Stage, u64)> {
    let mut session = session();
    session.events = events;
    facts(&kind(), &session.counted()).stage_ms
}

#[test]
fn stage_time_is_the_open_time_across_stage_moves_in_the_order_of_the_form() {
    let spent = times(vec![
        event(0, EventKind::Opened, 0),
        event(1, EventKind::Moved(Some(Stage::Make)), 2_000),
        event(2, EventKind::Moved(Some(Stage::Plan)), 5_000),
        event(3, EventKind::Closed, 6_000),
    ]);

    assert_eq!(
        spent,
        [
            (Stage::Understand, 2_000),
            (Stage::Plan, 1_000),
            (Stage::Make, 3_000)
        ]
    );
}

#[test]
fn a_span_the_app_never_closed_ends_at_the_last_event_before_the_next_open() {
    let spent = times(vec![
        event(0, EventKind::Opened, 0),
        event(1, EventKind::Other, 2_000),
        event(2, EventKind::Opened, 10_000),
        event(3, EventKind::Closed, 11_000),
    ]);

    assert_eq!(spent, [(Stage::Understand, 3_000)]);
}

#[test]
fn a_span_still_open_at_the_end_ends_at_the_last_event() {
    let spent = times(vec![
        event(0, EventKind::Opened, 0),
        event(1, EventKind::Other, 4_000),
    ]);

    assert_eq!(spent, [(Stage::Understand, 4_000)]);
}

#[test]
fn a_move_to_a_stage_the_row_does_not_name_splits_the_span_and_keeps_the_stage() {
    let spent = times(vec![
        event(0, EventKind::Opened, 0),
        event(1, EventKind::Moved(None), 1_000),
        event(2, EventKind::Closed, 3_000),
    ]);

    assert_eq!(spent, [(Stage::Understand, 3_000)]);
}

#[test]
fn events_are_read_in_ordinal_order() {
    let spent = times(vec![
        event(2, EventKind::Closed, 3_000),
        event(0, EventKind::Opened, 0),
        event(1, EventKind::Moved(Some(Stage::Plan)), 1_000),
    ]);

    assert_eq!(spent, [(Stage::Understand, 1_000), (Stage::Plan, 2_000)]);
}

#[test]
fn a_session_with_no_events_has_no_stage_time() {
    assert!(times(Vec::new()).is_empty());
}

#[test]
fn a_repair_lists_its_own_stages() {
    let mut session = session();
    session.form = Form::Repair;
    session.events = vec![
        event(0, EventKind::Opened, 0),
        event(1, EventKind::Moved(Some(Stage::Locate)), 1_000),
        event(2, EventKind::Closed, 3_000),
    ];

    let spent = facts(&kind(), &session.counted()).stage_ms;

    assert_eq!(spent, [(Stage::Reproduce, 1_000), (Stage::Locate, 2_000)]);
}

#[test]
fn the_engine_debrief_names_nothing_of_a_subject() -> std::io::Result<()> {
    let session = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/practice/session");
    let mut sources = vec![session.join("debrief.rs"), session.join("record.rs")];
    for entry in std::fs::read_dir(session.join("debrief"))? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "rs")
            && path.file_name().is_some_and(|name| name != "tests.rs")
        {
            sources.push(path);
        }
    }
    for path in sources {
        let source = std::fs::read_to_string(&path)?;
        for named in [
            "practice::coding",
            "practice::kinds",
            "kinds::program",
            "drills::runner",
            "lab_runtime",
        ] {
            assert!(!source.contains(named), "{} names {named}", path.display());
        }
    }
    Ok(())
}
