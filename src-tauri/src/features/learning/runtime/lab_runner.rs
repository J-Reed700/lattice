//! The lab runner: a child copy of this executable that hosts the embedded
//! engines, so a learner's code never shares the app's address space, its
//! database pool or its keyring access.
//!
//! The app starts the runner with [`LAB_RUNNER_ARG`] as its only argument;
//! `main.rs` hands that invocation to [`run_child`] before any app state
//! exists. The child starts with an empty environment (only a temporary
//! directory is set) and the run's own scratch directory as its working
//! directory. The two sides speak length-prefixed JSON over the child's
//! stdin and stdout:
//!
//! 1. the child writes [`GREETING`] — anything printed before it is skipped,
//!    and a missing or different greeting is a protocol failure;
//! 2. the app writes one [`RunRequest`] frame and closes stdin;
//! 3. the child answers [`RunEvent::Started`] once the engine is loaded and
//!    the lab's clock runs, then [`RunEvent::Finished`] or
//!    [`RunEvent::Failed`].
//!
//! A frame is a 4-byte big-endian length and that many bytes of JSON. The
//! engines enforce the lab's own limits; the app kills the child when the run
//! is cancelled, when it outlives its time limit plus [`RUN_GRACE`], or when
//! it has not started within [`STARTUP_LIMIT`].

use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
};
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;

use super::embedded_runtime::BuiltinLabExecutionSpec;
use super::lab_runtime::{self, LearningLabExecutionResult, LearningLabRunStatus};
use super::python_runtime::PythonBundle;
use crate::shared::error::{AppError, Result};

/// The argument that makes this executable a lab runner instead of the app.
pub const LAB_RUNNER_ARG: &str = "--lattice-lab-runner";

/// The child's first output; the version changes with the protocol.
const GREETING: &[u8] = b"lattice-lab-runner/1\n";
/// Output tolerated before the greeting: a test harness's banner, say.
const MAX_PREAMBLE_BYTES: usize = 64 * 1024;
/// Lab files are capped at 2 MiB; JSON escaping can grow them several times.
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
/// Loading the engine may first compile the bundled interpreter, which takes
/// far longer on a cold cache than any run should.
const STARTUP_LIMIT: Duration = Duration::from_secs(300);
/// Time past the lab's own limit before the app stops waiting on the child.
/// The engine enforces the limit itself; this catches one that cannot.
const RUN_GRACE: Duration = Duration::from_secs(5);
/// How long a child that has answered may take to exit on its own.
const EXIT_GRACE: Duration = Duration::from_secs(2);
/// The runner's own stderr kept for a failure report.
const DIAGNOSTIC_BYTES: usize = 8 * 1024;
/// Guest stacks: QuickJS and the WASI interpreter run on runtime threads.
const RUNNER_STACK_BYTES: usize = 32 * 1024 * 1024;

/// One run, as the app hands it to the runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunRequest {
    pub spec: BuiltinLabExecutionSpec,
    /// Present for a Python run.
    pub python: Option<PythonBundle>,
}

/// What the runner reports about a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub(crate) enum RunEvent {
    /// The engine is loaded and the lab's clock has started.
    Started,
    /// The guest ran: its output, exit, timing and any limit it hit.
    Finished { result: LearningLabExecutionResult },
    /// The run could not be carried out.
    Failed { message: String },
}

/// Whether this process was started as a lab runner.
pub fn is_runner_invocation() -> bool {
    std::env::args_os()
        .nth(1)
        .is_some_and(|argument| argument == LAB_RUNNER_ARG)
}

/// Serve one run over stdin and stdout; the process's exit code.
pub fn run_child() -> i32 {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(RUNNER_STACK_BYTES)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("The lab runner could not start: {error}");
            return 1;
        }
    };
    match runtime.block_on(serve(tokio::io::stdin(), tokio::io::stdout())) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("The lab runner failed: {error}");
            1
        }
    }
}

/// The runner's side of the protocol: greet, take one request, run it.
pub(crate) async fn serve<R, W>(mut input: R, mut output: W) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    output.write_all(GREETING).await?;
    output.flush().await?;
    let request: RunRequest = read_frame(&mut input).await?;
    let (ready, started) = tokio::sync::oneshot::channel();
    let run = execute_guest(request, ready);
    tokio::pin!(run);
    let outcome = tokio::select! {
        biased;
        announced = started => {
            if announced.is_ok() {
                write_frame(&mut output, &RunEvent::Started).await?;
            }
            run.await
        }
        outcome = &mut run => outcome,
    };
    let event = match outcome {
        Ok(result) => RunEvent::Finished { result },
        Err(error) => RunEvent::Failed {
            message: error.to_string(),
        },
    };
    write_frame(&mut output, &event).await
}

#[cfg(feature = "learning-labs")]
async fn execute_guest(
    request: RunRequest,
    ready: tokio::sync::oneshot::Sender<()>,
) -> Result<LearningLabExecutionResult> {
    // The app stops a run by killing this process, so nothing here cancels.
    super::guest::execute(
        request.spec,
        request.python,
        CancellationToken::new(),
        ready,
    )
    .await
}

#[cfg(not(feature = "learning-labs"))]
async fn execute_guest(
    _request: RunRequest,
    _ready: tokio::sync::oneshot::Sender<()>,
) -> Result<LearningLabExecutionResult> {
    Err(AppError::ServiceNotAvailable(
        "Embedded runtimes are unavailable in this build.".into(),
    ))
}

/// One frame: the length prefix and the JSON.
pub(crate) fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let body = serde_json::to_vec(value)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(AppError::InvalidInput(
            "A lab run is too large to hand to the lab runner.".into(),
        ));
    }
    let length = u32::try_from(body.len())
        .map_err(|_| AppError::InvalidInput("A lab-runner frame is too large.".into()))?;
    let mut frame = Vec::with_capacity(body.len() + 4);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

pub(crate) async fn write_frame<W, T>(writer: &mut W, value: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    writer.write_all(&encode_frame(value)?).await?;
    writer.flush().await?;
    Ok(())
}

pub(crate) async fn read_frame<R, T>(reader: &mut R) -> Result<T>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut prefix = [0u8; 4];
    reader.read_exact(&mut prefix).await?;
    let length = usize::try_from(u32::from_be_bytes(prefix)).unwrap_or(usize::MAX);
    if length > MAX_FRAME_BYTES {
        return Err(AppError::InvalidData(format!(
            "A lab-runner frame claims {length} bytes, over the {MAX_FRAME_BYTES}-byte limit."
        )));
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map_err(|error| AppError::InvalidData(format!("Malformed lab-runner frame: {error}")))
}

/// Skip to the end of the greeting.
pub(crate) async fn read_greeting<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<()> {
    let mut seen = Vec::new();
    loop {
        let read = reader
            .read_until(b'\n', &mut seen)
            .await
            .map_err(AppError::from)?;
        if seen.ends_with(GREETING) {
            return Ok(());
        }
        if read == 0 || seen.len() > MAX_PREAMBLE_BYTES {
            return Err(AppError::InvalidData(
                "The lab runner did not identify itself.".into(),
            ));
        }
    }
}

/// When the app gives up on a child.
#[derive(Debug, Clone, Copy)]
struct Deadlines {
    startup: Duration,
    run: Duration,
}

impl Deadlines {
    fn for_run(spec: &BuiltinLabExecutionSpec) -> Self {
        Self {
            startup: STARTUP_LIMIT,
            run: Duration::from_secs(spec.limits.timeout_seconds) + RUN_GRACE,
        }
    }
}

/// Only one Python run loads the interpreter at a time, so a cold cache is
/// compiled once and every other run waits to load the result.
static PYTHON_WARMUP: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Run `spec` in a fresh runner process, killing it on cancellation or when it
/// outlives its deadlines.
pub(crate) async fn run(
    spec: BuiltinLabExecutionSpec,
    python: Option<PythonBundle>,
    cancellation: CancellationToken,
) -> Result<LearningLabExecutionResult> {
    let warmup = if python.is_some() {
        tokio::select! {
            guard = PYTHON_WARMUP.lock() => Some(guard),
            _ = cancellation.cancelled() => return Ok(super::embedded_runtime::cancelled()),
        }
    } else {
        None
    };
    let scratch = tempfile::tempdir().map_err(|error| AppError::FileSystem(error.to_string()))?;
    let deadlines = Deadlines::for_run(&spec);
    let request = RunRequest { spec, python };
    // raw-spawn: one child per lab run; supervise kills it on cancel, timeout
    // and protocol failure, and kill_on_drop when the caller goes away.
    let mut child = runner_command(scratch.path())?.spawn().map_err(|error| {
        AppError::ServiceNotAvailable(format!("The lab runner could not start: {error}"))
    })?;
    supervise(&mut child, &request, &cancellation, deadlines, warmup).await
}

/// This executable as a runner: no inherited environment, the run's scratch
/// directory as working directory and temporary directory, piped stdio.
fn runner_command(scratch: &Path) -> Result<Command> {
    let executable = tauri::utils::platform::current_exe().map_err(|error| {
        AppError::InternalError(format!("Cannot locate this executable: {error}"))
    })?;
    let mut command = Command::new(executable);
    command
        .args(runner_args())
        .env_clear()
        .envs(runner_environment(scratch))
        .current_dir(scratch)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    Ok(command)
}

#[cfg(not(test))]
fn runner_args() -> Vec<OsString> {
    vec![LAB_RUNNER_ARG.into()]
}

/// The lib test binary cannot take [`LAB_RUNNER_ARG`]; it runs the runner from
/// a test that serves when [`tests::CHILD_MODE`] says so. One test thread:
/// the harness then prints nothing while the test runs (no slow-test notice
/// in the middle of the frames).
#[cfg(test)]
fn runner_args() -> Vec<OsString> {
    [
        "--exact",
        tests::CHILD_TEST,
        "--nocapture",
        "--test-threads=1",
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

/// What the engines need from the environment: somewhere to put temporary
/// files, which is the run's own scratch directory.
fn runner_environment(scratch: &Path) -> Vec<(OsString, OsString)> {
    let scratch = scratch.as_os_str().to_owned();
    let mut environment = vec![(OsString::from("TMPDIR"), scratch.clone())];
    if cfg!(windows) {
        environment.push(("TEMP".into(), scratch.clone()));
        environment.push(("TMP".into(), scratch));
        // Process start-up on Windows needs to find its system libraries.
        if let Some(root) = std::env::var_os("SystemRoot") {
            environment.push(("SystemRoot".into(), root));
        }
    }
    #[cfg(test)]
    environment.push((tests::CHILD_MODE.into(), "serve".into()));
    environment
}

/// How the exchange with a child ended.
enum Outcome {
    Finished(LearningLabExecutionResult),
    Failed(String),
    TimedOut,
    Cancelled,
    Broken(AppError),
}

async fn supervise(
    child: &mut Child,
    request: &RunRequest,
    cancellation: &CancellationToken,
    deadlines: Deadlines,
    mut warmup: Option<tokio::sync::MutexGuard<'static, ()>>,
) -> Result<LearningLabExecutionResult> {
    let started = Instant::now();
    let missing = || AppError::InternalError("The lab runner's pipes are missing.".into());
    let stdin = child.stdin.take().ok_or_else(missing)?;
    let stdout = child.stdout.take().ok_or_else(missing)?;
    let stderr = child.stderr.take().ok_or_else(missing)?;
    let outcome;
    let mut diagnostics = None;
    {
        let exchange = exchange(stdin, stdout, request, deadlines, &mut warmup);
        let drain = lab_runtime::bounded_read(stderr, DIAGNOSTIC_BYTES);
        tokio::pin!(exchange);
        tokio::pin!(drain);
        outcome = loop {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => break Outcome::Cancelled,
                outcome = &mut exchange => break outcome,
                read = &mut drain, if diagnostics.is_none() => diagnostics = Some(read),
            }
        };
        let answered = matches!(outcome, Outcome::Finished(_) | Outcome::Failed(_));
        if !answered
            || tokio::time::timeout(EXIT_GRACE, child.wait())
                .await
                .is_err()
        {
            // Already exited or not, the child must not outlive the run.
            let _ = child.kill().await;
        }
        if diagnostics.is_none() {
            diagnostics = tokio::time::timeout(EXIT_GRACE, drain).await.ok();
        }
    }
    let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    match outcome {
        Outcome::Finished(result) => Ok(result),
        Outcome::Failed(message) => Err(AppError::InternalError(message)),
        Outcome::Cancelled => Ok(LearningLabExecutionResult {
            duration_ms,
            ..super::embedded_runtime::cancelled()
        }),
        Outcome::TimedOut => Ok(LearningLabExecutionResult {
            status: LearningLabRunStatus::TimedOut,
            exit_code: None,
            stdout: String::new(),
            stderr: "Run exceeded its time limit and was stopped.".into(),
            output_truncated: false,
            duration_ms,
        }),
        Outcome::Broken(error) => {
            let detail = diagnostics
                .and_then(|read| read.ok())
                .map(|(bytes, _)| String::from_utf8_lossy(&bytes).trim().to_owned())
                .filter(|text| !text.is_empty())
                .map(|text| format!(" Runner output: {text}"))
                .unwrap_or_default();
            Err(AppError::InternalError(format!(
                "The lab runner failed: {error}.{detail}"
            )))
        }
    }
}

/// Send the request and wait for the child's answer, within its deadlines.
async fn exchange<W, R>(
    mut stdin: W,
    stdout: R,
    request: &RunRequest,
    deadlines: Deadlines,
    warmup: &mut Option<tokio::sync::MutexGuard<'static, ()>>,
) -> Outcome
where
    W: AsyncWrite + Unpin,
    R: AsyncRead + Unpin,
{
    let mut stdout = tokio::io::BufReader::new(stdout);
    let first = tokio::time::timeout(deadlines.startup, async {
        write_frame(&mut stdin, request).await?;
        stdin.shutdown().await?;
        read_greeting(&mut stdout).await?;
        read_frame::<_, RunEvent>(&mut stdout).await
    })
    .await;
    let event = match first {
        Err(_) => return Outcome::TimedOut,
        Ok(Err(error)) => return Outcome::Broken(error),
        Ok(Ok(RunEvent::Started)) => {
            warmup.take();
            match tokio::time::timeout(deadlines.run, read_frame::<_, RunEvent>(&mut stdout)).await
            {
                Err(_) => return Outcome::TimedOut,
                Ok(Err(error)) => return Outcome::Broken(error),
                Ok(Ok(event)) => event,
            }
        }
        Ok(Ok(event)) => event,
    };
    match event {
        RunEvent::Finished { result } => Outcome::Finished(result),
        RunEvent::Failed { message } => Outcome::Failed(message),
        RunEvent::Started => Outcome::Broken(AppError::InvalidData(
            "The lab runner announced its start twice.".into(),
        )),
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::features::learning::embedded_runtime::LearningBuiltinRuntime;
    use crate::features::learning::lab_runtime::{LearningLabFile, LearningLabLimits};

    /// The test that acts as the runner in a child copy of the test binary,
    /// after the crash-hook tests' pattern.
    pub(super) const CHILD_TEST: &str =
        "features::learning::runtime::lab_runner::tests::lab_runner_child";
    /// Set in a child's environment to the role it plays.
    pub(super) const CHILD_MODE: &str = "LATTICE_LAB_RUNNER_TEST_CHILD";

    fn javascript_spec(checks: &str, timeout_seconds: u64) -> BuiltinLabExecutionSpec {
        BuiltinLabExecutionSpec {
            run_id: uuid::Uuid::new_v4().to_string(),
            runtime: LearningBuiltinRuntime::Javascript,
            files: vec![LearningLabFile {
                path: "checks.mjs".into(),
                content: checks.into(),
            }],
            read_only_paths: vec!["checks.mjs".into()],
            limits: LearningLabLimits {
                timeout_seconds,
                ..Default::default()
            },
        }
    }

    fn request(checks: &str) -> RunRequest {
        RunRequest {
            spec: javascript_spec(checks, 5),
            python: None,
        }
    }

    /// Plays the runner when a parent test starts this binary with
    /// [`CHILD_MODE`] set; otherwise there is nothing to do.
    #[test]
    fn lab_runner_child() {
        let Some(mode) = std::env::var_os(CHILD_MODE) else {
            return;
        };
        let code = match mode.to_str() {
            Some("serve") => run_child(),
            Some(other) => {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(misbehave(other));
                0
            }
            None => 2,
        };
        std::process::exit(code);
    }

    /// The child roles that test the app's side of the protocol.
    async fn misbehave(mode: &str) {
        let mut stdout = tokio::io::stdout();
        let mut stdin = tokio::io::stdin();
        stdout.write_all(GREETING).await.unwrap();
        stdout.flush().await.unwrap();
        let _request: RunRequest = read_frame(&mut stdin).await.unwrap();
        match mode {
            // A wedged engine: it starts and never answers.
            "hang" => {
                write_frame(&mut stdout, &RunEvent::Started).await.unwrap();
                tokio::time::sleep(Duration::from_secs(120)).await;
            }
            // Reports what the process was given instead of running anything.
            "environment" => {
                let mut names: Vec<String> = std::env::vars_os()
                    .map(|(name, _)| name.to_string_lossy().into_owned())
                    .collect();
                names.sort();
                let report = serde_json::json!({
                    "environment": names,
                    "directory": std::env::current_dir().unwrap(),
                });
                let result = LearningLabExecutionResult {
                    status: LearningLabRunStatus::Passed,
                    exit_code: Some(0),
                    stdout: report.to_string(),
                    stderr: String::new(),
                    output_truncated: false,
                    duration_ms: 0,
                };
                write_frame(&mut stdout, &RunEvent::Finished { result })
                    .await
                    .unwrap();
            }
            _ => {}
        }
    }

    fn spawn_child(scratch: &Path, mode: &str) -> Child {
        let mut command = runner_command(scratch).unwrap();
        command.env(CHILD_MODE, mode);
        command.spawn().unwrap()
    }

    fn short_deadlines() -> Deadlines {
        Deadlines {
            startup: Duration::from_secs(60),
            run: Duration::from_millis(300),
        }
    }

    #[tokio::test]
    async fn frames_round_trip_a_request_and_every_event() -> Result<()> {
        let request = request("console.log('€ and \"quotes\"')");
        let (mut near, mut far) = tokio::io::duplex(64 * 1024);
        write_frame(&mut near, &request).await?;
        let events = [
            RunEvent::Started,
            RunEvent::Finished {
                result: LearningLabExecutionResult {
                    status: LearningLabRunStatus::TimedOut,
                    exit_code: None,
                    stdout: "partial".into(),
                    stderr: "Python execution timed out.".into(),
                    output_truncated: true,
                    duration_ms: 1200,
                },
            },
            RunEvent::Failed {
                message: "engine missing".into(),
            },
        ];
        for event in &events {
            write_frame(&mut near, event).await?;
        }
        assert_eq!(read_frame::<_, RunRequest>(&mut far).await?, request);
        for event in &events {
            assert_eq!(&read_frame::<_, RunEvent>(&mut far).await?, event);
        }
        Ok(())
    }

    #[tokio::test]
    async fn a_frame_over_the_limit_is_refused_before_it_is_read() {
        let (mut near, mut far) = tokio::io::duplex(1024);
        let claimed = u32::try_from(MAX_FRAME_BYTES + 1).unwrap();
        near.write_all(&claimed.to_be_bytes()).await.unwrap();
        let error = read_frame::<_, RunEvent>(&mut far).await.unwrap_err();
        assert!(error.to_string().contains("over the"), "{error}");
    }

    #[tokio::test]
    async fn the_greeting_is_found_after_unrelated_output_and_required() {
        let mut banner = b"\nrunning 1 test\n".to_vec();
        banner.extend_from_slice(GREETING);
        banner.extend_from_slice(b"rest");
        let mut reader = tokio::io::BufReader::new(banner.as_slice());
        read_greeting(&mut reader).await.unwrap();
        let mut rest = String::new();
        reader.read_to_string(&mut rest).await.unwrap();
        assert_eq!(rest, "rest");

        let mut silent = tokio::io::BufReader::new(&b"some other program\n"[..]);
        assert!(read_greeting(&mut silent).await.is_err());
    }

    #[cfg(feature = "learning-labs")]
    #[tokio::test]
    async fn the_runner_loop_reports_start_then_the_result() -> Result<()> {
        let (mut app, runner_end) = tokio::io::duplex(64 * 1024);
        let (runner_input, runner_output) = tokio::io::split(runner_end);
        let runner = tokio::spawn(serve(runner_input, runner_output));
        write_frame(&mut app, &request("console.log('ran in the runner')")).await?;
        let mut app = tokio::io::BufReader::new(app);
        read_greeting(&mut app).await?;
        assert_eq!(
            read_frame::<_, RunEvent>(&mut app).await?,
            RunEvent::Started
        );
        match read_frame::<_, RunEvent>(&mut app).await? {
            RunEvent::Finished { result } => {
                assert_eq!(result.status, LearningLabRunStatus::Passed);
                assert!(result.stdout.contains("ran in the runner"));
            }
            other => panic!("expected a result, got {other:?}"),
        }
        runner
            .await
            .map_err(|error| AppError::InternalError(error.to_string()))??;
        Ok(())
    }

    #[tokio::test]
    async fn the_runner_loop_reports_a_request_it_cannot_run() -> Result<()> {
        let (mut app, runner_end) = tokio::io::duplex(64 * 1024);
        let (runner_input, runner_output) = tokio::io::split(runner_end);
        let runner = tokio::spawn(serve(runner_input, runner_output));
        let mut unprotected = request("console.log('x')");
        unprotected.spec.read_only_paths.clear();
        write_frame(&mut app, &unprotected).await?;
        let mut app = tokio::io::BufReader::new(app);
        read_greeting(&mut app).await?;
        assert!(matches!(
            read_frame::<_, RunEvent>(&mut app).await?,
            RunEvent::Failed { .. }
        ));
        runner
            .await
            .map_err(|error| AppError::InternalError(error.to_string()))??;
        Ok(())
    }

    #[cfg(feature = "learning-labs")]
    #[tokio::test]
    async fn a_lab_runs_in_a_child_process() -> Result<()> {
        let result = run(
            javascript_spec("console.log('from the child')", 5),
            None,
            CancellationToken::new(),
        )
        .await?;
        assert_eq!(
            result.status,
            LearningLabRunStatus::Passed,
            "{}",
            result.stderr
        );
        assert!(result.stdout.contains("from the child"));
        Ok(())
    }

    #[tokio::test]
    async fn the_child_inherits_no_environment_and_works_in_its_scratch_directory() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let mut child = spawn_child(scratch.path(), "environment");
        let result = supervise(
            &mut child,
            &request(""),
            &CancellationToken::new(),
            short_deadlines(),
            None,
        )
        .await?;
        let report: serde_json::Value = serde_json::from_str(&result.stdout)?;
        let mut allowed = vec!["TMPDIR", CHILD_MODE];
        if cfg!(windows) {
            allowed.extend(["TEMP", "TMP", "SystemRoot"]);
        }
        if cfg!(target_os = "macos") {
            // CoreFoundation sets this in every process that loads it.
            allowed.push("__CF_USER_TEXT_ENCODING");
        }
        for name in report["environment"].as_array().into_iter().flatten() {
            let name = name.as_str().unwrap_or_default();
            assert!(allowed.contains(&name), "inherited {name}");
        }
        assert_eq!(
            std::fs::canonicalize(report["directory"].as_str().unwrap_or_default())?,
            std::fs::canonicalize(scratch.path())?
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_child_that_outlives_its_deadline_is_killed() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let mut child = spawn_child(scratch.path(), "hang");
        let result = supervise(
            &mut child,
            &request(""),
            &CancellationToken::new(),
            short_deadlines(),
            None,
        )
        .await?;
        assert_eq!(result.status, LearningLabRunStatus::TimedOut);
        assert!(child.try_wait()?.is_some(), "the child is still running");
        Ok(())
    }

    #[tokio::test]
    async fn cancelling_a_run_kills_its_child() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let mut child = spawn_child(scratch.path(), "hang");
        let cancellation = CancellationToken::new();
        let cancel = cancellation.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            cancel.cancel();
        });
        let deadlines = Deadlines {
            startup: Duration::from_secs(60),
            run: Duration::from_secs(60),
        };
        let started = Instant::now();
        let result = supervise(&mut child, &request(""), &cancellation, deadlines, None).await?;
        assert_eq!(result.status, LearningLabRunStatus::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(30));
        assert!(child.try_wait()?.is_some(), "the child is still running");
        Ok(())
    }
}
