//! What a session's commands need from outside the engine: storage, the place checks run, the model's aids and the
//! clock. Each is a port, so a session's tests need none of them.

use crate::practice::session::counterpart::Reply;
use crate::practice::session::debrief::Debrief;
use crate::practice::session::kind::{Hint, Kind};
use crate::practice::session::observation::Observation;
use crate::practice::session::policy::{Cue, Policy};
use crate::practice::session::record::DebriefSession;
use crate::practice::session::record::{
    Applied, Finish, NewHint, NewSession, Op, Session, SessionCheck, StageMove,
};
use async_trait::async_trait;
use lattice_core::domain::learning::practice::work::{
    Case, CheckScope, Exercise, Piece, Rung, Setting, Stage,
};
use lattice_core::shared::error::Result;

/// The session a call is about.
#[derive(Debug, Clone, Copy)]
pub struct At<'a> {
    pub program_id: &'a str,
    pub session_id: &'a str,
}

/// One check to make: the cases to run on a piece, and which of them are the learner's own.
pub struct CheckStart<'a> {
    pub operation_id: &'a str,
    pub check_id: &'a str,
    pub at: At<'a>,
    pub session: &'a Session,
    pub scope: CheckScope,
    pub piece: &'a Piece,
    pub cases: Vec<Case>,
    pub own_ids: Vec<String>,
    pub payload_hash: &'a str,
}

/// A check that finished, as the session records it.
#[derive(Debug, Clone, Copy)]
pub struct CheckFinished<'a> {
    pub check_id: &'a str,
    pub scope: CheckScope,
    pub passed: u32,
    pub total: u32,
    /// The check is the learner's submit: every case, and the kind's work gave an answer.
    pub submit: bool,
}

/// Where a session is stored. Every write is one transaction; a model call or a check never sits inside one.
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn session(&self, program_id: &str, session_id: &str) -> Result<Session>;

    /// Whether this request was already answered, for a command whose slow work should not start twice.
    async fn operation_seen(&self, program_id: &str, session_id: &str, op: &Op<'_>)
        -> Result<bool>;

    /// Creates the session. Returns its id and whether this call created it: a seen operation id answers with the
    /// session it made.
    async fn create(&self, op: &Op<'_>, new: &NewSession<'_>, now: i64) -> Result<(String, bool)>;

    /// Moves to `stage`, recording the stages passed over.
    async fn move_stage(
        &self,
        at: &At<'_>,
        op: &Op<'_>,
        stage: Stage,
        expected_revision: i64,
        now: i64,
    ) -> Result<Applied<StageMove>>;

    /// Saves the draft the screen holds. A finished session keeps the draft it finished with.
    async fn save_draft(&self, at: &At<'_>, draft: &Piece) -> Result<()>;

    /// Records a finished check once. Returns whether this call recorded it.
    async fn record_check(&self, at: &At<'_>, check: &CheckFinished<'_>, now: i64) -> Result<bool>;

    /// Offers a cue for the counterpart. It replaces a pending one only when it outranks it.
    async fn offer_cue(&self, at: &At<'_>, cue: &Cue) -> Result<()>;

    /// Takes the pending cue: it is cleared in the same write that returns it, so two callers never answer it twice.
    async fn take_cue(&self, at: &At<'_>) -> Result<Option<Cue>>;

    /// Puts a taken cue back, unless a newer one has arrived since.
    async fn restore_cue(&self, at: &At<'_>, cue: &Cue) -> Result<()>;

    /// Stores a reply as the counterpart's turn, and the draft it was written from as the last one it saw. A `None`
    /// reply is silence: only the draft is recorded.
    async fn record_reply(
        &self,
        at: &At<'_>,
        content: Option<&str>,
        model: &str,
        draft: &Piece,
        now: i64,
    ) -> Result<()>;

    /// Stores the learner's turn with the draft beside it.
    async fn append_turn(
        &self,
        at: &At<'_>,
        op: &Op<'_>,
        content: &str,
        draft: &Piece,
        expected_revision: i64,
        now: i64,
    ) -> Result<Applied<()>>;

    /// Stores a hint, numbered after the hints already given, with the draft beside it.
    async fn record_hint(
        &self,
        at: &At<'_>,
        op: &Op<'_>,
        hint: &NewHint<'_>,
        draft: &Piece,
        expected_revision: i64,
        now: i64,
    ) -> Result<()>;

    /// Changes the setting. Leaving Coached for a timed setting with no budget gives the session
    /// `default_budget_seconds`.
    async fn change_setting(
        &self,
        at: &At<'_>,
        op: &Op<'_>,
        setting: Setting,
        default_budget_seconds: Option<u32>,
        expected_revision: i64,
        now: i64,
    ) -> Result<()>;

    /// Marks the worked solution as seen, when the setting lets the learner see it yet.
    async fn reveal_solution(&self, at: &At<'_>, op: &Op<'_>, now: i64) -> Result<()>;

    /// Whether anything but the draft changed after `since`: a check, a stage move or a learner turn.
    async fn worked_since(&self, session_id: &str, since: i64) -> Result<bool>;

    /// Writes everything a finished session leaves behind. A session already finished, or an operation already seen,
    /// writes nothing.
    async fn finish(&self, at: &At<'_>, op: &Op<'_>, finish: &Finish<'_>, now: i64) -> Result<()>;
}

/// Makes one check of a session where the kind's work runs, and stores it. The same check id answers with the check
/// it stored.
#[async_trait]
pub trait Checks: Send + Sync {
    async fn run(&self, start: &CheckStart<'_>) -> Result<SessionCheck>;
}

/// The model calls a session makes, behind one seam so a session's tests need no model.
#[async_trait]
pub trait Aids: Send + Sync {
    /// The model that writes the counterpart's words and the hints.
    fn model_name(&self) -> &str;

    async fn hint(
        &self,
        kind: &Kind,
        exercise: &Exercise,
        observation: &Observation,
        rung: Rung,
        in_role: bool,
    ) -> Result<Hint>;

    /// The counterpart's checked reply on `cue`, or None when it chose silence.
    async fn reply(
        &self,
        kind: &Kind,
        exercise: &Exercise,
        policy: &Policy,
        cue: &Cue,
        observation: &Observation,
    ) -> Result<Option<Reply>>;
}

#[async_trait]
pub trait Debriefer: Send + Sync {
    async fn debrief(&self, kind: &Kind, session: &DebriefSession) -> Result<Debrief>;
}

/// What a session reads the time from.
pub trait Clock: Send + Sync {
    fn now(&self) -> i64;
}
