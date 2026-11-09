//! Coding sessions: the commands behind the Code card (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md).
//! Every write goes through the practical repository; a model call or a code run never sits inside a transaction.

mod aids;
mod cases;
mod evidence;
mod view;

use self::cases::cases_for;
use self::evidence::{last_draft_seen, outcome_of, submit_counts};
pub use self::aids::{CodingAids, LiveAids};
pub use self::view::session_view;
use super::debrief::Debriefer;
use super::dto::*;
use super::observation::observe;
use super::patterns::rating;
use super::policy::{policy_for, CodingEvent};
use super::problem::{CaseStatus, CodingSetting, ProblemDifficulty, SOLUTION_PATH};
use super::records::{Applied, CodingOp, FinishWrite, NewCodingSession, NewHint, RunFinished};
use crate::lab_runtime::LearningLabFile;
use crate::persistence::hash;
use crate::practical_dto::{
    LearningPracticalRunDto, LearningPracticalRunStatus, StartLearningPracticalRunRequestDto,
};
use crate::practical_repository::{validate_learner_turn, LearningPracticalRepository};
use crate::practical_runs::CodingRunStart;
use crate::simulation_policy::TurnPolicy;
use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};

/// How long nothing may happen before the Watched interviewer is offered the chance to speak: no run, learner turn
/// or stage move in this time (the design's starting value).
pub const IDLE_AFTER_MS: i64 = 180_000;

const SECONDS_PER_MINUTE: u32 = 60;
const EASY_BUDGET_MINUTES: u32 = 20;
const MEDIUM_BUDGET_MINUTES: u32 = 35;
const HARD_BUDGET_MINUTES: u32 = 45;

/// The time a Mock or Watched session gets when the learner chose none, by the problem's difficulty.
pub fn default_budget_seconds(difficulty: ProblemDifficulty) -> u32 {
    let minutes = match difficulty {
        ProblemDifficulty::Easy => EASY_BUDGET_MINUTES,
        ProblemDifficulty::Medium => MEDIUM_BUDGET_MINUTES,
        ProblemDifficulty::Hard => HARD_BUDGET_MINUTES,
    };
    minutes * SECONDS_PER_MINUTE
}

/// What a session reads the time from.
pub trait Clock: Send + Sync {
    fn now(&self) -> i64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> i64 {
        crate::persistence::now()
    }
}

/// Starts a coding run and waits for it to finish: the desktop runs it on the built-in runtime, tests on a fake.
#[async_trait]
pub trait CodingRuns: Send + Sync {
    async fn start(
        &self,
        start: &CodingRunStart,
        payload_hash: &str,
    ) -> Result<LearningPracticalRunDto>;
}

/// The help level the most generous setting of a session gave.
fn help_given(settings: &[CodingSetting]) -> &'static str {
    if settings.contains(&CodingSetting::Coached) {
        "full_help"
    } else {
        "hints_only"
    }
}

/// Whether the learner's code gave an answer, so a run of every case counts as a submit.
fn gave_an_answer(status: LearningPracticalRunStatus) -> bool {
    matches!(
        status,
        LearningPracticalRunStatus::Passed
            | LearningPracticalRunStatus::Failed
            | LearningPracticalRunStatus::TimedOut
    )
}

fn op<'a>(operation_id: &'a str, kind: &'static str, payload_hash: &'a str) -> CodingOp<'a> {
    CodingOp {
        operation_id,
        kind,
        payload_hash,
    }
}

/// The services a session's commands use.
pub struct CodingSessions<'a> {
    pub repo: &'a LearningPracticalRepository,
    pub aids: &'a dyn CodingAids,
    pub runs: &'a dyn CodingRuns,
    pub debriefer: &'a dyn Debriefer,
    pub clock: &'a dyn Clock,
}

impl CodingSessions<'_> {
    async fn view(&self, program_id: &str, session_id: &str) -> Result<CodingSessionDto> {
        session_view(&self.repo.coding_session(program_id, session_id).await?)
    }

    /// Offers the counterpart's cue for `event`, if the setting's policy has one.
    async fn cue(&self, program_id: &str, session_id: &str, event: CodingEvent) -> Result<()> {
        let bundle = self.repo.coding_session(program_id, session_id).await?;
        if bundle.row.finished_at.is_some() {
            return Ok(());
        }
        let observation = observe(&bundle.problem, &session_view(&bundle)?, None);
        let policy = policy_for(bundle.row.setting, bundle.row.task);
        if let Some(cue) = policy.cue(&event, &observation) {
            self.repo.offer_coding_cue(program_id, session_id, &cue).await?;
        }
        Ok(())
    }

    pub async fn start(&self, request: &StartCodingSessionRequestDto) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        let (session_id, created) = self
            .repo
            .create_coding_session(
                &op(&request.operation_id, "start", &payload_hash),
                &NewCodingSession {
                    program_id: &request.program_id,
                    activity_id: &request.activity_id,
                    setting: request.setting,
                    budget_seconds: request.budget_seconds,
                    default_budget_seconds,
                },
                self.clock.now(),
            )
            .await?;
        if created {
            self.cue(&request.program_id, &session_id, CodingEvent::Started)
                .await?;
        }
        self.view(&request.program_id, &session_id).await
    }

    pub async fn get(&self, request: &CodingSessionRequestDto) -> Result<CodingSessionDto> {
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn list(
        &self,
        request: &ListCodingSessionsRequestDto,
    ) -> Result<Vec<CodingSessionSummaryDto>> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    pub async fn move_stage(&self, request: &MoveCodingStageRequestDto) -> Result<CodingSessionDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    pub async fn check_example(
        &self,
        request: &CheckCodingExampleRequestDto,
    ) -> Result<CodingSessionDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    pub async fn remove_example(
        &self,
        request: &RemoveCodingExampleRequestDto,
    ) -> Result<CodingSessionDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    pub async fn run(&self, request: &RunCodingSessionRequestDto) -> Result<CodingSessionDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    pub async fn ask_hint(&self, request: &AskCodingHintRequestDto) -> Result<CodingSessionDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    pub async fn send_turn(&self, request: &SendCodingTurnRequestDto) -> Result<CodingSessionDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Answers the pending cue, if there is one. On failure the cue goes back unless a newer one arrived.
    pub async fn respond(&self, request: &CodingSessionRequestDto) -> Result<CodingSessionDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    async fn reply(
        &self,
        request: &CodingSessionRequestDto,
        cue: &super::policy::CodingCue,
    ) -> Result<()> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    pub async fn observe_draft(
        &self,
        request: &ObserveCodingDraftRequestDto,
    ) -> Result<CodingSessionDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    pub async fn set_clock(&self, request: &SetCodingClockRequestDto) -> Result<ClockDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    pub async fn set_setting(
        &self,
        request: &SetCodingSettingRequestDto,
    ) -> Result<CodingSessionDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    pub async fn reveal_solution(
        &self,
        request: &RevealCodingSolutionRequestDto,
    ) -> Result<CodingSessionDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Debriefs the session and writes its scorecard, ratings and evidence once; a session already finished is
    /// returned as it is stored.
    pub async fn finish(&self, request: &FinishCodingSessionRequestDto) -> Result<CodingSessionDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    pub async fn due(&self, request: &PracticeDueRequestDto) -> Result<PracticeDueDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }
}
