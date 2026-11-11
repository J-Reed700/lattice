//! The live measurement harnesses of the coding sessions (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md
//! section 9; evals/coding/README.md): problem builds, hints, and counterpart, debrief and latency. This module is what
//! they share: the output folders, the recording model, and a program in a test database with the app's own session
//! service over it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{
    CacheHold, CompletionRequest, CompletionResponse, Pace, RequestLoad,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::models::vision::VisionGeometry;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::background::BackgroundTasks;
use lattice_learning::drills::runner::{runner_for, CodeRunner};
use lattice_learning::embedded_runtime::BuiltinRuntimes;
use lattice_learning::practical_dto::LearningPracticalRunDto;
use lattice_learning::practical_repository::LearningPracticalRepository;
use lattice_learning::practical_runs::{start_coding_run, CodingRunStart, PracticalRunRegistry};
use lattice_learning::practice::coding::harness::{harness_for, CaseHarness};
use lattice_learning::practice::coding::jobs::{save_problem, ProblemSave};
use lattice_learning::practice::coding::problem::Problem;
use lattice_learning::practice::coding::session::CodingRuns;
use lattice_learning::practice::test_support::fixture;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::features::learning::tests::grading::live_grading_eval::model;

mod builds;
mod hints;
mod sessions;

/// The output folder of `eval` under $LATTICE_CODING_EVAL_OUT, created.
pub(super) fn out_dir(eval: &str) -> Result<PathBuf> {
    let root = std::env::var("LATTICE_CODING_EVAL_OUT").map_err(|_| {
        AppError::InvalidConfig(
            "Set LATTICE_CODING_EVAL_OUT to the folder the evals write to.".into(),
        )
    })?;
    let dir = PathBuf::from(root).join(eval);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// A count from `name`, or `default` (the design's number) when unset.
pub(super) fn count(name: &str, default: usize) -> Result<usize> {
    match std::env::var(name) {
        Ok(value) => value
            .trim()
            .parse()
            .map_err(|_| AppError::InvalidConfig(format!("{name} is not a number: {value}"))),
        Err(_) => Ok(default),
    }
}

/// The proven problems of a builds run: every `<dir>/<name>/problem.json` under $LATTICE_CODING_EVAL_PROBLEMS, in
/// folder-name order, with the folder name.
pub(super) fn problems() -> Result<Vec<(String, Problem)>> {
    let root = std::env::var("LATTICE_CODING_EVAL_PROBLEMS").map_err(|_| {
        AppError::InvalidConfig(
            "Set LATTICE_CODING_EVAL_PROBLEMS to the builds folder of a run of the builds eval."
                .into(),
        )
    })?;
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if entry.path().join("problem.json").is_file() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    let mut found = Vec::new();
    for name in names {
        let text = std::fs::read_to_string(Path::new(&root).join(&name).join("problem.json"))?;
        found.push((name, serde_json::from_str(&text)?));
    }
    Ok(found)
}

/// Writes `<dir>/<name>.json` (pretty) and `<dir>/<name>.md`.
pub(super) fn write_item(
    dir: &Path,
    name: &str,
    item: &impl Serialize,
    markdown: &str,
) -> Result<()> {
    std::fs::write(
        dir.join(format!("{name}.json")),
        serde_json::to_string_pretty(item)?,
    )?;
    std::fs::write(dir.join(format!("{name}.md")), markdown)?;
    Ok(())
}

/// Nearest-rank p50 and p90 of a list of milliseconds, and how many there were.
#[derive(Debug, Clone, Copy, Serialize)]
pub(super) struct Percentiles {
    pub count: usize,
    pub p50: Option<u64>,
    pub p90: Option<u64>,
}

/// Nearest-rank p50 and p90 of `ms`, and how many there were.
pub(super) fn percentiles(ms: &[u64]) -> Percentiles {
    let mut sorted = ms.to_vec();
    sorted.sort_unstable();
    let rank = |percent: usize| {
        let position = (sorted.len() * percent).div_ceil(100).max(1);
        sorted.get(position - 1).copied()
    };
    Percentiles {
        count: sorted.len(),
        p50: rank(50),
        p90: rank(90),
    }
}

/// `text` in a fenced code block that opens with `info`; the fence is one backtick longer than any run in `text`.
pub(super) fn fenced(info: &str, text: &str) -> String {
    let mut fence = "```".to_owned();
    while text.contains(&fence) {
        fence.push('`');
    }
    format!("{fence}{info}\n{}\n{fence}\n", text.trim_end_matches('\n'))
}

/// `code` with right-aligned line numbers, for Markdown.
pub(super) fn numbered(code: &str) -> String {
    let width = code.lines().count().to_string().len();
    code.lines()
        .enumerate()
        .map(|(position, line)| format!("{:>width$} | {line}", position + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One model request: how long it took, and whether it answered.
#[derive(Debug, Clone, Serialize)]
pub(super) struct ModelCall {
    pub ms: u64,
    pub ok: bool,
}

/// The eval's model: delegates every `LLMPort` method to the configured model, and records each completion.
pub(super) struct Recording {
    inner: Arc<dyn LLMPort>,
    calls: Mutex<Vec<ModelCall>>,
}

impl Recording {
    /// Wraps `inner`.
    pub(super) fn new(inner: Arc<dyn LLMPort>) -> Self {
        Self {
            inner,
            calls: Mutex::new(Vec::new()),
        }
    }

    /// The calls since the last `take`.
    pub(super) fn take(&self) -> Vec<ModelCall> {
        std::mem::take(&mut *self.calls.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn record(&self, started: Instant, ok: bool) {
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(ModelCall { ms, ok });
    }
}

#[async_trait]
impl LLMPort for Recording {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let started = Instant::now();
        let answer = self.inner.complete(request).await;
        self.record(started, answer.is_ok());
        answer
    }

    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let started = Instant::now();
        let answer = self.inner.complete_with_progress(request, on_text).await;
        self.record(started, answer.is_ok());
        answer
    }

    async fn complete_with_retry_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let started = Instant::now();
        let answer = self
            .inner
            .complete_with_retry_progress(request, on_text, on_retry)
            .await;
        self.record(started, answer.is_ok());
        answer
    }

    async fn complete_with_reasoning_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let started = Instant::now();
        let answer = self
            .inner
            .complete_with_reasoning_progress(request, on_text, on_reasoning, on_retry)
            .await;
        self.record(started, answer.is_ok());
        answer
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }

    fn chars_per_token(&self) -> f64 {
        self.inner.chars_per_token()
    }

    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }

    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        self.inner.count_tokens_exact(text).await
    }

    fn counts_tokens_exactly(&self) -> bool {
        self.inner.counts_tokens_exactly()
    }

    async fn is_ready(&self) -> Result<bool> {
        self.inner.is_ready().await
    }

    fn supports_tool_calling(&self) -> bool {
        self.inner.supports_tool_calling()
    }

    fn vision(&self) -> Option<VisionGeometry> {
        self.inner.vision()
    }

    fn thinks_before_answering(&self) -> bool {
        self.inner.thinks_before_answering()
    }

    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }

    fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }

    fn pace(&self) -> Option<Pace> {
        self.inner.pace()
    }

    fn requests(&self) -> Option<RequestLoad> {
        self.inner.requests()
    }

    fn hold_cache(&self, key: &str) -> Option<CacheHold> {
        self.inner.hold_cache(key)
    }

    fn keeps_read_prompts(&self) -> bool {
        self.inner.keeps_read_prompts()
    }
}

/// The app's runs: `CodingRuns::start` is `start_coding_run` with the lab's repository, its own run registry and
/// background tasks, and the runtimes.
pub(super) struct EvalRuns {
    repo: LearningPracticalRepository,
    registry: PracticalRunRegistry,
    background: BackgroundTasks,
    runtimes: BuiltinRuntimes,
}

#[async_trait]
impl CodingRuns for EvalRuns {
    async fn start(
        &self,
        start: &CodingRunStart,
        payload_hash: &str,
    ) -> Result<LearningPracticalRunDto> {
        start_coding_run(
            &self.repo,
            &self.registry,
            &self.background,
            &self.runtimes,
            start,
            payload_hash,
        )
        .await
    }
}

/// A program in a test database, with the configured model, the embedded Python runtime, and the app's own session
/// service.
pub(super) struct EvalLab {
    pub pool: SqlitePool,
    pub repo: LearningPracticalRepository,
    pub program_id: String,
    pub lesson_id: String,
    pub llm: Recording,
    pub runner: Box<dyn CodeRunner>,
    pub harness: Box<dyn CaseHarness>,
    pub runs: EvalRuns,
    pub kinds: lattice_learning::practice::session::kind::Kinds,
}

impl EvalLab {
    /// The configured model wrapped in `Recording`, the embedded runtimes, and Python's runner and harness.
    pub(super) async fn new() -> Result<Self> {
        let llm = Recording::new(model().await?);
        let runtimes = super::runtimes();
        let (pool, program_id, lesson_id, _) = fixture().await?;
        let repo = LearningPracticalRepository::new(pool.clone());
        let runner = runner_for("python", &runtimes).ok_or_else(|| {
            AppError::ServiceNotAvailable("No built-in runtime is available to run python.".into())
        })?;
        let harness = harness_for("python")
            .ok_or_else(|| AppError::InvalidInput("No case harness for python.".into()))?;
        let kinds = lattice_learning::practice::session::kind::Kinds::new([
            lattice_learning::practice::kinds::program::kind(std::sync::Arc::new(
                lattice_learning::practice::kinds::program::toolchain::BuiltinToolchains::new(
                    runtimes.clone(),
                ),
            )),
        ])?;
        let runs = EvalRuns {
            repo: repo.clone(),
            registry: PracticalRunRegistry::new(),
            background: BackgroundTasks::default(),
            runtimes,
        };
        Ok(Self {
            pool,
            repo,
            program_id,
            lesson_id,
            llm,
            runner,
            harness,
            runs,
            kinds,
        })
    }

    /// Saves `problem` as a coding activity of the program as the app's build does, and returns the activity id.
    pub(super) async fn activity(&self, problem: &Problem) -> Result<String> {
        let job_id = uuid::Uuid::new_v4().to_string();
        save_problem(
            &lattice_core::shared::persistence::Database::from(self.pool.clone()),
            &ProblemSave {
                job_id: &job_id,
                program_id: &self.program_id,
                lesson_id: &self.lesson_id,
                focus: None,
                language: &problem.spec.language,
                model: self.llm.model_name(),
            },
            problem,
            self.harness.as_ref(),
        )
        .await
    }
}

/// Binds `$sessions`, the app's `CodingSessions` over `$lab`, as the desktop plugin does: `LiveAids`, the debriefer
/// over the lab's model, the lab's runs, and the system clock.
macro_rules! lab_sessions {
    ($sessions:ident, $lab:expr) => {
        let lab = &$lab;
        let aids = lattice_learning::practice::session::live::LiveAids {
            llm: &lab.llm,
            cancel: tokio_util::sync::CancellationToken::new(),
        };
        let own = lattice_learning::practice::coding::session::LiveCodingAids {
            runner: lab.runner.as_ref(),
            cancel: tokio_util::sync::CancellationToken::new(),
        };
        let debriefer = lattice_learning::practice::session::live::LiveDebriefer { llm: &lab.llm };
        let $sessions = lattice_learning::practice::coding::session::CodingSessions {
            repo: &lab.repo,
            kinds: &lab.kinds,
            aids: &aids,
            own: &own,
            runs: &lab.runs,
            debriefer: &debriefer,
            clock: &lattice_learning::practice::coding::session::SystemClock,
        };
    };
}
pub(super) use lab_sessions;
