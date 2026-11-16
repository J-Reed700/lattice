use super::*;
use crate::features::llm::fitting::Fitted;
use crate::features::llm::scheduler::SchedulerRegistry;
use lattice_core::application::services::model_contract::legacy;
use lattice_core::application::ports::llm_port::{
    CompletionInput, InferencePriority, SamplingOverride, ServedTiming,
};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Answers "<prompt>#<n>", n counting the calls it has had, with a timing
/// large enough for a scheduler to measure.
struct Scripted {
    calls: AtomicUsize,
}

impl Scripted {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ModelBackend for Scripted {
    fn model_name(&self) -> &str {
        "scripted"
    }

    fn provider_name(&self) -> &str {
        "scripted"
    }

    fn window(&self) -> usize {
        8192
    }

    fn thinking(&self, _request: &CompletionRequest) -> Thinking {
        Thinking::Off
    }

    fn output_limits(&self) -> OutputLimits {
        OutputLimits::of(None)
    }

    fn supports_tool_calling(&self) -> bool {
        false
    }

    fn vision(&self) -> Option<VisionGeometry> {
        None
    }

    fn keeps_read_prompts(&self) -> bool {
        false
    }

    fn enforces(
        &self,
        _schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        false
    }

    fn is_alive(&self) -> bool {
        true
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }

    async fn send(
        &self,
        fitted: &Fitted<'_>,
        progress: Progress<'_>,
    ) -> Result<CompletionResponse> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        let prompt = match fitted.request().input().last() {
            Some(CompletionInput::Message { content, .. }) => content.clone(),
            _ => String::new(),
        };
        let mut response = CompletionResponse::from_text(format!("{prompt}#{n}"));
        response.input_tokens = 200;
        response.output_tokens = 100;
        response.reasoning = Some(format!("thought {n}"));
        response.timing = Some(ServedTiming {
            prompt_tokens: 200,
            prompt_seconds: 1.0 + n as f64,
            output_tokens: 100,
            output_seconds: 2.0 + n as f64,
        });
        if let Progress::Streamed { on_text, .. } = progress {
            on_text(response.text.clone())?;
        }
        Ok(response)
    }
}

/// Reads "<url>#<n>", n counting the calls it has had.
struct ScriptedWeb {
    calls: AtomicUsize,
}

impl ScriptedWeb {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn page(&self, url: &str) -> FetchUrlContentOutput {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        FetchUrlContentOutput {
            url: url.to_string(),
            title: Some("title".into()),
            content: format!("{url}#{n}"),
            content_truncated: false,
            word_count: 1,
            fetch_time_ms: 1.5,
            content_type: None,
            from_cache: false,
            read_via: Default::default(),
        }
    }
}

#[async_trait]
impl WebServiceTrait for ScriptedWeb {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(serde_json::from_value(json!({
            "results": [],
            "query": format!("{}#{n}", input.query),
            "result_count": 0,
        }))?)
    }

    async fn fetch_url_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        if url.contains("missing") {
            self.calls.fetch_add(1, Ordering::SeqCst);
            return Err(AppError::InvalidInput("gone".into()));
        }
        Ok(self.page(url))
    }

    fn validate_url(&self, _url: &str) -> Result<()> {
        Ok(())
    }
}

fn ask(prompt: &str) -> CompletionRequest {
    legacy::completion(legacy::Completion {
        input: vec![CompletionInput::Message {
            role: "user".into(),
            content: prompt.into(),
        }],
        ..Default::default()
    })
}

fn search(query: &str) -> WebSearchInput {
    serde_json::from_value(json!({ "query": query })).unwrap()
}

fn lines(dir: &Path, file: &str) -> Vec<Value> {
    std::fs::read_to_string(dir.join(file))
        .unwrap()
        .lines()
        .map(|text| serde_json::from_str(text).unwrap())
        .collect()
}

fn replay_of(from: &Path, into: &Path) -> CallLog {
    CallLog::Replay {
        from: from.to_path_buf(),
        into: into.to_path_buf(),
        misses: Misses::GoLive,
    }
}

fn strict_of(from: &Path, into: &Path) -> CallLog {
    CallLog::Replay {
        from: from.to_path_buf(),
        into: into.to_path_buf(),
        misses: Misses::Fail,
    }
}

/// One call through the wrapped backend, with no scheduler in front.
async fn send(backend: &Arc<dyn ModelBackend>, request: &CompletionRequest) -> CompletionResponse {
    let fitted = Fitted::uncut(backend.as_ref(), request);
    backend.send(&fitted, Progress::None).await.unwrap()
}

#[tokio::test]
async fn a_recorded_run_replays_every_call_without_the_inner_backend() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));

    let live = Scripted::new();
    let logged = CallLog::Record(a.clone())
        .open()
        .unwrap()
        .model(live.clone());
    let mut recorded = Vec::new();
    for prompt in ["one", "two", "three"] {
        recorded.push(send(&logged, &ask(prompt)).await);
    }
    assert_eq!(live.calls(), 3);
    let recorded_lines = lines(&a, "model.jsonl");
    let first = &recorded_lines[0];
    assert_eq!(first["live"], true);
    assert_eq!(first["method"], "send");
    assert_eq!(first["model"], "scripted");

    let idle = Scripted::new();
    let replayed = replay_of(&a, &b).open().unwrap().model(idle.clone());
    for (prompt, before) in ["one", "two", "three"].into_iter().zip(&recorded) {
        let after = send(&replayed, &ask(prompt)).await;
        assert_eq!(
            serde_json::to_value(&after).unwrap(),
            serde_json::to_value(before).unwrap()
        );
    }
    assert_eq!(idle.calls(), 0);
    let replayed_lines = lines(&b, "model.jsonl");
    assert_eq!(replayed_lines.len(), 3);
    assert!(replayed_lines.iter().all(|line| line["live"] == false));
}

#[tokio::test]
async fn a_changed_request_goes_live_and_is_logged_as_live() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    let logged = CallLog::Record(a.clone())
        .open()
        .unwrap()
        .model(Scripted::new());
    send(&logged, &ask("one")).await;

    let inner = Scripted::new();
    let replayed = replay_of(&a, &b).open().unwrap().model(inner.clone());
    send(&replayed, &ask("one")).await;
    let mut changed = ask("one");
    changed.sampling = Some(SamplingOverride::deterministic());
    send(&replayed, &changed).await;
    send(&replayed, &ask("other")).await;

    assert_eq!(inner.calls(), 2);
    let flags: Vec<_> = lines(&b, "model.jsonl")
        .iter()
        .map(|line| line["live"].clone())
        .collect();
    assert_eq!(flags, [json!(false), json!(true), json!(true)]);
}

#[test]
fn the_key_ignores_scheduling_fields_and_sees_the_model() {
    let plain = ask("one");
    let mut scheduled = ask("one");
    scheduled.cache_key = Some("course-9".into());
    scheduled.priority = InferencePriority::Interactive;
    assert_eq!(
        model_key("m", &plain).unwrap().0,
        model_key("m", &scheduled).unwrap().0
    );
    assert_ne!(
        model_key("m", &plain).unwrap().0,
        model_key("n", &plain).unwrap().0
    );
    let mut limited = ask("one");
    *legacy::says(&mut limited).max_output_tokens = Some(5);
    assert_ne!(
        model_key("m", &plain).unwrap().0,
        model_key("m", &limited).unwrap().0
    );
}

#[tokio::test]
async fn two_identical_requests_replay_their_two_answers_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    let logged = CallLog::Record(a.clone())
        .open()
        .unwrap()
        .model(Scripted::new());
    let first = send(&logged, &ask("same")).await;
    let second = send(&logged, &ask("same")).await;
    assert_eq!(
        (first.text.as_str(), second.text.as_str()),
        ("same#0", "same#1")
    );

    let inner = Scripted::new();
    let replayed = replay_of(&a, &b).open().unwrap().model(inner.clone());
    assert_eq!(send(&replayed, &ask("same")).await.text, "same#0");
    assert_eq!(send(&replayed, &ask("same")).await.text, "same#1");
    assert_eq!(inner.calls(), 0);
    assert_eq!(send(&replayed, &ask("same")).await.text, "same#0");
    assert_eq!(inner.calls(), 1);
}

#[tokio::test]
async fn a_replayed_stream_gives_the_callbacks_the_finished_text() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    let logged = CallLog::Record(a.clone())
        .open()
        .unwrap()
        .model(Scripted::new());
    send(&logged, &ask("q")).await;

    let inner = Scripted::new();
    let replayed = replay_of(&a, &b).open().unwrap().model(inner.clone());
    let seen = Mutex::new(Vec::new());
    let thoughts = Mutex::new(Vec::new());
    let on_text = |text: String| -> Result<()> {
        seen.lock().unwrap().push(text);
        Ok(())
    };
    let on_reasoning = |text: String| -> Result<()> {
        thoughts.lock().unwrap().push(text);
        Ok(())
    };
    let request = ask("q");
    let fitted = Fitted::uncut(replayed.as_ref(), &request);
    let progress = Progress::Streamed {
        on_text: &on_text,
        on_reasoning: Some(&on_reasoning),
        on_retry: None,
    };
    replayed.send(&fitted, progress).await.unwrap();
    assert_eq!(*seen.lock().unwrap(), ["q#0"]);
    assert_eq!(*thoughts.lock().unwrap(), ["thought 0"]);
    assert_eq!(inner.calls(), 0);
}

#[tokio::test]
async fn the_web_log_records_and_replays_the_same_way() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    let live = ScriptedWeb::new();
    let logged = CallLog::Record(a.clone()).open().unwrap().web(live.clone());
    let page = logged.fetch_url_content("https://x.test/p").await.unwrap();
    let again = logged.fetch_url_content("https://x.test/p").await.unwrap();
    let found = logged.search_web(&search("rust")).await.unwrap();
    assert!(logged
        .fetch_url_content("https://x.test/missing")
        .await
        .is_err());
    assert_eq!(
        (page.content.as_str(), again.content.as_str()),
        ("https://x.test/p#0", "https://x.test/p#1")
    );

    let inner = ScriptedWeb::new();
    let replayed = replay_of(&a, &b).open().unwrap().web(inner.clone());
    let r1 = replayed
        .fetch_url_content("https://x.test/p")
        .await
        .unwrap();
    let r2 = replayed
        .fetch_url_content("https://x.test/p")
        .await
        .unwrap();
    let rs = replayed.search_web(&search("rust")).await.unwrap();
    let rg = replayed.fetch_url_content("https://x.test/missing").await;
    let same = |x: &dyn erased::Json, y: &dyn erased::Json| x.json() == y.json();
    assert!(same(&r1, &page) && same(&r2, &again) && same(&rs, &found));
    assert!(matches!(rg, Err(AppError::InvalidInput(message)) if message == "gone"));
    assert_eq!(inner.calls(), 0);

    replayed
        .fetch_url_content("https://x.test/new")
        .await
        .unwrap();
    replayed
        .fetch_url_content_as_of("https://x.test/p", None)
        .await
        .unwrap();
    assert_eq!(inner.calls(), 2);
    let flags: Vec<_> = lines(&b, "web.jsonl")
        .iter()
        .map(|line| line["live"].clone())
        .collect();
    assert_eq!(
        flags,
        [false, false, false, false, true, true].map(Value::from)
    );
}

mod erased {
    use serde::Serialize;

    pub(super) trait Json {
        fn json(&self) -> serde_json::Value;
    }

    impl<T: Serialize> Json for T {
        fn json(&self) -> serde_json::Value {
            serde_json::to_value(self).unwrap()
        }
    }
}

#[tokio::test]
async fn finish_counts_hits_and_live_calls_per_log() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    let record = CallLog::Record(a.clone()).open().unwrap();
    let logged = record.model(Scripted::new());
    send(&logged, &ask("one")).await;
    send(&logged, &ask("two")).await;
    let pages = record.web(ScriptedWeb::new());
    pages.fetch_url_content("https://x.test/p").await.unwrap();
    let recorded = record.finish().unwrap();
    assert_eq!(
        recorded["model"],
        json!({ "calls": 2, "hits": 0, "live": 2, "misses": 0 })
    );
    assert_eq!(
        recorded["web"],
        json!({ "calls": 1, "hits": 0, "live": 1, "misses": 0 })
    );

    let replay = replay_of(&a, &b).open().unwrap();
    let replayed = replay.model(Scripted::new());
    send(&replayed, &ask("one")).await;
    send(&replayed, &ask("three")).await;
    let pages = replay.web(ScriptedWeb::new());
    pages.fetch_url_content("https://x.test/p").await.unwrap();
    pages.fetch_url_content("https://x.test/q").await.unwrap();
    let summary = replay.finish().unwrap();
    assert_eq!(
        summary["model"],
        json!({ "calls": 2, "hits": 1, "live": 1, "misses": 0 })
    );
    assert_eq!(
        summary["web"],
        json!({ "calls": 2, "hits": 1, "live": 1, "misses": 0 })
    );
    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(b.join("summary.json")).unwrap()).unwrap();
    assert_eq!(written, summary);
}

async fn record_one(dir: &Path) {
    let record = CallLog::Record(dir.to_path_buf()).open().unwrap();
    send(&record.model(Scripted::new()), &ask("one")).await;
    let pages = record.web(ScriptedWeb::new());
    pages.fetch_url_content("https://x.test/p").await.unwrap();
}

#[tokio::test]
async fn a_strict_replay_fails_a_changed_model_request_without_calling_inner() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    record_one(&a).await;

    let inner = Scripted::new();
    let replayed = strict_of(&a, &b).open().unwrap().model(inner.clone());
    let request = ask("other");
    let fitted = Fitted::uncut(replayed.as_ref(), &request);
    let failed = replayed.send(&fitted, Progress::None).await;
    assert!(matches!(failed, Err(AppError::InvalidConfig(_))));
    assert_eq!(inner.calls(), 0);
    let logged = lines(&b, "model.jsonl");
    assert_eq!(logged.len(), 1);
    assert_eq!(logged[0]["missed"], true);
    assert_eq!(logged[0]["live"], false);
    assert_eq!(logged[0]["request"]["input"][0]["content"], json!("other"));
    assert!(!logged[0]["error"].is_null());
}

#[tokio::test]
async fn a_strict_replay_fails_a_changed_web_call_without_calling_inner() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    record_one(&a).await;

    let inner = ScriptedWeb::new();
    let replayed = strict_of(&a, &b).open().unwrap().web(inner.clone());
    let failed = replayed.fetch_url_content("https://x.test/new").await;
    assert!(matches!(failed, Err(AppError::InvalidConfig(_))));
    assert_eq!(inner.calls(), 0);
    let logged = lines(&b, "web.jsonl");
    assert_eq!(logged.len(), 1);
    assert_eq!(logged[0]["missed"], true);
    assert_eq!(logged[0]["live"], false);
    assert_eq!(logged[0]["request"], json!({ "url": "https://x.test/new" }));
    assert!(!logged[0]["error"].is_null());
}

#[tokio::test]
async fn finish_counts_the_misses_of_a_strict_replay() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    record_one(&a).await;

    let replay = strict_of(&a, &b).open().unwrap();
    let model = replay.model(Scripted::new());
    send(&model, &ask("one")).await;
    let request = ask("two");
    let fitted = Fitted::uncut(model.as_ref(), &request);
    assert!(model.send(&fitted, Progress::None).await.is_err());
    let pages = replay.web(ScriptedWeb::new());
    assert!(pages.fetch_url_content("https://x.test/q").await.is_err());
    let summary = replay.finish().unwrap();
    assert_eq!(
        summary["model"],
        json!({ "calls": 2, "hits": 1, "live": 0, "misses": 1 })
    );
    assert_eq!(
        summary["web"],
        json!({ "calls": 1, "hits": 0, "live": 0, "misses": 1 })
    );
}

#[tokio::test]
async fn an_unchanged_request_in_a_strict_replay_still_hits() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    record_one(&a).await;

    let replay = strict_of(&a, &b).open().unwrap();
    let inner = Scripted::new();
    let answer = send(&replay.model(inner.clone()), &ask("one")).await;
    assert_eq!(answer.text, "one#0");
    let pages = replay.web(ScriptedWeb::new());
    pages.fetch_url_content("https://x.test/p").await.unwrap();
    assert_eq!(inner.calls(), 0);
    let summary = replay.finish().unwrap();
    assert_eq!(summary["model"]["hits"], 1);
    assert_eq!(summary["model"]["misses"], 0);
    assert_eq!(summary["web"]["hits"], 1);
}

#[test]
fn a_replay_refuses_to_write_into_the_log_it_reads() {
    let dir = tempfile::tempdir().unwrap();
    assert!(replay_of(dir.path(), dir.path()).open().is_err());
}

#[tokio::test]
async fn a_scheduled_backend_with_recorded_calls_lands_in_the_model_log() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SchedulerRegistry::default();
    registry.record_calls(CallLog::Record(dir.path().join("a")).open().unwrap());
    let port = registry.front(Scripted::new(), registry.cloud("test"), None);
    let answer = port.complete(&ask("hello")).await.unwrap();
    assert_eq!(answer.text, "hello#0");
    let recorded = lines(&dir.path().join("a"), "model.jsonl");
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0]["response"]["text"], "hello#0");
}

#[tokio::test]
async fn a_replay_through_a_fresh_registry_gives_the_scheduler_the_same_pace() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    let prompts = ["one", "two", "three"];

    let registry = SchedulerRegistry::default();
    registry.record_calls(CallLog::Record(a.clone()).open().unwrap());
    let port = registry.front(Scripted::new(), registry.cloud("test"), None);
    for prompt in prompts {
        port.complete(&ask(prompt)).await.unwrap();
    }
    let recorded_pace = port.pace();
    assert!(recorded_pace.is_some());

    let idle = Scripted::new();
    let fresh = SchedulerRegistry::default();
    fresh.record_calls(replay_of(&a, &b).open().unwrap());
    let replayed = fresh.front(idle.clone(), fresh.cloud("test"), None);
    assert!(replayed.pace().is_none());
    for prompt in prompts {
        replayed.complete(&ask(prompt)).await.unwrap();
    }
    assert_eq!(idle.calls(), 0);
    assert_eq!(replayed.pace(), recorded_pace);
}

#[tokio::test]
async fn a_container_with_calls_records_its_web_reads() -> anyhow::Result<()> {
    use crate::infrastructure::persistence::database::{initialize_database, DatabaseConnection};
    use crate::interfaces::di::Container;

    let data = tempfile::tempdir()?;
    let database = DatabaseConnection::new(data.path().join("app.db")).await?;
    initialize_database(database.database()).await?;
    let calls = CallLog::Record(data.path().join("calls")).open()?;
    let container = Container::new(
        database.database().clone(),
        None,
        "http://127.0.0.1:1",
        "test-model",
        data.path().to_path_buf(),
        Arc::default(),
    )
    .await?
    .with_calls(Arc::clone(&calls));

    let refused = container
        .web()
        .fetch_url_content("http://127.0.0.1/x")
        .await;
    assert!(refused.is_err());
    let recorded = lines(&data.path().join("calls"), "web.jsonl");
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0]["method"], "fetch_url_content");
    assert_eq!(calls.finish()?["web"]["live"], 1);
    Ok(())
}
