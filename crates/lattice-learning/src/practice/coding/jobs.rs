//! Background jobs for coding sessions: building and proving a problem before it is shown, then saving it as an
//! activity whose problem file only the app reads.

use crate::drills::runner::runner_for;
use crate::embedded_runtime::{BuiltinRuntimes, LearningBuiltinRuntime};
use crate::persistence::{hash, now};
use crate::practical_generation::{GeneratedPracticalActivity, GeneratedPracticalFile, PracticalFileRole};
use crate::practice::coding::dto::StartCodingBuildRequestDto;
use crate::practice::coding::harness::{harness_for, new_nonce, CaseHarness};
use crate::practice::coding::patterns::targets_in;
use crate::practice::coding::problem::{Problem, ProblemDifficulty, PROBLEM_FILE_PATH, SOLUTION_PATH};
use crate::practice::coding::source::{
    BuildSteps, GeneratedProblems, LessonBrief, ProblemRequest, ProblemSource, STEP_SAVING,
};
use crate::practice::practical_dto::{GenerateLearningPracticalActivityRequestDto, LearningPracticalActivityKind};
use crate::practice::practical_repository::LearningPracticalRepository;
use crate::practice_repository::profiles::profile_in;
use crate::quiz_bank_jobs::ModelLoader;
use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Transactions;
use lattice_core::shared::runtime::jobs::{
    Backoff, JobContext, JobHandler, JobKindConfig, JobOutcome, JobStore, NewJob, RecoveryPolicy,
};
use sqlx::SqlitePool;
use uuid::Uuid;

#[cfg(test)]
mod tests;

/// The job identifier for coding problem builds.
pub const CODING_BUILD: &str = "learning.coding_build";

/// The steps of a coding problem build, for its progress: writing, proving and saving.
pub const CODING_BUILD_STEPS: u32 = 3;

/// How many patterns an easy problem calls for. An easy problem is one idea; a harder one combines two.
const EASY_PATTERNS: usize = 1;

/// How many patterns a harder problem calls for.
const HARDER_PATTERNS: usize = 2;

/// One build per program at a time; a running build starts again after a restart, and a disconnected model defers it
/// rather than failing it.
pub fn coding_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .exclusive_per_subject()
        .backoff(Backoff::default())
}

/// Queues a build of a problem for a program and returns its job id. The same operation id is the same job.
pub async fn queue_coding_build(
    pool: &SqlitePool,
    request: &StartCodingBuildRequestDto,
) -> Result<String> {
    let requested = serde_json::to_value(request)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let payload_hash = hash(&requested)?;
    let mut transaction = pool
        .begin_write()
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    let admitted = JobStore::submit_in(
        &mut transaction,
        &NewJob {
            kind: CODING_BUILD.into(),
            subject_id: Some(request.program_id.clone()),
            operation_id: Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                format!("{CODING_BUILD}:{}", request.operation_id).as_bytes(),
            )
            .to_string(),
            payload_hash,
            requested,
            progress_total: CODING_BUILD_STEPS,
            message: "Waiting to build a problem".into(),
        },
    )
    .await?;
    transaction
        .commit()
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    Ok(admitted.id)
}

/// Bridges `BuildSteps` to `JobContext::progress`.
pub struct JobSteps<'a> {
    pub context: &'a JobContext,
}

impl<'a> JobSteps<'a> {
    pub fn new(context: &'a JobContext) -> Self {
        Self { context }
    }
}

#[async_trait]
impl BuildSteps for JobSteps<'_> {
    async fn step(&self, current: u32, message: &str) -> Result<bool> {
        self.context.progress(current, message, None).await
    }
}

/// Builds coding problems, on the Learning model, pinned for the whole build.
#[derive(Clone)]
pub struct CodingBuildWorker {
    pub pool: SqlitePool,
    pub load_llm: ModelLoader,
    pub runtimes: BuiltinRuntimes,
}

impl CodingBuildWorker {
    pub fn new(pool: SqlitePool, load_llm: ModelLoader, runtimes: BuiltinRuntimes) -> Self {
        Self {
            pool,
            load_llm,
            runtimes,
        }
    }
}

/// What a proven problem is saved as.
pub(crate) struct ProblemSave<'a> {
    pub job_id: &'a str,
    pub program_id: &'a str,
    pub lesson_id: &'a str,
    pub focus: Option<&'a str>,
    pub language: &'a str,
    pub model: &'a str,
}

/// Saves a proven problem as a coding activity and returns the activity id. The ids come from the job's, so a build
/// that runs again after the save committed finds its own activity and saves nothing twice.
pub(crate) async fn save_problem(
    pool: &SqlitePool,
    save: &ProblemSave<'_>,
    problem: &Problem,
    harness: &dyn CaseHarness,
) -> Result<String> {
    let derived = |what: &str| {
        Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("{CODING_BUILD}:{what}:{}", save.job_id).as_bytes(),
        )
        .to_string()
    };
    let activity_id = derived("activity");
    let repository = LearningPracticalRepository::new(pool.clone());
    let revision = repository.program_revision(save.program_id).await?;
    let stored = serde_json::to_string(problem)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let mut files = vec![
        GeneratedPracticalFile {
            path: SOLUTION_PATH.into(),
            role: PracticalFileRole::Starter,
            content: problem.spec.starter.clone(),
            editable: true,
        },
        GeneratedPracticalFile {
            path: PROBLEM_FILE_PATH.into(),
            role: PracticalFileRole::Check,
            content: stored,
            editable: false,
        },
    ];
    for file in harness.evaluator(&problem.spec, &problem.cases, &new_nonce())? {
        files.push(GeneratedPracticalFile {
            path: file.path,
            role: PracticalFileRole::Check,
            content: file.content,
            editable: false,
        });
    }
    let request = GenerateLearningPracticalActivityRequestDto {
        operation_id: derived("save"),
        activity_id: activity_id.clone(),
        program_id: save.program_id.to_owned(),
        lesson_id: save.lesson_id.to_owned(),
        expected_program_revision: revision,
        kind: LearningPracticalActivityKind::CodingProblem,
        learner_brief: save.focus.unwrap_or_default().to_owned(),
        practice_mode: crate::dto::LearningPracticeMode::Practice,
        allowed_aids: Vec::new(),
        runtime_profile_id: None,
        builtin_runtime: Some(LearningBuiltinRuntime::parse(save.language)?),
        interview_target: None,
    };
    let activity = GeneratedPracticalActivity {
        title: problem.spec.title.clone(),
        brief: problem.spec.statement.clone(),
        source_version_ids: Vec::new(),
        rubric: Vec::new(),
        files,
    };
    repository
        .save_generated_activity(&request, &activity, save.model, save.job_id)
        .await?;
    Ok(activity_id)
}

#[async_trait]
impl JobHandler for CodingBuildWorker {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let request: StartCodingBuildRequestDto =
            serde_json::from_value(context.job().requested.clone()).map_err(|error| {
                AppError::InvalidInput(format!("Invalid coding build job request: {error}"))
            })?;
        let repository = LearningPracticalRepository::new(self.pool.clone());
        let lesson = repository
            .build_lesson(&request.program_id, request.lesson_id.as_deref())
            .await?;

        let mut transaction = self
            .pool
            .begin_read()
            .await
            .map_err(|error| AppError::Database(error.to_string()))?;
        let profile = profile_in(&mut transaction, &request.program_id).await?;
        let language = Some(request.language.trim())
            .filter(|language| !language.is_empty())
            .map(str::to_owned)
            .or_else(|| profile.as_ref().and_then(|profile| profile.language.clone()))
            .ok_or_else(|| {
                AppError::InvalidInput(
                    "Choose a language for the problem; the practice profile names none.".into(),
                )
            })?;
        let wanted = if request.difficulty == ProblemDifficulty::Easy {
            EASY_PATTERNS
        } else {
            HARDER_PATTERNS
        };
        let patterns: Vec<String> =
            targets_in(&mut transaction, &request.program_id, &language, now())
                .await?
                .into_iter()
                .take(wanted)
                .collect();
        transaction
            .commit()
            .await
            .map_err(|error| AppError::Database(error.to_string()))?;

        let runner = runner_for(&language, &self.runtimes).ok_or_else(|| {
            AppError::ServiceNotAvailable(format!("No runtime is available to run {language}."))
        })?;
        let harness = harness_for(&language).ok_or_else(|| {
            AppError::InvalidInput(format!("Coding problems are not written in {language} yet."))
        })?;
        let Some(pin) = context.until_cancelled((self.load_llm)()).await else {
            return Ok(JobOutcome::Stopped);
        };
        let pin = pin?;
        let llm = pin.llm.as_ref();

        let problem_request = ProblemRequest {
            program_id: request.program_id.clone(),
            lesson: Some(LessonBrief {
                title: lesson.title.clone(),
                objective: lesson.objective.clone(),
            }),
            task: request.task,
            language: language.clone(),
            difficulty: request.difficulty,
            patterns,
            focus: request.focus.clone(),
            details: profile.map(|profile| profile.details),
        };
        let source = GeneratedProblems {
            llm,
            runner: runner.as_ref(),
            harness: harness.as_ref(),
        };
        let problem = match source
            .problem(
                &problem_request,
                &JobSteps::new(context),
                context.cancellation().clone(),
            )
            .await
        {
            Ok(problem) => problem,
            Err(_) if context.cancellation().is_cancelled() => return Ok(JobOutcome::Stopped),
            Err(error) => return Err(error),
        };
        if !context.progress(2, STEP_SAVING, None).await? {
            return Ok(JobOutcome::Stopped);
        }
        let activity_id = save_problem(
            &self.pool,
            &ProblemSave {
                job_id: &context.job().id,
                program_id: &request.program_id,
                lesson_id: &lesson.id,
                focus: request.focus.as_deref(),
                language: &language,
                model: llm.model_name(),
            },
            &problem,
            harness.as_ref(),
        )
        .await?;
        Ok(JobOutcome::Completed {
            result_ref: activity_id,
            message: "The problem is ready".into(),
        })
    }
}
