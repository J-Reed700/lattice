//! A web port that records what the real web answered and replays it, so two
//! course-eval runs read the same pages (course eval §2.2). A request the
//! trace does not hold is a miss: `replay` fills it live and counts it,
//! `replay-strict` refuses it, and the report lists every one.
use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, ReferencePage, WebSearchInput, WebSearchOutput,
};
use lattice_core::application::ports::WebServiceTrait;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::JobFailure;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum TraceMode {
    /// No trace is read or written.
    Live,
    /// Serve stored entries; fill a miss live and store it.
    Replay,
    /// Serve stored entries; a miss is an error.
    ReplayStrict,
}

impl TraceMode {
    pub(super) fn parse(raw: &str) -> Result<Self> {
        match raw {
            "live" => Ok(Self::Live),
            "replay" => Ok(Self::Replay),
            "replay-strict" => Ok(Self::ReplayStrict),
            other => Err(AppError::InvalidInput(format!(
                "course eval: web mode must be live, replay or replay-strict, not {other:?}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub(super) struct MethodCounts {
    pub hits: usize,
    pub misses_filled: usize,
    pub misses_failed: usize,
    /// Calls in `live` mode, which no trace sees.
    pub live: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct Missed {
    pub method: String,
    pub summary: String,
}

/// What `web-trace-report.json` holds.
#[derive(Debug, Clone, Serialize)]
pub(super) struct TraceReport {
    pub mode: TraceMode,
    pub trace_dir: String,
    pub entries_at_start: usize,
    pub entries_at_end: usize,
    pub hits: usize,
    pub misses_filled: usize,
    pub misses_failed: usize,
    pub untraced_calls: usize,
    /// Live failures the job runtime retries (network, unavailable, rate
    /// limit). They are passed through unrecorded, so a replay goes live
    /// again rather than repeating an outage.
    pub transient_errors: usize,
    pub by_method: BTreeMap<String, MethodCounts>,
    pub missed: Vec<Missed>,
}

#[derive(Default)]
struct State {
    /// Entry key -> sha256 of its entry file.
    index: BTreeMap<String, String>,
    entries_at_start: usize,
    untraced: usize,
    transient: usize,
    by_method: BTreeMap<String, MethodCounts>,
    missed: Vec<Missed>,
}

pub(super) struct TracedWeb {
    inner: Arc<dyn WebServiceTrait>,
    mode: TraceMode,
    dir: PathBuf,
    state: Mutex<State>,
}

enum Count {
    Hit,
    Filled,
    Failed,
    Live,
}

impl TracedWeb {
    pub(super) fn new(
        inner: Arc<dyn WebServiceTrait>,
        mode: TraceMode,
        dir: &Path,
    ) -> Result<Self> {
        let mut state = State::default();
        if mode != TraceMode::Live {
            std::fs::create_dir_all(dir.join("entries"))?;
            let index = dir.join("index.jsonl");
            if index.exists() {
                for line in std::fs::read_to_string(&index)?.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let row: Value = serde_json::from_str(line)?;
                    let (Some(key), Some(sha)) = (row["key"].as_str(), row["sha256"].as_str())
                    else {
                        return Err(AppError::InvalidInput(format!(
                            "course eval: unreadable trace index line in {}",
                            index.display()
                        )));
                    };
                    state.index.insert(key.to_owned(), sha.to_owned());
                }
            }
            state.entries_at_start = state.index.len();
        }
        Ok(Self {
            inner,
            mode,
            dir: dir.to_path_buf(),
            state: Mutex::new(state),
        })
    }

    pub(super) fn report(&self) -> TraceReport {
        let state = self.lock();
        let sum = |pick: fn(&MethodCounts) -> usize| state.by_method.values().map(pick).sum();
        TraceReport {
            mode: self.mode,
            trace_dir: self.dir.display().to_string(),
            entries_at_start: state.entries_at_start,
            entries_at_end: state.index.len(),
            hits: sum(|c| c.hits),
            misses_filled: sum(|c| c.misses_filled),
            misses_failed: sum(|c| c.misses_failed),
            untraced_calls: state.untraced,
            transient_errors: state.transient,
            by_method: state.by_method.clone(),
            missed: state.missed.clone(),
        }
    }

    /// sha256 over the sorted `key sha256` lines of the index, so two runs
    /// can say whether they replayed the same trace.
    pub(super) fn index_sha256(&self) -> String {
        let state = self.lock();
        let lines: String = state
            .index
            .iter()
            .map(|(key, sha)| format!("{key} {sha}\n"))
            .collect();
        sha256_hex(lines.as_bytes())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn count(&self, method: &str, count: Count, summary: &str) {
        let mut state = self.lock();
        let counts = state.by_method.entry(method.to_owned()).or_default();
        match count {
            Count::Hit => counts.hits += 1,
            Count::Filled => counts.misses_filled += 1,
            Count::Failed => counts.misses_failed += 1,
            Count::Live => counts.live += 1,
        }
        if matches!(count, Count::Filled | Count::Failed) {
            state.missed.push(Missed {
                method: method.to_owned(),
                summary: summary.to_owned(),
            });
        }
    }

    fn entry_path(&self, key: &str) -> PathBuf {
        self.dir.join("entries").join(format!("{key}.json"))
    }

    fn stored(&self, key: &str) -> Result<Option<Value>> {
        if !self.lock().index.contains_key(key) {
            return Ok(None);
        }
        Ok(Some(serde_json::from_slice(&std::fs::read(
            self.entry_path(key),
        )?)?))
    }

    /// Write the entry file atomically, then index it. A concurrent miss for
    /// the same key stores once.
    fn store(&self, method: &str, key: &str, summary: &str, entry: Value) -> Result<()> {
        let mut state = self.lock();
        if state.index.contains_key(key) {
            return Ok(());
        }
        let bytes = serde_json::to_vec_pretty(&entry)?;
        let path = self.entry_path(key);
        let partial = path.with_extension("json.tmp");
        std::fs::write(&partial, &bytes)?;
        std::fs::rename(&partial, &path)?;
        let sha = sha256_hex(&bytes);
        let line = json!({"key":key,"method":method,"summary":summary,"sha256":sha,"bytes":bytes.len(),"recorded_at":chrono::Utc::now().to_rfc3339()});
        let mut index = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("index.jsonl"))?;
        writeln!(index, "{line}")?;
        state.index.insert(key.to_owned(), sha);
        Ok(())
    }

    async fn traced<T: Serialize + DeserializeOwned>(
        &self,
        method: &str,
        input: Value,
        summary: &str,
        live: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        if self.mode == TraceMode::Live {
            self.count(method, Count::Live, summary);
            return live.await;
        }
        let key = trace_key(method, &input);
        if let Some(entry) = self.stored(&key)? {
            self.count(method, Count::Hit, summary);
            return replay(&entry);
        }
        if self.mode == TraceMode::ReplayStrict {
            self.count(method, Count::Failed, summary);
            return Err(AppError::InvalidInput(format!(
                "course eval: no trace entry for {method} {summary}"
            )));
        }
        let result = live.await;
        let entry = match &result {
            Ok(output) => {
                json!({"method":method,"key":key,"input":input,"ok":true,"output":output})
            }
            Err(error) if JobFailure::is_transient(error) => {
                self.lock().transient += 1;
                return result;
            }
            Err(error) => {
                json!({"method":method,"key":key,"input":input,"ok":false,"error":error.to_string(),"variant":error.error_code()})
            }
        };
        self.store(method, &key, summary, entry)?;
        self.count(method, Count::Filled, summary);
        result
    }
}

/// A recorded failure replays as `InvalidInput`, which the job runtime does
/// not retry, so a replay never defers on a page that failed when recorded.
fn replay<T: DeserializeOwned>(entry: &Value) -> Result<T> {
    if entry["ok"].as_bool() == Some(true) {
        return Ok(serde_json::from_value(entry["output"].clone())?);
    }
    Err(AppError::InvalidInput(
        entry["error"].as_str().unwrap_or_default().to_owned(),
    ))
}

/// `sha256(method + "\n" + canonical input)`.
pub(super) fn trace_key(method: &str, input: &Value) -> String {
    sha256_hex(format!("{method}\n{}", canonical(input)).as_bytes())
}

/// Keys sorted and null fields removed, at every depth.
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<&String, Value> = map
                .iter()
                .filter(|(_, field)| !field.is_null())
                .map(|(name, field)| (name, canonical(field)))
                .collect();
            Value::Object(
                sorted
                    .into_iter()
                    .map(|(name, field)| (name.clone(), field))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[async_trait::async_trait]
impl WebServiceTrait for TracedWeb {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        self.traced(
            "search_web",
            serde_json::to_value(input)?,
            &input.query,
            self.inner.search_web(input),
        )
        .await
    }

    async fn fetch_url_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.traced(
            "fetch_url_content",
            json!({ "url": url }),
            url,
            self.inner.fetch_url_content(url),
        )
        .await
    }

    /// One entry per page, whichever of the two reference reads asked.
    async fn fetch_reference_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.fetch_reference_page(url).await.map(|page| page.output)
    }

    async fn fetch_reference_page(&self, url: &str) -> Result<ReferencePage> {
        self.traced(
            "fetch_reference_page",
            json!({ "url": url }),
            url,
            self.inner.fetch_reference_page(url),
        )
        .await
    }

    async fn fetch_url_content_as_of(
        &self,
        url: &str,
        as_of: Option<chrono::NaiveDate>,
    ) -> Result<FetchUrlContentOutput> {
        self.traced(
            "fetch_url_content_as_of",
            json!({ "url": url, "as_of": as_of.map(|day| day.to_string()) }),
            url,
            self.inner.fetch_url_content_as_of(url, as_of),
        )
        .await
    }

    /// `Lead` has no serde form and no learning code follows links, so this
    /// goes live and is only counted.
    async fn follow_links(
        &self,
        url: &str,
        near_text: Option<&str>,
    ) -> Result<Vec<lattice_core::domain::sourcing::Lead>> {
        self.lock().untraced += 1;
        self.inner.follow_links(url, near_text).await
    }

    fn validate_url(&self, url: &str) -> Result<()> {
        self.inner.validate_url(url)
    }
}

#[cfg(test)]
#[path = "live_web_trace_tests.rs"]
mod tests;
