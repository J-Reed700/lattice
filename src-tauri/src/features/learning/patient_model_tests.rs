use super::*;
use crate::features::learning::dto::LearningCourseDepth;
use crate::features::learning::lessons::course_generation_tests::{outline, request, Model};
use crate::features::learning::repository::LearningRepository;
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
    cancel_after_first_wait: bool,
    cancelled: AtomicBool,
}

impl WaitObserver for Recorder {
    fn waiting(&self, seconds: u64) {
        self.waits.lock().unwrap().push(seconds);
        if self.cancel_after_first_wait {
            self.cancelled.store(true, Ordering::Relaxed);
        }
    }
    fn check_cancelled(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(AppError::ServiceNotAvailable(
                "Course generation cancelled.".into(),
            ))
        } else {
            Ok(())
        }
    }
}

async fn generate(llm: &dyn LLMPort) -> Result<usize> {
    let pool = crate::features::learning::tests::pool().await?;
    let program = crate::features::learning::service::generate(
        &LearningRepository::new(pool),
        &crate::features::learning::outline_draft::DraftRunRegistry::new(),
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
    assert!(error.to_string().contains("cancelled"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(flaky.calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn the_stop_button_ends_a_wait_in_an_outline_run() {
    let registry = crate::features::learning::outline_progress::OutlineRunRegistry::new();
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
    assert!(error.to_string().contains("cancelled"), "{error}");
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
