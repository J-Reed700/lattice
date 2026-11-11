//! Background jobs for coding sessions: building and proving a problem before it is shown, then saving it as an
//! activity whose problem file only the app reads.

use crate::embedded_runtime::LearningBuiltinRuntime;
use crate::persistence::hash;
use crate::practical_generation::{
    GeneratedPracticalActivity, GeneratedPracticalFile, PracticalFileRole,
};
use crate::practice::coding::dto::StartPracticeBuildRequestDto;
use crate::practice::coding::harness::{new_nonce, CaseHarness};
use crate::practice::coding::problem::{Problem, PROBLEM_FILE_PATH, SOLUTION_PATH};
use crate::practice::coding::source::ProgramOptions;
use crate::practice::kinds::program::KIND;
use crate::practice::practical_dto::{
    GenerateLearningPracticalActivityRequestDto, LearningPracticalActivityKind,
};
use crate::practice::practical_repository::LearningPracticalRepository;
use crate::practice::session::author::Lesson;
use crate::practice::session::builder::{
    build, BuildSteps, Built, Exercises, Making, NothingStore, Saving, STEP_SAVING,
};
use crate::practice::session::kind::Kinds;
use crate::quiz_bank_jobs::ModelLoader;
use async_trait::async_trait;
use lattice_core::domain::learning::practice::work::{Form, KindId};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, Transactions};
use lattice_core::shared::runtime::jobs::{
    Backoff, JobContext, JobHandler, JobKindConfig, JobOutcome, JobStore, NewJob, RecoveryPolicy,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

#[cfg(test)]
mod tests;

/// The job identifier for practice builds.
pub const PRACTICE_BUILD: &str = "learning.practice_build";

/// The steps of a coding problem build, for its progress: writing, proving and saving.
pub const PRACTICE_BUILD_STEPS: u32 = 3;

/// How many patterns an easy problem calls for. An easy problem is one idea; a harder one combines two.
pub const EASY_PATTERNS: usize = 1;

/// How many patterns a harder problem calls for.
pub(crate) const HARDER_PATTERNS: usize = 2;

/// One build per program at a time; a running build starts again after a restart, and a disconnected model defers it
/// rather than failing it.
pub fn practice_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .exclusive_per_subject()
        .backoff(Backoff::default())
}

/// What a practice build job carries: the lesson, the kind and form to build, and the kind's own options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PracticeBuildRequest {
    pub operation_id: String,
    pub program_id: String,
    pub lesson_id: Option<String>,
    pub kind: KindId,
    pub form: Form,
    pub options: Value,
}

/// The job payload of a program's build request.
pub fn practice_build_request(
    request: &StartPracticeBuildRequestDto,
) -> Result<PracticeBuildRequest> {
    let options = serde_json::to_value(ProgramOptions {
        language: request.language.clone(),
        difficulty: request.difficulty,
        focus: request.focus.clone(),
    })
    .map_err(|error| AppError::Serialization(error.to_string()))?;
    Ok(PracticeBuildRequest {
        operation_id: request.operation_id.clone(),
        program_id: request.program_id.clone(),
        lesson_id: request.lesson_id.clone(),
        kind: KindId::new(KIND),
        form: request.task,
        options,
    })
}

/// Queues a build of a problem for a program and returns its job id. The same operation id is the same job.
pub async fn queue_coding_build(
    database: &Database,
    request: &StartPracticeBuildRequestDto,
) -> Result<String> {
    let requested = serde_json::to_value(practice_build_request(request)?)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let payload_hash = hash(&requested)?;
    let mut transaction = database
        .begin_write()
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    let admitted = JobStore::submit_in(
        &mut transaction,
        &NewJob {
            kind: PRACTICE_BUILD.into(),
            subject_id: Some(request.program_id.clone()),
            operation_id: Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                format!("{PRACTICE_BUILD}:{}", request.operation_id).as_bytes(),
            )
            .to_string(),
            payload_hash,
            requested,
            progress_total: PRACTICE_BUILD_STEPS,
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

/// Builds practice exercises of every registered kind, on the Learning model, pinned for the whole build.
#[derive(Clone)]
pub struct PracticeBuildWorker {
    pub database: Database,
    pub load_llm: ModelLoader,
    pub kinds: Kinds,
    pub nothing: Arc<dyn NothingStore>,
    pub exercises: Arc<dyn Exercises>,
}

/// What a proven problem is saved as.
pub struct ProblemSave<'a> {
    pub job_id: &'a str,
    pub program_id: &'a str,
    pub lesson_id: &'a str,
    pub focus: Option<&'a str>,
    pub language: &'a str,
    pub model: &'a str,
}

/// Saves a proven problem as a coding activity and returns the activity id. The ids come from the job's, so a build
/// that runs again after the save committed finds its own activity and saves nothing twice.
pub async fn save_problem(
    database: &Database,
    save: &ProblemSave<'_>,
    problem: &Problem,
    harness: &dyn CaseHarness,
) -> Result<String> {
    let derived = |what: &str| {
        Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("{PRACTICE_BUILD}:{what}:{}", save.job_id).as_bytes(),
        )
        .to_string()
    };
    let activity_id = derived("activity");
    let repository = LearningPracticalRepository::new(database.clone());
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
        kind: LearningPracticalActivityKind::Exercise,
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
impl JobHandler for PracticeBuildWorker {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let request: PracticeBuildRequest = serde_json::from_value(context.job().requested.clone())
            .map_err(|error| {
                AppError::InvalidInput(format!("Invalid practice build job request: {error}"))
            })?;
        let kind = self.kinds.get(&request.kind)?;
        let repository = LearningPracticalRepository::new(self.database.clone());
        let found = repository
            .build_lesson(&request.program_id, request.lesson_id.as_deref())
            .await?;
        let lesson = Lesson {
            id: found.id,
            title: found.title,
            objective: found.objective,
        };
        let facts = self
            .exercises
            .facts(
                &request.kind,
                &request.program_id,
                request.form,
                &request.options,
            )
            .await?;
        let Some(pin) = context.until_cancelled((self.load_llm)()).await else {
            return Ok(JobOutcome::Stopped);
        };
        let pin = pin?;
        let llm = pin.llm.as_ref();

        let built = match build(
            kind,
            &Making {
                program_id: &request.program_id,
                lesson: Some(&lesson),
                form: request.form,
                options: &request.options,
                facts: &facts,
            },
            llm,
            self.nothing.as_ref(),
            &JobSteps::new(context),
            context.cancellation().clone(),
        )
        .await
        {
            Ok(built) => built,
            Err(_) if context.cancellation().is_cancelled() => return Ok(JobOutcome::Stopped),
            Err(error) => return Err(error),
        };
        let exercise = match built {
            Built::Exercise(exercise) => exercise,
            Built::NothingToMake(reason) => {
                return Ok(JobOutcome::Completed {
                    result_ref: String::new(),
                    message: reason,
                })
            }
        };
        if !context.progress(2, STEP_SAVING, None).await? {
            return Ok(JobOutcome::Stopped);
        }
        let exercise_id = self
            .exercises
            .save(
                &Saving {
                    job_id: &context.job().id,
                    program_id: &request.program_id,
                    lesson_id: &lesson.id,
                    options: &request.options,
                    model: llm.model_name(),
                },
                &exercise,
            )
            .await?;
        Ok(JobOutcome::Completed {
            result_ref: exercise_id,
            message: "The problem is ready".into(),
        })
    }
}
