//! Shipped-path evaluation: questions asked of the desktop's own chat
//! orchestrator, end to end, against a live llama-server or a cloud model
//! (a recipe's `cloud` block; see `recipe.rs`).
//!
//! Run (from `crates/lattice-desktop/`, with the server from the recipe already up):
//! ```text
//! cargo run --example shipped_path_eval -- RECIPE_JSON OUT_JSONL [--only ID,ID] [--keep-data]
//! ```
//!
//! What runs is what the desktop runs for a chat turn: `Container::new` on a
//! fresh database and settings file, the embedding model activated through
//! the Settings command and served by a llama-server the run starts itself
//! with the app's flags (`LATTICE_LLAMA_SERVER` names the binary, else the
//! bundled build in `binaries/`), the corpus imported through the import use case, and
//! each question sent through `chat::headless::run_headless_turn` — the same
//! `run_turn` the window command calls — so intent, routing, the corpus
//! planner, HyDE, hybrid search, the corrective retry, openings and sections,
//! the tool loop, the answer, the grounding check and memory consolidation are
//! all the shipped code. The model is reached through a recording proxy
//! (`proxy.rs`), which is how the run sees each prompt and the server's
//! cached-versus-evaluated token split.
//!
//! The run's home directory is pointed at a scratch directory before anything
//! starts, because importing copies files into `~/.lattice/files` and a run
//! must never touch the real library.
//!
//! Output: `OUT_JSONL` (one row per item, see `record.rs`),
//! `OUT_JSONL.calls.jsonl` (every model request verbatim) and
//! `OUT_JSONL.run.json` (recipe, hashes, server properties, settings, index
//! statistics). `scripts/shipped_path_eval.py` scores the rows.

mod app;
mod proxy;
mod recipe;
mod record;

use lattice::features::conversation::chat::headless::run_headless_turn;
use lattice::features::conversation::chat::{ChatEventSink, ChatResponse, ChatStreamEventDto};
use lattice_core::shared::error::AppError;
use parking_lot::Mutex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// The QA rate limiter allows ten turns a minute; a run that outpaces it is
/// waiting on a desktop guard, not measuring anything.
const RATE_LIMIT_PAUSE: Duration = Duration::from_secs(7);
const RATE_LIMIT_RETRIES: usize = 12;

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
    let [recipe, out] = <[PathBuf; 2]>::try_from(positional).map_err(|_| {
        "usage: shipped_path_eval RECIPE_JSON OUT_JSONL [--only ID,ID] [--keep-data]"
    })?;
    Ok(Args {
        recipe,
        out,
        only,
        keep_data,
    })
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
        .prefix("lattice-shipped-path-")
        .tempdir()?;
    let home = workdir.path().join("home");
    std::fs::create_dir_all(&home)?;
    // Before any thread exists: the library root is derived from HOME.
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

fn sha256_file(path: &Path) -> Res<String> {
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

fn git(args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .args(args)
        .current_dir(repo_root())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The server must be the one the recipe describes: a different window or
/// slot count changes the turn budget and the cache behaviour under test.
async fn probe_server(server: &recipe::Server) -> Res<Value> {
    let url = server.url.trim_end_matches('/');
    let mut probe = reqwest::Client::new().get(format!("{url}/props"));
    if let Some((name, value)) = proxy::server_auth() {
        probe = probe.header(name, value);
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
        "modalities": props.get("modalities"),
    }))
}

async fn run(args: &Args, real_home: &Path, workdir: &Path) -> Res<()> {
    let root = repo_root();
    let recipe = recipe::Recipe::load(&args.recipe)?;
    let corpus_path = recipe::resolve(&recipe.corpus, &root, real_home);
    let questions_path = recipe::resolve(&recipe.questions, &root, real_home);
    let embedding_gguf = recipe::resolve(&recipe.embedding_gguf, &root, real_home);
    let mut items = recipe::load_items(&questions_path)?;
    if let Some(only) = &args.only {
        items.retain(|item| only.contains(&item.id));
        if items.len() != only.len() {
            return Err("--only names an item the question set does not have".into());
        }
    }

    // What answers, recorded with the run so a result is never read against
    // another model's: local weights by their hash and the server's own
    // report, a cloud model by provider and the id requested (the id each
    // reply was served under is kept per call, in the calls file).
    let (gguf_sha256, server, upstream, base_url_variable) = match recipe.answerer()? {
        recipe::Answerer::Server(server) => {
            let gguf_sha256 = match (&server.gguf, &server.gguf_sha256) {
                (Some(gguf), Some(expected)) => {
                    let gguf = recipe::resolve(gguf, &root, real_home);
                    eprintln!("hashing {}", gguf.display());
                    let gguf_sha256 = sha256_file(&gguf)?;
                    if &gguf_sha256 != expected {
                        return Err(
                            format!("weights hash {gguf_sha256} is not the recipe's").into()
                        );
                    }
                    Some(gguf_sha256)
                }
                (None, None) => None,
                _ => return Err("a recipe gives a server's gguf and gguf_sha256 together".into()),
            };
            (
                gguf_sha256,
                probe_server(server).await?,
                server.url.clone(),
                None,
            )
        }
        recipe::Answerer::Cloud(cloud) => {
            let (upstream, variable) = cloud.upstream()?;
            let server = json!({"provider": cloud.provider, "model": cloud.model});
            (None, server, upstream.to_owned(), Some(variable))
        }
    };

    let recorder = proxy::Recorder::new();
    // A cloud provider's key is the desktop's own; only a llama-server's
    // header is the proxy's to add.
    let auth = recipe.server.as_ref().and_then(|_| proxy::server_auth());
    let proxy_url = proxy::start(
        upstream,
        recipe.thinking_off()?,
        auth,
        Arc::clone(&recorder),
    )
    .await?;
    if let Some(variable) = base_url_variable {
        // Read when the desktop builds its cloud client, so every request of
        // the run crosses the proxy and is recorded.
        std::env::set_var(variable, &proxy_url);
    }
    let settings = recipe.settings(&proxy_url)?;
    let tool_preferences = recipe.tool_preferences()?;

    let data_dir = workdir.join("app-data");
    std::fs::create_dir_all(&data_dir)?;
    let container = app::open(&data_dir, &settings).await?;
    eprintln!("activating {}", embedding_gguf.display());
    let embedding =
        app::activate_embedding(&container, &embedding_gguf, &recipe.embedding_model_id).await?;
    let utility = match &recipe.utility_role {
        Some(role) => Some(app::assign_utility(&container, &role.model_id, &role.served_as).await?),
        None => None,
    };
    eprintln!("importing {}", corpus_path.display());
    let (to_dataset, index_report) = app::import_corpus(
        &container,
        &corpus_path,
        &workdir.join("vault"),
        recipe.index.chunk_tokens,
    )
    .await?;
    eprintln!("indexed: {index_report}");

    let mut out = std::fs::File::create(&args.out)?;
    let mut calls_out = std::fs::File::create(with_suffix(&args.out, ".calls.jsonl"))?;
    let run_started = Instant::now();
    let started_at = chrono::Utc::now().to_rfc3339();
    let mut failed = Vec::new();
    for (position, item) in items.iter().enumerate() {
        let calls_from = recorder.len();
        let (row, error) = run_item(
            &container,
            &recipe,
            &tool_preferences,
            &recorder,
            &to_dataset,
            item,
        )
        .await;
        if let Some(error) = error {
            failed.push(json!({"query_id": item.id, "error": error}));
        }
        writeln!(out, "{}", serde_json::to_string(&row)?)?;
        out.flush()?;
        for call in recorder.since(calls_from) {
            writeln!(
                calls_out,
                "{}",
                serde_json::to_string(&json!({"query_id": item.id, "call": call}))?
            )?;
        }
        calls_out.flush()?;
        eprintln!(
            "[{}/{}] {} {} ms{}",
            position + 1,
            items.len(),
            item.id,
            row.get("latency_ms")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            if row.get("error").is_some_and(|e| !e.is_null()) {
                " ERROR"
            } else {
                ""
            }
        );
    }

    let harness_files = [
        "crates/lattice-desktop/examples/shipped_path_eval/main.rs",
        "crates/lattice-desktop/examples/shipped_path_eval/app.rs",
        "crates/lattice-desktop/examples/shipped_path_eval/proxy.rs",
        "crates/lattice-desktop/examples/shipped_path_eval/recipe.rs",
        "crates/lattice-desktop/examples/shipped_path_eval/record.rs",
        "crates/lattice-desktop/src/features/conversation/chat/headless.rs",
    ];
    let mut harness = serde_json::Map::new();
    for file in harness_files {
        harness.insert(file.into(), json!(sha256_file(&root.join(file))?));
    }
    let run = json!({
        "schema": record::SCHEMA,
        "name": recipe.name,
        "server_launch": recipe.server.as_ref().map(|server| server.launch.clone()),
        "answering_model": {
            "kind": if recipe.cloud.is_some() { "cloud" } else { "llama-server" },
            "model": recipe.model_name()?,
            "served_as": record::served_models(&recorder.since(0)),
        },
        "recipe_path": args.recipe,
        "recipe_sha256": sha256_file(&args.recipe)?,
        "recipe": serde_json::from_str::<Value>(&std::fs::read_to_string(&args.recipe)?)?,
        "corpus_sha256": sha256_file(&corpus_path)?,
        "questions_sha256": sha256_file(&questions_path)?,
        "questions_path": recipe.questions,
        "gguf_sha256": gguf_sha256,
        "server": server,
        "thinking": recipe.server.as_ref().map_or("as-sent", |server| server.thinking.as_str()),
        "only": args.only,
        "commit": git(&["rev-parse", "HEAD"]),
        "dirty_files": git(&["status", "--porcelain"]).map(|s| s.lines().count()),
        "harness_sha256": harness,
        "settings": settings,
        "tool_preferences": tool_preferences,
        "embedding": embedding,
        "utility_role": utility,
        "index": index_report,
        "items": items.len(),
        "failed": failed,
        "started_at": started_at,
        "finished_at": chrono::Utc::now().to_rfc3339(),
        "run_ms": run_started.elapsed().as_millis(),
    });
    std::fs::write(
        with_suffix(&args.out, ".run.json"),
        serde_json::to_string_pretty(&run)? + "\n",
    )?;
    eprintln!(
        "wrote {} ({} failed)",
        args.out.display(),
        run.get("failed")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len)
    );
    Ok(())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// One conversation: each turn in order, each waited out to its grounding
/// check and its background work before the next starts.
async fn run_item(
    container: &lattice::Container,
    recipe: &recipe::Recipe,
    tool_preferences: &lattice::features::conversation::chat::ToolPreferences,
    recorder: &Arc<proxy::Recorder>,
    to_dataset: &HashMap<String, String>,
    item: &recipe::Item,
) -> (Value, Option<String>) {
    let model = recipe.model_name().unwrap_or_default();
    let verify = recipe
        .settings
        .pointer("/llm/verification/enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let mut conversation_id: Option<String> = None;
    let mut turns = Vec::new();
    let mut item_error = None;
    for (index, ask) in item.turns.iter().enumerate() {
        let calls_from = recorder.len();
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
                container,
                conversation_id.clone(),
                ask.clone(),
                tool_preferences.clone(),
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
        if response.is_some() && verify {
            let waited = Instant::now();
            let deadline = waited + Duration::from_secs(recipe.wait.verification_secs);
            while Instant::now() < deadline {
                let found = events
                    .lock()
                    .iter()
                    .find_map(|e| e.get("verification").filter(|v| !v.is_null()).cloned());
                if let Some(found) = found {
                    verification = found.get("verification").cloned();
                    verified_turn = found.get("turn").cloned();
                    verification_wait_ms = Some(elapsed_ms(waited));
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
        let went_quiet = recorder
            .wait_quiet(
                Duration::from_millis(recipe.wait.quiet_ms),
                Duration::from_secs(recipe.wait.quiet_cap_secs),
            )
            .await;
        let calls = recorder.since(calls_from);
        let events_snapshot = events.lock().clone();
        let outcome = outcome_of(
            index,
            ask,
            response.as_ref(),
            error.clone(),
            answered_ms,
            verification_wait_ms,
            went_quiet,
            verification,
            verified_turn,
            &events_snapshot,
            &calls,
            retries,
        );
        turns.push(record::turn(&outcome, to_dataset));
        if let Some(response) = &response {
            conversation_id = Some(response.conversation_id.clone());
        }
        if error.is_some() {
            item_error = error;
            break;
        }
    }
    (
        record::item(&item.id, &item.tags, turns, model, item_error.clone()),
        item_error,
    )
}

#[allow(clippy::too_many_arguments)]
fn outcome_of<'a>(
    index: usize,
    ask: &'a str,
    response: Option<&'a ChatResponse>,
    error: Option<String>,
    answered_ms: u64,
    verification_wait_ms: Option<u64>,
    went_quiet: bool,
    verification: Option<Value>,
    verified_turn_record: Option<Value>,
    events: &'a [Value],
    calls: &'a [proxy::CallRecord],
    rate_limit_retries: usize,
) -> record::TurnOutcome<'a> {
    // The persisted assistant message is the record the desktop reloads.
    let assistant_metadata = response
        .and_then(|r| r.messages.iter().rev().find(|m| m.role == "assistant"))
        .and_then(|m| m.metadata.as_deref())
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or(Value::Null);
    record::TurnOutcome {
        index,
        ask,
        conversation_id: response.map(|r| r.conversation_id.as_str()),
        answer: response.map(|r| r.message.as_str()),
        error,
        answered_ms,
        verification_wait_ms,
        went_quiet,
        sources: response
            .and_then(|r| serde_json::to_value(&r.sources).ok())
            .unwrap_or(Value::Null),
        assistant_metadata,
        verification,
        verified_turn_record,
        flow_timing: response
            .and_then(|r| serde_json::to_value(&r.timing_metrics).ok())
            .unwrap_or(Value::Null),
        events,
        calls,
        rate_limit_retries,
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
