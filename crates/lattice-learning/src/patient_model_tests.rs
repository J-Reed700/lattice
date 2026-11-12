use super::*;
use crate::dto::LearningCourseDepth;
use crate::lessons::course_generation_tests::{outline, request, Model};
use crate::repository::LearningRepository;
use lattice_core::shared::error::AppError;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const FAST: Backoff = Backoff {
    initial: Duration::from_millis(5),
    max: Duration::from_millis(20),
};

/// Fails the listed calls (counted from 1) with `error`, or every call.
struct Flaky {
    inner: Model,
    calls: AtomicUsize,
    failing: Vec<usize>,
    always: bool,
    error: fn() -> AppError,
}

impl Flaky {
    fn new(failing: Vec<usize>) -> Self {
        Self {
            inner: outline(4, 3),
            calls: AtomicUsize::new(0),
            failing,
            always: false,
            error: || AppError::ServiceNotAvailable("503: loading model".into()),
        }
    }
}

#[async_trait::async_trait]
impl LLMPort for Flaky {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let call = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
        if self.always || self.failing.contains(&call) {
            return Err((self.error)());
        }
        self.inner.complete(request).await
    }
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[derive(Default)]
struct Recorder {
    waits: Mutex<Vec<u64>>,
    events: Mutex<Vec<String>>,
    cancel_after_first_wait: bool,
    cancelled: AtomicBool,
}

impl WaitObserver for Recorder {
    fn waiting(&self, seconds: u64, cause: &AppError) {
        let code = lattice_core::shared::ipc::ApiError::from(cause.clone()).code;
        self.events
            .lock()
            .unwrap()
            .push(format!("waiting {code:?}"));
        self.waits.lock().unwrap().push(seconds);
        if self.cancel_after_first_wait {
            self.cancelled.store(true, Ordering::Relaxed);
        }
    }
    fn answered(&self) {
        self.events.lock().unwrap().push("answered".into());
    }
    fn check_cancelled(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(AppError::Cancelled("Course generation cancelled.".into()))
        } else {
            Ok(())
        }
    }
}

async fn generate(llm: &dyn LLMPort) -> Result<usize> {
    let pool = crate::tests::pool().await?;
    let db = lattice_core::shared::persistence::Database::new(pool);
    let program = crate::service::generate(
        &LearningRepository::new(db),
        &crate::outline_draft::DraftRunRegistry::new(),
        llm,
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await?;
    Ok(program.modules.len())
}

#[tokio::test]
async fn an_outline_waits_out_two_outages_and_repeats_no_completed_call() -> Result<()> {
    let clean = Flaky::new(vec![]);
    let recorder = Recorder::default();
    assert_eq!(
        generate(&PatientModel::new(&clean, &recorder).with_backoff(FAST)).await?,
        4
    );
    let calls = clean.calls.load(Ordering::Relaxed);
    assert!(calls >= 2, "the fixture makes several model calls");

    // The first request succeeds; the server then answers 503 twice.
    let flaky = Flaky::new(vec![2, 3]);
    let recorder = Recorder::default();
    assert_eq!(
        generate(&PatientModel::new(&flaky, &recorder).with_backoff(FAST)).await?,
        4
    );
    // Only the two failed requests were sent again; nothing completed was.
    assert_eq!(flaky.calls.load(Ordering::Relaxed), calls + 2);
    assert!(!recorder.waits.lock().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn cancelling_during_the_wait_stops_it() {
    let mut flaky = Flaky::new(vec![]);
    flaky.always = true;
    let recorder = Recorder {
        cancel_after_first_wait: true,
        ..Recorder::default()
    };
    let long = Backoff {
        initial: Duration::from_secs(300),
        max: Duration::from_secs(300),
    };
    let started = std::time::Instant::now();
    let request = CompletionRequest::default();
    let error = patiently(&recorder, long, |_| flaky.complete(&request))
        .await
        .err()
        .unwrap();
    assert!(matches!(error, AppError::Cancelled(_)), "{error}");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(flaky.calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn the_stop_button_ends_a_wait_in_an_outline_run() {
    let registry = crate::outline_progress::OutlineRunRegistry::new();
    let id = uuid::Uuid::new_v4().to_string();
    let run = registry.register(Some(id.clone()), |_| {}).unwrap();
    let mut flaky = Flaky::new(vec![]);
    flaky.always = true;
    let progress: Arc<OutlineProgress> = run.progress.clone();
    let watched = progress.clone();
    // Press stop once the wait is on screen.
    let stopper = tokio::spawn(async move {
        loop {
            let detail = watched.snapshot().detail.unwrap_or_default();
            if detail.starts_with("Waiting for the model server (retrying in ") {
                return registry.cancel(&id);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    let llm = PatientModel::new(&flaky, progress.as_ref());
    let error = progress
        .run(llm.complete(&CompletionRequest::default()))
        .await
        .err()
        .unwrap();
    assert!(stopper.await.unwrap());
    assert!(matches!(error, AppError::Cancelled(_)), "{error}");
}

#[tokio::test]
async fn a_non_transient_error_still_fails_at_once() {
    let mut flaky = Flaky::new(vec![]);
    flaky.always = true;
    flaky.error = || AppError::InvalidInput("The prompt is malformed.".into());
    let recorder = Recorder::default();
    let error = PatientModel::new(&flaky, &recorder)
        .with_backoff(FAST)
        .complete(&CompletionRequest::default())
        .await
        .err()
        .unwrap();
    assert!(matches!(error, AppError::InvalidInput(_)));
    assert_eq!(flaky.calls.load(Ordering::Relaxed), 1);
    assert!(recorder.waits.lock().unwrap().is_empty());
}

/// A model that reports a measured pace.
struct Paced;

#[async_trait::async_trait]
impl LLMPort for Paced {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        Ok(CompletionResponse::from_text("ok"))
    }
    fn model_name(&self) -> &str {
        "paced"
    }
    fn max_context_tokens(&self) -> usize {
        8_192
    }
    fn pace(&self) -> Option<lattice_core::application::ports::llm_port::Pace> {
        Some(lattice_core::application::ports::llm_port::Pace {
            prompt_tokens_per_second: 850.0,
            output_tokens_per_second: 11.3,
        })
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[test]
fn waiting_out_outages_keeps_the_pace_of_the_model_it_waits_on() {
    let recorder = Recorder::default();
    assert_eq!(PatientModel::new(&Paced, &recorder).pace(), Paced.pace());
}

const SERVER_FAULT: &str = "llama-server.exe was not found";
const COULD_NOT_START: &str = "The model server could not start: llama-server.exe was not found. \
    Reinstall Lattice, or choose another model in Settings.";

fn bundled_server_cannot_run() -> AppError {
    lattice_core::shared::error::server_cannot_start(SERVER_FAULT)
}

#[tokio::test]
async fn a_bundled_server_that_cannot_start_is_not_waited_for() {
    let mut flaky = Flaky::new(vec![]);
    flaky.always = true;
    flaky.error = bundled_server_cannot_run;
    let recorder = Recorder::default();
    let long = Backoff {
        initial: Duration::from_secs(300),
        max: Duration::from_secs(300),
    };
    let request = CompletionRequest::default();

    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        patiently(&recorder, long, |_| flaky.complete(&request)),
    )
    .await
    .expect("a server that cannot start ends the call, it is not waited out");

    let error = outcome.err().unwrap();
    assert!(error.to_string().contains(COULD_NOT_START), "{error}");
    assert_eq!(flaky.calls.load(Ordering::Relaxed), 1);
    assert!(recorder.waits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_outline_stops_at_once_when_the_bundled_server_cannot_start() {
    let mut flaky = Flaky::new(vec![]);
    flaky.always = true;
    flaky.error = bundled_server_cannot_run;
    let recorder = Recorder {
        cancel_after_first_wait: true,
        ..Recorder::default()
    };
    let patient = PatientModel::new(&flaky, &recorder).with_backoff(FAST);

    let outcome = tokio::time::timeout(Duration::from_secs(120), generate(&patient))
        .await
        .expect("outline generation does not wait on an install problem");

    let error = outcome.err().unwrap();
    assert!(error.to_string().contains(COULD_NOT_START), "{error}");
    assert_eq!(flaky.calls.load(Ordering::Relaxed), 1);
    assert!(recorder.waits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_server_that_is_starting_busy_or_unreachable_is_still_waited_for() {
    let outages: [fn() -> AppError; 4] = [
        || AppError::ServiceNotAvailable("503: loading model".into()),
        || AppError::ServiceNotAvailable("llama-server is still starting".into()),
        || AppError::Network("connection reset".into()),
        || AppError::RateLimitExceeded("the server is busy".into()),
    ];
    for outage in outages {
        let mut flaky = Flaky::new(vec![1]);
        flaky.error = outage;
        let recorder = Recorder::default();
        let response = PatientModel::new(&flaky, &recorder)
            .with_backoff(FAST)
            .complete(&CompletionRequest::default())
            .await;
        assert!(response.is_ok(), "{:?}", response.err());
        assert_eq!(flaky.calls.load(Ordering::Relaxed), 2);
        assert!(!recorder.waits.lock().unwrap().is_empty());
    }
}

type Cause = (fn() -> AppError, &'static str);

#[tokio::test]
async fn a_wait_carries_the_code_of_its_cause_and_ends_when_the_server_answers() {
    let causes: [Cause; 3] = [
        (
            || AppError::Network("connection reset".into()),
            "NetworkError",
        ),
        (
            || AppError::RateLimitExceeded("the server is busy".into()),
            "RateLimitExceeded",
        ),
        (
            || AppError::ServiceNotAvailable("503: loading model".into()),
            "ServiceNotAvailable",
        ),
    ];
    for (error, expected) in causes {
        let mut flaky = Flaky::new(vec![1, 2]);
        flaky.error = error;
        let recorder = Recorder::default();
        PatientModel::new(&flaky, &recorder)
            .with_backoff(FAST)
            .complete(&CompletionRequest::default())
            .await
            .unwrap();

        let events = recorder.events.lock().unwrap().clone();
        assert_eq!(
            events.first().map(String::as_str),
            Some(format!("waiting {expected}").as_str()),
            "{events:?}"
        );
        assert_eq!(
            events.last().map(String::as_str),
            Some("answered"),
            "{events:?}"
        );
        assert_eq!(
            events.iter().filter(|event| *event == "answered").count(),
            1,
            "{events:?}"
        );
    }
}

#[tokio::test]
async fn a_failure_that_is_not_an_outage_also_ends_the_wait() {
    let recorder = Recorder::default();
    let calls = AtomicUsize::new(0);
    let error = patiently(&recorder, FAST, |_| {
        let call = calls.fetch_add(1, Ordering::Relaxed);
        async move {
            if call == 0 {
                Err::<(), _>(AppError::Network("connection reset".into()))
            } else {
                Err(AppError::InvalidInput("The prompt is malformed.".into()))
            }
        }
    })
    .await
    .err()
    .unwrap();
    assert!(matches!(error, AppError::InvalidInput(_)));
    assert_eq!(
        recorder.events.lock().unwrap().last().map(String::as_str),
        Some("answered")
    );
}
