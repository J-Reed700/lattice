//! Toolchains over the fake runner and harness the coding tests share.

use super::Toolchains;
use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::harness::CaseHarness;
use crate::practice::coding::proof::fakes::{FakeHarness, FakeRunner};
use async_trait::async_trait;
use lattice_core::domain::learning::drills::DrillFile;
use lattice_core::shared::error::Result;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Hands out the one fake runner, which the test keeps to read what ran.
pub(crate) struct FakeToolchains {
    pub runner: Arc<FakeRunner>,
    timed_out: bool,
}

impl FakeToolchains {
    pub(crate) fn new(runner: FakeRunner) -> Self {
        Self {
            runner: Arc::new(runner),
            timed_out: false,
        }
    }

    /// Toolchains whose runs all end timed out, whatever the code does.
    pub(crate) fn timing_out(runner: FakeRunner) -> Self {
        Self {
            timed_out: true,
            ..Self::new(runner)
        }
    }
}

impl Toolchains for FakeToolchains {
    fn runner(&self, _language: &str) -> Result<Box<dyn CodeRunner>> {
        Ok(Box::new(SharedRunner(self.runner.clone(), self.timed_out)))
    }

    fn harness(&self, _language: &str) -> Result<Box<dyn CaseHarness>> {
        Ok(Box::new(FakeHarness))
    }
}

struct SharedRunner(Arc<FakeRunner>, bool);

#[async_trait]
impl CodeRunner for SharedRunner {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn entrypoint(&self) -> &str {
        self.0.entrypoint()
    }

    fn contract(&self) -> &str {
        self.0.contract()
    }

    fn running(&self, path: &str) -> String {
        self.0.running(path)
    }

    async fn run(
        &self,
        files: &[DrillFile],
        test: &str,
        cancel: CancellationToken,
    ) -> Result<RunOutcome> {
        self.0.run(files, test, cancel).await
    }

    async fn run_capturing(
        &self,
        files: &[LearningLabFile],
        read_only_paths: &[String],
        cancel: CancellationToken,
    ) -> Result<(RunOutcome, String)> {
        let (outcome, stdout) = self.0.run_capturing(files, read_only_paths, cancel).await?;
        match outcome {
            RunOutcome::Passed | RunOutcome::Failed { .. } if self.1 => Ok((
                RunOutcome::TimedOut {
                    output: stdout.clone(),
                },
                stdout,
            )),
            other => Ok((other, stdout)),
        }
    }
}
