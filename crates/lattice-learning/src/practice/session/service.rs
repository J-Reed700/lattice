//! The commands of a practice session, for every kind of work: each reads what it needs, makes its model calls and
//! checks outside any write, and then writes.

use crate::practical_repository::validate_learner_turn;
use crate::practice::session::hints::next_rung;
use crate::practice::session::kind::{Kind, Kinds};
use crate::practice::session::observe::{cases_for, observe, outcome, submit_counts};
use crate::practice::session::policy::{policy_for, Cue, Event};
use crate::practice::session::ports::{
    Aids, At, CheckFinished, CheckStart, Checks, Clock, Debriefer, SessionStore,
};
use crate::practice::session::record::{Applied, Finish, Help, NewHint, NewSession, Op, Session};
use lattice_core::domain::learning::practice::work::{
    CaseStatus, CheckScope, CheckStatus, Exercise, Piece, Setting, Stage,
};
use lattice_core::shared::error::{AppError, Result};

#[cfg(test)]
mod tests;

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

/// Whether the work gave an answer, so a check of every case counts as a submit.
fn gave_an_answer(status: CheckStatus) -> bool {
    matches!(
        status,
        CheckStatus::Passed | CheckStatus::Failed | CheckStatus::TimedOut
    )
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

impl PracticeSessions<'_> {
    fn kind_of(&self, session: &Session) -> Result<&Kind> {
        self.kinds.get(&session.exercise.kind)
    }

    async fn read(&self, at: &At<'_>) -> Result<Session> {
        self.store.session(at.program_id, at.session_id).await
    }

    /// Reports something that happened in a session, so the setting's policy may offer the counterpart a cue.
    pub async fn happened(&self, at: &At<'_>, event: Event) -> Result<()> {
        let session = self.read(at).await?;
        if session.row.finished_at.is_some() {
            return Ok(());
        }
        let kind = self.kind_of(&session)?;
        let observation = observe(kind, &session, &session.row.draft, None)?;
        let policy = policy_for(session.row.setting, kind, session.row.form);
        if let Some(cue) = policy.cue(&event, &observation) {
            self.store.offer_cue(at, &cue).await?;
        }
        Ok(())
    }

    /// Starts a session and returns its id.
    pub async fn start(&self, op: &Op<'_>, start: &Start<'_>) -> Result<String> {
        let kinds = self.kinds;
        let default_budget = |exercise: &Exercise| -> Result<Option<u32>> {
            Ok(kinds.get(&exercise.kind)?.budget.seconds(&exercise.brief))
        };
        let (session_id, created) = self
            .store
            .create(
                op,
                &NewSession {
                    program_id: start.program_id,
                    activity_id: start.activity_id,
                    setting: start.setting,
                    budget_seconds: start.budget_seconds,
                    default_budget: &default_budget,
                },
                self.clock.now(),
            )
            .await?;
        if created {
            let at = At {
                program_id: start.program_id,
                session_id: &session_id,
            };
            self.happened(&at, Event::Started).await?;
        }
        Ok(session_id)
    }

    pub async fn move_stage(
        &self,
        at: &At<'_>,
        op: &Op<'_>,
        stage: Stage,
        expected_revision: i64,
    ) -> Result<()> {
        let applied = self
            .store
            .move_stage(at, op, stage, expected_revision, self.clock.now())
            .await?;
        if let Applied::Done(moved) = applied {
            if moved.from != moved.to {
                self.happened(
                    at,
                    Event::StageMoved {
                        from: moved.from,
                        to: moved.to,
                        skipped: moved.skipped,
                    },
                )
                .await?;
            }
        }
        Ok(())
    }

    /// Checks `ask.piece` against the cases of `ask.scope`: Shown or All. Own is a kind's own command.
    pub async fn check(&self, at: &At<'_>, op: &Op<'_>, ask: &CheckAsk<'_>) -> Result<()> {
        let session = self.read(at).await?;
        if session.row.finished_at.is_some() {
            return Err(AppError::InvalidState("This session is finished.".into()));
        }
        let kind = self.kind_of(&session)?;
        let (cases, own_ids) = cases_for(kind, &session, ask.scope)?;
        self.store.save_draft(at, ask.piece).await?;
        let made = self
            .checks
            .run(&CheckStart {
                operation_id: op.operation_id,
                check_id: ask.check_id,
                at: *at,
                session: &session,
                scope: ask.scope,
                piece: ask.piece,
                cases,
                own_ids,
                payload_hash: op.payload_hash,
            })
            .await?;
        let passed = made
            .report
            .results
            .iter()
            .filter(|result| result.status == CaseStatus::Passed)
            .count();
        let recorded = self
            .store
            .record_check(
                at,
                &CheckFinished {
                    check_id: ask.check_id,
                    scope: ask.scope,
                    passed: count(passed),
                    total: count(made.report.results.len()),
                    submit: ask.scope == CheckScope::All && gave_an_answer(made.status),
                },
                self.clock.now(),
            )
            .await?;
        if recorded {
            self.happened(at, Event::Checked { scope: ask.scope })
                .await?;
        }
        Ok(())
    }

    pub async fn ask_hint(
        &self,
        at: &At<'_>,
        op: &Op<'_>,
        piece: &Piece,
        expected_revision: i64,
    ) -> Result<()> {
        if self
            .store
            .operation_seen(at.program_id, at.session_id, op)
            .await?
        {
            return Ok(());
        }
        let session = self.read(at).await?;
        let kind = self.kind_of(&session)?;
        let observation = observe(kind, &session, piece, None)?;
        let in_role = session.row.setting == Setting::Watched;
        let rung = next_rung(session.hints.iter().map(|hint| hint.rung));
        let hint = self
            .aids
            .hint(kind, &session.exercise, &observation, rung, in_role)
            .await?;
        self.store
            .record_hint(
                at,
                op,
                &NewHint {
                    rung: hint.rung,
                    text: &hint.text,
                    reason: &hint.reason,
                    parts: &hint.parts,
                    in_role,
                    model: self.aids.model_name(),
                },
                piece,
                expected_revision,
                self.clock.now(),
            )
            .await
    }

    pub async fn send_turn(
        &self,
        at: &At<'_>,
        op: &Op<'_>,
        content: &str,
        piece: &Piece,
        spoken: bool,
        expected_revision: i64,
    ) -> Result<()> {
        validate_learner_turn(content)?;
        let content = content.trim();
        let applied = self
            .store
            .append_turn(at, op, content, piece, expected_revision, self.clock.now())
            .await?;
        if matches!(applied, Applied::Done(())) {
            self.happened(at, Event::LearnerMessage { spoken }).await?;
        }
        Ok(())
    }

    /// Answers the pending cue, if there is one. On failure the cue goes back unless a newer one arrived.
    pub async fn respond(&self, at: &At<'_>) -> Result<()> {
        let Some(cue) = self.store.take_cue(at).await? else {
            return Ok(());
        };
        let replied = self.reply(at, &cue).await;
        if replied.is_err() {
            if let Err(restore) = self.store.restore_cue(at, &cue).await {
                tracing::error!(%restore, "Could not put a cue back after a failed reply");
            }
        }
        replied
    }

    async fn reply(&self, at: &At<'_>, cue: &Cue) -> Result<()> {
        let session = self.read(at).await?;
        let kind = self.kind_of(&session)?;
        let observation = observe(
            kind,
            &session,
            &session.row.draft,
            session.draft_seen.as_ref(),
        )?;
        let policy = policy_for(session.row.setting, kind, session.row.form);
        let reply = self
            .aids
            .reply(kind, &session.exercise, policy.as_ref(), cue, &observation)
            .await?;
        self.store
            .record_reply(
                at,
                reply.as_ref().map(|reply| reply.content.as_str()),
                self.aids.model_name(),
                &session.row.draft,
                self.clock.now(),
            )
            .await
    }

    pub async fn observe_draft(
        &self,
        at: &At<'_>,
        piece: &Piece,
        reason: DraftReason,
    ) -> Result<()> {
        let session = self.read(at).await?;
        if session.row.finished_at.is_some() {
            return Ok(());
        }
        self.store.save_draft(at, piece).await?;
        let event = match reason {
            DraftReason::Pause => {
                let baseline = session
                    .draft_seen
                    .clone()
                    .unwrap_or_else(|| session.exercise.start.clone());
                Event::DraftPaused {
                    changed: baseline != *piece,
                }
            }
            DraftReason::Idle => {
                let since = self.clock.now() - IDLE_AFTER_MS;
                if self.store.worked_since(at.session_id, since).await? {
                    return Ok(());
                }
                Event::Idle
            }
        };
        self.happened(at, event).await
    }

    pub async fn set_setting(
        &self,
        at: &At<'_>,
        op: &Op<'_>,
        setting: Setting,
        expected_revision: i64,
    ) -> Result<()> {
        let session = self.read(at).await?;
        let default = self
            .kind_of(&session)?
            .budget
            .seconds(&session.exercise.brief);
        self.store
            .change_setting(
                at,
                op,
                setting,
                default,
                expected_revision,
                self.clock.now(),
            )
            .await
    }

    pub async fn reveal_solution(&self, at: &At<'_>, op: &Op<'_>) -> Result<()> {
        self.store.reveal_solution(at, op, self.clock.now()).await
    }

    /// Debriefs the session and writes what it leaves behind once; a session already finished is left as it is.
    pub async fn finish(&self, at: &At<'_>, op: &Op<'_>, piece: &Piece) -> Result<()> {
        let session = self.read(at).await?;
        if session.row.finished_at.is_some() {
            return Ok(());
        }
        let kind = self.kind_of(&session)?;
        let debrief = self
            .debriefer
            .debrief(kind, &session.debrief(piece.clone()))
            .await?;
        let (passed, total) = submit_counts(&session);
        let help = if session.settings.contains(&Setting::Coached) {
            Help::Full
        } else {
            Help::HintsOnly
        };
        self.store
            .finish(
                at,
                op,
                &Finish {
                    debrief: &debrief,
                    model: self.aids.model_name(),
                    outcome: outcome(&session),
                    draft: piece,
                    passed,
                    total,
                    help,
                    hints: count(session.hints.len()),
                },
                self.clock.now(),
            )
            .await
    }
}
