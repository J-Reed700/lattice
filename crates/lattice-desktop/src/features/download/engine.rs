use async_trait::async_trait;
use lattice_core::domain::download::DownloadError;
use lattice_core::shared::http::reqwest_client_builder;
use reqwest::{header, Client, StatusCode};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use sysinfo::Disks;
use tracing::{debug, error, info, warn};
use url::Url;

mod transfer;

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
            let remaining_bytes = total_bytes.saturating_sub(options.resume_from.unwrap_or(0));
            if remaining_bytes > 0 {
                Self::check_disk_space(&options.destination, remaining_bytes)?;
            }
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
pub mod mock;

#[cfg(test)]
mod tests;
