#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::answer::{DebriefAnswer, Input, Score};
use super::plan::Plan;
use super::statements::DebriefStatements;
use super::telling::{render, Rendered};
use super::things::{Example, Hinted, Line, Run, Said, Scored};
use super::*;
use crate::lessons::structured_call::task_messages;
use crate::practice::practical_dto::{LearningSimulationSpeaker, LearningSimulationTurnDto};
use crate::practice::session::fake_kind::{fake_kind, letters_exercise};
use crate::practice::session::kind::Kind;
use crate::practice::session::record::{
    DebriefSession, EventKind, SessionCheck, SessionEvent, SessionHint, SessionLearnerCase,
};
use lattice_core::domain::learning::practice::work::{
    CaseResult, CaseStatus, CheckReport, CheckScope, CheckStatus, Form, LearnerCaseVerdict,
    PartRef, Piece, Rung, Setting, Stage,
};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::services::model_contract::shape::Vocabulary;
use lattice_core::application::services::model_contract::shown::Told;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use lattice_core::application::services::model_contract::Contract;
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
        status: CheckStatus::Passed,
        cases: Vec::new(),
    }
}

fn hint(ordinal: u32, rung: Rung) -> SessionHint {
    SessionHint {
        ordinal,
        rung,
        text: "Look at the second slot.".to_owned(),
        stage: Stage::Make,
        check_id: None,
        reason: String::new(),
        parts: Vec::new(),
        in_role: false,
        at: 0,
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
            catches: Vec::new(),
            found: Value::Null,
        }],
        hints: vec![hint(0, Rung::Nudge)],
        events: Vec::new(),
        draft: piece(&["a", "b", "c"]),
    }
}

const HOLE_S0: &str = "\u{1}s0\u{2}";

fn reply(
    text: String,
) -> BoxFuture<'static, lattice_core::shared::error::Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn scripted(rows: impl Fn(&Sent, usize) -> String + Send + Sync + 'static) -> ScriptedModel {
    ScriptedModel::new().on_request::<DebriefAnswer>(move |sent, nth| reply(rows(sent, nth)))
}

fn also_stating(
    model: ScriptedModel,
    texts: impl Fn(&Sent, usize) -> String + Send + Sync + 'static,
) -> ScriptedModel {
    model.on_request::<DebriefStatements>(move |sent, nth| reply(texts(sent, nth)))
}

/// The nth of `texts` for the nth request, and an answer with no entries after them.
fn in_turn(texts: Vec<String>) -> impl Fn(&Sent, usize) -> String + Send + Sync + 'static {
    move |_, nth| {
        texts
            .get(nth)
            .cloned()
            .unwrap_or_else(|| json!({"entries": []}).to_string())
    }
}

/// Everything a request says: its instructions and its task.
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
        "feedback": "You did this.",
        "nextStep": "Do that next time.",
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
        entry("c2", "2", json!({"checks": ["r1"]})),
        entry("c3", "insufficient_evidence", json!({})),
    ]
}

fn answer(entries: Vec<Value>) -> String {
    json!({"entries": entries}).to_string()
}

fn statements(model: &str, pitfall: &str) -> String {
    json!({"entries": [
        {"of": "k1", "text": model},
        {"of": "k2", "text": pitfall},
    ]})
    .to_string()
}

fn stating_kind() -> Kind {
    fake_kind(&["a", "b", "c"])
        .with_learner_cases()
        .stating_a_model_and_a_pitfall()
        .build()
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
    let model = scripted(in_turn(vec![answer(good_entries())]));
    let debrief = debrief(&model, &kind(), &session())
        .await
        .expect("a debrief");

    assert_eq!(model.count::<DebriefAnswer>(), 1);
    assert_eq!(model.count::<DebriefStatements>(), 0);
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
            text: T1.into()
        }]
    );
    assert_eq!(accuracy.feedback, "You did this.");
    assert_eq!(accuracy.next_step, "Do that next time.");
    let process = row_of(&debrief, "process");
    assert_eq!(
        process.evidence,
        [Evidence::Check {
            check_id: "c1".into()
        }]
    );
    let clarity = row_of(&debrief, "clarity");
    assert_eq!(clarity.status, ScoreStatus::InsufficientEvidence);
    assert_eq!(clarity.points, None);
    assert!(debrief.stated.is_empty());
}

#[tokio::test]
async fn prompt_snapshot_debrief_answer() {
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let model = scripted(in_turn(vec![answer(good_entries())]));
    debrief(&model, &kind(), &session())
        .await
        .expect("a debrief");
    let requests: Vec<_> = model
        .sent()
        .into_iter()
        .map(|sent| sent.request)
        .collect();
    snapshot::assert_requests(env!("CARGO_MANIFEST_DIR"), "debrief-answer", &requests);
}

#[tokio::test]
async fn every_kind_of_evidence_the_session_has_is_kept() {
    let everything = json!({
        "quotes": ["q1"],
        "parts": ["l2"],
        "checks": ["r2"],
        "cases": ["e1"],
        "hints": ["h1"],
    });
    let mut entries = good_entries();
    entries[0] = entry("c1", "4", everything);

    let model = scripted(in_turn(vec![answer(entries)]));
    let debrief = debrief(&model, &kind(), &session())
        .await
        .expect("a debrief");

    assert_eq!(model.count::<DebriefAnswer>(), 1, "nothing to ask again");
    assert_eq!(
        row_of(&debrief, "accuracy").evidence,
        [
            Evidence::Quote {
                turn_id: "t1".into(),
                text: T1.into()
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
async fn parts_the_kind_does_not_join_stay_items_of_their_own_in_the_order_of_the_session() {
    let mut entries = good_entries();
    entries[0] = entry("c1", "3", json!({"parts": ["l3", "l1", "l5"]}));

    let model = scripted(in_turn(vec![answer(entries)]));
    let debrief = debrief(&model, &kind(), &session())
        .await
        .expect("a debrief");

    assert_eq!(
        row_of(&debrief, "accuracy").evidence,
        [
            Evidence::Part {
                check_id: "c1".into(),
                parts: vec![PartRef::new("0")]
            },
            Evidence::Part {
                check_id: "c1".into(),
                parts: vec![PartRef::new("2")]
            },
            Evidence::Part {
                check_id: "c2".into(),
                parts: vec![PartRef::new("1")]
            },
        ]
    );
}

#[tokio::test]
async fn a_name_the_session_does_not_have_costs_only_that_citation() {
    let mut entries = good_entries();
    entries[0] = entry(
        "c1",
        "3",
        json!({"quotes": ["q1", "q9"], "hints": ["h1"]}),
    );

    let model = scripted(in_turn(vec![answer(entries)]));
    let debrief = debrief(&model, &kind(), &session())
        .await
        .expect("a debrief");

    assert_eq!(model.count::<DebriefAnswer>(), 1);
    let accuracy = row_of(&debrief, "accuracy");
    assert_eq!(accuracy.points, Some(3));
    assert_eq!(
        accuracy.evidence,
        [
            Evidence::Quote {
                turn_id: "t1".into(),
                text: T1.into()
            },
            Evidence::Hint { ordinal: 0 },
        ]
    );
}

#[tokio::test]
async fn each_sentence_of_a_learner_turn_is_a_quote_of_its_own() {
    let mut session = session();
    session.turns[0].content = "I would start here. Then I would check it.".to_owned();
    let mut entries = good_entries();
    entries[0] = entry("c1", "3", json!({"quotes": ["q2"]}));

    let model = scripted(in_turn(vec![answer(entries)]));
    let debrief = debrief(&model, &kind(), &session)
        .await
        .expect("a debrief");

    assert_eq!(
        row_of(&debrief, "accuracy").evidence,
        [Evidence::Quote {
            turn_id: "t1".into(),
            text: "Then I would check it.".into()
        }]
    );
}

#[tokio::test]
async fn a_criterion_the_answer_lacks_is_asked_again_alone_while_the_others_stand() {
    let model = scripted(|sent, nth| match nth {
        0 => answer(good_entries()[..2].to_vec()),
        _ => answer(vec![entry(
            &handle_of(sent, "Clarity"),
            "insufficient_evidence",
            json!({}),
        )]),
    });

    let debrief = debrief(&model, &kind(), &session())
        .await
        .expect("a debrief");

    let asked = sent_to::<DebriefAnswer>(&model);
    assert_eq!(asked.len(), 2);
    let again = said_in(&asked[1]);
    assert!(again.contains("- Clarity ["));
    assert!(!again.contains("- Accuracy ["));
    assert!(!again.contains("- Process ["));
    assert_eq!(row_of(&debrief, "accuracy").points, Some(3));
    assert_eq!(row_of(&debrief, "process").points, Some(2));
    assert_eq!(
        row_of(&debrief, "clarity").status,
        ScoreStatus::InsufficientEvidence
    );
}

#[tokio::test]
async fn a_criterion_still_unanswered_alone_is_a_row_without_enough_evidence() {
    let model = scripted(|_, nth| match nth {
        0 => answer(good_entries()[..2].to_vec()),
        _ => answer(Vec::new()),
    });

    let debrief = debrief(&model, &kind(), &session())
        .await
        .expect("a debrief");

    assert_eq!(debrief.rows.len(), 3);
    let clarity = row_of(&debrief, "clarity");
    assert_eq!(clarity.status, ScoreStatus::InsufficientEvidence);
    assert_eq!(clarity.points, None);
    assert!(clarity.evidence.is_empty());
    assert!(clarity.feedback.is_empty());
    assert!(clarity.next_step.is_empty());
    assert_eq!(clarity.max_points, MAX_POINTS);
    assert_eq!(row_of(&debrief, "accuracy").points, Some(3));
}

#[tokio::test]
async fn points_on_no_evidence_the_session_holds_become_insufficient_evidence_and_keep_the_words() {
    let mut entries = good_entries();
    entries[0] = entry("c1", "3", json!({}));
    entries[1] = entry("c2", "2", json!({"quotes": ["q9"]}));

    let model = scripted(in_turn(vec![answer(entries)]));
    let debrief = debrief(&model, &kind(), &session())
        .await
        .expect("a debrief");

    assert_eq!(model.count::<DebriefAnswer>(), 1);
    for id in ["accuracy", "process"] {
        let row = row_of(&debrief, id);
        assert_eq!(row.status, ScoreStatus::InsufficientEvidence, "{id}");
        assert_eq!(row.points, None, "{id}");
        assert!(row.evidence.is_empty(), "{id}");
        assert_eq!(row.feedback, "You did this.", "{id}");
        assert_eq!(row.next_step, "Do that next time.", "{id}");
    }
}

#[tokio::test]
async fn a_row_that_says_insufficient_evidence_cites_nothing() {
    let mut entries = good_entries();
    entries[2] = entry(
        "c3",
        "insufficient_evidence",
        json!({"quotes": ["q1"], "checks": ["r1"]}),
    );

    let model = scripted(in_turn(vec![answer(entries)]));
    let debrief = debrief(&model, &kind(), &session())
        .await
        .expect("a debrief");

    let clarity = row_of(&debrief, "clarity");
    assert_eq!(clarity.status, ScoreStatus::InsufficientEvidence);
    assert_eq!(clarity.points, None);
    assert!(clarity.evidence.is_empty());
}

#[tokio::test]
async fn a_kind_without_learner_cases_does_not_take_them_as_evidence() {
    let plain = fake_kind(&["a", "b", "c"]).build();
    let mut session = session();
    session.learner_cases.clear();
    let mut entries = good_entries();
    entries[0] = entry("c1", "3", json!({"cases": ["e1"]}));

    let model = scripted(in_turn(vec![answer(entries)]));
    let debrief = debrief(&model, &plain, &session).await.expect("a debrief");

    let said = said_in(&sent_to::<DebriefAnswer>(&model)[0]);
    assert!(!said.contains("A case:"));
    let accuracy = row_of(&debrief, "accuracy");
    assert_eq!(accuracy.points, None);
    assert!(accuracy.evidence.is_empty());
}

#[tokio::test]
async fn the_request_says_how_each_kind_of_evidence_is_cited_and_how_a_criterion_is_scored() {
    let model = scripted(in_turn(vec![answer(good_entries())]));
    debrief(&model, &kind(), &session())
        .await
        .expect("a debrief");

    let said = said_in(&sent_to::<DebriefAnswer>(&model)[0]);
    for sentence in [
        "A quote: quotes holds the names of the learner's sentences it cites.",
        "A slot: parts holds the names of the slots of a check it cites.",
        "A check: checks holds the names of the checks it cites.",
        "A case: cases holds the names of the cases it cites.",
        "A hint: hints holds the names of the hints it cites.",
        "Each criterion is scored from 0 to 4",
        "return score insufficient_evidence and cite nothing",
        "- Accuracy [c1]: ",
        "- Process [c2]: ",
        "- Clarity [c3]: ",
    ] {
        assert!(said.contains(sentence), "{sentence}");
    }
}

#[test]
fn the_scores_the_model_may_write_are_the_points_a_criterion_can_earn_and_no_evidence() {
    let top = Score::WORDS
        .iter()
        .filter_map(|(_, score)| match score {
            Score::Points(points) => Some(*points),
            Score::Insufficient => None,
        })
        .max();

    assert_eq!(top.map(i64::from), Some(i64::from(MAX_POINTS)));
    assert_eq!(Score::WORDS.len(), usize::from(MAX_POINTS) + 2);
}

#[tokio::test]
async fn what_the_kind_states_is_asked_beside_the_rows_from_the_exercise_alone() {
    let model = also_stating(
        scripted(in_turn(vec![answer(good_entries())])),
        in_turn(vec![statements("Fill a, b, c.", "Do not reverse them.")]),
    );

    let debrief = debrief(&model, &stating_kind(), &session())
        .await
        .expect("a debrief");

    assert_eq!(
        debrief.stated,
        [
            ("model".to_owned(), "Fill a, b, c.".to_owned()),
            ("pitfall".to_owned(), "Do not reverse them.".to_owned()),
        ]
    );
    assert_eq!(model.count::<DebriefStatements>(), 1);
    let said = said_in(&sent_to::<DebriefStatements>(&model)[0]);
    assert!(said.contains("- model [k1]"));
    assert!(said.contains("- pitfall [k2]"));
    assert!(said.contains("Spell it"));
    assert!(!said.contains("transcript"));
    assert!(!said_in(&sent_to::<DebriefAnswer>(&model)[0]).contains("- model ["));
}

#[tokio::test]
async fn a_blank_statement_is_refused_and_asked_for_again_alone() {
    let model = also_stating(
        scripted(in_turn(vec![answer(good_entries())])),
        |sent, nth| match nth {
            0 => statements("  ", "Do not reverse them."),
            _ => json!({"entries": [{"of": handle_of(sent, "model"), "text": "Fill a, b, c."}]})
                .to_string(),
        },
    );

    let debrief = debrief(&model, &stating_kind(), &session())
        .await
        .expect("a debrief");

    assert_eq!(
        debrief.stated,
        [
            ("model".to_owned(), "Fill a, b, c.".to_owned()),
            ("pitfall".to_owned(), "Do not reverse them.".to_owned()),
        ]
    );
    let asked = sent_to::<DebriefStatements>(&model);
    assert_eq!(asked.len(), 2);
    assert!(!said_in(&asked[1]).contains("- pitfall ["));
}

#[tokio::test]
async fn a_statement_the_model_never_writes_fails_the_debrief() {
    let model = also_stating(
        scripted(in_turn(vec![answer(good_entries())])),
        in_turn(Vec::new()),
    );

    let result = debrief(&model, &stating_kind(), &session()).await;

    assert!(result.is_err());
}

#[test]
fn the_answer_request_holds_what_it_is_told_and_no_identity_of_the_app() {
    let mut marks = Marks::new();
    let kind = kind();
    let plan = Plan::new(&kind, Form::Build);
    let scored = Scored {
        id: marks.reference("criterion-sentinel-0001".to_owned()),
        title: marks.told("A criterion title".to_owned()),
        earns: marks.told("What the criterion earns".to_owned()),
    };
    let note = marks.told("A told sentence of the session".to_owned());
    let rendered = Rendered {
        body: json!({
            "note": note.0,
            "transcript": [{"text": format!("{HOLE_S0} The sentence said")}],
            "runs": [{"id": "\u{1}r0\u{2}"}],
            "slots": ["\u{1}p0\u{2} a"],
            "cases": [{"case": "\u{1}e0\u{2}"}],
            "hints": [{"hint": "\u{1}h0\u{2}"}],
        }),
        runs: vec![Run {
            id: marks.reference("check-sentinel-0002".to_owned()),
        }],
        lines: vec![Line {
            check: marks.reference("check-sentinel-0003".to_owned()),
            part: marks.reference(PartRef::new("part-sentinel-0004")),
        }],
        said: vec![Said {
            turn: marks.reference("turn-sentinel-0005".to_owned()),
            index: 0,
            words: Told("The sentence said".to_owned()),
        }],
        examples: vec![Example {
            ordinal: marks.reference(424_242),
        }],
        hints: vec![Hinted {
            ordinal: marks.reference(535_353),
        }],
    };

    let input = Input::of(&plan, &rendered, &[&scored]);

    holds::<DebriefAnswer>(&input, &marks);
}

#[test]
fn the_session_is_told_in_the_order_the_kind_names_it_with_a_place_for_every_name() {
    let rendered = render(&kind(), &session()).expect("a told session");
    let told = rendered.body.clone();

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
            {"speaker": "learner", "text": format!("{HOLE_S0} {T1}")},
            {"speaker": "counterpart", "text": T2},
        ])
    );
    assert_eq!(
        told["cases"],
        json!([{"case": "\u{1}e0\u{2}", "letter": "a", "verdict": "right"}])
    );
    assert_eq!(told["checks"][0]["id"], json!("\u{1}r0\u{2}"));
    assert_eq!(told["checks"][1]["id"], json!("\u{1}r1\u{2}"));
    assert_eq!(
        told["hints"],
        json!([{"hint": "\u{1}h0\u{2}", "rung": "nudge", "text": "Look at the second slot.", "stage": "make"}])
    );
    let runs: Vec<&str> = rendered.runs.iter().map(|run| run.id.as_str()).collect();
    assert_eq!(runs, ["c1", "c2"]);
    assert_eq!(rendered.lines.len(), 6);
    assert_eq!(rendered.said.len(), 1);
    assert_eq!(rendered.examples.len(), 1);
    assert_eq!(rendered.hints.len(), 1);
}

#[test]
fn a_kind_without_learner_cases_has_no_list_of_them_in_the_told_session() {
    let plain = fake_kind(&["a", "b", "c"]).build();
    let mut session = session();
    session.learner_cases.clear();

    let told = render(&plain, &session).unwrap().body;

    assert!(told.get("cases").is_none());
}

#[test]
fn a_check_of_the_same_piece_as_the_one_before_says_so_and_names_no_part_twice() {
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

    let rendered = render(&kind(), &session).unwrap();
    let told = rendered.body;

    assert_eq!(
        told["checks"][0]["slots"],
        json!(["\u{1}p0\u{2} a", "\u{1}p1\u{2} b", "\u{1}p2\u{2} c"])
    );
    assert_eq!(
        told["checks"][1]["slots"],
        json!("the same as check \u{1}r0\u{2}")
    );
    assert_eq!(rendered.lines.len(), 3);
}

#[test]
fn the_final_draft_is_told_only_when_no_check_holds_it() {
    let mut held = session();
    held.draft = piece(&["a", "b", "c"]);
    let mut changed = session();
    changed.draft = piece(&["a", "b", "z"]);
    let mut unchecked = session();
    unchecked.checks.clear();

    let held = render(&kind(), &held).unwrap().body;
    let changed = render(&kind(), &changed).unwrap().body;
    let unchecked = render(&kind(), &unchecked).unwrap().body;

    assert_eq!(held["finalDraft"], Value::Null);
    assert_eq!(changed["finalDraft"], json!(["a", "b", "z"]));
    assert_eq!(unchecked["finalDraft"], json!(["a", "b", "c"]));
}

#[test]
fn a_session_with_learner_cases_for_a_kind_without_them_is_a_fault() {
    let plain = fake_kind(&["a", "b", "c"]).build();

    let result = render(&plain, &session());

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
fn the_engines_debrief_and_fit_check_name_nothing_of_a_subject() -> std::io::Result<()> {
    let practice = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/practice");
    let session = practice.join("session");
    let mut sources = vec![
        session.join("debrief.rs"),
        session.join("record.rs"),
        practice.join("practice_fit.rs"),
    ];
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
            "program code",
        ] {
            assert!(!source.contains(named), "{} names {named}", path.display());
        }
    }
    Ok(())
}
