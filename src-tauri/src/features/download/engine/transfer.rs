//! The HTTP transfer: ranged requests, retries, resume, and the checksum check.
use super::{DownloadResult, HttpDownloadEngine, ProgressCallback};
use crate::domain::download::DownloadError;
use reqwest::{header, Response, StatusCode};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, error, info, warn};

impl HttpDownloadEngine {
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
    pub(super) async fn complete_from_local_file(
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

    pub(super) async fn download_with_retry(
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

    pub(super) async fn download_impl(
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
