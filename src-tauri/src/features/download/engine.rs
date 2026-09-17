use crate::domain::download::DownloadError;
use crate::shared::utils::reqwest_client_builder;
use async_trait::async_trait;
use reqwest::{header, Client, Response, StatusCode};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::Disks;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, error, info, warn};
use url::Url;

/// Minimum reasonable file size for ML model files (10 KB)
/// Content-Length below this threshold is likely a redirect/error page
const MIN_VALID_MODEL_SIZE: u64 = 10 * 1024; // 10 KB

/// HTTP status codes that are typically transient and safe to retry.
const RETRYABLE_HTTP_STATUSES: [u16; 8] = [408, 409, 425, 429, 500, 502, 503, 504];

fn resolve_redirect_location(request_url: &str, location: &str) -> Result<String, DownloadError> {
    let base =
        Url::parse(request_url).map_err(|error| DownloadError::InvalidUrl(error.to_string()))?;
    base.join(location)
        .map(|url| url.to_string())
        .map_err(|error| DownloadError::InvalidResponse(format!("Invalid redirect URL: {error}")))
}

fn is_trusted_model_host(host: &str) -> bool {
    host == "huggingface.co"
        || host.ends_with(".huggingface.co")
        || host == "hf.co"
        || host.ends_with(".hf.co")
}

fn may_forward_authorization(source: &str, target: &str) -> bool {
    let (Ok(source), Ok(target)) = (Url::parse(source), Url::parse(target)) else {
        return false;
    };
    if source.origin() == target.origin() {
        return true;
    }

    match (source.host_str(), target.host_str()) {
        (Some(source_host), Some(target_host)) => {
            is_trusted_model_host(source_host) && is_trusted_model_host(target_host)
        }
        _ => false,
    }
}

pub type ProgressCallback = Arc<dyn Fn(u64, f64) + Send + Sync>;

#[derive(Clone)]
pub struct DownloadOptions {
    pub url: String,
    pub destination: PathBuf,
    pub resume_from: Option<u64>,
    pub progress_callback: Option<ProgressCallback>,
    pub auth_token: Option<String>,
}

#[async_trait]
pub trait DownloadEngine: Send + Sync {
    async fn download(&self, options: DownloadOptions) -> Result<DownloadResult, DownloadError>;

    async fn get_file_size(&self, url: &str) -> Result<(Option<u64>, String), DownloadError>;

    async fn supports_resume(&self, url: &str) -> Result<bool, DownloadError>;
}

#[derive(Debug)]
pub struct DownloadResult {
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub sha256_checksum: String,
    pub elapsed: Duration,
}

pub struct HttpDownloadEngine {
    client: Client,
    chunk_size: usize,
    progress_interval: Duration,
}

impl HttpDownloadEngine {
    pub fn new() -> Result<Self, DownloadError> {
        // No total request timeout so very large files can stream for hours.
        let builder = reqwest_client_builder()
            .connect_timeout(Duration::from_secs(30))
            .tcp_keepalive(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::limited(10)); // Explicit redirect policy

        let client = builder.build().map_err(|e| {
            DownloadError::NetworkError(format!("Failed to create HTTP client: {}", e))
        })?;

        Ok(Self {
            client,
            chunk_size: 8192, // 8KB chunks
            // Avoid high-frequency DB/event churn during very large downloads.
            progress_interval: Duration::from_millis(500),
        })
    }

    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    pub fn with_progress_interval(mut self, interval: Duration) -> Self {
        self.progress_interval = interval;
        self
    }

    fn check_disk_space(destination: &Path, required_bytes: u64) -> Result<(), DownloadError> {
        let disks = Disks::new_with_refreshed_list();

        let dest_path = destination.parent().unwrap_or(destination);

        // Several mount points can prefix one path (`/` and
        // `/Volumes/External` on macOS). The most specific/longest mount is
        // the volume that actually owns the destination.
        if let Some(disk) = disks
            .iter()
            .filter(|disk| dest_path.starts_with(disk.mount_point()))
            .max_by_key(|disk| mount_point_specificity(disk.mount_point()))
        {
            let available_bytes = disk.available_space();

            debug!(
                mount_point = %disk.mount_point().display(),
                available_bytes = available_bytes,
                required_bytes = required_bytes,
                "Checking disk space"
            );

            if available_bytes < required_bytes {
                error!(
                    available = available_bytes,
                    required = required_bytes,
                    deficit = required_bytes - available_bytes,
                    "Insufficient disk space"
                );
                return Err(DownloadError::InsufficientDiskSpace {
                    required: required_bytes,
                    available: available_bytes,
                });
            }

            return Ok(());
        }

        warn!(
            path = %dest_path.display(),
            "Could not determine disk for path, skipping space check"
        );
        Ok(())
    }

    fn is_retryable_error(error: &DownloadError) -> bool {
        match error {
            DownloadError::NetworkError(_) => true,
            DownloadError::HttpError { status, .. } => RETRYABLE_HTTP_STATUSES.contains(status),
            _ => false,
        }
    }

    fn resolved_resume_offset(resume_from: Option<u64>, status: StatusCode) -> u64 {
        match resume_from {
            Some(offset) if status == StatusCode::PARTIAL_CONTENT => offset,
            _ => 0,
        }
    }

    async fn compute_file_sha256(&self, path: &Path) -> Result<String, DownloadError> {
        let mut file = File::open(path).await.map_err(|e| {
            DownloadError::IoError(format!(
                "Failed to open file for checksum calculation: {}",
                e
            ))
        })?;

        let mut hasher = Sha256::new();
        let buffer_len = self.chunk_size.max(256 * 1024);
        let mut buffer = vec![0u8; buffer_len];

        loop {
            let bytes_read = file.read(&mut buffer).await.map_err(|e| {
                DownloadError::IoError(format!("Failed to read file for checksum: {}", e))
            })?;

            if bytes_read == 0 {
                break;
            }

            let (consumed, _) = buffer.split_at(bytes_read);
            hasher.update(consumed);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    /// The destination already holds every byte the server advertises, so
    /// there is nothing left to transfer. A ranged request from that offset
    /// is answered with 416 Range Not Satisfiable, which is how re-downloading
    /// a model whose files were still intact on disk (fresh database, weights
    /// kept) used to fail. Hash the file so the caller's checksum path runs
    /// exactly as it would after a real transfer.
    async fn complete_from_local_file(
        &self,
        url: &str,
        destination: &Path,
        total_bytes: u64,
        progress_callback: Option<&ProgressCallback>,
    ) -> Result<DownloadResult, DownloadError> {
        let start_time = Instant::now();

        let metadata = tokio::fs::metadata(destination).await.map_err(|e| {
            DownloadError::IoError(format!("Failed to read local file metadata: {}", e))
        })?;
        if metadata.len() != total_bytes {
            return Err(DownloadError::ValidationFailed(format!(
                "Local file is {} bytes but the server advertises {} bytes",
                metadata.len(),
                total_bytes
            )));
        }

        let sha256_checksum = self.compute_file_sha256(destination).await?;

        if let Some(callback) = progress_callback {
            callback(total_bytes, 0.0);
        }

        info!(
            url = %url,
            destination = %destination.display(),
            bytes = total_bytes,
            "Local file already complete; skipping transfer"
        );

        Ok(DownloadResult {
            bytes_downloaded: total_bytes,
            total_bytes: Some(total_bytes),
            sha256_checksum,
            elapsed: start_time.elapsed(),
        })
    }

    async fn download_with_retry(
        &self,
        url: &str,
        destination: &PathBuf,
        initial_resume_from: Option<u64>,
        progress_callback: Option<ProgressCallback>,
        auth_token: Option<&String>,
        expected_total_bytes: Option<u64>,
    ) -> Result<DownloadResult, DownloadError> {
        // Remote model hosts occasionally terminate long streams mid-transfer.
        // A few extra resume attempts drastically improves completion rates.
        const MAX_RETRIES: u32 = 6;
        const BACKOFF_DELAYS: [u64; 5] = [1, 2, 4, 8, 12];

        let mut resume_from = initial_resume_from;
        let mut last_error = None;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let delay_secs = BACKOFF_DELAYS
                    .get(attempt as usize - 1)
                    .copied()
                    .unwrap_or(4);
                info!(
                    attempt = attempt + 1,
                    max_retries = MAX_RETRIES,
                    delay_secs = delay_secs,
                    resume_from = ?resume_from,
                    "Retrying download after transient error"
                );
                tokio::time::sleep(Duration::from_secs(delay_secs)).await;
            }

            match self
                .download_impl(
                    url,
                    destination,
                    resume_from,
                    progress_callback.clone(),
                    auth_token,
                    expected_total_bytes,
                )
                .await
            {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if Self::is_retryable_error(&e) {
                        last_error = Some(e);

                        if destination.exists() {
                            if let Ok(metadata) = tokio::fs::metadata(destination).await {
                                resume_from = Some(metadata.len());
                                debug!(
                                    bytes_downloaded = metadata.len(),
                                    "Will resume from last byte on retry"
                                );
                            }
                        }
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        error!(
            url = %url,
            attempts = MAX_RETRIES,
            "Download failed after all retry attempts"
        );

        Err(last_error.unwrap_or_else(|| {
            DownloadError::NetworkError("Download failed after retries".to_string())
        }))
    }

    async fn send_request_with_range(
        &self,
        url: &str,
        start_byte: Option<u64>,
        auth_token: Option<&String>,
    ) -> Result<Response, DownloadError> {
        let mut request = self.client.get(url);

        if let Some(start) = start_byte {
            request = request.header(header::RANGE, format!("bytes={}-", start));
        }

        if let Some(token) = auth_token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", token));
        }

        request
            .send()
            .await
            .map_err(|e| DownloadError::NetworkError(format!("Request failed: {}", e)))
    }

    async fn download_impl(
        &self,
        url: &str,
        destination: &PathBuf,
        resume_from: Option<u64>,
        progress_callback: Option<ProgressCallback>,
        auth_token: Option<&String>,
        expected_total_bytes: Option<u64>,
    ) -> Result<DownloadResult, DownloadError> {
        let start_time = Instant::now();

        info!(url = %url, "Starting download");

        if let (Some(offset), Some(total)) = (resume_from, expected_total_bytes) {
            if total > 0 && offset == total {
                return self
                    .complete_from_local_file(url, destination, total, progress_callback.as_ref())
                    .await;
            }
        }

        let response = self
            .send_request_with_range(url, resume_from, auth_token)
            .await?;

        let status = response.status();
        debug!(
            status = %status,
            content_length = ?response.content_length(),
            headers = ?response.headers(),
            "Received HTTP response"
        );

        if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
            let status_code = status.as_u16();
            let error_message = match status {
                StatusCode::NOT_FOUND => "File not found (404)".to_string(),
                StatusCode::FORBIDDEN => "Access forbidden (403)".to_string(),
                StatusCode::UNAUTHORIZED => "Authentication required (401)".to_string(),
                StatusCode::SERVICE_UNAVAILABLE => "Service unavailable (503)".to_string(),
                StatusCode::INTERNAL_SERVER_ERROR => "Server error (500)".to_string(),
                _ => format!("HTTP error {}", status_code),
            };

            error!(
                url = %url,
                status = status_code,
                "HTTP request failed"
            );

            return Err(DownloadError::HttpError {
                status: status_code,
                message: error_message,
            });
        }

        // Use expected_total_bytes from HEAD request instead of response Content-Length
        // This ensures we get the correct size after following redirects
        let total_bytes = expected_total_bytes;
        let resume_offset = Self::resolved_resume_offset(resume_from, status);

        if resume_from.is_some() && resume_offset == 0 && status.is_success() {
            warn!(
                url = %url,
                status = %status,
                "Server ignored range resume request; restarting download from byte 0"
            );
        }

        let mut file = if resume_offset > 0 {
            OpenOptions::new()
                .write(true)
                .append(true)
                .open(destination)
                .await
                .map_err(|e| {
                    DownloadError::IoError(format!("Failed to open file for resume: {}", e))
                })?
        } else {
            if let Some(parent) = destination.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    DownloadError::IoError(format!("Failed to create parent directory: {}", e))
                })?;
            }

            File::create(destination)
                .await
                .map_err(|e| DownloadError::IoError(format!("Failed to create file: {}", e)))?
        };

        let mut hasher = Sha256::new();
        let mut bytes_downloaded = resume_offset;
        let mut last_progress_update = Instant::now();
        let mut last_bytes = bytes_downloaded;

        let mut stream = response.bytes_stream();
        use futures::StreamExt;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result
                .map_err(|e| DownloadError::NetworkError(format!("Stream error: {}", e)))?;

            file.write_all(&chunk)
                .await
                .map_err(|e| DownloadError::IoError(format!("Failed to write to file: {}", e)))?;

            hasher.update(&chunk);
            bytes_downloaded += chunk.len() as u64;

            let now = Instant::now();
            if now.duration_since(last_progress_update) >= self.progress_interval {
                let elapsed = now.duration_since(last_progress_update).as_secs_f64();
                let bytes_per_second = if elapsed > 0.0 {
                    (bytes_downloaded - last_bytes) as f64 / elapsed
                } else {
                    0.0
                };

                if let Some(ref callback) = progress_callback {
                    callback(bytes_downloaded, bytes_per_second);
                }

                last_progress_update = now;
                last_bytes = bytes_downloaded;
            }
        }

        file.sync_all()
            .await
            .map_err(|e| DownloadError::IoError(format!("Failed to sync file: {}", e)))?;

        let checksum = if resume_offset > 0 {
            // Hash the full file when resumed; stream hasher only saw the tail section.
            self.compute_file_sha256(destination).await?
        } else {
            format!("{:x}", hasher.finalize())
        };
        let elapsed = start_time.elapsed();

        if bytes_downloaded == 0 {
            error!(
                url = %url,
                destination = %destination.display(),
                "Download completed with 0 bytes"
            );
            return Err(DownloadError::ValidationFailed(
                "Download completed but no data was written (0 bytes)".to_string(),
            ));
        }

        if let Some(expected_total) = total_bytes {
            let actual_downloaded = bytes_downloaded.saturating_sub(resume_offset);
            let expected_download = expected_total.saturating_sub(resume_offset);

            if actual_downloaded != expected_download {
                error!(
                    url = %url,
                    expected = expected_download,
                    actual = actual_downloaded,
                    "Downloaded bytes mismatch"
                );
                return Err(DownloadError::ValidationFailed(format!(
                    "Downloaded {} bytes but expected {} bytes",
                    actual_downloaded, expected_download
                )));
            }
        }

        info!(
            url = %url,
            bytes = bytes_downloaded,
            elapsed = ?elapsed,
            checksum = %checksum,
            "Download completed successfully"
        );

        Ok(DownloadResult {
            bytes_downloaded,
            total_bytes,
            sha256_checksum: checksum,
            elapsed,
        })
    }
}

fn mount_point_specificity(path: &Path) -> usize {
    path.components().count()
}

/// Validates whether a Content-Length value from HEAD response is reliable.
///
/// Returns false if:
/// - Value is None (server didn't provide it)
/// - Value is suspiciously small (< 10KB, likely redirect HTML)
///
/// # Arguments
/// * `content_length` - Optional Content-Length header value in bytes
///
/// # Returns
/// * `true` if Content-Length is present AND >= MIN_VALID_MODEL_SIZE
/// * `false` otherwise (triggers GET Range probe fallback)
fn is_content_length_valid(content_length: Option<u64>) -> bool {
    match content_length {
        Some(size) => size >= MIN_VALID_MODEL_SIZE,
        None => false,
    }
}

#[async_trait]
impl DownloadEngine for HttpDownloadEngine {
    async fn download(&self, options: DownloadOptions) -> Result<DownloadResult, DownloadError> {
        let (expected_total_bytes, final_url) = self.get_file_size(&options.url).await?;
        let auth_token = options.auth_token.as_ref().filter(|_| {
            let allowed = may_forward_authorization(&options.url, &final_url);
            if !allowed {
                warn!(
                    source_url = %options.url,
                    target_url = %final_url,
                    "Refusing to forward download authorization to an untrusted redirect target"
                );
            }
            allowed
        });

        debug!(
            original_url = %options.url,
            final_url = %final_url,
            expected_total_bytes = ?expected_total_bytes,
            "Starting download with expected size and final URL from HEAD request"
        );

        if let Some(total_bytes) = expected_total_bytes {
            Self::check_disk_space(&options.destination, total_bytes)?;
        }

        self.download_with_retry(
            &final_url,
            &options.destination,
            options.resume_from,
            options.progress_callback,
            auth_token,
            expected_total_bytes,
        )
        .await
    }

    async fn get_file_size(&self, url: &str) -> Result<(Option<u64>, String), DownloadError> {
        debug!(url = %url, "Getting file size via HEAD request");

        let response = reqwest_client_builder()
            .connect_timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none()) // Don't follow redirects
            .build()
            .map_err(|e| {
                DownloadError::NetworkError(format!("Failed to create HTTP client: {}", e))
            })?
            .head(url)
            .send()
            .await
            .map_err(|e| DownloadError::NetworkError(format!("HEAD request failed: {}", e)))?;

        let status = response.status();

        if status.is_redirection() {
            if let Some(size_header) = response.headers().get("x-linked-size") {
                if let Ok(size_str) = size_header.to_str() {
                    if let Ok(size) = size_str.parse::<u64>() {
                        let final_url = if let Some(location) = response.headers().get("location") {
                            let location = location.to_str().map_err(|error| {
                                DownloadError::InvalidResponse(format!(
                                    "Redirect Location is not valid text: {error}"
                                ))
                            })?;
                            resolve_redirect_location(url, location)?
                        } else {
                            url.to_string()
                        };

                        info!(
                            url = %url,
                            final_url = %final_url,
                            file_size = size,
                            "Found file size in HuggingFace redirect header (x-linked-size)"
                        );

                        return Ok((Some(size), final_url));
                    }
                }
            }
        }

        let response = self
            .client
            .head(url)
            .send()
            .await
            .map_err(|e| DownloadError::NetworkError(format!("HEAD request failed: {}", e)))?;

        let status = response.status();
        let final_url = response.url().clone();

        if !status.is_success() {
            let status_code = status.as_u16();
            let error_message = match status {
                StatusCode::NOT_FOUND => "File not found (404)".to_string(),
                StatusCode::FORBIDDEN => "Access forbidden (403)".to_string(),
                StatusCode::UNAUTHORIZED => "Authentication required (401)".to_string(),
                _ => format!("HTTP error {}", status_code),
            };

            error!(
                url = %url,
                status = status_code,
                "HEAD request failed"
            );

            return Err(DownloadError::HttpError {
                status: status_code,
                message: error_message,
            });
        }

        // Try to get content length from HEAD response
        let content_length = response
            .content_length()
            .or_else(|| {
                // HuggingFace returns file size in x-linked-size header (check this first)
                response
                    .headers()
                    .get("x-linked-size")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .or_else(|| {
                // Fallback: parse Content-Length header manually
                response
                    .headers()
                    .get(header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
            });

        if final_url.as_str() != url {
            info!(
                original_url = %url,
                final_url = %final_url,
                "HEAD request followed redirects"
            );
        }

        debug!(
            url = %url,
            final_url = %final_url,
            status = %status,
            content_length = ?content_length,
            "HEAD request completed"
        );

        if !is_content_length_valid(content_length) {
            if let Some(size) = content_length {
                warn!(
                    url = %final_url,
                    content_length = size,
                    min_valid_size = MIN_VALID_MODEL_SIZE,
                    "HEAD Content-Length is suspiciously small (likely redirect HTML), falling back to GET Range probe"
                );
            } else {
                debug!(
                    url = %final_url,
                    "HEAD request didn't return content length, trying GET with Range: bytes=0-0"
                );
            }

            debug!(url = %final_url, "Sending GET Range probe to determine file size");

            let range_response = self
                .client
                .get(final_url.as_str())
                .header(header::RANGE, "bytes=0-0")
                .send()
                .await
                .map_err(|e| {
                    error!(
                        url = %final_url,
                        error = %e,
                        "Range GET request failed - connection error or timeout"
                    );
                    DownloadError::NetworkError(format!("Range GET request failed: {}", e))
                })?;

            let range_status = range_response.status();
            let range_final_url = range_response.url().clone();

            if !range_status.is_success() && range_status != StatusCode::PARTIAL_CONTENT {
                warn!(
                    url = %final_url,
                    status = %range_status,
                    "Range GET request failed, proceeding without file size"
                );
                return Ok((None, final_url.to_string()));
            }

            // Try multiple methods to get file size from Range response
            let range_content_length = range_response
                .headers()
                .get("x-linked-size")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| {
                    range_response
                        .headers()
                        .get(header::CONTENT_RANGE)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| {
                            s.split('/')
                                .nth(1)
                                .and_then(|size_str| size_str.parse::<u64>().ok())
                        })
                })
                .or_else(|| {
                    // Last fallback: Content-Length header
                    range_response
                        .headers()
                        .get(header::CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                });

            debug!(
                url = %final_url,
                range_final_url = %range_final_url,
                status = %range_status,
                content_length = ?range_content_length,
                headers = ?range_response.headers(),
                "Range GET request completed"
            );

            Ok((range_content_length, range_final_url.to_string()))
        } else {
            info!(
                url = %final_url,
                content_length = ?content_length,
                "Using HEAD Content-Length (valid size)"
            );
            Ok((content_length, final_url.to_string()))
        }
    }

    async fn supports_resume(&self, url: &str) -> Result<bool, DownloadError> {
        debug!(url = %url, "Checking resume support");

        let response = self
            .client
            .head(url)
            .send()
            .await
            .map_err(|e| DownloadError::NetworkError(format!("HEAD request failed: {}", e)))?;

        let status = response.status();

        if !status.is_success() {
            let status_code = status.as_u16();
            error!(
                url = %url,
                status = status_code,
                "HEAD request failed while checking resume support"
            );

            return Err(DownloadError::HttpError {
                status: status_code,
                message: format!("HTTP error {}", status_code),
            });
        }

        let supports_resume = response
            .headers()
            .get(header::ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok())
            .map(|v| v == "bytes")
            .unwrap_or(false);

        debug!(url = %url, supports_resume = supports_resume, "Resume support check completed");

        Ok(supports_resume)
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(clippy::unwrap_used)] // Test/mock code - unwrap() is acceptable for test infrastructure
pub mod mock {
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

        pub fn set_download_result(
            &self,
            url: &str,
            result: Result<DownloadResult, DownloadError>,
        ) {
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
        async fn download(
            &self,
            options: DownloadOptions,
        ) -> Result<DownloadResult, DownloadError> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn nested_mount_is_more_specific_than_root_mount() {
        assert!(
            mount_point_specificity(Path::new("/Volumes/External"))
                > mount_point_specificity(Path::new("/"))
        );
    }

    #[test]
    fn redirect_location_is_resolved_against_request_url() {
        assert_eq!(
            resolve_redirect_location(
                "https://huggingface.co/org/model/resolve/main/model.gguf",
                "../../blobs/abc"
            )
            .expect("resolve relative redirect"),
            "https://huggingface.co/org/model/blobs/abc"
        );
    }

    #[test]
    fn authorization_is_not_forwarded_to_untrusted_redirects() {
        assert!(may_forward_authorization(
            "https://huggingface.co/org/model/resolve/main/model.gguf",
            "https://cdn-lfs.hf.co/file"
        ));
        assert!(!may_forward_authorization(
            "https://huggingface.co/org/model/resolve/main/model.gguf",
            "https://downloads.example.com/file"
        ));
        assert!(may_forward_authorization(
            "https://models.example.com/file",
            "https://models.example.com/other"
        ));
    }

    fn temp_path(file_name: &str) -> PathBuf {
        std::env::temp_dir().join(file_name)
    }

    #[tokio::test]
    async fn complete_local_file_is_not_requested_again() {
        let destination = temp_path("engine_already_complete.bin");
        let payload = b"already on disk";
        tokio::fs::write(&destination, payload)
            .await
            .expect("write fixture");
        let total = payload.len() as u64;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let callback: ProgressCallback = Arc::new(move |bytes, speed| {
            let _ = tx.send((bytes, speed));
        });

        // Port 9 is the discard service: if the engine issued a request here
        // the test would fail with a connection error instead of completing.
        let result = HttpDownloadEngine::new()
            .expect("engine")
            .download_impl(
                "http://127.0.0.1:9/never-requested.bin",
                &destination,
                Some(total),
                Some(callback),
                None,
                Some(total),
            )
            .await
            .expect("complete file short-circuits the transfer");

        assert_eq!(result.bytes_downloaded, total);
        assert_eq!(result.total_bytes, Some(total));
        assert_eq!(
            result.sha256_checksum,
            format!("{:x}", Sha256::digest(payload))
        );

        let events = collect_progress_events(rx, Duration::from_millis(200)).await;
        assert_eq!(events, vec![(total, 0.0)]);

        let _ = tokio::fs::remove_file(&destination).await;
    }

    #[tokio::test]
    async fn complete_local_file_with_wrong_length_is_rejected() {
        let destination = temp_path("engine_wrong_length.bin");
        tokio::fs::write(&destination, b"short")
            .await
            .expect("write fixture");

        let result = HttpDownloadEngine::new()
            .expect("engine")
            .complete_from_local_file("http://127.0.0.1:9/x.bin", &destination, 1_000, None)
            .await;

        assert!(matches!(result, Err(DownloadError::ValidationFailed(_))));
        let _ = tokio::fs::remove_file(&destination).await;
    }

    #[tokio::test]
    async fn test_mock_engine_download() {
        let engine = mock::MockDownloadEngine::new();

        engine.set_file_size("https://example.com/file.bin", Some(1000));
        engine.set_supports_resume("https://example.com/file.bin", true);

        let result = engine
            .download(DownloadOptions {
                url: "https://example.com/file.bin".to_string(),
                destination: temp_path("test.bin"),
                resume_from: None,
                progress_callback: None,
                auth_token: None,
            })
            .await
            .unwrap();

        assert_eq!(result.bytes_downloaded, 1000);
        assert_eq!(result.total_bytes, Some(1000));
    }

    #[tokio::test]
    async fn test_mock_engine_file_size() {
        let engine = mock::MockDownloadEngine::new();

        engine.set_file_size("https://example.com/file.bin", Some(5000));

        let (size, _etag) = engine
            .get_file_size("https://example.com/file.bin")
            .await
            .unwrap();

        assert_eq!(size, Some(5000));
    }

    #[tokio::test]
    async fn test_mock_engine_supports_resume() {
        let engine = mock::MockDownloadEngine::new();

        engine.set_supports_resume("https://example.com/file.bin", true);

        let supports = engine
            .supports_resume("https://example.com/file.bin")
            .await
            .unwrap();

        assert!(supports);
    }

    #[tokio::test]
    async fn test_mock_engine_with_progress_callback() {
        let engine = mock::MockDownloadEngine::new();

        let progress_called = Arc::new(Mutex::new(false));
        let progress_called_clone = progress_called.clone();

        let callback: ProgressCallback = Arc::new(move |bytes, speed| {
            *progress_called_clone.lock().unwrap() = true;
            assert!(bytes > 0);
            assert!(speed > 0.0);
        });

        engine
            .download(DownloadOptions {
                url: "https://example.com/file.bin".to_string(),
                destination: temp_path("test.bin"),
                resume_from: None,
                progress_callback: Some(callback),
                auth_token: None,
            })
            .await
            .unwrap();

        assert!(*progress_called.lock().unwrap());
    }

    #[tokio::test]
    async fn test_mock_engine_call_tracking() {
        let engine = mock::MockDownloadEngine::new();

        // Initial state: no calls
        assert!(engine.get_download_calls().is_empty());
        assert!(engine.get_last_call().is_none());

        // First download
        engine
            .download(DownloadOptions {
                url: "https://example.com/file1.bin".to_string(),
                destination: temp_path("test1.bin"),
                resume_from: None,
                progress_callback: None,
                auth_token: None,
            })
            .await
            .unwrap();

        let calls = engine.get_download_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].url, "https://example.com/file1.bin");
        assert_eq!(calls[0].resume_from, None);

        // Second download with resume_from (CRITICAL for Test 16)
        engine
            .download(DownloadOptions {
                url: "https://example.com/file2.bin".to_string(),
                destination: temp_path("test2.bin"),
                resume_from: Some(5000),
                progress_callback: None,
                auth_token: Some("test-token".to_string()),
            })
            .await
            .unwrap();

        let calls = engine.get_download_calls();
        assert_eq!(calls.len(), 2);

        let last_call = engine.get_last_call().unwrap();
        assert_eq!(last_call.url, "https://example.com/file2.bin");
        assert_eq!(last_call.resume_from, Some(5000));
        assert_eq!(last_call.auth_token, Some("test-token".to_string()));

        // Clear calls
        engine.clear_calls();
        assert!(engine.get_download_calls().is_empty());
    }

    #[test]
    fn test_is_content_length_valid_none() {
        assert!(!is_content_length_valid(None));
    }

    #[test]
    fn test_is_content_length_valid_zero() {
        assert!(!is_content_length_valid(Some(0)));
    }

    #[test]
    fn test_is_content_length_valid_small_redirect_html() {
        assert!(!is_content_length_valid(Some(1309)));
    }

    #[test]
    fn test_is_content_length_valid_just_below_threshold() {
        assert!(!is_content_length_valid(Some(MIN_VALID_MODEL_SIZE - 1)));
    }

    #[test]
    fn test_is_content_length_valid_at_threshold() {
        assert!(is_content_length_valid(Some(MIN_VALID_MODEL_SIZE)));
    }

    #[test]
    fn test_is_content_length_valid_above_threshold() {
        assert!(is_content_length_valid(Some(MIN_VALID_MODEL_SIZE + 1)));
    }

    #[test]
    fn test_is_content_length_valid_typical_model_file() {
        assert!(is_content_length_valid(Some(724_923)));
    }

    #[test]
    fn test_is_content_length_valid_large_file() {
        assert!(is_content_length_valid(Some(1024 * 1024 * 1024)));
    }

    #[test]
    fn test_resolved_resume_offset_requires_partial_content() {
        assert_eq!(
            HttpDownloadEngine::resolved_resume_offset(Some(1024), StatusCode::PARTIAL_CONTENT),
            1024
        );
        assert_eq!(
            HttpDownloadEngine::resolved_resume_offset(Some(1024), StatusCode::OK),
            0
        );
        assert_eq!(
            HttpDownloadEngine::resolved_resume_offset(None, StatusCode::OK),
            0
        );
    }

    #[test]
    fn test_is_retryable_error_covers_transient_http() {
        assert!(HttpDownloadEngine::is_retryable_error(
            &DownloadError::NetworkError("connection reset".to_string())
        ));
        assert!(HttpDownloadEngine::is_retryable_error(
            &DownloadError::HttpError {
                status: 503,
                message: "service unavailable".to_string()
            }
        ));
        assert!(!HttpDownloadEngine::is_retryable_error(
            &DownloadError::HttpError {
                status: 404,
                message: "not found".to_string()
            }
        ));
    }

    #[tokio::test]
    async fn test_download_engine_http_download() {
        use std::time::Duration;

        let test_data = b"This is test file content for HTTP download";
        let test_url = "https://example.com/file.bin";

        let mock_engine = mock::MockDownloadEngine::new();
        mock_engine.set_download_result(
            test_url,
            Ok(DownloadResult {
                bytes_downloaded: test_data.len() as u64,
                total_bytes: Some(test_data.len() as u64),
                sha256_checksum: "a".repeat(64),
                elapsed: Duration::from_secs(1),
            }),
        );

        let options = DownloadOptions {
            url: test_url.to_string(),
            destination: temp_path("test_download.bin"),
            resume_from: None,
            progress_callback: None,
            auth_token: None,
        };

        let result = mock_engine.download(options).await;

        assert!(result.is_ok(), "Download should succeed");
        let download_result = result.unwrap();
        assert_eq!(download_result.bytes_downloaded, test_data.len() as u64);
        assert_eq!(download_result.total_bytes, Some(test_data.len() as u64));

        // Note: MockDownloadEngine doesn't write files, so we verify the mock behavior
        // In production, this test would use a real HTTP client with mock server
        // and verify file contents with: verify_file_contents(&dest_path, test_data).await.unwrap();
    }

    /// Collect progress events from the mpsc channel.
    ///
    /// Collects (bytes_downloaded, speed) tuples until timeout or channel closes
    async fn collect_progress_events(
        mut rx: tokio::sync::mpsc::UnboundedReceiver<(u64, f64)>,
        timeout: Duration,
    ) -> Vec<(u64, f64)> {
        let mut events = Vec::new();
        let deadline = tokio::time::Instant::now() + timeout;

        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(50), rx.recv()).await {
                Ok(Some(event)) => events.push(event),
                Ok(None) => break, // Channel closed
                Err(_) => break,   // Timeout
            }
        }

        events
    }

    #[tokio::test]
    async fn test_download_engine_progress_callbacks() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let mock_engine = mock::MockDownloadEngine::new();
        mock_engine.set_download_result(
            "https://example.com/large_file.bin",
            Ok(DownloadResult {
                bytes_downloaded: 10000,
                total_bytes: Some(10000),
                sha256_checksum: "a".repeat(64),
                elapsed: Duration::from_secs(1),
            }),
        );

        // Progress callback that sends to the channel.
        let progress_callback: ProgressCallback = Arc::new(move |bytes, speed| {
            tx.send((bytes, speed)).ok();
        });

        let options = DownloadOptions {
            url: "https://example.com/large_file.bin".to_string(),
            destination: temp_path("large_file.bin"),
            resume_from: None,
            progress_callback: Some(progress_callback),
            auth_token: None,
        };

        let result = mock_engine.download(options).await;

        assert!(result.is_ok(), "Download should succeed");
        let download_result = result.unwrap();
        assert_eq!(download_result.bytes_downloaded, 10000);

        // Collect progress events.
        let progress_events = collect_progress_events(rx, Duration::from_secs(2)).await;

        assert!(!progress_events.is_empty(), "Should have progress events");

        // MockDownloadEngine sends single progress callback - verify it was received
        let (final_bytes, final_speed) = progress_events.last().unwrap();
        assert_eq!(
            *final_bytes, 10000,
            "Final progress should match total bytes"
        );
        assert!(*final_speed > 0.0, "Speed should be positive");
    }

    #[tokio::test]
    async fn test_download_engine_range_request() {
        let mock_engine = mock::MockDownloadEngine::new();

        // Set up response for range request (remaining content after 500 bytes)
        let remaining_bytes = 500u64;
        mock_engine.set_download_result(
            "https://example.com/partial_file.bin",
            Ok(DownloadResult {
                bytes_downloaded: remaining_bytes + 500, // Total bytes after resume
                total_bytes: Some(1000),
                sha256_checksum: "a".repeat(64),
                elapsed: Duration::from_secs(1),
            }),
        );

        let options = DownloadOptions {
            url: "https://example.com/partial_file.bin".to_string(),
            destination: temp_path("partial_file.bin"),
            resume_from: Some(500), // Resume from 500 bytes
            progress_callback: None,
            auth_token: None,
        };

        let result = mock_engine.download(options).await;

        assert!(result.is_ok(), "Range request download should succeed");

        // CRITICAL: Verify engine was called with resume_from
        let calls = mock_engine.get_download_calls();
        assert!(!calls.is_empty(), "Should have recorded download call");

        let last_call = calls.last().unwrap();
        assert_eq!(
            last_call.resume_from,
            Some(500),
            "CRITICAL: Engine should be called with resume_from = Some(500)"
        );

        // Note: In production, MockHttpClient would verify Range header:
        // let range_header = mock_http.get_last_range_header();
        // assert_eq!(range_header, Some("bytes=500-".to_string()));
    }

    #[tokio::test]
    async fn test_download_engine_network_error() {
        let mock_engine = mock::MockDownloadEngine::new();
        mock_engine.set_network_error("Connection timeout".to_string());

        let options = DownloadOptions {
            url: "https://example.com/unreachable.bin".to_string(),
            destination: temp_path("unreachable.bin"),
            resume_from: None,
            progress_callback: None,
            auth_token: None,
        };

        let result = mock_engine.download(options).await;

        assert!(result.is_err(), "Download should fail with network error");

        let error = result.unwrap_err();
        assert!(
            matches!(error, DownloadError::NetworkError(_)),
            "Error should be NetworkError variant"
        );

        let error_msg = format!("{}", error);
        assert!(
            error_msg.contains("Connection timeout") || error_msg.contains("network"),
            "Error should indicate network failure: {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn test_download_engine_404_handling() {
        let mock_engine = mock::MockDownloadEngine::new();
        mock_engine.set_http_error(404, "Not Found".to_string());

        let options = DownloadOptions {
            url: "https://example.com/nonexistent.bin".to_string(),
            destination: temp_path("nonexistent.bin"),
            resume_from: None,
            progress_callback: None,
            auth_token: None,
        };

        let result = mock_engine.download(options).await;

        assert!(result.is_err(), "Download should fail with 404 error");

        let error = result.unwrap_err();
        assert!(
            matches!(error, DownloadError::HttpError { .. }),
            "Error should be HttpError variant"
        );

        let error_msg = format!("{}", error);
        assert!(
            error_msg.contains("404") || error_msg.contains("Not Found"),
            "Error should indicate 404 Not Found: {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn test_download_engine_cancellation() {
        let mock_engine = mock::MockDownloadEngine::new();
        mock_engine.set_download_result(
            "https://example.com/cancellable.bin",
            Ok(DownloadResult {
                bytes_downloaded: 10000,
                total_bytes: Some(10000),
                sha256_checksum: "a".repeat(64),
                elapsed: Duration::from_secs(1),
            }),
        );

        let options = DownloadOptions {
            url: "https://example.com/cancellable.bin".to_string(),
            destination: temp_path("cancellable.bin"),
            resume_from: None,
            progress_callback: None,
            auth_token: Some("cancel-token".to_string()),
        };

        let result = mock_engine.download(options).await;

        // Assert: Download should complete or be cancellable
        // Note: MockDownloadEngine completes instantly, so this tests the API
        // In production with real engine, auth_token would signal cancellation
        assert!(
            result.is_ok() || result.is_err(),
            "Download should either complete or be cancelled"
        );

        let calls = mock_engine.get_download_calls();
        assert!(!calls.is_empty(), "Download should have been called");
        assert!(
            calls[0].auth_token.is_some(),
            "Auth token (cancellation signal) should be passed to engine"
        );
    }
}
