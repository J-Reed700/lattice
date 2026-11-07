//! Running code for a drill: a suite or a killing test against a set of files. Runners are an Adapter over what
//! the app can run: the built-in runtimes today, containers later, behind one trait.

use crate::embedded_runtime::{
    BuiltinLabExecutionSpec, BuiltinRuntimes, LearningBuiltinRuntime,
};
use crate::lab_runtime::{
    LearningLabExecutionResult, LearningLabFile, LearningLabLimits, LearningLabRunStatus,
};
use async_trait::async_trait;
use lattice_core::domain::learning::drills::DrillFile;
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

/// How one run of a test ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RunOutcome {
    /// The test ran and passed.
    Passed,
    /// The test ran and failed. `output` is what it wrote.
    Failed { output: String },
    /// The run did not finish within the runtime's time limit and was stopped. `output` is what it wrote before.
    TimedOut { output: String },
    /// The run was cancelled.
    Cancelled,
    /// The runtime is not there to run anything.
    Unavailable { why: String },
}

impl RunOutcome {
    /// Why a run says nothing about the code, when it does not: it timed out, was cancelled, or had no runtime.
    pub(crate) fn says_nothing(&self) -> Option<String> {
        match self {
            Self::Passed | Self::Failed { .. } => None,
            Self::TimedOut { .. } => {
                Some("The run exceeded the runtime's time limit and was stopped.".to_owned())
            }
            Self::Cancelled => Some("The run was cancelled.".to_owned()),
            Self::Unavailable { why } => Some(why.clone()),
        }
    }
}

/// Something that can run a test against files.
#[async_trait]
pub trait CodeRunner:
    Send + Sync
{
    /// What the suite and the proofs ran on, as a drill records it: `builtin-python`.
    fn name(&self) -> &str;

    /// The file a test is written to and run as.
    fn entrypoint(&self) -> &str;

    /// What a program that runs here may use, for the model that writes one.
    fn contract(&self) -> &str;

    /// The source of an entrypoint that runs the file at `path` as a program, for a test file that is part of the
    /// code.
    fn running(&self, path: &str) -> String;

    /// Runs `test` as the entrypoint beside `files`. The entrypoint is read-only to the run.
    async fn run(
        &self,
        files: &[DrillFile],
        test: &str,
        cancel: CancellationToken,
    ) -> Result<RunOutcome>;
}

const PYTHON_CONTRACT: &str = "Programs run on CPython 3.14 in a sandbox: the standard library only, no packages, no network, no threads or subprocesses, and no writing to the file system. Files import each other from the project root: src/limiter.py is imported as `from src.limiter import RateLimiter`, and no __init__.py is needed. A test fails by raising an exception, as `assert` does.";

const JAVASCRIPT_CONTRACT: &str = "Programs run as ES modules on QuickJS in a sandbox. Files end in .mjs and import each other by relative path, as `import { RateLimiter } from './src/limiter.mjs'`. There is no Node.js, DOM, fetch, timer, file system, network or package, and console.log is the only output. A test fails by throwing an error.";

/// The built-in runtimes as a runner: Python and JavaScript, with no container.
pub(crate) struct BuiltinRunner {
    runtimes: BuiltinRuntimes,
    runtime: LearningBuiltinRuntime,
    name: String,
}

impl BuiltinRunner {
    pub(crate) fn new(
        runtimes: BuiltinRuntimes,
        runtime: LearningBuiltinRuntime,
    ) -> Self {
        Self {
            name: format!("builtin-{}", runtime.as_str()),
            runtimes,
            runtime,
        }
    }
}

#[async_trait]
impl CodeRunner for BuiltinRunner {
    fn name(&self) -> &str {
        &self.name
    }

    fn entrypoint(&self) -> &str {
        self.runtime.entrypoint()
    }

    fn contract(&self) -> &str {
        match self.runtime {
            LearningBuiltinRuntime::Python => PYTHON_CONTRACT,
            LearningBuiltinRuntime::Javascript => JAVASCRIPT_CONTRACT,
        }
    }

    fn running(&self, path: &str) -> String {
        match self.runtime {
            LearningBuiltinRuntime::Python => {
                let literal = serde_json::to_string(path).unwrap_or_else(|_| format!("\"{path}\""));
                format!("import os\nimport runpy\n\nrunpy.run_path(os.path.join(os.path.dirname(__file__), {literal}), run_name=\"__main__\")\n")
            }
            LearningBuiltinRuntime::Javascript => {
                let relative = format!("./{path}");
                let literal =
                    serde_json::to_string(&relative).unwrap_or_else(|_| format!("\"{relative}\""));
                format!("import {literal};\n")
            }
        }
    }

    async fn run(
        &self,
        files: &[DrillFile],
        test: &str,
        cancel: CancellationToken,
    ) -> Result<RunOutcome> {
        let entrypoint = self.runtime.entrypoint();
        let mut lab_files: Vec<LearningLabFile> = files
            .iter()
            .map(|file| LearningLabFile {
                path: file.path.clone(),
                content: file.text.clone(),
            })
            .collect();
        lab_files.push(LearningLabFile {
            path: entrypoint.to_owned(),
            content: test.to_owned(),
        });
        let spec = BuiltinLabExecutionSpec {
            run_id: uuid::Uuid::new_v4().to_string(),
            runtime: self.runtime,
            files: lab_files,
            read_only_paths: vec![entrypoint.to_owned()],
            limits: LearningLabLimits::default(),
        };
        let result = self.runtimes.execute(spec, cancel).await?;
        Ok(outcome_of(&result))
    }
}

/// What a runtime's result means for the test: only a run that finished says anything about the code.
pub(crate) fn outcome_of(result: &LearningLabExecutionResult) -> RunOutcome {
    match result.status {
        LearningLabRunStatus::Passed => RunOutcome::Passed,
        LearningLabRunStatus::Failed => RunOutcome::Failed {
            output: output_of(result),
        },
        LearningLabRunStatus::TimedOut => RunOutcome::TimedOut {
            output: output_of(result),
        },
        LearningLabRunStatus::Cancelled => RunOutcome::Cancelled,
        LearningLabRunStatus::RuntimeUnavailable => RunOutcome::Unavailable {
            why: if result.stderr.trim().is_empty() {
                "The runtime is not available.".into()
            } else {
                result.stderr.trim().to_owned()
            },
        },
    }
}

/// Everything a run wrote, standard output first.
fn output_of(result: &LearningLabExecutionResult) -> String {
    [result.stdout.trim_end(), result.stderr.trim_end()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A runner for `language`, where the runtime that runs it is available here.
pub fn runner_for(
    language: &str,
    runtimes: &BuiltinRuntimes,
) -> Option<Box<dyn CodeRunner>> {
    let runtime = match language {
        "python" => LearningBuiltinRuntime::Python,
        "javascript" => LearningBuiltinRuntime::Javascript,
        _ => return None,
    };
    let available = runtimes
        .capabilities()
        .into_iter()
        .any(|capability| capability.id == runtime && capability.available);
    available
        .then(|| Box::new(BuiltinRunner::new(runtimes.clone(), runtime)) as Box<dyn CodeRunner>)
}
