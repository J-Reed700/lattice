//! Tauri IPC commands for coding sessions: thin wrappers over `lattice_learning::practice::coding::session`
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md section 7.5).

use async_trait::async_trait;
use sha2::Digest;
use tauri::State;

use crate::features::learning::practical_repository::LearningPracticalRepository;
use crate::features::learning::practical_runs::start_coding_run;
use crate::interfaces::di::Container;
use lattice_core::application::ports::llm_port::LLMPort;
use lattice_core::shared::error::{AppError, Result as CoreResult};
use lattice_core::shared::ipc::ApiError;
use lattice_learning::drills::runner::{runner_for, CodeRunner};
use lattice_learning::embedded_runtime::BuiltinRuntimes;
use lattice_learning::practical_dto::LearningPracticalRunDto;
use lattice_learning::practical_runs::CodingRunStart;
use lattice_learning::practice::coding::counterpart::CounterpartReply;
use lattice_learning::practice::coding::debrief::{CodingDebriefer, Debrief, Debriefer, SessionEvidence};
use lattice_learning::practice::coding::dto::*;
use lattice_learning::practice::coding::examples::ExampleCheck;
use lattice_learning::practice::coding::hints::CodeHint;
use lattice_learning::practice::coding::jobs::{CODING_BUILD, CODING_BUILD_STEPS};
use lattice_learning::practice::coding::observation::WorkspaceObservation;
use lattice_learning::practice::coding::policy::{CodingCue, CodingPolicy};
use lattice_learning::practice::coding::problem::{CaseResult, Problem};
use lattice_learning::practice::coding::session::{
    CodingAids, CodingRuns, CodingSessions, LiveAids, SystemClock,
};

fn repo_of(container: &Container) -> LearningPracticalRepository {
    LearningPracticalRepository::new(container.db_pool().clone())
}

fn unused<T>() -> CoreResult<T> {
    Err(AppError::InvalidState(
        "This command does not use the model or the runner.".into(),
    ))
}

/// Aids for a command that never asks the model or the runner.
struct NoAids;

#[async_trait]
impl CodingAids for NoAids {
    fn model_name(&self) -> &str {
        ""
    }
    async fn check_example(&self, _: &Problem, _: &[String], _: &str) -> CoreResult<ExampleCheck> {
        unused()
    }
    async fn reference_answer(&self, _: &Problem, _: &[String]) -> CoreResult<CaseResult> {
        unused()
    }
    async fn hint(
        &self,
        _: &Problem,
        _: &WorkspaceObservation,
        _: CodeHintRung,
        _: bool,
    ) -> CoreResult<CodeHint> {
        unused()
    }
    async fn reply(
        &self,
        _: &Problem,
        _: &CodingPolicy,
        _: &CodingCue,
        _: &WorkspaceObservation,
    ) -> CoreResult<Option<CounterpartReply>> {
        unused()
    }
}

/// A debriefer for a command that never finishes a session.
struct NoDebrief;

#[async_trait]
impl Debriefer for NoDebrief {
    async fn debrief(&self, _: &SessionEvidence) -> CoreResult<Debrief> {
        unused()
    }
}

/// Debriefs with the model a command pinned.
struct PinnedDebriefer<'a>(&'a dyn LLMPort);

#[async_trait]
impl Debriefer for PinnedDebriefer<'_> {
    async fn debrief(&self, evidence: &SessionEvidence) -> CoreResult<Debrief> {
        CodingDebriefer { llm: self.0 }.debrief(evidence).await
    }
}

/// Runs a coding run on the built-in runtime and waits for it.
struct DesktopRuns<'a> {
    container: &'a Container,
    runtimes: &'a BuiltinRuntimes,
}

#[async_trait]
impl CodingRuns for DesktopRuns<'_> {
    async fn start(
        &self,
        start: &CodingRunStart,
        payload_hash: &str,
    ) -> CoreResult<LearningPracticalRunDto> {
        start_coding_run(
            &repo_of(self.container),
            &self.container.practical_run_registry(),
            self.container.core.background(),
            self.runtimes,
            start,
            payload_hash,
        )
        .await
    }
}

/// The runner for the language of the session's problem.
async fn runner_of(
    repo: &LearningPracticalRepository,
    program_id: &str,
    session_id: &str,
    runtimes: &BuiltinRuntimes,
) -> Result<Box<dyn CodeRunner>, ApiError> {
    let language = repo
        .coding_session(program_id, session_id)
        .await
        .map_err(ApiError::from)?
        .problem
        .spec
        .language;
    runner_for(&language, runtimes).ok_or_else(|| {
        ApiError::from(AppError::ServiceNotAvailable(format!(
            "No built-in runtime for {language}."
        )))
    })
}

/// Binds `$sessions` to the session services around `$aids` and `$debriefer`.
macro_rules! sessions {
    ($sessions:ident, $container:expr, $runtimes:expr, $repo:expr, $aids:expr, $debriefer:expr) => {
        let runs = DesktopRuns {
            container: &$container,
            runtimes: &$runtimes,
        };
        let $sessions = CodingSessions {
            repo: &$repo,
            aids: $aids,
            runs: &runs,
            debriefer: $debriefer,
            clock: &SystemClock,
        };
    };
}

/// Binds `$sessions` for a command that never reaches the model or the runner.
macro_rules! plain_sessions {
    ($sessions:ident, $container:expr, $runtimes:expr, $repo:expr) => {
        sessions!($sessions, $container, $runtimes, $repo, &NoAids, &NoDebrief);
    };
}

/// Binds `$sessions` and the aids for a command that asks the model and runs code.
macro_rules! live_sessions {
    ($sessions:ident, $request:expr, $container:expr, $runtimes:expr, $repo:expr, $debriefer:expr) => {
        let runner =
            runner_of(&$repo, &$request.program_id, &$request.session_id, &$runtimes).await?;
        let pin = $container
            .learning_llm(Default::default())
            .await
            .map_err(ApiError::from)?;
        let aids = LiveAids {
            llm: pin.llm.as_ref(),
            runner: runner.as_ref(),
            cancel: $container.core.background().token(),
        };
        sessions!($sessions, $container, $runtimes, $repo, &aids, $debriefer);
    };
}

/// Queues a coding problem build job that generates and proves a problem before saving it.
#[tauri::command]
#[specta::specta]
pub async fn start_coding_build(
    request: StartCodingBuildRequestDto,
    container: State<'_, Container>,
) -> Result<StartCodingBuildResultDto, ApiError> {
    let requested = serde_json::to_value(&request)
        .map_err(|e| ApiError::internal(format!("Failed to serialize build request: {e}")))?;
    let payload_hash = format!(
        "{:x}",
        sha2::Sha256::digest(requested.to_string().as_bytes())
    );
    let new_job = lattice_core::shared::runtime::jobs::NewJob {
        kind: CODING_BUILD.into(),
        subject_id: Some(format!("{}:coding", request.program_id)),
        operation_id: request.operation_id.clone(),
        payload_hash,
        requested,
        progress_total: CODING_BUILD_STEPS,
        message: "Waiting to build coding problem".into(),
    };
    let job = container
        .jobs()
        .submit(&new_job)
        .await
        .map_err(ApiError::from)?;
    container.jobs().submitted(&job.id).await;
    Ok(StartCodingBuildResultDto { job_id: job.id })
}

#[tauri::command]
#[specta::specta]
pub async fn start_coding_session(
    request: StartCodingSessionRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let repo = repo_of(&container);
    plain_sessions!(sessions, container, runtimes, repo);
    sessions.start(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_coding_session(
    request: CodingSessionRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let repo = repo_of(&container);
    plain_sessions!(sessions, container, runtimes, repo);
    sessions.get(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn list_coding_sessions(
    request: ListCodingSessionsRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<Vec<CodingSessionSummaryDto>, ApiError> {
    let repo = repo_of(&container);
    plain_sessions!(sessions, container, runtimes, repo);
    sessions.list(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn move_coding_stage(
    request: MoveCodingStageRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let repo = repo_of(&container);
    plain_sessions!(sessions, container, runtimes, repo);
    sessions.move_stage(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn check_coding_example(
    request: CheckCodingExampleRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let repo = repo_of(&container);
    live_sessions!(sessions, request, container, runtimes, repo, &NoDebrief);
    sessions.check_example(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn remove_coding_example(
    request: RemoveCodingExampleRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let repo = repo_of(&container);
    plain_sessions!(sessions, container, runtimes, repo);
    sessions.remove_example(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn run_coding_session(
    request: RunCodingSessionRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let repo = repo_of(&container);
    live_sessions!(sessions, request, container, runtimes, repo, &NoDebrief);
    sessions.run(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn ask_coding_hint(
    request: AskCodingHintRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let repo = repo_of(&container);
    live_sessions!(sessions, request, container, runtimes, repo, &NoDebrief);
    sessions.ask_hint(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn send_coding_turn(
    request: SendCodingTurnRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let repo = repo_of(&container);
    plain_sessions!(sessions, container, runtimes, repo);
    sessions.send_turn(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn respond_coding_session(
    request: CodingSessionRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let repo = repo_of(&container);
    live_sessions!(sessions, request, container, runtimes, repo, &NoDebrief);
    sessions.respond(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn observe_coding_draft(
    request: ObserveCodingDraftRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let repo = repo_of(&container);
    plain_sessions!(sessions, container, runtimes, repo);
    sessions.observe_draft(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn set_coding_clock(
    request: SetCodingClockRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<ClockDto, ApiError> {
    let repo = repo_of(&container);
    plain_sessions!(sessions, container, runtimes, repo);
    sessions.set_clock(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn set_coding_setting(
    request: SetCodingSettingRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let repo = repo_of(&container);
    plain_sessions!(sessions, container, runtimes, repo);
    sessions.set_setting(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn reveal_coding_solution(
    request: RevealCodingSolutionRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let repo = repo_of(&container);
    plain_sessions!(sessions, container, runtimes, repo);
    sessions.reveal_solution(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn finish_coding_session(
    request: FinishCodingSessionRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let repo = repo_of(&container);
    let runner = runner_of(&repo, &request.program_id, &request.session_id, &runtimes).await?;
    let pin = container
        .learning_llm(Default::default())
        .await
        .map_err(ApiError::from)?;
    let aids = LiveAids {
        llm: pin.llm.as_ref(),
        runner: runner.as_ref(),
        cancel: container.core.background().token(),
    };
    let debriefer = PinnedDebriefer(pin.llm.as_ref());
    sessions!(sessions, container, runtimes, repo, &aids, &debriefer);
    sessions.finish(&request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn practice_due(
    request: PracticeDueRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<PracticeDueDto, ApiError> {
    let code_fits = crate::features::learning::practice_fit::code_fits(
        &super::practice::practice_repo(&container),
        &request.program_id,
    )
    .await
    .map_err(ApiError::from)?;
    let repo = repo_of(&container);
    plain_sessions!(sessions, container, runtimes, repo);
    sessions
        .due(&request, code_fits)
        .await
        .map_err(ApiError::from)
}
