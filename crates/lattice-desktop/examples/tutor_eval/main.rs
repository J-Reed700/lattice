//! Tutor evaluation: scripted learners talking to a tutor chat, end to end,
//! against a live llama-server.
//!
//! Run (from `crates/lattice-desktop/`, with the recipe's server up):
//! ```text
//! cargo run --example tutor_eval -- RECIPE_JSON OUT_JSONL [--only ID,ID] [--keep-data]
//! ```
//!
//! What runs is what the desktop runs: `Container::new` on a fresh database and
//! settings file, the embedding model activated through the Settings command,
//! a tutor chat made as the Tutor tab makes it and bound with
//! `bind_conversation`, and each turn through
//! `chat::headless::run_headless_turn`, so `ChatRuntime::turn_source`,
//! `CourseTurn`, the course's tools and the citation checks are the shipped
//! code. Each session gets its own copy of the course, indexed with the real
//! embedding model. The model is reached through the shipped-path eval's
//! recording proxy.
//!
//! The recipe's `corpus` names the course (`evals/tutor/course-v1.json`) and
//! `questions` the sessions. A row names the model the server says it serves.
//! A recipe's server URL is never written to a result.
//!
//! Output: `OUT_JSONL` (one row per session), `OUT_JSONL.calls.jsonl` (every
//! model request verbatim) and `OUT_JSONL.run.json`.
//! `scripts/tutor_eval.py` scores the rows.

#[allow(dead_code)]
#[path = "../shipped_path_eval/app.rs"]
mod app;
mod course;
mod learner;
#[allow(dead_code)]
#[path = "../shipped_path_eval/proxy.rs"]
mod proxy;
#[allow(dead_code)]
#[path = "../shipped_path_eval/recipe.rs"]
mod recipe;
#[allow(dead_code)]
#[path = "../shipped_path_eval/record.rs"]
mod record;

use async_trait::async_trait;
use lattice::features::conversation::chat::headless::run_headless_turn;
use lattice::features::conversation::chat::{ChatEventSink, ChatResponse, ChatStreamEventDto};
use lattice::features::learning::references::course_index::{CourseIndexes, CourseProgress};
use lattice::features::learning::tutor::{bind_conversation, SetConversationCourseRequestDto};
use lattice::infrastructure::passage_index::EmbedderSource;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::shared::error::AppError;
use learner::Turn;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

pub type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// As the shipped-path eval: a run that outpaces the QA rate limiter is waiting
/// on a desktop guard, not measuring anything.
const RATE_LIMIT_PAUSE: Duration = Duration::from_secs(7);
const RATE_LIMIT_RETRIES: usize = 12;

/// How much of a tool's result a row keeps; the calls file has all of it.
const TOOL_RESULT_CHARS: usize = 600;

struct Args {
    recipe: PathBuf,
    out: PathBuf,
    only: Option<Vec<String>>,
    keep_data: bool,
}

fn parse_args() -> Res<Args> {
    let mut positional = Vec::new();
    let mut only = None;
    let mut keep_data = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--only" => {
                let list = args.next().ok_or("--only needs a comma-separated list")?;
                only = Some(list.split(',').map(str::trim).map(str::to_owned).collect());
            }
            "--keep-data" => keep_data = true,
            _ => positional.push(PathBuf::from(arg)),
        }
    }
    let [recipe, out] = <[PathBuf; 2]>::try_from(positional)
        .map_err(|_| "usage: tutor_eval RECIPE_JSON OUT_JSONL [--only ID,ID] [--keep-data]")?;
    Ok(Args {
        recipe,
        out,
        only,
        keep_data,
    })
}

#[derive(Deserialize)]
struct Sessions {
    sessions: Vec<Session>,
}

#[derive(Deserialize)]
struct Session {
    id: String,
    #[serde(default)]
    tags: Vec<String>,
    /// The lesson the chat is opened from, by the course file's key.
    #[serde(default)]
    lesson: Option<String>,
    turns: Vec<Turn>,
    /// What the scorer holds the course to afterwards.
    #[serde(default)]
    expect: Value,
}

fn main() -> Res<()> {
    let args = parse_args()?;
    if args.out.exists() {
        return Err(format!("{} exists; runs never overwrite", args.out.display()).into());
    }
    let real_home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is not set")?;
    let workdir = tempfile::Builder::new()
        .prefix("lattice-tutor-eval-")
        .tempdir()?;
    let home = workdir.path().join("home");
    std::fs::create_dir_all(&home)?;
    // Before any thread exists. It covers Unix only: on Windows `dirs` reads the
    // profile folder, so `~/.lattice` is the real library here too. What keeps a
    // run out of it is that a run imports nothing, deletes no conversation (that
    // sweeps web snapshots against this fresh database) and has no vault.
    std::env::set_var("HOME", &home);
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(8 * 1024 * 1024)
        .build()?;
    let result = runtime.block_on(run(&args, &real_home, workdir.path()));
    if args.keep_data {
        let kept = workdir.keep();
        eprintln!("kept the run's data directory at {}", kept.display());
    }
    result
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
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

/// The one model the server says it is serving: what a row is measured on,
/// asked of the server, as `scripts/replay_model.py` asks it.
async fn served_model(upstream: &str, auth: Option<&(String, String)>) -> Res<String> {
    let url = format!("{}/v1/models", upstream.trim_end_matches('/'));
    let mut request = reqwest::Client::new().get(url);
    if let Some((name, value)) = auth {
        request = request.header(name.as_str(), value.as_str());
    }
    let reply: Value = request.send().await?.error_for_status()?.json().await?;
    let mut models: Vec<&str> = reply
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| model.get("id").and_then(Value::as_str))
        .collect();
    match (models.pop(), models.is_empty()) {
        (Some(model), true) => Ok(model.to_owned()),
        _ => Err("the server does not name exactly one model".into()),
    }
}

/// The window and slots the recipe expects, as the server reports them.
async fn probe_server(server: &recipe::Server, auth: Option<&(String, String)>) -> Res<Value> {
    let url = format!("{}/props", server.url.trim_end_matches('/'));
    let mut probe = reqwest::Client::new().get(url);
    if let Some((name, value)) = auth {
        probe = probe.header(name.as_str(), value.as_str());
    }
    let props: Value = probe.send().await?.error_for_status()?.json().await?;
    let n_ctx = props
        .pointer("/default_generation_settings/n_ctx")
        .or_else(|| props.get("n_ctx"))
        .and_then(Value::as_u64);
    let slots = props.get("total_slots").and_then(Value::as_u64);
    if n_ctx != Some(server.expected_ctx) || slots != Some(server.expected_slots) {
        return Err(format!(
            "server reports n_ctx {n_ctx:?} and {slots:?} slots; the recipe expects {} and {}",
            server.expected_ctx, server.expected_slots
        )
        .into());
    }
    Ok(json!({
        "n_ctx": n_ctx,
        "total_slots": slots,
        "model_path": props.get("model_path"),
        "build_info": props.get("build_info"),
    }))
}

/// The container's embedding model, handed to the course indexes as the app's
/// own source hands it.
struct ActiveEmbedding(Arc<dyn EmbeddingPort>);

#[async_trait]
impl EmbedderSource for ActiveEmbedding {
    async fn embedder(&self) -> Option<Arc<dyn EmbeddingPort>> {
        Some(Arc::clone(&self.0))
    }
}

struct Quiet;
impl CourseProgress for Quiet {}

/// Everything a session needs from the run.
struct Bench<'a> {
    container: &'a lattice::Container,
    courses: Arc<CourseIndexes>,
    recipe: &'a recipe::Recipe,
    course: &'a course::Course,
    tool_preferences: &'a lattice::features::conversation::chat::ToolPreferences,
    recorder: &'a Arc<proxy::Recorder>,
    upstream: &'a str,
    auth: Option<&'a (String, String)>,
    /// The name requests go out under.
    requested_model: &'a str,
    /// The name the server gives the model it serves.
    served_model: &'a str,
}

async fn run(args: &Args, real_home: &Path, workdir: &Path) -> Res<()> {
    let root = repo_root();
    let recipe = recipe::Recipe::load(&args.recipe)?;
    let course_path = recipe::resolve(&recipe.corpus, &root, real_home);
    let sessions_path = recipe::resolve(&recipe.questions, &root, real_home);
    let embedding_gguf = recipe::resolve(&recipe.embedding_gguf, &root, real_home);
    let course = course::Course::load(&course_path)?;
    let mut sessions: Sessions = serde_json::from_str(&std::fs::read_to_string(&sessions_path)?)?;
    if let Some(only) = &args.only {
        sessions
            .sessions
            .retain(|session| only.contains(&session.id));
        if sessions.sessions.len() != only.len() {
            return Err("--only names a session the file does not have".into());
        }
    }

    let recipe::Answerer::Server(server) = recipe.answerer()? else {
        return Err("the tutor eval runs on a llama-server".into());
    };
    let auth = proxy::server_auth();
    let served = served_model(&server.url, auth.as_ref()).await?;
    let server_report = probe_server(server, auth.as_ref()).await?;
    eprintln!("the server serves {served}");

    let recorder = proxy::Recorder::new();
    let proxy_url = proxy::start(
        server.url.clone(),
        recipe.thinking_off()?,
        auth.clone(),
        Arc::clone(&recorder),
    )
    .await?;
    let settings = recipe.settings(&proxy_url)?;
    if settings.vault.enabled {
        return Err("a run keeps out of the real vault: leave vault.enabled off".into());
    }
    let tool_preferences = recipe.tool_preferences()?;

    let data_dir = workdir.join("app-data");
    std::fs::create_dir_all(&data_dir)?;
    let container = app::open(&data_dir, &settings, None).await?;
    eprintln!("activating {}", embedding_gguf.display());
    let embedding =
        app::activate_embedding(&container, &embedding_gguf, &recipe.embedding_model_id).await?;
    let embedder = container.get_or_load_embedding().await?;
    let container = container.with_passage_indexes(Arc::new(ActiveEmbedding(embedder)));
    let courses = container
        .course_indexes()
        .cloned()
        .ok_or("the container has no course indexes")?;
    courses.register().await?;

    let bench = Bench {
        container: &container,
        courses,
        recipe: &recipe,
        course: &course,
        tool_preferences: &tool_preferences,
        recorder: &recorder,
        upstream: &server.url,
        auth: auth.as_ref(),
        requested_model: &server.model,
        served_model: &served,
    };

    let mut out = std::fs::File::create(&args.out)?;
    let mut calls_out = std::fs::File::create(with_suffix(&args.out, ".calls.jsonl"))?;
    let started = Instant::now();
    let started_at = chrono::Utc::now().to_rfc3339();
    let mut failed = Vec::new();
    for (position, session) in sessions.sessions.iter().enumerate() {
        let calls_from = recorder.len();
        let (row, error) = run_session(&bench, session).await?;
        if let Some(error) = error {
            failed.push(json!({"query_id": session.id, "error": error}));
        }
        writeln!(out, "{}", serde_json::to_string(&row)?)?;
        out.flush()?;
        for call in recorder.since(calls_from) {
            writeln!(
                calls_out,
                "{}",
                serde_json::to_string(&json!({"query_id": session.id, "call": call}))?
            )?;
        }
        calls_out.flush()?;
        eprintln!(
            "[{}/{}] {} {} ms",
            position + 1,
            sessions.sessions.len(),
            session.id,
            row.get("latency_ms")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        );
    }

    // The server the run asked may have changed under it.
    let served_at_end = served_model(&server.url, auth.as_ref()).await?;
    if served_at_end != served {
        return Err(format!("the server changed model mid-run: {served_at_end}").into());
    }
    let mut recipe_kept: Value = serde_json::from_str(&std::fs::read_to_string(&args.recipe)?)?;
    if let Some(url) = recipe_kept.pointer_mut("/server/url") {
        *url = json!("(not kept)");
    }
    let run = json!({
        "schema": "lattice-tutor-eval/1",
        "name": recipe.name,
        "model": served,
        "requested_model": server.model,
        "recipe": recipe_kept,
        "server": server_report,
        "thinking": server.thinking,
        "commit": git(&["rev-parse", "HEAD"]),
        "dirty_files": git(&["status", "--porcelain"]).map(|status| status.lines().count()),
        "embedding": embedding,
        "sessions": sessions.sessions.len(),
        "only": args.only,
        "failed": failed,
        "started_at": started_at,
        "finished_at": chrono::Utc::now().to_rfc3339(),
        "run_ms": started.elapsed().as_millis(),
    });
    std::fs::write(
        with_suffix(&args.out, ".run.json"),
        serde_json::to_string_pretty(&run)? + "\n",
    )?;
    Ok(())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

async fn run_session(bench: &Bench<'_>, session: &Session) -> Res<(Value, Option<String>)> {
    let pool = bench.container.db_pool();
    let seeded = course::seed(pool, bench.course).await?;
    let mode = bench
        .courses
        .ensure_current(&seeded.program_id, &CancellationToken::new(), &mut Quiet)
        .await?;
    let conversation = lattice::features::conversation::commands::create_conversation_impl(
        bench.container,
        format!("{} {}", bench.course.title, session.id),
        None,
    )
    .await?
    .id
    .to_string();
    let lesson_id = match &session.lesson {
        Some(key) => Some(seeded.lessons.get(key).ok_or("an unknown lesson")?.clone()),
        None => None,
    };
    bind_conversation(
        pool,
        &SetConversationCourseRequestDto {
            conversation_id: conversation.clone(),
            program_id: seeded.program_id.clone(),
            lesson_id,
        },
    )
    .await?;

    let course_before = course::read_back(pool, &seeded).await?;
    let mut turns = Vec::new();
    let mut session_error = None;
    let mut tutor_said = String::new();
    for (index, turn) in session.turns.iter().enumerate() {
        let (ask, played) = match turn {
            Turn::Say { say } => (say.clone(), false),
            Turn::Play {
                play,
                misconception,
            } => (
                learner::played(
                    bench.upstream,
                    bench.auth,
                    bench.requested_model,
                    bench.course,
                    &tutor_said,
                    *play,
                    misconception.as_deref(),
                )
                .await?,
                true,
            ),
        };
        let (row, response, error) = run_turn(bench, &conversation, index, &ask).await;
        let mut row = row;
        if let Some(row) = row.as_object_mut() {
            row.insert("played".into(), json!(played));
            row.insert(
                "course_after".into(),
                course::read_back(pool, &seeded).await?,
            );
        }
        turns.push(row);
        if let Some(response) = response {
            tutor_said = response.message;
        }
        if error.is_some() {
            session_error = error;
            break;
        }
    }
    let mut row = record::item(
        &session.id,
        &session.tags,
        turns,
        bench.served_model,
        session_error.clone(),
    );
    if let Some(row) = row.as_object_mut() {
        row.insert("lesson".into(), json!(session.lesson));
        row.insert("expect".into(), session.expect.clone());
        row.insert("index_mode".into(), json!(mode.label()));
        row.insert("course_before".into(), course_before);
    }
    Ok((row, session_error))
}

/// The tool calls of a turn and what came back, in the order they ran, read
/// from the requests themselves so a call the model wrote into its text counts
/// too.
fn tool_trace(calls: &[proxy::CallRecord]) -> Value {
    let mut asked: Vec<Value> = Vec::new();
    let mut results = Map::new();
    let mut note = |tool_call: &Value, name: &str, arguments: &Value| {
        let id = tool_call.get("id").and_then(Value::as_str);
        if !asked
            .iter()
            .any(|seen| seen.get("id").and_then(Value::as_str) == id)
        {
            asked.push(json!({"id": id, "name": name, "arguments": arguments}));
        }
    };
    for call in calls {
        let messages = call
            .request
            .as_ref()
            .and_then(|request| request.get("messages"))
            .and_then(Value::as_array);
        for message in messages.into_iter().flatten() {
            if message.get("role").and_then(Value::as_str) == Some("tool") {
                let id = message.get("tool_call_id").and_then(Value::as_str);
                let text = message.get("content").and_then(Value::as_str);
                if let (Some(id), Some(text)) = (id, text) {
                    let kept: String = text.chars().take(TOOL_RESULT_CHARS).collect();
                    results.insert(id.to_owned(), json!(kept));
                }
            }
            for tool_call in message
                .get("tool_calls")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let function = tool_call.get("function");
                let name = function
                    .and_then(|function| function.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let arguments = function
                    .and_then(|function| function.get("arguments"))
                    .unwrap_or(&Value::Null);
                note(tool_call, name, arguments);
            }
        }
        for tool_call in call
            .reply
            .as_ref()
            .map(|reply| reply.tool_calls.as_slice())
            .unwrap_or_default()
        {
            let name = tool_call
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let arguments = tool_call.get("arguments").unwrap_or(&Value::Null);
            note(tool_call, name, arguments);
        }
    }
    for tool_call in &mut asked {
        let result = tool_call
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| results.get(id))
            .cloned();
        if let Some(tool_call) = tool_call.as_object_mut() {
            tool_call.insert("result".into(), result.unwrap_or(Value::Null));
        }
    }
    Value::Array(asked)
}

/// The fronts of the due cards the turn's facts showed the model.
fn cards_shown(calls: &[proxy::CallRecord]) -> Vec<String> {
    let message = calls
        .iter()
        .filter_map(|call| call.request.as_ref()?.get("messages")?.as_array()?.last())
        .find_map(|message| message.get("content")?.as_str());
    let mut lines = message.unwrap_or_default().lines();
    let mut fronts = Vec::new();
    while let Some(line) = lines.next() {
        if line.starts_with("- card_id: ") {
            fronts.extend(lines.next().map(|front| front.trim().to_owned()));
        }
    }
    fronts
}

async fn run_turn(
    bench: &Bench<'_>,
    conversation_id: &str,
    index: usize,
    ask: &str,
) -> (Value, Option<ChatResponse>, Option<String>) {
    let calls_from = bench.recorder.len();
    let events: Arc<Mutex<Vec<Value>>> = Arc::default();
    let sink_events = Arc::clone(&events);
    let sink: ChatEventSink = Arc::new(move |event: ChatStreamEventDto| {
        sink_events
            .lock()
            .push(serde_json::to_value(&event).unwrap_or(Value::Null));
        Ok(())
    });
    let started = Instant::now();
    let mut retries = 0;
    let result = loop {
        let attempt = run_headless_turn(
            bench.container,
            Some(conversation_id.to_owned()),
            ask.to_owned(),
            bench.tool_preferences.clone(),
            Arc::clone(&sink),
        )
        .await;
        match attempt {
            Err(AppError::RateLimitExceeded(_)) if retries < RATE_LIMIT_RETRIES => {
                retries += 1;
                tokio::time::sleep(RATE_LIMIT_PAUSE).await;
            }
            other => break other,
        }
    };
    let answered_ms = elapsed_ms(started);
    let (response, error) = match result {
        Ok(response) => (Some(response), None),
        Err(error) => (None, Some(error.to_string())),
    };
    let mut verification = None;
    let mut verified_turn = None;
    let mut verification_wait_ms = None;
    if response.is_some() {
        let waited = Instant::now();
        let deadline = waited + Duration::from_secs(bench.recipe.wait.verification_secs);
        while Instant::now() < deadline {
            let found = events
                .lock()
                .iter()
                .find_map(|event| event.get("verification").filter(|v| !v.is_null()).cloned());
            if let Some(found) = found {
                verification = found.get("verification").cloned();
                verified_turn = found.get("turn").cloned();
                verification_wait_ms = Some(elapsed_ms(waited));
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    let went_quiet = bench
        .recorder
        .wait_quiet(
            Duration::from_millis(bench.recipe.wait.quiet_ms),
            Duration::from_secs(bench.recipe.wait.quiet_cap_secs),
        )
        .await;
    let calls = bench.recorder.since(calls_from);
    let events_snapshot = events.lock().clone();
    let assistant_metadata = response
        .as_ref()
        .and_then(|r| r.messages.iter().rev().find(|m| m.role == "assistant"))
        .and_then(|m| m.metadata.as_deref())
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or(Value::Null);
    let outcome = record::TurnOutcome {
        index,
        ask,
        conversation_id: Some(conversation_id),
        answer: response.as_ref().map(|r| r.message.as_str()),
        error: error.clone(),
        answered_ms,
        verification_wait_ms,
        went_quiet,
        sources: response
            .as_ref()
            .and_then(|r| serde_json::to_value(&r.sources).ok())
            .unwrap_or(Value::Null),
        assistant_metadata,
        verification,
        verified_turn_record: verified_turn,
        flow_timing: response
            .as_ref()
            .and_then(|r| serde_json::to_value(&r.timing_metrics).ok())
            .unwrap_or(Value::Null),
        events: &events_snapshot,
        calls: &calls,
        rate_limit_retries: retries,
    };
    let mut row = record::turn(&outcome, &HashMap::new());
    if let Some(row) = row.as_object_mut() {
        row.insert("tool_calls".into(), tool_trace(&calls));
        row.insert("cards_shown".into(), json!(cards_shown(&calls)));
    }
    (row, response, error)
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
