//! The commands of a practice session, for every kind of work: each reads what it needs, makes its model calls and
//! checks outside any write, and then writes.

use crate::practice::session::kind::Kinds;
use crate::practice::session::policy::Event;
use crate::practice::session::ports::{Aids, At, Checks, Clock, Debriefer, SessionStore};
use crate::practice::session::record::Op;
use lattice_core::domain::learning::practice::work::{CheckScope, Piece, Setting, Stage};
use lattice_core::shared::error::{AppError, Result};

/// How long nothing may happen before a watching counterpart is offered the chance to speak: no check, learner turn
/// or stage move in this time.
pub const IDLE_AFTER_MS: i64 = 180_000;

/// Why a draft is being reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftReason {
    /// The learner stopped typing.
    Pause,
    /// Nothing has happened for a while.
    Idle,
}

/// A session to start.
#[derive(Debug, Clone, Copy)]
pub struct Start<'a> {
    pub program_id: &'a str,
    pub activity_id: &'a str,
    pub setting: Setting,
    pub budget_seconds: Option<u32>,
}

/// A check to make.
#[derive(Debug, Clone, Copy)]
pub struct CheckAsk<'a> {
    pub check_id: &'a str,
    pub scope: CheckScope,
    pub piece: &'a Piece,
}

/// The services a session's commands use.
pub struct PracticeSessions<'a> {
    pub store: &'a dyn SessionStore,
    pub kinds: &'a Kinds,
    pub aids: &'a dyn Aids,
    pub checks: &'a dyn Checks,
    pub debriefer: &'a dyn Debriefer,
    pub clock: &'a dyn Clock,
}

fn unbuilt<T>() -> Result<T> {
    Err(AppError::InvalidState("Not built yet.".into()))
}

impl PracticeSessions<'_> {
    /// Starts a session and returns its id.
    pub async fn start(&self, _op: &Op<'_>, _start: &Start<'_>) -> Result<String> {
        unbuilt()
    }

    pub async fn move_stage(
        &self,
        _at: &At<'_>,
        _op: &Op<'_>,
        _stage: Stage,
        _expected_revision: i64,
    ) -> Result<()> {
        unbuilt()
    }

    /// Checks `ask.piece` against the cases of `ask.scope`: Shown or All. Own is a kind's own command.
    pub async fn check(&self, _at: &At<'_>, _op: &Op<'_>, _ask: &CheckAsk<'_>) -> Result<()> {
        unbuilt()
    }

    pub async fn ask_hint(
        &self,
        _at: &At<'_>,
        _op: &Op<'_>,
        _piece: &Piece,
        _expected_revision: i64,
    ) -> Result<()> {
        unbuilt()
    }

    pub async fn send_turn(
        &self,
        _at: &At<'_>,
        _op: &Op<'_>,
        _content: &str,
        _piece: &Piece,
        _spoken: bool,
        _expected_revision: i64,
    ) -> Result<()> {
        unbuilt()
    }

    /// Answers the pending cue, if there is one. On failure the cue goes back unless a newer one arrived.
    pub async fn respond(&self, _at: &At<'_>) -> Result<()> {
        unbuilt()
    }

    pub async fn observe_draft(
        &self,
        _at: &At<'_>,
        _piece: &Piece,
        _reason: DraftReason,
    ) -> Result<()> {
        unbuilt()
    }

    pub async fn set_setting(
        &self,
        _at: &At<'_>,
        _op: &Op<'_>,
        _setting: Setting,
        _expected_revision: i64,
    ) -> Result<()> {
        unbuilt()
    }

    pub async fn reveal_solution(&self, _at: &At<'_>, _op: &Op<'_>) -> Result<()> {
        unbuilt()
    }

    /// Debriefs the session and writes what it leaves behind once; a session already finished is left as it is.
    pub async fn finish(&self, _at: &At<'_>, _op: &Op<'_>, _piece: &Piece) -> Result<()> {
        unbuilt()
    }

    /// Reports something a kind's own command did, so the setting's policy may offer the counterpart a cue.
    pub async fn happened(&self, _at: &At<'_>, _event: Event) -> Result<()> {
        unbuilt()
    }
}
