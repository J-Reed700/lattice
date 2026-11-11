//! A session's check, made on the run machinery: a stored row while it runs, a cancel registry, recovery at startup.

use super::session::CodingRuns;
use super::store::run_as_check;
use crate::practical_dto::StartLearningPracticalRunRequestDto;
use crate::practical_repository::LearningPracticalRepository;
use crate::practical_runs::CodingRunStart;
use crate::practice::kinds::program::shape::{
    coding_scope_of, default_compare_of, files_of, problem_case_of, spec_of,
};
use crate::practice::session::ports::{CheckStart, Checks};
use crate::practice::session::record::SessionCheck;
use async_trait::async_trait;
use lattice_core::shared::error::Result;

/// Makes a session's checks by running the learner's code on `runs`, and reads the stored run back.
pub struct CodingChecks<'a> {
    pub repo: &'a LearningPracticalRepository,
    pub runs: &'a dyn CodingRuns,
}

#[async_trait]
impl Checks for CodingChecks<'_> {
    async fn run(&self, start: &CheckStart<'_>) -> Result<SessionCheck> {
        let session = start.session;
        let run = CodingRunStart {
            base: StartLearningPracticalRunRequestDto {
                operation_id: start.operation_id.to_owned(),
                run_id: start.check_id.to_owned(),
                program_id: start.at.program_id.to_owned(),
                activity_id: session.row.activity_id.clone(),
                expected_activity_revision: session.activity_revision,
                practice_session_id: None,
                learner_files: files_of(start.piece)?,
            },
            exercise_session_id: start.at.session_id.to_owned(),
            scope: coding_scope_of(start.scope),
            cases: start
                .cases
                .iter()
                .map(problem_case_of)
                .collect::<Result<Vec<_>>>()?,
            mine_ids: start.own_ids.clone(),
            spec: spec_of(&session.exercise.brief)?,
        };
        self.runs.start(&run, start.payload_hash).await?;
        let stored = self
            .repo
            .coding_run(start.at.program_id, start.at.session_id, start.check_id)
            .await?;
        run_as_check(&stored, &default_compare_of(&session.exercise)?)
    }
}
