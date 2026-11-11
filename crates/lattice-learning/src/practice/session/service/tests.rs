//! The learner's turn rule, on a store that only keeps the turn it is given.

#![allow(clippy::unwrap_used, clippy::panic)]

use super::*;
use crate::practice::session::counterpart::Reply;
use crate::practice::session::debrief::Debrief;
use crate::practice::session::kind::Hint;
use crate::practice::session::observation::Observation;
use crate::practice::session::policy::Policy;
use crate::practice::session::ports::CheckFinished;
use crate::practice::session::record::{
    DebriefSession, Finish, NewHint, NewSession, SessionCheck, StageMove,
};
use async_trait::async_trait;
use lattice_core::domain::learning::practice::work::Rung;
use std::sync::Mutex;

#[derive(Default)]
struct TurnStore {
    turns: Mutex<Vec<String>>,
}

#[async_trait]
impl SessionStore for TurnStore {
    async fn session(&self, _: &str, _: &str) -> Result<Session> {
        unimplemented!()
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
    async fn finish(&self, _: &At<'_>, _: &Op<'_>, _: &Finish<'_>, _: i64) -> Result<()> {
        unimplemented!()
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
        unimplemented!()
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
