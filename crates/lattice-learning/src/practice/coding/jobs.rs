//! Background jobs for coding sessions: building and proving a problem before it is shown.

use async_trait::async_trait;
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::BTreeMap;

use crate::drills::runner::runner_for;
use crate::embedded_runtime::BuiltinRuntimes;
use crate::practice::coding::harness::{CaseHarness, PythonHarness, DEFAULT_NONCE};
use crate::practice::coding::problem::ProblemSpec;
use crate::practice::coding::source::{
    BuildSteps, GeneratedProblems, ProblemRequest, ProblemSource,
};
use crate::quiz_bank_jobs::ModelLoader;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{
    Backoff, JobContext, JobHandler, JobKindConfig, JobOutcome, RecoveryPolicy,
};

/// The job identifier for coding problem builds.
pub const CODING_BUILD: &str = "learning.coding_build";

/// The number of progress steps in a coding problem build.
pub const CODING_BUILD_STEPS: u32 = 3;

/// Job configuration for coding problem builds: exclusive per subject and requeued on failure.
pub fn coding_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .exclusive_per_subject()
        .backoff(Backoff::default())
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
impl<'a> BuildSteps for JobSteps<'a> {
    async fn step(&self, index: u32, message: &str) -> Result<bool> {
        self.context.progress(index, message, None).await
    }
}

/// Worker that executes coding problem build jobs.
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredProblemJson<'a> {
    #[serde(flatten)]
    spec: &'a ProblemSpec,
    #[serde(default)]
    reference_ms: &'a BTreeMap<String, u64>,
}

struct FileToInsert {
    path: String,
    role: String,
    content: String,
    editable: i64,
}

#[async_trait]
impl JobHandler for CodingBuildWorker {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let request: ProblemRequest = serde_json::from_value(context.job().requested.clone())
            .map_err(|e| {
                AppError::InvalidInput(format!("Invalid coding build job request: {e}"))
            })?;

        let runner = runner_for(&request.language, &self.runtimes).ok_or_else(|| {
            AppError::InvalidInput(format!(
                "No runner available for language {}",
                request.language
            ))
        })?;

        let Some(pin) = context.until_cancelled((self.load_llm)()).await else {
            return Ok(JobOutcome::Stopped);
        };
        let pin = pin?;
        let llm = pin.llm.as_ref();

        let harness = PythonHarness;
        let steps = JobSteps::new(context);
        let generator = GeneratedProblems::new(llm, runner.as_ref(), &harness);
        let problem = generator
            .problem(&request, &steps, context.cancellation().clone())
            .await?;

        let activity_id = uuid::Uuid::new_v4().to_string();

        let lesson_id = match &request.lesson_id {
            Some(id) => id.clone(),
            None => {
                let current: Option<String> = sqlx::query_scalar(
                    "SELECT current_lesson_id FROM learning_programs WHERE id=?",
                )
                .bind(&request.program_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?
                .flatten();

                match current {
                    Some(id) => id,
                    None => {
                        let first_lesson: Option<String> = sqlx::query_scalar(
                            "SELECT id FROM learning_lessons WHERE program_id=? ORDER BY ordinal LIMIT 1",
                        )
                        .bind(&request.program_id)
                        .fetch_optional(&self.pool)
                        .await
                        .map_err(|e| AppError::Database(e.to_string()))?;

                        first_lesson.unwrap_or_default()
                    }
                }
            }
        };

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        let timestamp = crate::persistence::now();
        let generator_model = llm.model_name();

        sqlx::query(
            "INSERT INTO learning_practical_activities(id,program_id,lesson_id,predecessor_id,kind,title,brief,status,practice_mode,allowed_aids_json,outcome_ids_json,source_version_ids_json,rubric_json,runtime_kind,runtime_profile_id,runtime_engine,runtime_image_id,runtime_command_json,runtime_limits_json,generator_model,revision,created_at,updated_at,builtin_runtime,builtin_runtime_version) VALUES(?,?,?,NULL,'coding_problem',?,?,'ready','practice','[]','[]','[]','[]','none',NULL,NULL,NULL,NULL,NULL,?,0,?,?,'python','3.14')"
        )
        .bind(&activity_id)
        .bind(&request.program_id)
        .bind(&lesson_id)
        .bind(problem.spec.title.trim())
        .bind(problem.spec.statement.trim())
        .bind(generator_model)
        .bind(timestamp)
        .bind(timestamp)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let stored_json = serde_json::to_string(&StoredProblemJson {
            spec: &problem.spec,
            reference_ms: &problem.reference_ms,
        })
        .map_err(|e| AppError::Serialization(e.to_string()))?;

        let misreadings_json = serde_json::to_string(&problem.misreadings)
            .map_err(|e| AppError::Serialization(e.to_string()))?;

        let evaluator_files = harness.evaluator(&problem.spec, &problem.cases, DEFAULT_NONCE);

        let mut files_to_insert = vec![
            FileToInsert {
                path: "solution.py".into(),
                role: "starter".into(),
                content: problem.spec.starter.clone(),
                editable: 1,
            },
            FileToInsert {
                path: "problem.json".into(),
                role: "reference".into(),
                content: stored_json,
                editable: 0,
            },
        ];

        for eval_file in evaluator_files {
            files_to_insert.push(FileToInsert {
                path: eval_file.path,
                role: "check".into(),
                content: eval_file.content,
                editable: 0,
            });
        }

        files_to_insert.push(FileToInsert {
            path: "misreadings.json".into(),
            role: "check".into(),
            content: misreadings_json,
            editable: 0,
        });

        files_to_insert.push(FileToInsert {
            path: "reference.py".into(),
            role: "solution".into(),
            content: problem.reference.clone(),
            editable: 0,
        });

        for (ordinal, file) in files_to_insert.iter().enumerate() {
            let hash = crate::persistence::hash_text(&file.content);
            sqlx::query(
                "INSERT INTO learning_practical_files(activity_id, ordinal, path, role, content, content_sha256, editable) VALUES(?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&activity_id)
            .bind(ordinal as i64)
            .bind(&file.path)
            .bind(&file.role)
            .bind(&file.content)
            .bind(hash)
            .bind(file.editable)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(JobOutcome::Completed {
            result_ref: Some(activity_id),
            message: "Coding problem is ready".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coding_job_config() {
        let config = coding_job_config();
        assert_eq!(CODING_BUILD, "learning.coding_build");
        assert_eq!(CODING_BUILD_STEPS, 3);
        assert!(matches!(config.recovery, RecoveryPolicy::Requeue));
    }
}
