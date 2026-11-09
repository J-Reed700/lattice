//! Coding sessions: the commands behind the Code card (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md).
//! Every write goes through the practical repository; a model call or a code run never sits inside a transaction.

mod aids;
mod cases;
mod evidence;
mod view;

pub use self::aids::{CodingAids, LiveAids};
pub use self::evidence::last_draft_seen;
use self::cases::cases_for;
use self::evidence::{outcome_of, submit_counts};
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
            self.repo
                .offer_coding_cue(program_id, session_id, &cue)
                .await?;
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
        self.repo.list_coding_sessions(&request.program_id).await
    }

    pub async fn move_stage(
        &self,
        request: &MoveCodingStageRequestDto,
    ) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        let applied = self
            .repo
            .move_coding_stage(
                &request.program_id,
                &request.session_id,
                &op(&request.operation_id, "stage", &payload_hash),
                request.stage,
                request.expected_revision,
                self.clock.now(),
            )
            .await?;
        if let Applied::Done(moved) = applied {
            if moved.from != moved.to {
                self.cue(
                    &request.program_id,
                    &request.session_id,
                    CodingEvent::StageMoved {
                        from: moved.from,
                        to: moved.to,
                        skipped: moved.skipped,
                    },
                )
                .await?;
            }
        }
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn check_example(
        &self,
        request: &CheckCodingExampleRequestDto,
    ) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        let operation = op(&request.operation_id, "example", &payload_hash);
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
                .aids
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
                &op(&request.operation_id, "example", &payload_hash),
                request.ordinal,
                self.clock.now(),
            )
            .await?;
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn run(&self, request: &RunCodingSessionRequestDto) -> Result<CodingSessionDto> {
        let bundle = self
            .repo
            .coding_session(&request.program_id, &request.session_id)
            .await?;
        if bundle.row.finished_at.is_some() {
            return Err(AppError::InvalidState(
                "This coding session is finished.".into(),
            ));
        }
        let run_cases = cases_for(
            &bundle.problem,
            &bundle.examples,
            request.scope,
            request.input.as_deref(),
        )?;
        self.repo
            .save_coding_draft(&request.program_id, &request.session_id, &request.files)
            .await?;
        let start = CodingRunStart {
            base: StartLearningPracticalRunRequestDto {
                operation_id: request.operation_id.clone(),
                run_id: request.run_id.clone(),
                program_id: request.program_id.clone(),
                activity_id: bundle.row.activity_id.clone(),
                expected_activity_revision: bundle.activity_revision,
                practice_session_id: None,
                learner_files: request.files.clone(),
            },
            coding_session_id: request.session_id.clone(),
            scope: request.scope,
            cases: run_cases.cases,
            mine_ids: run_cases.mine_ids,
            spec: bundle.problem.spec.clone(),
        };
        let finished = self.runs.start(&start, &hash(request)?).await?;
        let stored = self
            .repo
            .coding_run(&request.program_id, &request.session_id, &request.run_id)
            .await?;
        let reference_actual = match (request.scope, bundle.row.setting, request.input.as_deref()) {
            (CodingRunScope::Input, CodingSetting::Coached, Some(input)) => {
                self.aids
                    .reference_answer(&bundle.problem, input)
                    .await?
                    .actual
            }
            _ => None,
        };
        let passed = stored
            .cases
            .iter()
            .filter(|case| case.status == CaseStatus::Passed)
            .count();
        let recorded = self
            .repo
            .record_coding_run(
                &request.program_id,
                &request.session_id,
                &RunFinished {
                    run_id: &request.run_id,
                    scope: request.scope,
                    passed: u32::try_from(passed).unwrap_or(u32::MAX),
                    total: u32::try_from(stored.cases.len()).unwrap_or(u32::MAX),
                    reference_actual: reference_actual.as_deref(),
                    submit: request.scope == CodingRunScope::All && gave_an_answer(finished.status),
                },
                self.clock.now(),
            )
            .await?;
        if recorded {
            self.cue(
                &request.program_id,
                &request.session_id,
                CodingEvent::RunFinished {
                    scope: request.scope,
                },
            )
            .await?;
        }
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn ask_hint(&self, request: &AskCodingHintRequestDto) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        let operation = op(&request.operation_id, "hint", &payload_hash);
        let seen = self
            .repo
            .coding_operation_seen(&request.program_id, &request.session_id, &operation)
            .await?;
        if !seen {
            let bundle = self
                .repo
                .coding_session(&request.program_id, &request.session_id)
                .await?;
            let mut view = session_view(&bundle)?;
            view.draft = request.files.clone();
            let observation = observe(&bundle.problem, &view, None);
            let in_role = bundle.row.setting == CodingSetting::Watched;
            let hint = self
                .aids
                .hint(&bundle.problem, &observation, view.next_rung, in_role)
                .await?;
            self.repo
                .record_coding_hint(
                    &request.program_id,
                    &request.session_id,
                    &operation,
                    &NewHint {
                        rung: hint.rung,
                        text: &hint.text,
                        reason: &hint.reason,
                        lines: &hint.lines,
                        path: hint.path.as_deref(),
                        in_role,
                        model: self.aids.model_name(),
                    },
                    &request.files,
                    request.expected_revision,
                    self.clock.now(),
                )
                .await?;
        }
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn send_turn(&self, request: &SendCodingTurnRequestDto) -> Result<CodingSessionDto> {
        validate_learner_turn(&request.content)?;
        let payload_hash = hash(request)?;
        let applied = self
            .repo
            .append_coding_turn(
                &request.program_id,
                &request.session_id,
                &op(&request.operation_id, "turn", &payload_hash),
                request.content.trim(),
                &request.files,
                request.expected_revision,
                self.clock.now(),
            )
            .await?;
        if matches!(applied, Applied::Done(())) {
            self.cue(
                &request.program_id,
                &request.session_id,
                CodingEvent::LearnerMessage {
                    spoken: request.spoken,
                },
            )
            .await?;
        }
        self.view(&request.program_id, &request.session_id).await
    }

    /// Answers the pending cue, if there is one. On failure the cue goes back unless a newer one arrived.
    pub async fn respond(&self, request: &CodingSessionRequestDto) -> Result<CodingSessionDto> {
        let Some(cue) = self
            .repo
            .take_coding_cue(&request.program_id, &request.session_id)
            .await?
        else {
            return self.view(&request.program_id, &request.session_id).await;
        };
        match self.reply(request, &cue).await {
            Ok(()) => self.view(&request.program_id, &request.session_id).await,
            Err(error) => {
                if let Err(restore) = self
                    .repo
                    .restore_coding_cue(&request.program_id, &request.session_id, &cue)
                    .await
                {
                    tracing::error!(%restore, "Could not put a coding cue back after a failed reply");
                }
                Err(error)
            }
        }
    }

    async fn reply(
        &self,
        request: &CodingSessionRequestDto,
        cue: &super::policy::CodingCue,
    ) -> Result<()> {
        let bundle = self
            .repo
            .coding_session(&request.program_id, &request.session_id)
            .await?;
        let view = session_view(&bundle)?;
        let seen = last_draft_seen(&bundle)?;
        let observation = observe(&bundle.problem, &view, seen.as_deref());
        let policy = policy_for(bundle.row.setting, bundle.row.task);
        let reply = self
            .aids
            .reply(&bundle.problem, policy.as_ref(), cue, &observation)
            .await?;
        self.repo
            .record_coding_reply(
                &request.program_id,
                &request.session_id,
                reply.as_ref().map(|reply| reply.content.as_str()),
                self.aids.model_name(),
                &view.draft,
                self.clock.now(),
            )
            .await
    }

    pub async fn observe_draft(
        &self,
        request: &ObserveCodingDraftRequestDto,
    ) -> Result<CodingSessionDto> {
        let bundle = self
            .repo
            .coding_session(&request.program_id, &request.session_id)
            .await?;
        if bundle.row.finished_at.is_some() {
            return self.view(&request.program_id, &request.session_id).await;
        }
        self.repo
            .save_coding_draft(&request.program_id, &request.session_id, &request.files)
            .await?;
        let now = self.clock.now();
        let event = match request.reason {
            DraftObservation::Pause => {
                let baseline = last_draft_seen(&bundle)?.unwrap_or_else(|| {
                    vec![LearningLabFile {
                        path: SOLUTION_PATH.into(),
                        content: bundle.problem.spec.starter.clone(),
                    }]
                });
                CodingEvent::DraftPaused {
                    changed: baseline != request.files,
                }
            }
            DraftObservation::Idle => {
                if self
                    .repo
                    .coding_worked_since(&request.session_id, now - IDLE_AFTER_MS)
                    .await?
                {
                    return self.view(&request.program_id, &request.session_id).await;
                }
                CodingEvent::Idle
            }
        };
        self.cue(&request.program_id, &request.session_id, event)
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
        let bundle = self
            .repo
            .coding_session(&request.program_id, &request.session_id)
            .await?;
        self.repo
            .change_coding_setting(
                &request.program_id,
                &request.session_id,
                &op(&request.operation_id, "setting", &payload_hash),
                request.setting,
                default_budget_seconds(bundle.problem.spec.difficulty),
                request.expected_revision,
                self.clock.now(),
            )
            .await?;
        self.view(&request.program_id, &request.session_id).await
    }

    pub async fn reveal_solution(
        &self,
        request: &RevealCodingSolutionRequestDto,
    ) -> Result<CodingSessionDto> {
        let payload_hash = hash(request)?;
        self.repo
            .reveal_coding_solution(
                &request.program_id,
                &request.session_id,
                &op(&request.operation_id, "reveal", &payload_hash),
                self.clock.now(),
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
        let bundle = self
            .repo
            .coding_session(&request.program_id, &request.session_id)
            .await?;
        if bundle.row.finished_at.is_some() {
            return session_view(&bundle);
        }
        let evidence = evidence::evidence_of(&bundle, &request.files)?;
        let debrief = self.debriefer.debrief(&evidence).await?;
        let outcome = outcome_of(&bundle)?;
        let (passed, total) = submit_counts(&bundle);
        let payload_hash = hash(request)?;
        self.repo
            .finish_coding_session(
                &request.program_id,
                &request.session_id,
                &op(&request.operation_id, "finish", &payload_hash),
                &FinishWrite {
                    scorecard: &debrief.scorecard,
                    approach: &debrief.approach,
                    complexity: &debrief.complexity,
                    model: self.aids.model_name(),
                    language: &bundle.problem.spec.language,
                    pattern_ids: &bundle.problem.spec.patterns,
                    rating: rating(&outcome),
                    files: &request.files,
                    passed,
                    total,
                    help: help_given(&evidence.settings),
                    hints: u32::try_from(evidence.hints.len()).unwrap_or(u32::MAX),
                },
                self.clock.now(),
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
