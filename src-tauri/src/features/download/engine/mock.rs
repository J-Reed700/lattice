//! Scriptable in-memory `DownloadEngine` for tests.
use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct MockDownloadEngine {
    file_sizes: Arc<Mutex<HashMap<String, Option<u64>>>>,
    supports_resume: Arc<Mutex<HashMap<String, bool>>>,
    download_results: Arc<Mutex<HashMap<String, Result<DownloadResult, DownloadError>>>>,
    download_calls: Arc<Mutex<Vec<DownloadOptions>>>,
    failure_count: Arc<Mutex<usize>>,
    permanent_failure: Arc<Mutex<Option<String>>>,
    network_error: Arc<Mutex<Option<String>>>,
    http_error: Arc<Mutex<Option<(u16, String)>>>,
    /// Makes `download` block until cancelled, so tests can exercise
    /// pause/cancel against a transfer that is genuinely in flight.
    /// Without it the mock returns instantly and any test that pauses is
    /// racing the completion.
    stall: Arc<Mutex<bool>>,
}

impl MockDownloadEngine {
    pub fn new() -> Self {
        Self {
            file_sizes: Arc::new(Mutex::new(HashMap::new())),
            supports_resume: Arc::new(Mutex::new(HashMap::new())),
            download_results: Arc::new(Mutex::new(HashMap::new())),
            download_calls: Arc::new(Mutex::new(Vec::new())),
            failure_count: Arc::new(Mutex::new(0)),
            permanent_failure: Arc::new(Mutex::new(None)),
            network_error: Arc::new(Mutex::new(None)),
            http_error: Arc::new(Mutex::new(None)),
            stall: Arc::new(Mutex::new(false)),
        }
    }

    /// Park `download` indefinitely so pause/cancel can be tested
    /// against an in-flight transfer.
    pub fn set_stall(&self, stall: bool) {
        *self.stall.lock().unwrap() = stall;
    }

    pub fn set_file_size(&self, url: &str, size: Option<u64>) {
        self.file_sizes
            .lock()
            .unwrap()
            .insert(url.to_string(), size);
    }

    pub fn set_supports_resume(&self, url: &str, supports: bool) {
        self.supports_resume
            .lock()
            .unwrap()
            .insert(url.to_string(), supports);
    }

    pub fn set_download_result(&self, url: &str, result: Result<DownloadResult, DownloadError>) {
        self.download_results
            .lock()
            .unwrap()
            .insert(url.to_string(), result);
    }

    /// Get all download calls (CRITICAL for Test 16)
    pub fn get_download_calls(&self) -> Vec<DownloadOptions> {
        self.download_calls.lock().unwrap().clone()
    }

    /// Get last download call
    pub fn get_last_call(&self) -> Option<DownloadOptions> {
        self.download_calls.lock().unwrap().last().cloned()
    }

    /// Clear call history
    pub fn clear_calls(&self) {
        self.download_calls.lock().unwrap().clear()
    }

    /// Set number of failures before success (for retry testing)
    pub fn set_failure_count(&self, count: usize) {
        *self.failure_count.lock().unwrap() = count;
    }

    /// Set permanent failure with error message
    pub fn set_permanent_failure(&self, error_message: String) {
        *self.permanent_failure.lock().unwrap() = Some(error_message);
    }

    /// Set network error
    pub fn set_network_error(&self, error_message: String) {
        *self.network_error.lock().unwrap() = Some(error_message);
    }

    /// Set HTTP error (status code + message)
    pub fn set_http_error(&self, status_code: u16, error_message: String) {
        *self.http_error.lock().unwrap() = Some((status_code, error_message));
    }
}

impl Default for MockDownloadEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DownloadEngine for MockDownloadEngine {
    async fn download(&self, options: DownloadOptions) -> Result<DownloadResult, DownloadError> {
        // Record call for verification (CRITICAL for Test 16)
        self.download_calls.lock().unwrap().push(options.clone());

        // Simulate a long transfer: park here until the caller's
        // `tokio::select!` drops this future because it was told to stop.
        if *self.stall.lock().unwrap() {
            if let Some(callback) = options.progress_callback.as_ref() {
                callback(1, 1024.0);
            }
            std::future::pending::<()>().await;
        }

        if let Some(error) = self.permanent_failure.lock().unwrap().as_ref() {
            return Err(DownloadError::ValidationFailed(error.clone()));
        }

        if let Some(error) = self.network_error.lock().unwrap().as_ref() {
            return Err(DownloadError::NetworkError(error.clone()));
        }

        if let Some((status, message)) = self.http_error.lock().unwrap().as_ref() {
            return Err(DownloadError::HttpError {
                status: *status,
                message: message.clone(),
            });
        }

        let mut failures = self.failure_count.lock().unwrap();
        if *failures > 0 {
            *failures -= 1;
            return Err(DownloadError::NetworkError(
                "Simulated transient failure".to_string(),
            ));
        }

        let results = self.download_results.lock().unwrap();
        match results.get(&options.url) {
            Some(Ok(result)) => {
                if let Some(callback) = options.progress_callback {
                    callback(result.bytes_downloaded, 1024.0 * 1024.0);
                }
                Ok(DownloadResult {
                    bytes_downloaded: result.bytes_downloaded,
                    total_bytes: result.total_bytes,
                    sha256_checksum: result.sha256_checksum.clone(),
                    elapsed: result.elapsed,
                })
            }
            Some(Err(e)) => Err(DownloadError::NetworkError(format!("Mock error: {:?}", e))),
            None => {
                let default_result = DownloadResult {
                    bytes_downloaded: 1000,
                    total_bytes: Some(1000),
                    sha256_checksum: "a".repeat(64),
                    elapsed: Duration::from_secs(1),
                };
                if let Some(callback) = options.progress_callback {
                    callback(default_result.bytes_downloaded, 1024.0 * 1024.0);
                }
                Ok(default_result)
            }
        }
    }

    async fn get_file_size(&self, url: &str) -> Result<(Option<u64>, String), DownloadError> {
        let sizes = self.file_sizes.lock().unwrap();
        let size = sizes.get(url).copied().unwrap_or(Some(1000));
        Ok((size, url.to_string()))
    }

    async fn supports_resume(&self, url: &str) -> Result<bool, DownloadError> {
        let supports = self.supports_resume.lock().unwrap();
        Ok(*supports.get(url).unwrap_or(&true))
    }
}
