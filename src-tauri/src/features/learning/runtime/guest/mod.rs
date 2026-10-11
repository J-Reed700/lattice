//! The engines that run a learner's code: QuickJS for JavaScript and CPython
//! on Wasmtime for Python. Built with the `learning-labs` feature and entered
//! only inside the lab runner's child process, never in the app's own.

mod javascript;
mod python;

use super::embedded_runtime::{BuiltinLabExecutionSpec, LearningBuiltinRuntime};
use super::lab_runtime::LearningLabExecutionResult;
use super::python_runtime::{self, PythonBundle};
use crate::shared::error::{AppError, Result};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

/// Run one lab in this process. `ready` fires when the engine is loaded and
/// the lab's clock starts; a run that fails before then never fires it.
pub(super) async fn execute(
    spec: BuiltinLabExecutionSpec,
    python: Option<PythonBundle>,
    cancellation: CancellationToken,
    ready: oneshot::Sender<()>,
) -> Result<LearningLabExecutionResult> {
    spec.validate()?;
    match spec.runtime {
        LearningBuiltinRuntime::Javascript => {
            let _ = ready.send(());
            tokio::task::spawn_blocking(move || javascript::execute(&spec, cancellation))
                .await
                .map_err(|error| {
                    AppError::InternalError(format!("JavaScript worker failed: {error}"))
                })?
        }
        LearningBuiltinRuntime::Python => {
            let Some(bundle) = python else {
                return Ok(python_runtime::unavailable_result());
            };
            python::execute(&spec, &bundle, cancellation, move || {
                let _ = ready.send(());
            })
            .await
        }
    }
}
