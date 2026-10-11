//! CPython hosted as a capability-limited WASI guest.
//!
//! Learner programs run in a fresh Wasmtime store with no inherited
//! environment, no network interfaces, read-only exercise files and no
//! writable filesystem preopens. Compiling the bundled interpreter is costly,
//! and each run is a fresh process, so the compiled module is kept in the
//! app's cache directory and reloaded from there.

use super::super::embedded_runtime::BuiltinLabExecutionSpec;
use super::super::lab_runtime::{self, LearningLabExecutionResult, LearningLabRunStatus};
use super::super::python_runtime::{self, PythonBundle, WASM_NAME};
use crate::shared::error::{AppError, Result};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::AsyncWrite;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
use wasmtime::{Config, Engine, Linker, Module, ResourceLimiter, Store};
use wasmtime_wasi::cli::AsyncStdoutStream;
use wasmtime_wasi::p1::{self, WasiP1Ctx};
use wasmtime_wasi::I32Exit;
use wasmtime_wasi::{FsPerms, WasiCtxBuilder, WasiCtxView, WasiView};

const EPOCH_INTERVAL: Duration = Duration::from_millis(10);
const WASM_FUEL_PER_CPU_MILLISECOND: u64 = 2_000_000;
const FUEL_EXHAUSTED_MARKER: &str = "__LATTICE_PYTHON_FUEL_EXHAUSTED__";
const MAX_TEMP_INPUT_BYTES: usize = 2 * 1024 * 1024;
const COMPILED_EXTENSION: &str = "cwasm";

static WASM_ENGINE: OnceLock<std::result::Result<Engine, String>> = OnceLock::new();
static WASM_MODULE: OnceLock<std::result::Result<Module, String>> = OnceLock::new();

/// Execute a protected `checks.py` entry point under Wasmtime. `ready` is
/// called once the interpreter module is loaded, when the lab's clock starts.
pub(super) async fn execute(
    spec: &BuiltinLabExecutionSpec,
    bundle: &PythonBundle,
    cancellation: CancellationToken,
    ready: impl FnOnce() + Send + 'static,
) -> Result<LearningLabExecutionResult> {
    if cancellation.is_cancelled() {
        return Ok(result(
            LearningLabRunStatus::Cancelled,
            None,
            String::new(),
            "Run cancelled.".into(),
            false,
            0,
        ));
    }
    let worker_spec = spec.clone();
    let worker_bundle = bundle.clone();
    let worker_token = CancellationToken::new();
    let worker_stop = worker_token.clone();
    let worker = tokio::task::spawn_blocking(move || {
        let engine = engine().map_err(|error| AppError::InternalError(error.to_string()))?;
        let module = module(&engine, &worker_bundle).map_err(|error| {
            AppError::InternalError(format!(
                "The bundled Python module could not be loaded: {error}"
            ))
        })?;
        if worker_token.is_cancelled() {
            return Ok(result(
                LearningLabRunStatus::Cancelled,
                None,
                String::new(),
                "Run cancelled.".into(),
                false,
                0,
            ));
        }
        // Compiling the trusted, bundled interpreter module is one-time setup,
        // not learner CPU. Begin lab time only once the shared module is ready.
        ready();
        let started = Instant::now();
        tokio::runtime::Handle::current().block_on(execute_python_guest(
            &worker_bundle.root,
            &engine,
            &module,
            &worker_spec,
            worker_token,
            started,
        ))
    });

    let mut worker = worker;
    // Hold the caller until module compilation or async guest cancellation
    // really exits, so a cancelled run leaves no detached compiler behind.
    tokio::select! {
        joined = &mut worker => joined.map_err(|error| AppError::InternalError(format!("Python guest worker failed: {error}")))?,
        _ = cancellation.cancelled() => {
            worker_stop.cancel();
            worker.await.map_err(|error| AppError::InternalError(format!("Python guest worker failed: {error}")))?
        },
    }
}

async fn execute_python_guest(
    root: &Path,
    engine: &Engine,
    module: &Module,
    spec: &BuiltinLabExecutionSpec,
    cancellation: CancellationToken,
    started: Instant,
) -> Result<LearningLabExecutionResult> {
    let timeout = Duration::from_secs(spec.limits.timeout_seconds);
    let deadline = started + timeout;
    let temp = tempfile::tempdir().map_err(|error| AppError::FileSystem(error.to_string()))?;
    let workspace = temp.path().join("work");
    fs::create_dir(&workspace).map_err(|error| AppError::FileSystem(error.to_string()))?;
    materialize_lab_files(&workspace, spec)?;

    if cancellation.is_cancelled() {
        return Ok(result(
            LearningLabRunStatus::Cancelled,
            None,
            String::new(),
            "Run cancelled.".into(),
            false,
            elapsed_ms(started),
        ));
    }

    let output = Arc::new(Mutex::new(BoundedOutput::new(spec.limits.output_bytes)));
    let stdout = AsyncStdoutStream::new(
        8_192,
        BoundedOutputWriter::new(output.clone(), OutputChannel::Stdout),
    );
    let stderr = AsyncStdoutStream::new(
        8_192,
        BoundedOutputWriter::new(output.clone(), OutputChannel::Stderr),
    );
    let mut wasi = WasiCtxBuilder::new();
    wasi.env("PYTHONHOME", "/runtime")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONNOUSERSITE", "1")
        .args(&[
            "/runtime/python.wasm",
            "-S",
            "-B",
            "-c",
            "import sys; sys.path[:] = ['/work'] + [p for p in sys.path if p.startswith('/runtime/')]; import runpy; runpy.run_path('/work/checks.py', run_name='__main__')",
        ])
        .stdin(std::io::empty())
        .stdout(stdout)
        .stderr(stderr)
        .preopened_dir(root, "/runtime", FsPerms::ReadOnly)
        .map_err(|error| AppError::FileSystem(error.to_string()))?
        .preopened_dir(&workspace, "/work", FsPerms::ReadOnly)
        .map_err(|error| AppError::FileSystem(error.to_string()))?;

    let state = StoreState {
        wasi: wasi.build_p1(),
        maximum_memory: (u64::from(spec.limits.memory_megabytes) * 1024 * 1024) as usize,
        maximum_table_elements: 16_384,
    };
    let mut store = Store::new(engine, state);
    store.limiter(|state| state);
    // CPU millicores are converted into an aggregate instruction budget for
    // the lab's wall-clock allowance (Wasmtime does not expose a host CPU quota).
    let fuel = u64::from(spec.limits.cpu_millis)
        .saturating_mul(timeout.as_secs())
        .saturating_mul(WASM_FUEL_PER_CPU_MILLISECOND);
    store
        .set_fuel(fuel)
        .map_err(|error| AppError::InternalError(error.to_string()))?;
    store.epoch_deadline_async_yield_and_update(1);

    let mut linker = Linker::new(engine);
    p1::add_to_linker_async(&mut linker, |state: &mut StoreState| &mut state.wasi)
        .map_err(|error| AppError::InternalError(error.to_string()))?;

    let ticker_engine = engine.clone();
    let ticker = tokio::runtime::Handle::current().spawn(async move {
        let mut interval = tokio::time::interval(EPOCH_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            ticker_engine.increment_epoch();
        }
    });

    let execution = tokio::select! {
        _ = cancellation.cancelled() => Err("Python execution cancelled.".to_string()),
        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => Err("Python execution timed out.".to_string()),
        result = run_module(&mut store, &linker, module) => result,
    };
    ticker.abort();

    let mut captured = output
        .lock()
        .map_err(|_| AppError::InternalError("Python output capture failed.".into()))?;
    let timed_out = started.elapsed() >= timeout;
    let fuel_exhausted = execution
        .as_ref()
        .is_err_and(|error| error.starts_with(FUEL_EXHAUSTED_MARKER));
    let status = if cancellation.is_cancelled() {
        LearningLabRunStatus::Cancelled
    } else if timed_out || fuel_exhausted {
        LearningLabRunStatus::TimedOut
    } else if execution.is_ok() {
        LearningLabRunStatus::Passed
    } else {
        LearningLabRunStatus::Failed
    };
    if let Err(error) = execution {
        let detail = error.strip_prefix(FUEL_EXHAUSTED_MARKER).unwrap_or(&error);
        captured.append_stderr(format!("{detail}\n").as_bytes());
    }
    let exit_code = match &status {
        LearningLabRunStatus::Passed => Some(0),
        LearningLabRunStatus::Failed => Some(1),
        _ => None,
    };
    let mut stdout = String::from_utf8_lossy(&captured.stdout).into_owned();
    let mut stderr = String::from_utf8_lossy(&captured.stderr).into_owned();
    captured.truncated |= truncate_string(&mut stdout, spec.limits.output_bytes);
    let remaining_output = spec.limits.output_bytes.saturating_sub(stdout.len());
    captured.truncated |= truncate_string(&mut stderr, remaining_output);
    Ok(result(
        status,
        exit_code,
        stdout,
        stderr,
        captured.truncated,
        elapsed_ms(started),
    ))
}

fn materialize_lab_files(workspace: &Path, spec: &BuiltinLabExecutionSpec) -> Result<()> {
    let mut total_bytes = 0usize;
    for file in &spec.files {
        let relative = lab_runtime::validate_lab_path(&file.path)?;
        let bytes = file.content.as_bytes();
        if bytes.len() > 256 * 1024 {
            return Err(AppError::FileTooLarge {
                path: file.path.clone(),
                size_bytes: bytes.len() as u64,
                max_size_bytes: 256 * 1024,
            });
        }
        total_bytes = total_bytes.saturating_add(bytes.len());
        if total_bytes > MAX_TEMP_INPUT_BYTES {
            return Err(AppError::FileTooLarge {
                path: file.path.clone(),
                size_bytes: total_bytes as u64,
                max_size_bytes: MAX_TEMP_INPUT_BYTES as u64,
            });
        }
        let destination = workspace.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| AppError::FileSystem(error.to_string()))?;
        }
        fs::write(destination, bytes).map_err(|error| AppError::FileSystem(error.to_string()))?;
    }
    Ok(())
}

fn engine() -> std::result::Result<Engine, String> {
    WASM_ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config
                .consume_fuel(true)
                .epoch_interruption(true)
                .cranelift_opt_level(wasmtime::OptLevel::None)
                .max_wasm_stack(1_048_576);
            Engine::new(&config).map_err(|error| error.to_string())
        })
        .clone()
}

fn module(engine: &Engine, bundle: &PythonBundle) -> std::result::Result<Module, String> {
    WASM_MODULE
        .get_or_init(|| load_module(engine, bundle))
        .clone()
}

/// The compiled interpreter: from the cache when an artifact for this exact
/// bundle and engine is there, otherwise compiled and cached for later runs.
fn load_module(engine: &Engine, bundle: &PythonBundle) -> std::result::Result<Module, String> {
    let wasm = bundle.root.join(WASM_NAME);
    let Some(cached) = compiled_module_path(engine, bundle) else {
        return Module::from_file(engine, &wasm).map_err(|error| error.to_string());
    };
    if cached.is_file() {
        // SAFETY: the artifact is one this executable's own engine wrote from
        // the digest-verified bundle into the app's cache directory, under a
        // name bound to that digest and to the engine's compatibility hash;
        // Wasmtime still rejects one from another version or configuration,
        // and a rejected one is recompiled below. Artifacts are only ever
        // replaced by rename, never rewritten in place, so the mapped file
        // cannot change under the module.
        #[allow(unsafe_code)]
        let loaded = unsafe { Module::deserialize_file(engine, &cached) };
        if let Ok(module) = loaded {
            return Ok(module);
        }
    }
    let module = Module::from_file(engine, &wasm).map_err(|error| error.to_string())?;
    if let Err(error) = store_compiled_module(&module, &cached) {
        tracing::warn!(%error, "The compiled Python module could not be cached");
    }
    Ok(module)
}

fn compiled_module_path(engine: &Engine, bundle: &PythonBundle) -> Option<PathBuf> {
    let directory = bundle.module_cache.as_ref()?;
    let digest = python_runtime::wasm_digest(&bundle.root).ok()?;
    let mut hasher = DefaultHasher::new();
    engine.precompile_compatibility_hash().hash(&mut hasher);
    Some(directory.join(format!(
        "python-{digest}-{:016x}.{COMPILED_EXTENSION}",
        hasher.finish()
    )))
}

/// Write `module` to `path` whole or not at all, replacing artifacts of
/// earlier bundles or engines: one compiled interpreter is kept.
fn store_compiled_module(module: &Module, path: &Path) -> io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("The module cache path has no directory"))?;
    fs::create_dir_all(directory)?;
    let bytes = module.serialize().map_err(io::Error::other)?;
    let mut staged = tempfile::NamedTempFile::new_in(directory)?;
    staged.write_all(&bytes)?;
    staged.persist(path).map_err(|error| error.error)?;
    for entry in fs::read_dir(directory)?.flatten() {
        let stale = entry.path();
        if stale != path
            && stale
                .extension()
                .is_some_and(|extension| extension == COMPILED_EXTENSION)
        {
            let _ = fs::remove_file(stale);
        }
    }
    Ok(())
}

async fn run_module(
    store: &mut Store<StoreState>,
    linker: &Linker<StoreState>,
    module: &Module,
) -> std::result::Result<(), String> {
    let instance = linker
        .instantiate_async(&mut *store, module)
        .await
        .map_err(classify_wasmtime_error)?;
    let start = instance
        .get_typed_func::<(), ()>(&mut *store, "_start")
        .map_err(|error| error.to_string())?;
    match start.call_async(&mut *store, ()).await {
        Ok(()) => Ok(()),
        Err(error) => match error.downcast_ref::<I32Exit>() {
            Some(exit) if exit.0 == 0 => Ok(()),
            Some(exit) => Err(format!("Python exited with status {}.", exit.0)),
            None => Err(classify_wasmtime_error(error)),
        },
    }
}

fn classify_wasmtime_error(error: wasmtime::Error) -> String {
    if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel) {
        return format!("{FUEL_EXHAUSTED_MARKER}Python instruction budget exceeded.");
    }
    error.to_string()
}

fn truncate_string(value: &mut String, max_bytes: usize) -> bool {
    if value.len() <= max_bytes {
        return false;
    }
    let mut boundary = max_bytes;
    while !value.is_char_boundary(boundary) {
        boundary = boundary.saturating_sub(1);
    }
    value.truncate(boundary);
    true
}

struct StoreState {
    wasi: WasiP1Ctx,
    maximum_memory: usize,
    maximum_table_elements: usize,
}

impl WasiView for StoreState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        self.wasi.ctx()
    }
}

impl ResourceLimiter for StoreState {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(desired <= self.maximum_memory && maximum.is_none_or(|limit| desired <= limit))
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(desired <= self.maximum_table_elements && maximum.is_none_or(|limit| desired <= limit))
    }
}

#[derive(Default)]
struct BoundedOutput {
    maximum: usize,
    used: usize,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    truncated: bool,
}

impl BoundedOutput {
    fn new(maximum: usize) -> Self {
        Self {
            maximum,
            ..Self::default()
        }
    }

    fn append(&mut self, channel: OutputChannel, bytes: &[u8]) -> usize {
        let remaining = self.maximum.saturating_sub(self.used);
        let count = bytes.len().min(remaining);
        match channel {
            OutputChannel::Stdout => {
                if let Some(prefix) = bytes.get(..count) {
                    self.stdout.extend_from_slice(prefix);
                }
            }
            OutputChannel::Stderr => {
                if let Some(prefix) = bytes.get(..count) {
                    self.stderr.extend_from_slice(prefix);
                }
            }
        }
        self.used = self.used.saturating_add(count);
        self.truncated |= count < bytes.len();
        count
    }

    fn append_stderr(&mut self, bytes: &[u8]) {
        self.append(OutputChannel::Stderr, bytes);
    }
}

#[derive(Clone, Copy)]
enum OutputChannel {
    Stdout,
    Stderr,
}

struct BoundedOutputWriter {
    output: Arc<Mutex<BoundedOutput>>,
    channel: OutputChannel,
}

impl BoundedOutputWriter {
    fn new(output: Arc<Mutex<BoundedOutput>>, channel: OutputChannel) -> Self {
        Self { output, channel }
    }
}

impl AsyncWrite for BoundedOutputWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let mut output = match this.output.lock() {
            Ok(output) => output,
            Err(_) => {
                return Poll::Ready(Err(io::Error::other("Python output lock was poisoned")));
            }
        };
        let count = output.append(this.channel, bytes);
        if count == 0 && !bytes.is_empty() {
            return Poll::Ready(Err(io::Error::other("Python output limit exceeded")));
        }
        Poll::Ready(Ok(count))
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn elapsed_ms(started: Instant) -> i64 {
    started.elapsed().as_millis().min(i64::MAX as u128) as i64
}

fn result(
    status: LearningLabRunStatus,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    output_truncated: bool,
    duration_ms: i64,
) -> LearningLabExecutionResult {
    LearningLabExecutionResult {
        status,
        exit_code,
        stdout,
        stderr,
        output_truncated,
        duration_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::embedded_runtime::LearningBuiltinRuntime;
    use crate::features::learning::lab_runtime::{LearningLabFile, LearningLabLimits};

    async fn execute_python_lab(
        spec: &BuiltinLabExecutionSpec,
        cancellation: CancellationToken,
    ) -> Result<LearningLabExecutionResult> {
        let bundle = python_runtime::test_bundle();
        execute(spec, &bundle, cancellation, || {}).await
    }

    fn spec(solution: &str, checks: &str) -> BuiltinLabExecutionSpec {
        BuiltinLabExecutionSpec {
            run_id: uuid::Uuid::new_v4().to_string(),
            runtime: LearningBuiltinRuntime::Python,
            files: vec![
                LearningLabFile {
                    path: "solution.py".into(),
                    content: solution.into(),
                },
                LearningLabFile {
                    path: "checks.py".into(),
                    content: checks.into(),
                },
            ],
            read_only_paths: vec!["checks.py".into()],
            limits: LearningLabLimits::default(),
        }
    }

    #[tokio::test]
    async fn executes_cpython_and_standard_library() -> Result<()> {
        let input = spec(
            "def answer():\n    return 42\n",
            "import math, solution\nassert solution.answer() == 42\nassert math.gcd(24, 18) == 6\nprint('passed')\n",
        );
        let execution = execute_python_lab(&input, CancellationToken::new()).await?;
        assert_eq!(
            execution.status,
            LearningLabRunStatus::Passed,
            "{}",
            execution.stderr
        );
        assert!(execution.stdout.contains("passed"));
        Ok(())
    }

    #[tokio::test]
    async fn failures_are_reported_and_filesystem_is_read_only() -> Result<()> {
        let input = spec(
            "def answer():\n    return 41\n",
            "import solution\nassert solution.answer() == 42, 'wrong answer'\n",
        );
        let execution = execute_python_lab(&input, CancellationToken::new()).await?;
        assert_eq!(execution.status, LearningLabRunStatus::Failed);
        assert!(execution.stderr.contains("wrong answer"));

        let input = spec(
            "",
            "import errno, os\nassert set(os.environ) <= {'PYTHONHOME', 'PYTHONDONTWRITEBYTECODE', 'PYTHONNOUSERSITE'}\ntry:\n open('/etc/passwd').read()\nexcept OSError:\n pass\nelse:\n raise AssertionError('host file visible')\ntry:\n open('/work/new.txt', 'w').write('no')\nexcept OSError:\n pass\nelse:\n raise AssertionError('workspace writable')\n# Probe the low-level API directly: this bypasses getaddrinfo, which WASI may not implement.\ntry:\n import _socket\nexcept ImportError:\n pass\nelse:\n denied = {getattr(errno, name) for name in ('EPERM', 'EACCES', 'ENOSYS', 'ENOTSUP', 'EOPNOTSUPP', 'EAFNOSUPPORT', 'EPROTONOSUPPORT', 'ENOPROTOOPT') if hasattr(errno, name)}\n try:\n  raw = _socket.socket(_socket.AF_INET, _socket.SOCK_STREAM)\n except NotImplementedError:\n  pass\n except OSError as error:\n  assert error.errno in denied, f'socket creation failed for an unexpected reason: {error!r}'\n else:\n  try:\n   raw.connect(('192.0.2.1', 9))\n  except NotImplementedError:\n   pass\n  except OSError as error:\n   assert error.errno in denied, f'socket connect failed for an unexpected reason: {error!r}'\n  else:\n   raise AssertionError('network connection succeeded')\n  finally:\n   raw.close()\nprint('host access denied')\n",
        );
        let execution = execute_python_lab(&input, CancellationToken::new()).await?;
        assert_eq!(
            execution.status,
            LearningLabRunStatus::Passed,
            "{}",
            execution.stderr
        );
        assert!(execution.stdout.contains("host access denied"));
        Ok(())
    }

    #[tokio::test]
    async fn bounds_loop_output_and_cancellation() -> Result<()> {
        let mut input = spec("while True:\n    pass\n", "import solution\n");
        input.limits.timeout_seconds = 1;
        let execution = execute_python_lab(&input, CancellationToken::new()).await?;
        assert_eq!(execution.status, LearningLabRunStatus::TimedOut);

        let mut input = spec("print('x' * 10000)\n", "import solution\n");
        input.limits.output_bytes = 4_096;
        let execution = execute_python_lab(&input, CancellationToken::new()).await?;
        assert!(execution.output_truncated);
        assert!(execution.stdout.len() + execution.stderr.len() <= 4_096);

        let token = CancellationToken::new();
        let cancel = token.clone();
        let task = tokio::spawn(async move {
            let input = spec("while True:\n    pass\n", "import solution\n");
            execute_python_lab(&input, cancel).await
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        token.cancel();
        assert_eq!(
            task.await
                .map_err(|error| AppError::InternalError(error.to_string()))??
                .status,
            LearningLabRunStatus::Cancelled
        );
        Ok(())
    }
}
