//! The learner's turn rule and the rating a finish carries, on a store that only keeps what it is given.

#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

use super::*;
use crate::practice::session::counterpart::Reply;
use crate::practice::session::debrief::Debrief;
use crate::practice::session::fake_kind::{fake_kind, letters_exercise};
use crate::practice::session::kind::Hint;
use crate::practice::session::observation::Observation;
use crate::practice::session::policy::Policy;
use crate::practice::session::ports::CheckFinished;
use crate::practice::session::record::{DebriefSession, Finish, NewHint, NewSession, StageMove};
use crate::practice::session::record::{SessionCheck, SessionHint, SessionRow};
use async_trait::async_trait;
use lattice_core::domain::learning::drills::PatternRating;
use lattice_core::domain::learning::practice::work::{
    CaseResult, CaseStatus, CheckReport, CheckStatus, Form, Rung,
};
use std::sync::Mutex;

#[derive(Default)]
struct TurnStore {
    turns: Mutex<Vec<String>>,
    held: Mutex<Option<Session>>,
    rated: Mutex<Vec<PatternRating>>,
}

#[async_trait]
impl SessionStore for TurnStore {
    async fn session(&self, _: &str, _: &str) -> Result<Session> {
        Ok(self.held.lock().unwrap().clone().unwrap())
    }
    async fn operation_seen(&self, _: &str, _: &str, _: &Op<'_>) -> Result<bool> {
        unimplemented!()
    }
    async fn create(&self, _: &Op<'_>, _: &NewSession<'_>, _: i64) -> Result<(String, bool)> {
        unimplemented!()
    }
    async fn move_stage(
        &self,
        _: &At<'_>,
        _: &Op<'_>,
        _: Stage,
        _: i64,
        _: i64,
    ) -> Result<Applied<StageMove>> {
        unimplemented!()
    }
    async fn save_draft(&self, _: &At<'_>, _: &Piece) -> Result<()> {
        unimplemented!()
    }
    async fn record_check(&self, _: &At<'_>, _: &CheckFinished<'_>, _: i64) -> Result<bool> {
        unimplemented!()
    }
    async fn offer_cue(&self, _: &At<'_>, _: &Cue) -> Result<()> {
        unimplemented!()
    }
    async fn take_cue(&self, _: &At<'_>) -> Result<Option<Cue>> {
        unimplemented!()
    }
    async fn restore_cue(&self, _: &At<'_>, _: &Cue) -> Result<()> {
        unimplemented!()
    }
    async fn record_reply(
        &self,
        _: &At<'_>,
        _: Option<&str>,
        _: &str,
        _: &Piece,
        _: i64,
    ) -> Result<()> {
        unimplemented!()
    }
    async fn append_turn(
        &self,
        _: &At<'_>,
        _: &Op<'_>,
        content: &str,
        _: &Piece,
        _: i64,
        _: i64,
    ) -> Result<Applied<()>> {
        self.turns.lock().unwrap().push(content.to_owned());
        Ok(Applied::Replayed)
    }
    async fn record_hint(
        &self,
        _: &At<'_>,
        _: &Op<'_>,
        _: &NewHint<'_>,
        _: &Piece,
        _: i64,
        _: i64,
    ) -> Result<()> {
        unimplemented!()
    }
    async fn change_setting(
        &self,
        _: &At<'_>,
        _: &Op<'_>,
        _: Setting,
        _: Option<u32>,
        _: i64,
        _: i64,
    ) -> Result<()> {
        unimplemented!()
    }
    async fn reveal_solution(&self, _: &At<'_>, _: &Op<'_>, _: i64) -> Result<()> {
        unimplemented!()
    }
    async fn worked_since(&self, _: &str, _: i64) -> Result<bool> {
        unimplemented!()
    }
    async fn finish(&self, _: &At<'_>, _: &Op<'_>, finish: &Finish<'_>, _: i64) -> Result<()> {
        self.rated.lock().unwrap().push(finish.rating);
        Ok(())
    }
}

struct Unused;

#[async_trait]
impl Aids for Unused {
    fn model_name(&self) -> &str {
        ""
    }
    async fn hint(
        &self,
        _: &Kind,
        _: &Exercise,
        _: &Observation,
        _: Rung,
        _: bool,
    ) -> Result<Hint> {
        unimplemented!()
    }
    async fn reply(
        &self,
        _: &Kind,
        _: &Exercise,
        _: &Policy,
        _: &Cue,
        _: &Observation,
    ) -> Result<Option<Reply>> {
        unimplemented!()
    }
}

#[async_trait]
impl Checks for Unused {
    async fn run(&self, _: &CheckStart<'_>) -> Result<SessionCheck> {
        unimplemented!()
    }
}

#[async_trait]
impl Debriefer for Unused {
    async fn debrief(&self, _: &Kind, _: &DebriefSession) -> Result<Debrief> {
        Ok(Debrief {
            rows: Vec::new(),
            stated: Vec::new(),
        })
    }
}

impl Clock for Unused {
    fn now(&self) -> i64 {
        0
    }
}

async fn send(store: &TurnStore, content: &str) -> Result<()> {
    let kinds = Kinds::new([]).unwrap();
    let sessions = PracticeSessions {
        store,
        kinds: &kinds,
        aids: &Unused,
        checks: &Unused,
        debriefer: &Unused,
        clock: &Unused,
    };
    sessions
        .send_turn(
            &At {
                program_id: "p",
                session_id: "s",
            },
            &Op {
                operation_id: "o",
                command: "turn",
                payload_hash: "h",
            },
            content,
            &Piece::new(serde_json::Value::Null),
            false,
            1,
        )
        .await
}

#[tokio::test]
async fn a_blank_turn_is_refused_before_anything_is_stored() {
    let store = TurnStore::default();
    let refused = send(&store, "  \n ").await;
    assert!(
        matches!(refused, Err(AppError::InvalidInput(_))),
        "{refused:?}"
    );
    assert!(store.turns.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_turn_is_stored_without_the_space_around_it() {
    let store = TurnStore::default();
    send(&store, "  What does it return?\n").await.unwrap();
    assert_eq!(
        store.turns.lock().unwrap().as_slice(),
        ["What does it return?".to_owned()]
    );
}

fn held_session(case: CaseStatus, hint: Option<Rung>) -> Session {
    let check = SessionCheck {
        id: "c1".to_owned(),
        scope: CheckScope::All,
        piece: Piece::new(serde_json::Value::Null),
        report: CheckReport {
            results: vec![CaseResult {
                case_id: "case-1".to_owned(),
                status: case,
                detail: serde_json::Value::Null,
            }],
            marks: Vec::new(),
            detail: serde_json::Value::Null,
        },
        at: 10,
        status: if case == CaseStatus::Passed {
            CheckStatus::Passed
        } else {
            CheckStatus::Failed
        },
        cases: Vec::new(),
    };
    Session {
        row: SessionRow {
            id: "s".to_owned(),
            program_id: "p".to_owned(),
            activity_id: "a".to_owned(),
            form: Form::Build,
            setting: Setting::Mock,
            stage: Stage::Make,
            budget_seconds: None,
            active_ms: 0,
            opened_at: None,
            draft: Piece::new(serde_json::Value::Null),
            pending_cue: None,
            submitted_at: None,
            solution_seen_at: None,
            finished_at: None,
            revision: 1,
            created_at: 0,
        },
        activity_revision: 1,
        exercise: letters_exercise(),
        settings: vec![Setting::Mock],
        turns: Vec::new(),
        checks: vec![check],
        learner_cases: Vec::new(),
        hints: hint
            .map(|rung| SessionHint {
                ordinal: 1,
                rung,
                text: String::new(),
                stage: Stage::Make,
                check_id: None,
                reason: String::new(),
                parts: Vec::new(),
                in_role: false,
                at: 0,
            })
            .into_iter()
            .collect(),
        events: Vec::new(),
        draft_seen: None,
        lesson_id: "lesson-1".to_owned(),
        debrief: None,
    }
}

async fn rated_after(case: CaseStatus, hint: Option<Rung>) -> PatternRating {
    let store = TurnStore::default();
    *store.held.lock().unwrap() = Some(held_session(case, hint));
    let kinds = Kinds::new([fake_kind(&["a", "b", "c"]).build()]).unwrap();
    let sessions = PracticeSessions {
        store: &store,
        kinds: &kinds,
        aids: &Unused,
        checks: &Unused,
        debriefer: &Unused,
        clock: &Unused,
    };
    sessions
        .finish(
            &At {
                program_id: "p",
                session_id: "s",
            },
            &Op {
                operation_id: "o",
                command: "finish",
                payload_hash: "h",
            },
            &Piece::new(serde_json::Value::Null),
        )
        .await
        .unwrap();
    let rated = store.rated.lock().unwrap();
    assert_eq!(rated.len(), 1);
    rated[0]
}

#[tokio::test]
async fn a_session_that_passes_every_case_after_an_idea_finishes_hard() {
    let rated = rated_after(CaseStatus::Passed, Some(Rung::Idea)).await;
    assert_eq!(rated, PatternRating::Hard);
}

#[tokio::test]
async fn a_session_that_passes_every_case_with_no_hint_finishes_easy() {
    let rated = rated_after(CaseStatus::Passed, None).await;
    assert_eq!(rated, PatternRating::Easy);
}

#[tokio::test]
async fn a_session_that_ends_with_a_failing_case_finishes_again() {
    let rated = rated_after(CaseStatus::Failed, None).await;
    assert_eq!(rated, PatternRating::Again);
}
