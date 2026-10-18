//! A `PageCapturePort` for tests: answers queued outcomes in order, then a
//! fixed page, and counts its calls.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use tokio_util::sync::CancellationToken;

use crate::features::web::traits::page_snapshot::{
    CaptureError, CaptureMethod, CaptureProgress, CaptureRequest, CapturedPage, PageCapturePort,
    ProgressSink, ResourceReport,
};

pub struct MockPageCapture {
    outcomes: Mutex<VecDeque<Result<CapturedPage, CaptureError>>>,
    calls: AtomicUsize,
}

impl Default for MockPageCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl MockPageCapture {
    pub fn new() -> Self {
        Self {
            outcomes: Mutex::new(VecDeque::new()),
            calls: AtomicUsize::new(0),
        }
    }

    /// The page every unqueued call returns.
    pub fn page(url: &str) -> CapturedPage {
        CapturedPage {
            requested_url: url.to_string(),
            final_url: url.to_string(),
            title: Some("Frozen".into()),
            html: "<!doctype html><html><head><title>Frozen</title></head>\
                   <body><p>Hello</p></body></html>"
                .into(),
            favicon: None,
            method: CaptureMethod::Http,
            resources: ResourceReport {
                inlined: 2,
                dropped: Vec::new(),
                bytes_inlined: 10,
            },
            partial: false,
            partial_reasons: Vec::new(),
            captured_at: Utc::now(),
        }
    }

    /// Answer the next call with `outcome`.
    pub fn push(&self, outcome: Result<CapturedPage, CaptureError>) {
        self.outcomes.lock().unwrap().push_back(outcome);
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl PageCapturePort for MockPageCapture {
    async fn capture(
        &self,
        request: CaptureRequest,
        progress: ProgressSink,
        cancel: CancellationToken,
    ) -> Result<CapturedPage, CaptureError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if cancel.is_cancelled() {
            return Err(CaptureError::Cancelled);
        }
        progress(CaptureProgress {
            done: 1,
            total: 2,
            message: "Fetching resources".into(),
        });
        let queued = self.outcomes.lock().unwrap().pop_front();
        queued.unwrap_or_else(|| Ok(Self::page(&request.url)))
    }
}
