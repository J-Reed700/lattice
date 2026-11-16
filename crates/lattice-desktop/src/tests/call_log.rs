//! Records the model and web calls of a live run and replays them, so a
//! pipeline fix is checked against an earlier run and only the calls it
//! changed go live.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, ReferencePage, WebSearchInput, WebSearchOutput,
};
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::web_service::WebServiceTrait;
use lattice_core::domain::models::vision::VisionGeometry;
use lattice_core::shared::error::{AppError, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::features::llm::backend::{ModelBackend, Progress};
use crate::features::llm::fitting::{Fitted, OutputLimits, Thinking};

#[cfg(test)]
mod tests;

/// Where a run's calls are written, and where a replay reads them from.
pub(crate) enum CallLog {
    Record(PathBuf),
    Replay {
        from: PathBuf,
        into: PathBuf,
        misses: Misses,
    },
}

/// What a replay does with a call that has no recorded answer left.
#[derive(Clone, Copy)]
pub(crate) enum Misses {
    GoLive,
    Fail,
}

impl CallLog {
    /// Creates the log directory, loads what a replay reads, and opens both journals.
    pub(crate) fn open(self) -> Result<Arc<Calls>> {
        let (dir, from, misses) = match self {
            Self::Record(dir) => (dir, None, Misses::GoLive),
            Self::Replay { from, into, misses } => {
                if from == into {
                    return Err(AppError::InvalidConfig(
                        "A replay cannot write into the log it reads.".into(),
                    ));
                }
                (into, Some(from), misses)
            }
        };
        std::fs::create_dir_all(&dir)?;
        let journal = |file: &str| {
            let source = from.as_ref().map(|from| from.join(file));
            Journal::open(&dir.join(file), source.as_deref(), misses)
        };
        Ok(Arc::new(Calls {
            model: journal("model.jsonl")?,
            web: journal("web.jsonl")?,
            dir,
        }))
    }
}

/// One run's model and web journals.
pub(crate) struct Calls {
    dir: PathBuf,
    model: Journal,
    web: Journal,
}

impl Calls {
    /// Wraps a backend beneath its scheduler, so a replayed answer still reaches the scheduler's estimators.
    pub(crate) fn model(self: &Arc<Self>, backend: Arc<dyn ModelBackend>) -> Arc<dyn ModelBackend> {
        Arc::new(LoggedBackend {
            inner: backend,
            calls: Arc::clone(self),
        })
    }

    /// Wraps `inner` so each network read is recorded, or replayed from the log.
    pub(crate) fn web(
        self: &Arc<Self>,
        inner: Arc<dyn WebServiceTrait>,
    ) -> Arc<dyn WebServiceTrait> {
        Arc::new(LoggedWeb {
            inner,
            calls: Arc::clone(self),
        })
    }

    /// The hits, live calls and misses of each log, also written to `summary.json`.
    pub(crate) fn finish(&self) -> Result<Value> {
        let summary = json!({
            "model": count(&self.dir.join("model.jsonl"))?,
            "web": count(&self.dir.join("web.jsonl"))?,
        });
        std::fs::write(
            self.dir.join("summary.json"),
            serde_json::to_string_pretty(&summary)?,
        )?;
        Ok(summary)
    }
}

fn count(path: &Path) -> Result<Value> {
    let (mut hits, mut live, mut misses) = (0u64, 0u64, 0u64);
    if path.exists() {
        for text in std::fs::read_to_string(path)?.lines() {
            if text.trim().is_empty() {
                continue;
            }
            let line: Value = serde_json::from_str(text)?;
            if line["missed"].as_bool().unwrap_or(false) {
                misses += 1;
            } else if line["live"].as_bool().unwrap_or(true) {
                live += 1;
            } else {
                hits += 1;
            }
        }
    }
    Ok(json!({ "calls": hits + live + misses, "hits": hits, "live": live, "misses": misses }))
}

struct Stored {
    response: Option<Value>,
    error: Option<Value>,
}

impl Stored {
    fn decode<T: DeserializeOwned>(self) -> Result<T> {
        if let Some(error) = self.error {
            return Err(
                serde_json::from_value::<AppError>(error).unwrap_or_else(|e| {
                    AppError::InternalError(format!("A recorded error could not be read: {e}"))
                }),
            );
        }
        let response = self.response.unwrap_or(Value::Null);
        serde_json::from_value(response).map_err(|e| {
            AppError::InternalError(format!("A recorded response could not be read: {e}"))
        })
    }
}

/// One log file, with the recorded answers a replay still has to give.
struct Journal {
    file: Mutex<std::fs::File>,
    queue: Mutex<HashMap<String, VecDeque<Stored>>>,
    misses: Misses,
}

impl Journal {
    fn open(path: &Path, from: Option<&Path>, misses: Misses) -> Result<Self> {
        let queue = match from {
            Some(from) => load(from)?,
            None => HashMap::new(),
        };
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            file: Mutex::new(file),
            queue: Mutex::new(queue),
            misses,
        })
    }

    fn take(&self, key: &str) -> Option<Stored> {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        queue.get_mut(key)?.pop_front()
    }

    fn append(&self, line: &Value) -> Result<()> {
        let mut text = serde_json::to_string(line)?;
        text.push('\n');
        let mut file = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        file.write_all(text.as_bytes())?;
        file.flush()?;
        Ok(())
    }

    /// The next recorded answer for the call's key, logged as a hit; with none left, `None` to go live, or a logged failure in a strict replay.
    fn replay<T: DeserializeOwned + Serialize>(
        &self,
        call: &Call<'_>,
        on_hit: impl FnOnce(&T) -> Result<()>,
    ) -> Option<Result<T>> {
        let started = Instant::now();
        let Some(stored) = self.take(call.key) else {
            return match self.misses {
                Misses::GoLive => None,
                Misses::Fail => Some(self.miss(call, started)),
            };
        };
        let outcome = stored.decode::<T>();
        let logged = match &outcome {
            Ok(response) => on_hit(response),
            Err(_) => Ok(()),
        }
        .and_then(|()| line(call, &outcome, started, false))
        .and_then(|line| self.append(&line));
        Some(logged.and(outcome))
    }

    /// Logs a call a strict replay refused, and fails it; `InvalidConfig` is no transient error, so no caller retries it.
    fn miss<T>(&self, call: &Call<'_>, started: Instant) -> Result<T> {
        let failure = || {
            AppError::InvalidConfig("No recorded answer for this call in a strict replay.".into())
        };
        let mut missed = line(call, &Err::<(), _>(failure()), started, false)?;
        missed["missed"] = Value::Bool(true);
        self.append(&missed)?;
        Err(failure())
    }

    /// Logs a call that went to the real backend.
    fn record<T: Serialize>(
        &self,
        call: &Call<'_>,
        outcome: Result<T>,
        started: Instant,
    ) -> Result<T> {
        self.append(&line(call, &outcome, started, true)?)?;
        outcome
    }
}

fn load(path: &Path) -> Result<HashMap<String, VecDeque<Stored>>> {
    let mut queue: HashMap<String, VecDeque<Stored>> = HashMap::new();
    if !path.exists() {
        return Ok(queue);
    }
    for text in std::fs::read_to_string(path)?.lines() {
        if text.trim().is_empty() {
            continue;
        }
        let mut line: Value = serde_json::from_str(text)?;
        let key = line["key"].as_str().map(str::to_owned).ok_or_else(|| {
            AppError::InvalidConfig(format!("{} has a line with no key.", path.display()))
        })?;
        let take = |line: &mut Value, field: &str| {
            Some(line[field].take()).filter(|value| !value.is_null())
        };
        let stored = Stored {
            response: take(&mut line, "response"),
            error: take(&mut line, "error"),
        };
        if stored.response.is_none() && stored.error.is_none() {
            return Err(AppError::InvalidConfig(format!(
                "{} has a line with neither response nor error.",
                path.display()
            )));
        }
        queue.entry(key).or_default().push_back(stored);
    }
    Ok(queue)
}

/// Keys sorted at every depth, so equal values write the same text.
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut pairs: Vec<(&String, &Value)> = map.iter().collect();
            pairs.sort_by(|a, b| a.0.cmp(b.0));
            Value::Object(
                pairs
                    .into_iter()
                    .map(|(name, field)| (name.clone(), canonical(field)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

fn digest(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

struct Call<'a> {
    key: &'a str,
    method: &'a str,
    model: Option<&'a str>,
    request: &'a Value,
}

fn line<T: Serialize>(
    call: &Call<'_>,
    outcome: &Result<T>,
    started: Instant,
    live: bool,
) -> Result<Value> {
    let (response, error) = match outcome {
        Ok(response) => (serde_json::to_value(response)?, Value::Null),
        Err(error) => (Value::Null, serde_json::to_value(error)?),
    };
    Ok(json!({
        "key": call.key,
        "method": call.method,
        "model": call.model,
        "request": call.request,
        "response": response,
        "error": error,
        "ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "live": live,
    }))
}

/// The request's answer-changing fields and the model's name. Priority and
/// cache key only schedule the call; the contract, time budget, no-time-limit
/// flag and cancel token are never sent to a provider. The reply ceiling and
/// slot the scheduler adds drift with its estimates and are left out.
fn model_key(model: &str, request: &CompletionRequest) -> Result<(String, Value)> {
    let full = serde_json::to_value(request)?;
    let mut sent = full.clone();
    if let Some(map) = sent.as_object_mut() {
        map.remove("priority");
        map.remove("cache_key");
    }
    let key = digest(&canonical(&json!({ "model": model, "request": sent })).to_string());
    Ok((key, full))
}

fn deliver(progress: &Progress<'_>, response: &CompletionResponse) -> Result<()> {
    if let Progress::Streamed {
        on_text,
        on_reasoning,
        ..
    } = progress
    {
        if !response.text.is_empty() {
            on_text(response.text.clone())?;
        }
        if let (Some(on_reasoning), Some(reasoning)) = (
            on_reasoning,
            response.reasoning.as_ref().filter(|text| !text.is_empty()),
        ) {
            on_reasoning(reasoning.clone())?;
        }
    }
    Ok(())
}

struct LoggedBackend {
    inner: Arc<dyn ModelBackend>,
    calls: Arc<Calls>,
}

#[async_trait]
impl ModelBackend for LoggedBackend {
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }

    fn window(&self) -> usize {
        self.inner.window()
    }

    fn thinking(&self, request: &CompletionRequest) -> Thinking {
        self.inner.thinking(request)
    }

    fn output_limits(&self) -> OutputLimits {
        self.inner.output_limits()
    }

    fn supports_tool_calling(&self) -> bool {
        self.inner.supports_tool_calling()
    }

    fn vision(&self) -> Option<VisionGeometry> {
        self.inner.vision()
    }

    fn keeps_read_prompts(&self) -> bool {
        self.inner.keeps_read_prompts()
    }

    fn enforces(
        &self,
        schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        self.inner.enforces(schema)
    }

    fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }

    async fn is_ready(&self) -> Result<bool> {
        self.inner.is_ready().await
    }

    /// A replayed answer reaches the callbacks as a finished call's would.
    async fn send(
        &self,
        fitted: &Fitted<'_>,
        progress: Progress<'_>,
    ) -> Result<CompletionResponse> {
        let model = self.inner.model_name();
        let (key, sent) = model_key(model, fitted.request())?;
        let call = Call {
            key: &key,
            method: "send",
            model: Some(model),
            request: &sent,
        };
        let journal = &self.calls.model;
        if let Some(replayed) = journal.replay(&call, |response| deliver(&progress, response)) {
            return replayed;
        }
        let started = Instant::now();
        let outcome = self.inner.send(fitted, progress).await;
        journal.record(&call, outcome, started)
    }
}

struct LoggedWeb {
    inner: Arc<dyn WebServiceTrait>,
    calls: Arc<Calls>,
}

impl LoggedWeb {
    async fn run<T, F, Fut>(&self, method: &str, args: Value, live: F) -> Result<T>
    where
        T: Serialize + DeserializeOwned + Send,
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<T>> + Send,
    {
        let key = digest(&format!("{method}\n{}", canonical(&args)));
        let call = Call {
            key: &key,
            method,
            model: None,
            request: &args,
        };
        if let Some(replayed) = self.calls.web.replay(&call, |_| Ok(())) {
            return replayed;
        }
        let started = Instant::now();
        let outcome = live().await;
        self.calls.web.record(&call, outcome, started)
    }
}

#[async_trait]
impl WebServiceTrait for LoggedWeb {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        let args = serde_json::to_value(input)?;
        self.run("search_web", args, || self.inner.search_web(input))
            .await
    }

    async fn fetch_url_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.run("fetch_url_content", json!({ "url": url }), || {
            self.inner.fetch_url_content(url)
        })
        .await
    }

    async fn fetch_reference_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.run("fetch_reference_content", json!({ "url": url }), || {
            self.inner.fetch_reference_content(url)
        })
        .await
    }

    async fn fetch_reference_page(&self, url: &str) -> Result<ReferencePage> {
        self.run("fetch_reference_page", json!({ "url": url }), || {
            self.inner.fetch_reference_page(url)
        })
        .await
    }

    async fn fetch_url_content_as_of(
        &self,
        url: &str,
        as_of: Option<chrono::NaiveDate>,
    ) -> Result<FetchUrlContentOutput> {
        let args = json!({ "url": url, "as_of": as_of.map(|day| day.to_string()) });
        self.run("fetch_url_content_as_of", args, || {
            self.inner.fetch_url_content_as_of(url, as_of)
        })
        .await
    }

    /// `Lead` has no serde form and no lesson code follows links: always live, never logged.
    async fn follow_links(
        &self,
        url: &str,
        near_text: Option<&str>,
    ) -> Result<Vec<lattice_core::domain::sourcing::Lead>> {
        self.inner.follow_links(url, near_text).await
    }

    fn validate_url(&self, url: &str) -> Result<()> {
        self.inner.validate_url(url)
    }
}
