//! JavaScript in QuickJS. No shell, filesystem, network, app IPC, or
//! credential bindings are installed in the context.

use super::super::embedded_runtime::{cancelled, BuiltinLabExecutionSpec};
use super::super::lab_runtime::{LearningLabExecutionResult, LearningLabRunStatus};
use crate::shared::error::{AppError, Result};
use rquickjs::{function::Rest, Coerced, Context, Function, Module, Object, Runtime};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct CapturedOutput {
    stdout: String,
    stderr: String,
    bytes: usize,
    truncated: bool,
}

impl CapturedOutput {
    fn append(&mut self, value: &str, stderr: bool, maximum: usize) {
        let remaining = maximum.saturating_sub(self.bytes);
        let mut keep = value.len().min(remaining);
        while !value.is_char_boundary(keep) {
            keep = keep.saturating_sub(1);
        }
        if let Some(text) = value.get(..keep) {
            if stderr {
                self.stderr.push_str(text)
            } else {
                self.stdout.push_str(text)
            }
        }
        self.bytes += keep;
        self.truncated |= keep < value.len();
    }
}

pub(super) fn execute(
    spec: &BuiltinLabExecutionSpec,
    token: CancellationToken,
) -> Result<LearningLabExecutionResult> {
    if token.is_cancelled() {
        return Ok(cancelled());
    }
    let started = Instant::now();
    let timeout = Duration::from_secs(spec.limits.timeout_seconds);
    let output = Arc::new(Mutex::new(CapturedOutput::default()));
    let runtime = Runtime::new().map_err(|error| AppError::InternalError(error.to_string()))?;
    runtime.set_memory_limit((u64::from(spec.limits.memory_megabytes) * 1024 * 1024) as usize);
    runtime.set_max_stack_size(512 * 1024);
    let interrupt_token = token.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || {
        interrupt_token.is_cancelled() || started.elapsed() >= timeout
    })));
    let context =
        Context::full(&runtime).map_err(|error| AppError::InternalError(error.to_string()))?;
    let maximum = spec.limits.output_bytes;
    let evaluation = context.with(|ctx| -> std::result::Result<(), String> {
        let execute = || -> rquickjs::Result<()> {
            let console = Object::new(ctx.clone())?;
            for (name, is_error) in [
                ("log", false),
                ("info", false),
                ("warn", true),
                ("error", true),
            ] {
                let captured = output.clone();
                let logger = Function::new(ctx.clone(), move |parts: Rest<Coerced<String>>| {
                    if let Ok(mut buffer) = captured.lock() {
                        for (index, part) in parts.0.iter().enumerate() {
                            if index > 0 {
                                buffer.append(" ", is_error, maximum);
                            }
                            buffer.append(&part.0, is_error, maximum);
                        }
                        buffer.append("\n", is_error, maximum);
                    }
                })?;
                console.set(name, logger)?;
            }
            ctx.globals().set("console", console)?;
            // Declare only supplied source modules. No module loader is
            // installed, so importing a host path or package cannot perform I/O.
            let mut entrypoint = None;
            for file in &spec.files {
                if file.path.ends_with(".js") || file.path.ends_with(".mjs") {
                    let module =
                        Module::declare(ctx.clone(), file.path.as_str(), file.content.as_bytes())?;
                    if file.path == spec.runtime.entrypoint() {
                        entrypoint = Some(module);
                    }
                }
            }
            let entrypoint =
                entrypoint.ok_or_else(|| rquickjs::Error::new_loading("checks.mjs"))?;
            let (_, promise) = entrypoint.eval()?;
            promise.finish::<()>()?;
            Ok(())
        };
        execute().map_err(|error| rquickjs::CaughtError::from_error(&ctx, error).to_string())
    });
    let status = if token.is_cancelled() {
        LearningLabRunStatus::Cancelled
    } else if started.elapsed() >= timeout {
        LearningLabRunStatus::TimedOut
    } else if evaluation.is_ok() {
        LearningLabRunStatus::Passed
    } else {
        LearningLabRunStatus::Failed
    };
    let mut captured = output
        .lock()
        .map_err(|_| AppError::InternalError("Learning output capture failed.".into()))?;
    if let Err(error) = evaluation {
        captured.append(&error, true, maximum);
    }
    Ok(LearningLabExecutionResult {
        exit_code: match status {
            LearningLabRunStatus::Passed => Some(0),
            LearningLabRunStatus::Failed => Some(1),
            _ => None,
        },
        status,
        stdout: captured.stdout.clone(),
        stderr: captured.stderr.clone(),
        output_truncated: captured.truncated,
        duration_ms: started.elapsed().as_millis().min(i64::MAX as u128) as i64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::embedded_runtime::LearningBuiltinRuntime;
    use crate::features::learning::lab_runtime::{LearningLabFile, LearningLabLimits};

    async fn execute_builtin_lab(
        spec: BuiltinLabExecutionSpec,
        token: CancellationToken,
    ) -> Result<LearningLabExecutionResult> {
        super::super::execute(spec, None, token, tokio::sync::oneshot::channel().0).await
    }

    fn spec(solution: &str, checks: &str) -> BuiltinLabExecutionSpec {
        BuiltinLabExecutionSpec {
            run_id: uuid::Uuid::new_v4().to_string(),
            runtime: LearningBuiltinRuntime::Javascript,
            files: vec![
                LearningLabFile {
                    path: "solution.mjs".into(),
                    content: solution.into(),
                },
                LearningLabFile {
                    path: "checks.mjs".into(),
                    content: checks.into(),
                },
            ],
            read_only_paths: vec!["checks.mjs".into()],
            limits: LearningLabLimits {
                timeout_seconds: 1,
                ..Default::default()
            },
        }
    }
    #[tokio::test]
    async fn executes_real_modules_and_assertion_failures() -> Result<()> {
        let checks = "import {add} from './solution.mjs'; if (add(2,3)!==5) throw new Error('expected five'); console.log('passed');";
        let result = execute_builtin_lab(
            spec("export const add=(a,b)=>a+b", checks),
            CancellationToken::new(),
        )
        .await?;
        assert_eq!(result.status, LearningLabRunStatus::Passed);
        assert!(result.stdout.contains("passed"));
        let result = execute_builtin_lab(
            spec("export const add=(a,b)=>a-b", checks),
            CancellationToken::new(),
        )
        .await?;
        assert_eq!(result.status, LearningLabRunStatus::Failed);
        assert!(result.stderr.contains("expected five"));
        Ok(())
    }
    #[tokio::test]
    async fn bounds_loops_output_and_cancellation() -> Result<()> {
        let result = execute_builtin_lab(
            spec("while(true){}", "import './solution.mjs'"),
            CancellationToken::new(),
        )
        .await?;
        assert_eq!(result.status, LearningLabRunStatus::TimedOut);
        let mut input = spec("console.log('€'.repeat(20000))", "import './solution.mjs'");
        input.limits.output_bytes = 4096;
        let result = execute_builtin_lab(input, CancellationToken::new()).await?;
        assert!(result.output_truncated);
        assert!(result.stdout.len() + result.stderr.len() <= 4096);
        let token = CancellationToken::new();
        token.cancel();
        assert_eq!(
            execute_builtin_lab(spec("", ""), token).await?.status,
            LearningLabRunStatus::Cancelled
        );
        Ok(())
    }
    #[tokio::test]
    async fn cancellation_interrupts_an_active_javascript_loop() -> Result<()> {
        let mut input = spec("while(true){}", "import './solution.mjs'");
        input.limits.timeout_seconds = 10;
        let token = CancellationToken::new();
        let cancel = token.clone();
        let running = tokio::task::spawn_blocking(move || execute(&input, token));
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_secs(2), running)
            .await
            .map_err(|_| {
                AppError::InternalError("JavaScript cancellation did not finish promptly.".into())
            })?
            .map_err(|error| AppError::InternalError(error.to_string()))??;
        assert_eq!(result.status, LearningLabRunStatus::Cancelled);
        Ok(())
    }
    #[tokio::test]
    async fn javascript_heap_exhaustion_fails_without_exceeding_guest_memory_cap() -> Result<()> {
        let mut input = spec(
            "const allocations=[]; while(true) allocations.push(new Uint8Array(1024*1024));",
            "import './solution.mjs'",
        );
        input.limits.timeout_seconds = 10;
        input.limits.memory_megabytes = 64;
        let result = execute_builtin_lab(input, CancellationToken::new()).await?;
        assert_eq!(result.status, LearningLabRunStatus::Failed);
        assert!(
            result.stderr.to_ascii_lowercase().contains("memory")
                || result.stderr.to_ascii_lowercase().contains("allocation")
        );
        Ok(())
    }
    #[tokio::test]
    async fn does_not_expose_host_capabilities_or_import_files() -> Result<()> {
        let code="for (const name of ['process','require','fetch','XMLHttpRequest','WebSocket','__TAURI_INTERNALS__']) { if (typeof globalThis[name] !== 'undefined') throw Error(name); }";
        assert_eq!(
            execute_builtin_lab(
                spec(code, "import './solution.mjs'"),
                CancellationToken::new()
            )
            .await?
            .status,
            LearningLabRunStatus::Passed
        );
        for import in ["node:fs", "/etc/passwd", "https://example.com/module.js"] {
            let code = format!("import {:?}", import);
            assert_eq!(
                execute_builtin_lab(
                    spec(&code, "import './solution.mjs'"),
                    CancellationToken::new()
                )
                .await?
                .status,
                LearningLabRunStatus::Failed
            );
        }
        Ok(())
    }
}
