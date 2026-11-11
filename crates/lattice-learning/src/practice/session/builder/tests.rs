#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::session::author::{Authored, Failed, Lesson};
use crate::practice::session::fake_kind::{fake_kind, Commissioned, ScriptedModel};
use lattice_core::domain::learning::practice::work::{
    Brief, Case, CaseVisibility, Difficulty, Draft, Misreading, Piece,
};
use lattice_core::shared::error::AppError;
use serde_json::json;
use std::sync::{Arc, Mutex};

const REFERENCE: [&str; 4] = ["a", "b", "c", "d"];

fn piece(slots: &[&str]) -> Piece {
    Piece::new(json!(slots))
}

fn cases() -> Vec<Case> {
    REFERENCE
        .iter()
        .enumerate()
        .map(|(index, expected)| Case {
            id: format!("c{index}"),
            title: format!("Slot {index}"),
            visibility: CaseVisibility::Hidden,
            body: json!({"index": index, "expected": expected}),
        })
        .collect()
}

fn draft(form: Form, reference: &[&str]) -> Draft {
    Draft {
        kind: KindId::new("letters"),
        form,
        brief: Brief {
            title: "Spell it".to_owned(),
            difficulty: Difficulty::Easy,
            spec: json!(null),
        },
        reference: piece(reference),
        misreadings: if form == Form::Repair {
            vec![Misreading {
                id: "m1".to_owned(),
                description: "Starts with the wrong letter.".to_owned(),
                piece: piece(&["x", "b", "c", "d"]),
                killed_by: Vec::new(),
            }]
        } else {
            Vec::new()
        },
        cases: cases(),
        starter: (form == Form::Build).then(|| piece(&["", "", "", ""])),
        asks: Vec::new(),
    }
}

fn right(form: Form) -> Draft {
    draft(form, &REFERENCE)
}

/// A draft whose reference gets slot 1 wrong: the fake truth refuses it with "The reference fails the cases c1.".
fn wrong_at_slot_one() -> Draft {
    draft(Form::Build, &["a", "x", "c", "d"])
}

const SLOT_ONE_REASON: &str = "The reference fails the cases c1.";

/// A draft whose reference gets slot 0 wrong.
fn wrong_at_slot_zero() -> Draft {
    draft(Form::Build, &["x", "b", "c", "d"])
}

const SLOT_ZERO_REASON: &str = "The reference fails the cases c0.";

fn written(draft: Draft, raw: &str) -> Authored {
    Authored::Draft {
        draft: Box::new(draft),
        raw: raw.to_owned(),
    }
}

fn lesson() -> Lesson {
    Lesson {
        id: "lesson-1".to_owned(),
        title: "Spelling".to_owned(),
        objective: "Spell four letters.".to_owned(),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Recorded {
    program_id: String,
    lesson_id: String,
    kind: String,
    form: Form,
    reason: String,
}

/// A store that remembers what it is told, and answers `known` when asked.
#[derive(Default)]
struct Memory {
    known: Option<String>,
    looked: Mutex<usize>,
    recorded: Mutex<Vec<Recorded>>,
}

impl Memory {
    fn knowing(reason: &str) -> Self {
        Self {
            known: Some(reason.to_owned()),
            ..Self::default()
        }
    }

    fn lookups(&self) -> usize {
        *self.looked.lock().unwrap()
    }

    fn recorded(&self) -> Vec<Recorded> {
        self.recorded.lock().unwrap().clone()
    }
}

#[async_trait]
impl NothingStore for Memory {
    async fn nothing_known(&self, _target: &Target<'_>) -> Result<Option<String>> {
        *self.looked.lock().unwrap() += 1;
        Ok(self.known.clone())
    }

    async fn record_nothing(&self, target: &Target<'_>, reason: &str) -> Result<()> {
        self.recorded.lock().unwrap().push(Recorded {
            program_id: target.program_id.to_owned(),
            lesson_id: target.lesson_id.to_owned(),
            kind: target.kind.as_str().to_owned(),
            form: target.form,
            reason: reason.to_owned(),
        });
        Ok(())
    }
}

struct Steps {
    seen: Mutex<Vec<(u32, String)>>,
    stop_after: Option<usize>,
}

impl Steps {
    fn new() -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
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
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait]
impl BuildSteps for Steps {
    async fn step(&self, current: u32, message: &str) -> Result<bool> {
        let mut seen = self.seen.lock().unwrap();
        seen.push((current, message.to_owned()));
        Ok(self.stop_after.is_none_or(|limit| seen.len() <= limit))
    }
}

async fn made(
    kind: &Kind,
    form: Form,
    lesson: Option<&Lesson>,
    store: &Memory,
    steps: &Steps,
) -> Result<Built> {
    let model = ScriptedModel::new([]);
    let options = json!({});
    let facts = json!(null);
    build(
        kind,
        &Making {
            program_id: "program",
            lesson,
            form,
            options: &options,
            facts: &facts,
        },
        &model,
        store,
        steps,
        CancellationToken::new(),
    )
    .await
}

fn exercise(built: Result<Built>) -> Exercise {
    match built {
        Ok(Built::Exercise(exercise)) => *exercise,
        other => panic!("expected an exercise, got {other:?}"),
    }
}

fn asked_of(asked: &Arc<Mutex<Vec<Commissioned>>>) -> Vec<Commissioned> {
    asked.lock().unwrap().clone()
}

fn writing() -> (u32, String) {
    (0, STEP_WRITING.to_owned())
}

fn writing_again() -> (u32, String) {
    (1, STEP_WRITING.to_owned())
}

fn proving() -> (u32, String) {
    (1, STEP_PROVING.to_owned())
}

#[tokio::test]
async fn a_draft_that_holds_is_built_after_one_ask() {
    let (kind, asked) = fake_kind(&REFERENCE)
        .authoring(vec![written(right(Form::Build), "first")])
        .build_authoring();
    let steps = Steps::new();
    let built = exercise(
        made(
            &kind,
            Form::Build,
            Some(&lesson()),
            &Memory::default(),
            &steps,
        )
        .await,
    );
    assert_eq!(built.form, Form::Build);
    assert_eq!(built.kind, KindId::new("letters"));
    assert_eq!(asked_of(&asked).len(), 1);
    assert_eq!(steps.recorded(), [writing(), proving()]);
}

#[tokio::test]
async fn the_author_is_told_the_form_and_the_lesson() {
    let (kind, asked) = fake_kind(&REFERENCE)
        .authoring(vec![written(right(Form::Repair), "first")])
        .build_authoring();
    let built = exercise(
        made(
            &kind,
            Form::Repair,
            Some(&lesson()),
            &Memory::default(),
            &Steps::new(),
        )
        .await,
    );
    assert_eq!(built.form, Form::Repair);
    assert_eq!(
        asked_of(&asked),
        [Commissioned {
            form: Form::Repair,
            lesson_id: Some("lesson-1".to_owned()),
            failed: Vec::new(),
        }]
    );
}

#[tokio::test]
async fn a_draft_the_proof_refuses_is_asked_again_with_the_reason_and_the_answer() {
    let (kind, asked) = fake_kind(&REFERENCE)
        .authoring(vec![
            written(wrong_at_slot_one(), "first answer"),
            written(right(Form::Build), "second answer"),
        ])
        .build_authoring();
    let steps = Steps::new();
    exercise(
        made(
            &kind,
            Form::Build,
            Some(&lesson()),
            &Memory::default(),
            &steps,
        )
        .await,
    );
    let asked = asked_of(&asked);
    assert_eq!(asked.len(), 2);
    assert!(asked[0].failed.is_empty());
    assert_eq!(
        asked[1].failed,
        [Failed {
            raw: "first answer".to_owned(),
            reason: SLOT_ONE_REASON.to_owned(),
        }]
    );
    assert_eq!(
        steps.recorded(),
        [writing(), proving(), writing_again(), proving()]
    );
}

#[tokio::test]
async fn every_refusal_is_answered_in_the_order_it_came() {
    let (kind, asked) = fake_kind(&REFERENCE)
        .authoring(vec![
            written(wrong_at_slot_one(), "first answer"),
            written(wrong_at_slot_zero(), "second answer"),
            written(right(Form::Build), "third answer"),
        ])
        .build_authoring();
    exercise(
        made(
            &kind,
            Form::Build,
            Some(&lesson()),
            &Memory::default(),
            &Steps::new(),
        )
        .await,
    );
    let asked = asked_of(&asked);
    assert_eq!(asked.len(), MAX_PROOF_REASKS + 1);
    assert_eq!(
        asked[2].failed,
        [
            Failed {
                raw: "first answer".to_owned(),
                reason: SLOT_ONE_REASON.to_owned(),
            },
            Failed {
                raw: "second answer".to_owned(),
                reason: SLOT_ZERO_REASON.to_owned(),
            },
        ]
    );
}

#[tokio::test]
async fn a_draft_that_is_never_proven_fails_with_the_last_reason_after_the_reasks() {
    let (kind, asked) = fake_kind(&REFERENCE)
        .authoring(vec![
            written(wrong_at_slot_one(), "first"),
            written(wrong_at_slot_one(), "second"),
            written(wrong_at_slot_zero(), "third"),
        ])
        .build_authoring();
    let built = made(
        &kind,
        Form::Build,
        Some(&lesson()),
        &Memory::default(),
        &Steps::new(),
    )
    .await;
    match built {
        Err(AppError::ModelAnswerUnusable(reason)) => assert_eq!(reason, SLOT_ZERO_REASON),
        other => panic!("expected an unusable answer, got {other:?}"),
    }
    assert_eq!(asked_of(&asked).len(), MAX_PROOF_REASKS + 1);
}

#[tokio::test]
async fn nothing_to_make_is_stored_and_ends_the_build() {
    let (kind, asked) = fake_kind(&REFERENCE)
        .authoring(vec![Authored::NothingToMake(
            "The lesson teaches no letters.".to_owned(),
        )])
        .build_authoring();
    let store = Memory::default();
    let steps = Steps::new();
    let built = made(&kind, Form::Build, Some(&lesson()), &store, &steps)
        .await
        .unwrap();
    assert_eq!(
        built,
        Built::NothingToMake("The lesson teaches no letters.".to_owned())
    );
    assert_eq!(
        store.recorded(),
        [Recorded {
            program_id: "program".to_owned(),
            lesson_id: "lesson-1".to_owned(),
            kind: "letters".to_owned(),
            form: Form::Build,
            reason: "The lesson teaches no letters.".to_owned(),
        }]
    );
    assert_eq!(asked_of(&asked).len(), 1);
    assert_eq!(steps.recorded(), [writing()]);
}

#[tokio::test]
async fn a_lesson_known_to_have_nothing_to_make_is_not_asked_again() {
    let (kind, asked) = fake_kind(&REFERENCE)
        .authoring(vec![written(right(Form::Build), "never asked")])
        .build_authoring();
    let store = Memory::knowing("The lesson teaches no letters.");
    let steps = Steps::new();
    let built = made(&kind, Form::Build, Some(&lesson()), &store, &steps)
        .await
        .unwrap();
    assert_eq!(
        built,
        Built::NothingToMake("The lesson teaches no letters.".to_owned())
    );
    assert!(asked_of(&asked).is_empty());
    assert!(store.recorded().is_empty());
    assert!(steps.recorded().is_empty());
}

#[tokio::test]
async fn a_build_without_a_lesson_never_consults_the_store() {
    let (kind, _) = fake_kind(&REFERENCE)
        .authoring(vec![Authored::NothingToMake("Nothing.".to_owned())])
        .build_authoring();
    let store = Memory::knowing("Known.");
    let built = made(&kind, Form::Build, None, &store, &Steps::new())
        .await
        .unwrap();
    assert_eq!(built, Built::NothingToMake("Nothing.".to_owned()));
    assert_eq!(store.lookups(), 0);
    assert!(store.recorded().is_empty());
}

#[tokio::test]
async fn a_build_that_is_stopped_asks_nothing() {
    let (kind, asked) = fake_kind(&REFERENCE)
        .authoring(vec![written(right(Form::Build), "never asked")])
        .build_authoring();
    let built = made(
        &kind,
        Form::Build,
        Some(&lesson()),
        &Memory::default(),
        &Steps::stopping_after(0),
    )
    .await;
    assert!(matches!(built, Err(AppError::InvalidState(_))), "{built:?}");
    assert!(asked_of(&asked).is_empty());
}

#[tokio::test]
async fn a_cancelled_build_stops_before_proving() {
    let (kind, _) = fake_kind(&REFERENCE)
        .authoring(vec![written(right(Form::Build), "first")])
        .build_authoring();
    let model = ScriptedModel::new([]);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let options = json!({});
    let facts = json!(null);
    let built = build(
        &kind,
        &Making {
            program_id: "program",
            lesson: None,
            form: Form::Build,
            options: &options,
            facts: &facts,
        },
        &model,
        &Memory::default(),
        &Steps::new(),
        cancel,
    )
    .await;
    assert!(matches!(built, Err(AppError::InvalidState(_))), "{built:?}");
}
