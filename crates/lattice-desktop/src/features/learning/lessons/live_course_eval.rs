//! The course eval's live harness (docs/design/2026-10-20-course-eval.md
//! §3.4, §6): one topic-only request through outline generation and the
//! selected lessons, on the recipe's model server and a traced web. It resumes
//! from what its run directory holds and records every outcome in the files
//! of §6.2; it never asserts on one.
use super::web_trace::{TraceMode, TracedWeb};
use super::*;
use crate::features::learning::patient_model::PatientModel;
use crate::features::learning::{
    curriculum::{LearningGenerationJob, LearningGenerationJobKind, LearningGenerationJobStatus},
    dto::*,
    outline_draft::{
        DraftRunRegistry, LearningOutlineReviewStatus, RepairLearningOutlineRequestDto,
    },
    outline_progress::OutlineRunRegistry,
    plan_dto::{LearningGenerationJobActionRequestDto, StartLearningGenerationJobRequestDto},
};
use crate::features::web::services::web::WebService;
use lattice_core::application::contracts::settings::{
    LLMProvider, LLMSettingsDto, LlamaCppSettingsDto, ThinkingOverrideDto,
};
use lattice_core::application::ports::WebServiceTrait;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

const RECIPE_SCHEMA: &str = "course-eval-recipe/1";

/// The parts of a course-eval recipe (§4) the harness reads; the whole file
/// is copied into the run directory as written.
#[derive(Debug, Deserialize)]
pub(super) struct CourseRecipe {
    schema: String,
    #[serde(default)]
    lessons: Vec<String>,
    server: RecipeServer,
    /// `{"llm": {...}}`, merged over the shipped LLM settings.
    #[serde(default)]
    settings: Value,
    web: RecipeWeb,
    /// The embedding model's GGUF (with its `tokenizer.json` beside it) that
    /// lessons search each course's evidence index with. Required: without
    /// one a run would not be the shipped path.
    embedding_gguf: Option<String>,
    embedding_model_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecipeServer {
    url: Option<String>,
    url_env: Option<String>,
    /// Names a variable holding `Header-Name: value`, sent only to the server.
    auth_env: Option<String>,
    model: String,
    gguf: Option<String>,
    gguf_sha256: Option<String>,
    model_revision: Option<String>,
    expected_ctx: u32,
    expected_slots: u64,
    thinking: String,
}

#[derive(Debug, Deserialize)]
struct RecipeWeb {
    #[serde(default = "default_web_mode")]
    mode: String,
    traces: String,
}

fn default_web_mode() -> String {
    "replay".into()
}

fn refuse(message: impl std::fmt::Display) -> AppError {
    AppError::InvalidInput(format!("course eval: {message}"))
}

fn db(error: impl std::fmt::Display) -> AppError {
    AppError::Database(error.to_string())
}

impl CourseRecipe {
    pub(super) fn parse(text: &str) -> Result<Self> {
        let recipe: Self = serde_json::from_str(text)?;
        if recipe.schema != RECIPE_SCHEMA {
            return Err(refuse(format!(
                "recipe schema is {:?}, not {RECIPE_SCHEMA:?}",
                recipe.schema
            )));
        }
        let server = &recipe.server;
        if server.url.is_some() == server.url_env.is_some() {
            return Err(refuse("server names exactly one of url and url_env"));
        }
        match (&server.gguf, &server.gguf_sha256, &server.model_revision) {
            (Some(_), Some(_), _) => {}
            (Some(_), None, _) => return Err(refuse("server.gguf needs server.gguf_sha256")),
            (None, _, Some(revision)) if !revision.trim().is_empty() => {}
            (None, _, _) => {
                return Err(refuse(
                    "server.model_revision is required when no server.gguf is named",
                ))
            }
        }
        thinking_off(&server.thinking)?;
        TraceMode::parse(&recipe.web.mode)?;
        for (field, value) in [
            ("embedding_gguf", &recipe.embedding_gguf),
            ("embedding_model_id", &recipe.embedding_model_id),
        ] {
            if value.as_deref().is_none_or(|v| v.trim().is_empty()) {
                return Err(refuse(format!(
                    "the recipe needs {field}: lessons search each course's evidence index with an embedding model, as the app does"
                )));
            }
        }
        for selector in &recipe.lessons {
            parse_selector(selector)?;
        }
        Ok(recipe)
    }

    /// The server URL and auth header, from the recipe or the variables it
    /// names.
    pub(super) fn connection(
        &self,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<(String, Option<(String, String)>)> {
        let read = |name: &str| {
            env(name)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| refuse(format!("{name} is not set")))
        };
        let url = match (&self.server.url, &self.server.url_env) {
            (Some(url), _) => url.clone(),
            (None, Some(name)) => read(name)?,
            (None, None) => return Err(refuse("server names no url")),
        };
        let auth = match &self.server.auth_env {
            None => None,
            Some(name) => {
                let raw = read(name)?;
                let (header, value) = raw
                    .split_once(':')
                    .map(|(header, value)| (header.trim(), value.trim()))
                    .filter(|(header, value)| !header.is_empty() && !value.is_empty())
                    .ok_or_else(|| refuse(format!("{name} must read `Header-Name: value`")))?;
                Some((header.to_owned(), value.to_owned()))
            }
        };
        Ok((url, auth))
    }

    /// The shipped LLM defaults with the harness wiring, then `settings.llm`
    /// merged over them. A field that does not exist is refused rather than
    /// dropped, and `server.thinking` must agree with the merged settings.
    pub(super) fn llm_settings(
        &self,
        url: &str,
        auth: Option<&(String, String)>,
    ) -> Result<LLMSettingsDto> {
        let patch = match &self.settings {
            Value::Null => json!({}),
            Value::Object(fields) => {
                if let Some(other) = fields.keys().find(|key| *key != "llm") {
                    return Err(refuse(format!(
                        "settings.{other} is not read; only settings.llm is"
                    )));
                }
                fields.get("llm").cloned().unwrap_or_else(|| json!({}))
            }
            _ => return Err(refuse("settings must be an object")),
        };
        for owned in ["provider", "llamaCpp"] {
            if patch.get(owned).is_some() {
                return Err(refuse(format!(
                    "settings.llm.{owned} is set from the recipe's server block"
                )));
            }
        }
        let defaults = LLMSettingsDto::default();
        let (header, value) = auth.cloned().unwrap_or_default();
        let wired = LLMSettingsDto {
            provider: LLMProvider::Llamacpp,
            context_window: self.server.expected_ctx,
            llama_cpp: LlamaCppSettingsDto {
                url: url.to_owned(),
                model: self.server.model.clone(),
                auth_header_name: header,
                auth_header_value: value,
            },
            ..defaults
        };
        let mut merged = serde_json::to_value(&wired)?;
        merge(&mut merged, &patch);
        let settings: LLMSettingsDto = serde_json::from_value(merged)?;
        let mut missing = Vec::new();
        unknown_paths(&patch, &serde_json::to_value(&settings)?, "", &mut missing);
        if !missing.is_empty() {
            return Err(refuse(format!(
                "settings.llm names fields that do not exist: {missing:?}"
            )));
        }
        // Learning requests never ask to think, and `LlamaCppLlm::new` gives
        // the model no thinking rule of its own, so only the pin decides.
        let agrees = match (
            thinking_off(&self.server.thinking)?,
            settings.thinking_override,
        ) {
            (true, ThinkingOverrideDto::Off) | (false, ThinkingOverrideDto::On) => true,
            (true, ThinkingOverrideDto::Auto) => !settings.answer_thinking,
            _ => false,
        };
        if !agrees {
            return Err(refuse(format!(
                "server.thinking is {:?} but settings.llm has thinkingOverride {:?} and answerThinking {}",
                self.server.thinking, settings.thinking_override, settings.answer_thinking
            )));
        }
        Ok(settings)
    }
}

fn thinking_off(thinking: &str) -> Result<bool> {
    match thinking {
        "off" => Ok(true),
        "on" => Ok(false),
        other => Err(refuse(format!(
            "server.thinking must be \"off\" or \"on\", not {other:?}"
        ))),
    }
}

/// Objects merge key by key; anything else replaces.
fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(base), Value::Object(patch)) => {
            for (key, value) in patch {
                match base.get_mut(key) {
                    Some(slot) => merge(slot, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, patch) => *base = patch.clone(),
    }
}

fn unknown_paths(patch: &Value, settled: &Value, prefix: &str, out: &mut Vec<String>) {
    let Value::Object(patch) = patch else {
        return;
    };
    for (key, value) in patch {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match settled.get(key) {
            Some(next) => unknown_paths(value, next, &path, out),
            None => out.push(path),
        }
    }
}

/// `m<module>.l<lesson>`, both counted from 1, as zero-based positions.
pub(super) fn parse_selector(raw: &str) -> Result<(usize, usize)> {
    let bad = || {
        refuse(format!(
            "lesson selector {raw:?} must read m<module>.l<lesson>, counting from 1"
        ))
    };
    let number = |part: &str, prefix: char| {
        part.strip_prefix(prefix)
            .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|digits| digits.parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1))
    };
    let (module, lesson) = raw.split_once('.').ok_or_else(bad)?;
    Ok((
        number(module, 'm').ok_or_else(bad)?,
        number(lesson, 'l').ok_or_else(bad)?,
    ))
}

/// A comma-separated selector list; empty means outline only.
pub(super) fn parse_selectors(raw: &str) -> Result<Vec<String>> {
    let mut selected: Vec<String> = Vec::new();
    for selector in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        parse_selector(selector)?;
        if !selected.iter().any(|seen| seen == selector) {
            selected.push(selector.to_owned());
        }
    }
    Ok(selected)
}

/// What the outline stage does with what is on disk (§3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutlineStep {
    /// No program yet: draft from the request.
    Generate,
    /// A draft whose review did not finish: the app's Repair continuation.
    Repair,
    /// A passed draft not yet accepted.
    Accept,
    /// A draft whose finished outline was recorded as not passing.
    Recorded,
    /// Accepted; go on to the lessons.
    Accepted,
}

pub(super) fn outline_step(
    program: Option<(&LearningProgramStatus, Option<&LearningOutlineReviewStatus>)>,
    recorded: bool,
) -> OutlineStep {
    match program {
        None => OutlineStep::Generate,
        Some((LearningProgramStatus::Active, _)) => OutlineStep::Accepted,
        Some((_, Some(LearningOutlineReviewStatus::Passed))) => OutlineStep::Accept,
        Some(_) if recorded => OutlineStep::Recorded,
        Some(_) => OutlineStep::Repair,
    }
}

/// What a lesson stage does with what is on disk (§3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LessonStep {
    Start,
    /// Requeue and dispatch: the runtime redelivers from checkpoints.
    Dispatch,
    /// The UI's Retry, which copies durable work into a new job.
    Retry,
    /// The job has settled; record it.
    Record,
    /// `lesson-<selector>.json` exists.
    Recorded,
}

pub(super) fn lesson_step(
    recorded: bool,
    job: Option<(LearningGenerationJobStatus, Option<&str>)>,
) -> LessonStep {
    use LearningGenerationJobStatus as Status;
    if recorded {
        return LessonStep::Recorded;
    }
    match job {
        None => LessonStep::Start,
        Some((Status::Pending | Status::Running, _)) => LessonStep::Dispatch,
        // An interrupted job is not redelivered; only Retry continues it.
        Some((Status::Interrupted, _)) => LessonStep::Retry,
        Some((Status::Failed, Some(error))) if transient_text(error) => LessonStep::Retry,
        Some(_) => LessonStep::Record,
    }
}

/// Whether a job's error text is the display of an error the job runtime
/// treats as transient (`JobFailure::is_transient`).
pub(super) fn transient_text(error: &str) -> bool {
    let variants: [fn(String) -> AppError; 3] = [
        AppError::Network,
        AppError::ServiceNotAvailable,
        AppError::RateLimitExceeded,
    ];
    variants.iter().any(|variant| {
        let shown = variant("\u{0}".into()).to_string();
        shown
            .split('\u{0}')
            .next()
            .is_some_and(|prefix| !prefix.is_empty() && error.starts_with(prefix))
    })
}

#[path = "live_course_eval_stages.rs"]
mod stages;
use stages::run_stages;

#[cfg(test)]
#[path = "live_course_eval_tests.rs"]
mod tests;

// ---- the live run -------------------------------------------------------

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn file_sha256(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(buffer.get(..read).unwrap_or_default());
    }
    Ok(hex::encode(hasher.finalize()))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// `~/` is the user's home; anything else not absolute is repo-relative.
fn resolve(raw: &str) -> PathBuf {
    match (raw.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ if Path::new(raw).is_absolute() => PathBuf::from(raw),
        _ => repo_root().join(raw),
    }
}

fn git(args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .args(args)
        .current_dir(repo_root())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn read_json(path: &Path) -> Result<Option<Value>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Written whole or not at all, so a resume never reads half a file.
fn write_json(path: &Path, value: &Value) -> Result<()> {
    let partial = path.with_extension("json.tmp");
    std::fs::write(&partial, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&partial, path)?;
    Ok(())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// The server's own account of itself. A server that reports its window
/// or slots must report the recipe's; one that does not is recorded as such.
async fn probe(settings: &LLMSettingsDto, expected_slots: u64) -> Result<Value> {
    let connection = &settings.llama_cpp;
    let base = connection.validated_base_url()?;
    let mut headers = reqwest::header::HeaderMap::new();
    if !connection.auth_header_name.is_empty() {
        headers.insert(
            reqwest::header::HeaderName::from_bytes(connection.auth_header_name.as_bytes())
                .map_err(|e| refuse(e.to_string()))?,
            reqwest::header::HeaderValue::from_str(&connection.auth_header_value)
                .map_err(|e| refuse(e.to_string()))?,
        );
    }
    let network = |e: reqwest::Error| AppError::Network(e.to_string());
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(network)?;
    let models: Value = client
        .get(format!("{base}/models"))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(network)?
        .json()
        .await
        .map_err(network)?;
    let root = base.trim_end_matches("/v1");
    let props: Value = match client.get(format!("{root}/props")).send().await {
        Ok(response) if response.status().is_success() => response.json().await.unwrap_or_default(),
        _ => Value::Null,
    };
    let n_ctx = props
        .pointer("/default_generation_settings/n_ctx")
        .or_else(|| props.get("n_ctx"))
        .and_then(Value::as_u64);
    let slots = props.get("total_slots").and_then(Value::as_u64);
    if n_ctx.is_some_and(|n| n != u64::from(settings.context_window))
        || slots.is_some_and(|s| s != expected_slots)
    {
        return Err(refuse(format!(
            "server reports n_ctx {n_ctx:?} and {slots:?} slots; the recipe expects {} and {expected_slots}",
            settings.context_window
        )));
    }
    let listed: Vec<Value> = models["data"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|model| model["id"].clone())
        .collect();
    Ok(
        json!({"requested_model": connection.model, "models": listed, "n_ctx": n_ctx, "total_slots": slots}),
    )
}

/// The merged settings as run.json records them: no URL, no credentials.
fn recorded_settings(settings: &LLMSettingsDto) -> Result<Value> {
    let mut value = serde_json::to_value(settings)?;
    if let Some(connection) = value.get_mut("llamaCpp").and_then(Value::as_object_mut) {
        for key in ["url", "authHeaderName", "authHeaderValue"] {
            connection.remove(key);
        }
    }
    if let Some(fields) = value.as_object_mut() {
        fields.remove("ollamaAuthHeaderValue");
    }
    Ok(value)
}

/// The recipe's embedding model, served by a llama-server this run starts
/// the way the app's loader does, which names it by its artifact identity.
async fn load_embedder(
    recipe: &CourseRecipe,
) -> Result<Arc<dyn lattice_core::application::ports::EmbeddingPort>> {
    let gguf = resolve(recipe.embedding_gguf.as_deref().unwrap_or_default());
    let model = crate::interfaces::di::modules::embedding::standalone::open_gguf(&gguf).await?;
    Ok(Arc::new(model))
}

struct Run {
    dir: PathBuf,
    model: Arc<ObservedModel>,
    web: Arc<TracedWeb>,
    embedder: Arc<dyn lattice_core::application::ports::EmbeddingPort>,
    metrics: Value,
}

impl Run {
    fn save(&self) -> Result<()> {
        write_json(&self.dir.join("metrics.json"), &self.metrics)?;
        write_json(
            &self.dir.join("web-trace-report.json"),
            &serde_json::to_value(self.web.report())?,
        )
    }

    fn calls(&self) -> usize {
        self.model.call.load(Ordering::SeqCst)
    }
}

#[tokio::test]
#[ignore = "course eval: requires LATTICE_COURSE_RECIPE, LATTICE_COURSE_REQUEST, LATTICE_COURSE_REQUEST_ID and LATTICE_COURSE_RUN; makes real model and web requests"]
async fn live_course_eval() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("lattice::features::learning=info")
        .try_init();
    let env = |name: &str| std::env::var(name).ok();
    let required = |name: &str| env(name).ok_or_else(|| refuse(format!("{name} is not set")));
    let recipe_text = std::fs::read_to_string(required("LATTICE_COURSE_RECIPE")?)?;
    let recipe = CourseRecipe::parse(&recipe_text)?;
    let request_bytes = std::fs::read(required("LATTICE_COURSE_REQUEST")?)?;
    let request: GenerateLearningProgramRequestDto = serde_json::from_slice(&request_bytes)?;
    if !request.document_ids.is_empty() || !request.source_urls.is_empty() {
        return Err(refuse("the course eval covers topic-only requests"));
    }
    let request_id = required("LATTICE_COURSE_REQUEST_ID")?;
    let dir = PathBuf::from(required("LATTICE_COURSE_RUN")?);
    let selectors = match env("LATTICE_COURSE_LESSONS") {
        Some(raw) => parse_selectors(&raw)?,
        None => parse_selectors(&recipe.lessons.join(","))?,
    };
    let mode =
        TraceMode::parse(&env("LATTICE_COURSE_WEB_MODE").unwrap_or(recipe.web.mode.clone()))?;
    // The traces root, one directory per request id under it.
    let trace_dir = resolve(&env("LATTICE_COURSE_WEB_TRACE").unwrap_or(recipe.web.traces.clone()))
        .join(&request_id);

    std::fs::create_dir_all(&dir)?;
    let database = dir.join("course.sqlite");
    // The driver leaves a `skipped` run.json for a missing optional request.
    let previous = read_json(&dir.join("run.json"))?.filter(|p| p.get("skipped").is_none());
    // Never attach to a database this harness did not create.
    if database.exists() && previous.is_none() {
        return Err(refuse(format!(
            "{} has no run.json; not a course-eval run",
            dir.display()
        )));
    }
    let (recipe_sha, request_sha) = (
        sha256_hex(recipe_text.as_bytes()),
        sha256_hex(&request_bytes),
    );
    if let Some(previous) = &previous {
        if previous["recipe_sha256"] != recipe_sha.as_str()
            || previous["request_sha256"] != request_sha.as_str()
        {
            return Err(refuse(
                "this run directory was started with a different recipe or request",
            ));
        }
    }
    let resumed = previous.is_some();
    std::fs::write(dir.join("request.json"), &request_bytes)?;
    std::fs::write(dir.join("recipe.json"), &recipe_text)?;

    let (url, auth) = recipe.connection(&env)?;
    let settings = recipe.llm_settings(&url, auth.as_ref())?;
    let weights = match &recipe.server.gguf {
        Some(path) => {
            let actual = file_sha256(&resolve(path))?;
            if recipe.server.gguf_sha256.as_deref() != Some(actual.as_str()) {
                return Err(refuse(format!(
                    "{path} hashes to {actual}, not the recipe's gguf_sha256"
                )));
            }
            json!({ "gguf_sha256": actual })
        }
        None => json!({ "model_revision": recipe.server.model_revision }),
    };
    let embedder = load_embedder(&recipe).await?;
    let identity = probe(&settings, recipe.server.expected_slots).await?;
    let next_call = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.file_name()
                .to_str()
                .and_then(|name| name.strip_prefix("call-"))
                .and_then(|name| name.strip_suffix(".json"))
                .and_then(|name| name.parse::<usize>().ok())
        })
        .max()
        .map_or(0, |n| n + 1);
    let model = Arc::new(ObservedModel {
        inner: LlamaCppLlm::new(&settings)?,
        directory: dir.clone(),
        call: std::sync::atomic::AtomicUsize::new(next_call),
    });
    let web_cache = dir.join("web");
    std::fs::create_dir_all(&web_cache)?;
    let web = Arc::new(TracedWeb::new(
        Arc::new(WebService::new(&web_cache)?),
        mode,
        &trace_dir,
    )?);
    let mut manifest = json!({
        "schema": "course-eval-run/1", "request_id": request_id,
        "recipe_sha256": recipe_sha, "request_sha256": request_sha,
        "recipe": serde_json::from_str::<Value>(&recipe_text)?,
        "binary_sha256": file_sha256(&std::env::current_exe()?)?,
        "commit": git(&["rev-parse", "HEAD"]),
        "dirty_files": git(&["status", "--porcelain"]).map(|s| s.lines().count()),
        "server": {"identity": identity, "url_sha256": sha256_hex(url.as_bytes()), "expected_ctx": recipe.server.expected_ctx, "thinking": recipe.server.thinking},
        "settings": recorded_settings(&settings)?,
        "embedding": {"model_id": recipe.embedding_model_id, "model_identity": embedder.model_identity(), "dimension": embedder.dimension()},
        "web": {"mode": mode, "trace_dir": trace_dir.display().to_string(), "trace_index_sha256": web.index_sha256(),
                "entries_at_start": previous.as_ref().map_or(json!(web.report().entries_at_start), |p| p["web"]["entries_at_start"].clone())},
        "started_at": previous.as_ref().map_or(json!(chrono::Utc::now().to_rfc3339()), |p| p["started_at"].clone()),
        "finished_at": null, "resumed": resumed, "lessons": selectors, "stages": {},
    });
    merge(&mut manifest["server"], &weights);
    write_json(&dir.join("run.json"), &manifest)?;

    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&database)
        .foreign_keys(true)
        .create_if_missing(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options.clone())
        .await
        .map_err(db)?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(db)?;
    let mut run = Run {
        metrics: read_json(&dir.join("metrics.json"))?.unwrap_or_else(|| json!({})),
        dir,
        model,
        web,
        embedder,
    };
    let (outline, lessons) = run_stages(&mut run, &pool, &options, &request, &selectors).await?;
    let finished = chrono::Utc::now().to_rfc3339();
    write_json(
        &run.dir.join("done.json"),
        &json!({"finished_at": finished, "outline": outline, "lessons": lessons}),
    )?;
    manifest["finished_at"] = json!(finished);
    manifest["stages"] = json!({"outline": outline, "lessons": lessons});
    manifest["web"]["trace_index_sha256"] = json!(run.web.index_sha256());
    manifest["web"]["entries_at_end"] = json!(run.web.report().entries_at_end);
    write_json(&run.dir.join("run.json"), &manifest)?;
    println!("COURSE EVAL DONE: outline {outline}, lessons {lessons}");
    pool.close().await;
    Ok(())
}
