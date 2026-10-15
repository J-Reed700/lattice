//! Built-in language runtimes. A learner's code never runs in the app's own
//! process: each run is handed to the lab runner, a child copy of this
//! executable that hosts the engine and is killed on timeout or cancellation.

use crate::features::learning::lab_runtime::{
    self, LearningLabExecutionResult, LearningLabFile, LearningLabLimits, LearningLabRunStatus,
};
use crate::features::learning::{lab_runner, python_runtime};
use crate::shared::error::{AppError, Result};
use python_runtime::PythonRuntime;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Why a built-in runtime cannot run in a build without the engines.
const UNAVAILABLE_IN_BUILD: &str = "Embedded runtimes are unavailable in this build.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum LearningBuiltinRuntime {
    Javascript,
    Python,
}

impl LearningBuiltinRuntime {
    /// Bump this when the interpreter or its execution contract changes. Old
    /// exercises keep their outputs and cannot silently rerun on a new guest.
    pub fn version(self) -> &'static str {
        match self {
            Self::Javascript => "quickjs-rquickjs-0.14.0-v1",
            Self::Python => "cpython-3.14.8-wasi-v1",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Javascript => "javascript",
            Self::Python => "python",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "javascript" => Ok(Self::Javascript),
            "python" => Ok(Self::Python),
            _ => Err(AppError::InvalidData(
                "Unknown built-in learning runtime.".into(),
            )),
        }
    }

    pub fn entrypoint(self) -> &'static str {
        match self {
            Self::Javascript => "checks.mjs",
            Self::Python => "checks.py",
        }
    }

    pub fn context(
        self,
    ) -> crate::features::learning::practical_generation::PracticalRuntimeContext {
        let (name, contract) = match self {
            Self::Javascript => ("JavaScript", "ECMAScript modules only. Put editable exported functions in solution.mjs. The hidden evaluator must be checks.mjs, importing './solution.mjs' and throwing on failed assertions. console.log/error are available. Node.js, DOM, React, fetch, timers, external packages, filesystem and networking are unavailable. Use relative imports among supplied .js/.mjs files only."),
            Self::Python => ("Python", "CPython with the Python standard library in WASI. Put editable functions in solution.py and hidden assertions in checks.py, which imports solution and must exit nonzero on failure. No pip packages, networking, subprocesses, filesystem writes or host files. Supplied exercise files are read-only during execution."),
        };
        crate::features::learning::practical_generation::PracticalRuntimeContext {
            name: name.into(),
            command: vec![self.entrypoint().into()],
            contract: contract.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningBuiltinRuntimeCapability {
    pub id: LearningBuiltinRuntime,
    pub name: String,
    pub description: String,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinLabExecutionSpec {
    pub run_id: String,
    pub runtime: LearningBuiltinRuntime,
    pub files: Vec<LearningLabFile>,
    pub read_only_paths: Vec<String>,
    pub limits: LearningLabLimits,
}

impl BuiltinLabExecutionSpec {
    pub fn validate(&self) -> Result<()> {
        lab_runtime::validate_lab_files(
            &self.run_id,
            &self.files,
            &self.read_only_paths,
            &self.limits,
        )?;
        let entrypoint = self.runtime.entrypoint();
        if !self.read_only_paths.iter().any(|path| path == entrypoint) {
            return Err(AppError::InvalidInput(format!(
                "The built-in runtime requires a protected {entrypoint} evaluator."
            )));
        }
        Ok(())
    }
}

/// How many guests run at once, whatever started them.
const CONCURRENT_GUESTS: usize = 2;

/// The built-in runtimes as one app runs them: the guest slots every run
/// shares, the bundled Python they run on, and its interpreter warm-up.
/// Clones share all three.
#[derive(Clone)]
pub struct BuiltinRuntimes {
    /// Limits concurrent guests as well as per-guest memory. Cancelling while
    /// queued cannot create a late guest after the caller has stopped the run.
    guest_slots: Arc<tokio::sync::Semaphore>,
    /// The verified bundle; `None` when it is missing or failed its check.
    python: Option<PythonRuntime>,
    /// Only one Python run loads the interpreter at a time, so a cold cache is
    /// compiled once and every other run waits to load the result.
    python_warmup: Arc<tokio::sync::Mutex<()>>,
}

impl BuiltinRuntimes {
    /// Runtimes whose Python labs run on `python`, or report it unavailable.
    pub fn new(python: Option<PythonRuntime>) -> Self {
        Self {
            guest_slots: Arc::new(tokio::sync::Semaphore::new(CONCURRENT_GUESTS)),
            python,
            python_warmup: Arc::default(),
        }
    }

    /// Runtimes on the source tree's Python bundle, for one test.
    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        Self::new(PythonRuntime::source_tree())
    }

    /// Whether each built-in runtime can run here, and why not when it cannot.
    pub fn capabilities(&self) -> Vec<LearningBuiltinRuntimeCapability> {
        if !cfg!(feature = "learning-labs") {
            return [
                (LearningBuiltinRuntime::Python, "Python"),
                (LearningBuiltinRuntime::Javascript, "JavaScript"),
            ]
            .into_iter()
            .map(|(id, name)| LearningBuiltinRuntimeCapability {
                id,
                name: name.into(),
                description: format!("{name} exercises."),
                available: false,
                reason: Some(UNAVAILABLE_IN_BUILD.into()),
            })
            .collect();
        }
        let python_available = self.python.is_some();
        vec![
            LearningBuiltinRuntimeCapability {
                id: LearningBuiltinRuntime::Python,
                name: "Python".into(),
                description: "Python fundamentals and standard-library exercises. Included with Lattice.".into(),
                available: python_available,
                reason: (!python_available).then(|| "The bundled Python runtime is missing or invalid. Reinstall this build, or use the optional Python container environment.".into()),
            },
            LearningBuiltinRuntimeCapability {
                id: LearningBuiltinRuntime::Javascript,
                name: "JavaScript".into(),
                description: "JavaScript functions, algorithms and module exercises. Included with Lattice.".into(),
                available: true, reason: None,
            },
        ]
    }

    /// Run one lab on a built-in runtime, waiting for a guest slot first.
    pub async fn execute(
        &self,
        spec: BuiltinLabExecutionSpec,
        token: CancellationToken,
    ) -> Result<LearningLabExecutionResult> {
        spec.validate()?;
        if !cfg!(feature = "learning-labs") {
            return Ok(LearningLabExecutionResult {
                status: LearningLabRunStatus::RuntimeUnavailable,
                exit_code: None,
                stdout: String::new(),
                stderr: UNAVAILABLE_IN_BUILD.into(),
                output_truncated: false,
                duration_ms: 0,
            });
        }
        let permit = tokio::select! {
            permit = self.guest_slots.acquire() => permit.map_err(|_| AppError::InternalError("Learning runtime is shutting down.".into()))?,
            _ = token.cancelled() => return Ok(cancelled()),
        };
        let python = match spec.runtime {
            LearningBuiltinRuntime::Javascript => None,
            LearningBuiltinRuntime::Python => match &self.python {
                Some(python) => Some(python.bundle()),
                None => return Ok(python_runtime::unavailable_result()),
            },
        };
        let warmup = if python.is_some() {
            tokio::select! {
                guard = self.python_warmup.lock() => Some(guard),
                _ = token.cancelled() => return Ok(cancelled()),
            }
        } else {
            None
        };
        let result = lab_runner::run(spec, python, warmup, token).await;
        drop(permit);
        result
    }
}

pub(super) fn cancelled() -> LearningLabExecutionResult {
    LearningLabExecutionResult {
        status: LearningLabRunStatus::Cancelled,
        exit_code: None,
        stdout: String::new(),
        stderr: "Run cancelled.".into(),
        output_truncated: false,
        duration_ms: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn javascript_spec() -> BuiltinLabExecutionSpec {
        BuiltinLabExecutionSpec {
            run_id: uuid::Uuid::new_v4().to_string(),
            runtime: LearningBuiltinRuntime::Javascript,
            files: vec![LearningLabFile {
                path: "checks.mjs".into(),
                content: "console.log('ran')".into(),
            }],
            read_only_paths: vec!["checks.mjs".into()],
            limits: LearningLabLimits::default(),
        }
    }

    #[tokio::test]
    async fn a_run_cancelled_before_it_starts_reports_cancelled() -> Result<()> {
        let token = CancellationToken::new();
        token.cancel();
        let result = BuiltinRuntimes::for_tests()
            .execute(javascript_spec(), token)
            .await?;
        let expected = if cfg!(feature = "learning-labs") {
            LearningLabRunStatus::Cancelled
        } else {
            LearningLabRunStatus::RuntimeUnavailable
        };
        assert_eq!(result.status, expected);
        Ok(())
    }

    #[test]
    fn capabilities_say_whether_this_build_has_the_engines() {
        let capabilities = BuiltinRuntimes::for_tests().capabilities();
        let javascript = capabilities
            .iter()
            .find(|capability| capability.id == LearningBuiltinRuntime::Javascript);
        assert_eq!(
            javascript.map(|capability| capability.available),
            Some(cfg!(feature = "learning-labs"))
        );
        if !cfg!(feature = "learning-labs") {
            assert!(capabilities
                .iter()
                .all(|capability| capability.reason.as_deref() == Some(UNAVAILABLE_IN_BUILD)));
        }
    }
}
