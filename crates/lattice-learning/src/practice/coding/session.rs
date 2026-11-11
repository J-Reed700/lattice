//! Coding sessions as the Code card calls them (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md):
//! request DTOs in, the engine's session service in the middle, `CodingSessionDto` out. `get`, `list`, `due` and
//! `set_clock` stay here: each is one read or write that answers with a screen DTO.

mod aids;
mod cases;
pub(super) mod evidence;
pub(super) mod view;

pub use self::aids::{CodingAids, LiveCodingAids};
use self::cases::input_case;
pub use self::evidence::last_draft_seen;
#[cfg(test)]
pub(crate) use self::view::case_is_open;
pub use self::view::session_view;
pub(crate) use self::view::{cut_case, example_view};
use super::checks::CodingChecks;
use super::dto::*;
use super::problem::CodingSetting;
use super::records::{CodingOp, RunFinished};
use super::store::{session_of, CodingStore};
use crate::persistence::hash;
use crate::practical_dto::LearningPracticalRunDto;
use crate::practical_repository::LearningPracticalRepository;
use crate::practical_runs::CodingRunStart;
use crate::practice::kinds::program::shape::{piece_of, scope_of, setting_of, stage_of};
use crate::practice::session::kind::Kinds;
pub use crate::practice::session::live::SystemClock;
use crate::practice::session::policy::Event;
pub use crate::practice::session::ports::Clock;
use crate::practice::session::ports::{Aids, At, CheckStart, Checks, Debriefer};
use crate::practice::session::record::Op;
pub use crate::practice::session::service::IDLE_AFTER_MS;
use crate::practice::session::service::{CheckAsk, DraftReason, PracticeSessions, Start};
use async_trait::async_trait;
use lattice_core::domain::learning::practice::work::{CaseStatus, CheckScope};
use lattice_core::shared::error::{AppError, Result};

/// Starts a coding run and waits for it to finish: the desktop runs it on the built-in runtime, tests on a fake.
#[async_trait]
pub trait CodingRuns: Send + Sync {
    async fn start(
        &self,
        start: &CodingRunStart,
        payload_hash: &str,
    ) -> Result<LearningPracticalRunDto>;
}

fn op<'a>(operation_id: &'a str, command: &'static str, payload_hash: &'a str) -> Op<'a> {
    Op {
        operation_id,
        command,
        payload_hash,
    }
}

fn coding_op<'a>(operation_id: &'a str, kind: &'static str, payload_hash: &'a str) -> CodingOp<'a> {
    CodingOp {
        operation_id,
        kind,
        payload_hash,
    }
}

fn at<'a>(program_id: &'a str, session_id: &'a str) -> At<'a> {
    At {
        program_id,
        session_id,
    }
}

/// What the engine's service stands on, over the coding tables and the run machinery.
struct Engine<'a> {
    store: CodingStore<'a>,
    checks: CodingChecks<'a>,
}

/// The services a session's commands use.
pub struct CodingSessions<'a> {
    pub repo: &'a LearningPracticalRepository,
    pub kinds: &'a Kinds,
    /// The model calls of the counterpart and the hints.
    pub aids: &'a dyn Aids,
    /// The runner calls of Program's own commands.
    pub own: &'a dyn CodingAids,
    pub runs: &'a dyn CodingRuns,
    pub debriefer: &'a dyn Debriefer,
    pub clock: &'a dyn Clock,
}

impl CodingSessions<'_> {
    fn engine(&self) -> Engine<'_> {
        Engine {
            store: CodingStore { repo: self.repo },
            checks: CodingChecks {
                repo: self.repo,
                runs: self.runs,
            },
        }
    }

    fn service<'s>(&'s self, engine: &'s Engine<'s>) -> PracticeSessions<'s> {
        PracticeSessions {
            store: &engine.store,
            kinds: self.kinds,
            aids: self.aids,
            checks: &engine.checks,
            debriefer: self.debriefer,
            clock: self.clock,
        }
    }

    async fn view(&self, program_id: &str, session_id: &str) -> Result<CodingSessionDto> {
        session_view(&self.repo.coding_session(program_id, session_id).await?)
    }

    pub async fn start(&self, request: &StartCodingSessionRequestDto) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        let engine = self.engine();
        let session_id = self
            .service(&engine)
            .start(
                &op(&request.operation_id, "start", &payload_hash),
                &Start {
                    program_id: &request.program_id,
                    activity_id: &request.activity_id,
                    setting: setting_of(request.setting),
                    budget_seconds: request.budget_seconds,
                },
            )
            .await?;
        self.view(&request.program_id, &session_id).await
    }

    pub async fn get(&self, request: &CodingSessionRequestDto) -> Result<CodingSessionDto> {
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn list(
        &self,
        request: &ListCodingSessionsRequestDto,
    ) -> Result<Vec<CodingSessionSummaryDto>> {
        self.repo.list_coding_sessions(&request.program_id).await
    }

    pub async fn move_stage(
        &self,
        request: &MoveCodingStageRequestDto,
    ) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        let engine = self.engine();
        self.service(&engine)
            .move_stage(
                &at(&request.program_id, &request.session_id),
                &op(&request.operation_id, "stage", &payload_hash),
                stage_of(request.stage),
                request.expected_revision,
            )
            .await?;
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn check_example(
        &self,
        request: &CheckCodingExampleRequestDto,
    ) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        let operation = coding_op(&request.operation_id, "example", &payload_hash);
        let seen = self
            .repo
            .coding_operation_seen(&request.program_id, &request.session_id, &operation)
            .await?;
        if !seen {
            let bundle = self
                .repo
                .coding_session(&request.program_id, &request.session_id)
                .await?;
            if request.args.len() != bundle.problem.spec.signature.params.len() {
                return Err(AppError::InvalidInput(
                    "An example needs one value for each parameter.".into(),
                ));
            }
            let check = self
                .own
                .check_example(&bundle.problem, &request.args, &request.expected)
                .await?;
            self.repo
                .save_coding_example(
                    &request.program_id,
                    &request.session_id,
                    &operation,
                    request.ordinal,
                    &request.args,
                    &request.expected,
                    &check,
                    self.clock.now(),
                )
                .await?;
        }
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn remove_example(
        &self,
        request: &RemoveCodingExampleRequestDto,
    ) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        self.repo
            .remove_coding_example(
                &request.program_id,
                &request.session_id,
                &coding_op(&request.operation_id, "example", &payload_hash),
                request.ordinal,
                self.clock.now(),
            )
            .await?;
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn run(&self, request: &RunCodingSessionRequestDto) -> Result<CodingSessionDto> {
        if request.scope == CodingRunScope::Input {
            self.run_input(request).await?;
        } else {
            let payload_hash = hash(request)?;
            let piece = piece_of(&request.files)?;
            let engine = self.engine();
            self.service(&engine)
                .check(
                    &at(&request.program_id, &request.session_id),
                    &op(&request.operation_id, "run", &payload_hash),
                    &CheckAsk {
                        check_id: &request.run_id,
                        scope: scope_of(request.scope),
                        piece: &piece,
                    },
                )
                .await?;
        }
        self.view(&request.program_id, &request.session_id).await
    }

    /// Program's own check: the learner's code on one input, with no verdict. In Coached it also records what the
    /// reference returns for that input.
    async fn run_input(&self, request: &RunCodingSessionRequestDto) -> Result<()> {
        let bundle = self
            .repo
            .coding_session(&request.program_id, &request.session_id)
            .await?;
        if bundle.row.finished_at.is_some() {
            return Err(AppError::InvalidState(
                "This coding session is finished.".into(),
            ));
        }
        let session = session_of(&bundle)?;
        let case = input_case(&session.exercise, request.input.as_deref())?;
        self.repo
            .save_coding_draft(&request.program_id, &request.session_id, &request.files)
            .await?;
        let payload_hash = hash(request)?;
        let piece = piece_of(&request.files)?;
        let engine = self.engine();
        let made = engine
            .checks
            .run(&CheckStart {
                operation_id: &request.operation_id,
                check_id: &request.run_id,
                at: at(&request.program_id, &request.session_id),
                session: &session,
                scope: CheckScope::Own,
                piece: &piece,
                cases: vec![case],
                own_ids: Vec::new(),
                payload_hash: &payload_hash,
            })
            .await?;
        let reference_actual = match (bundle.row.setting, request.input.as_deref()) {
            (CodingSetting::Coached, Some(input)) => {
                self.own
                    .reference_answer(&bundle.problem, input)
                    .await?
                    .actual
            }
            _ => None,
        };
        let passed = made
            .report
            .results
            .iter()
            .filter(|result| result.status == CaseStatus::Passed)
            .count();
        let recorded = self
            .repo
            .record_coding_run(
                &request.program_id,
                &request.session_id,
                &RunFinished {
                    run_id: &request.run_id,
                    scope: CodingRunScope::Input,
                    passed: u32::try_from(passed).unwrap_or(u32::MAX),
                    total: u32::try_from(made.report.results.len()).unwrap_or(u32::MAX),
                    reference_actual: reference_actual.as_deref(),
                    submit: false,
                },
                self.clock.now(),
            )
            .await?;
        if recorded {
            self.service(&engine)
                .happened(
                    &at(&request.program_id, &request.session_id),
                    Event::Checked {
                        scope: CheckScope::Own,
                    },
                )
                .await?;
        }
        Ok(())
    }

    pub async fn ask_hint(&self, request: &AskCodingHintRequestDto) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        let piece = piece_of(&request.files)?;
        let engine = self.engine();
        self.service(&engine)
            .ask_hint(
                &at(&request.program_id, &request.session_id),
                &op(&request.operation_id, "hint", &payload_hash),
                &piece,
                request.expected_revision,
            )
            .await?;
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn send_turn(&self, request: &SendCodingTurnRequestDto) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        let piece = piece_of(&request.files)?;
        let engine = self.engine();
        self.service(&engine)
            .send_turn(
                &at(&request.program_id, &request.session_id),
                &op(&request.operation_id, "turn", &payload_hash),
                &request.content,
                &piece,
                request.spoken,
                request.expected_revision,
            )
            .await?;
        self.view(&request.program_id, &request.session_id).await
    }

    /// Answers the pending cue, if there is one. On failure the cue goes back unless a newer one arrived.
    pub async fn respond(&self, request: &CodingSessionRequestDto) -> Result<CodingSessionDto> {
        let engine = self.engine();
        self.service(&engine)
            .respond(&at(&request.program_id, &request.session_id))
            .await?;
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn observe_draft(
        &self,
        request: &ObserveCodingDraftRequestDto,
    ) -> Result<CodingSessionDto> {
        let piece = piece_of(&request.files)?;
        let reason = match request.reason {
            DraftObservation::Pause => DraftReason::Pause,
            DraftObservation::Idle => DraftReason::Idle,
        };
        let engine = self.engine();
        self.service(&engine)
            .observe_draft(
                &at(&request.program_id, &request.session_id),
                &piece,
                reason,
            )
            .await?;
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn set_clock(&self, request: &SetCodingClockRequestDto) -> Result<ClockDto> {
        self.repo
            .set_coding_clock(
                &request.program_id,
                &request.session_id,
                request.open,
                self.clock.now(),
            )
            .await
    }

    pub async fn set_setting(
        &self,
        request: &SetCodingSettingRequestDto,
    ) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        let engine = self.engine();
        self.service(&engine)
            .set_setting(
                &at(&request.program_id, &request.session_id),
                &op(&request.operation_id, "setting", &payload_hash),
                setting_of(request.setting),
                request.expected_revision,
            )
            .await?;
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn reveal_solution(
        &self,
        request: &RevealCodingSolutionRequestDto,
    ) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        let engine = self.engine();
        self.service(&engine)
            .reveal_solution(
                &at(&request.program_id, &request.session_id),
                &op(&request.operation_id, "reveal", &payload_hash),
            )
            .await?;
        self.view(&request.program_id, &request.session_id).await
    }

    /// Debriefs the session and writes its scorecard, ratings and evidence once; a session already finished is
    /// returned as it is stored.
    pub async fn finish(
        &self,
        request: &FinishCodingSessionRequestDto,
    ) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        let piece = piece_of(&request.files)?;
        let engine = self.engine();
        self.service(&engine)
            .finish(
                &at(&request.program_id, &request.session_id),
                &op(&request.operation_id, "finish", &payload_hash),
                &piece,
            )
            .await?;
        self.view(&request.program_id, &request.session_id).await
    }

    /// `code_fits` is the course's fit, from `practice_fit::code_fits`.
    pub async fn due(
        &self,
        request: &PracticeDueRequestDto,
        code_fits: bool,
    ) -> Result<PracticeDueDto> {
        self.repo
            .practice_due(&request.program_id, self.clock.now(), code_fits)
            .await
    }
}
