// Downloads the reranker GGUF from its pinned commit, with resume, retry and
// checksum verification, and reports progress through Tauri events.

use crate::infrastructure::file_system::compute_file_sha256_async;
use crate::infrastructure::services::traits::ModelManagerTrait;
use async_trait::async_trait;
use lattice_core::domain::models::curated::CURATED_RERANKER;
use lattice_core::shared::error::WithCauses;
use lattice_core::shared::error::{AppError, Result, ResultExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use tauri::Emitter;
use tokio::fs;
use tokio::io::AsyncWriteExt;

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub percentage: f64,
    pub speed_mbps: f64,
    pub eta_seconds: u64,
    pub current_file: String,
}

// Download event payloads matching shared schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgressPayload {
    pub id: String,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: f64,
    pub percentage: Option<f64>,
    pub eta_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadCompletedPayload {
    pub id: String,
    pub total_bytes_completed: Option<u64>,
    pub elapsed_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadFailedPayload {
    pub id: String,
    pub error: String,
}

pub struct ModelManager {
    model_dir: PathBuf,
    client: Client,
    bytes_downloaded: Arc<AtomicU64>,
    download_current_bytes: Arc<AtomicU64>,
    download_total_bytes: Arc<AtomicU64>,
    is_cancelled: Arc<AtomicBool>,
    app_handle: Option<tauri::AppHandle>,
    download_start_time: Arc<Mutex<Option<std::time::Instant>>>,
}

impl ModelManager {
    pub fn new(model_dir: PathBuf) -> Result<Self> {
        let timeout = std::time::Duration::from_secs(300);
        let client = lattice_core::shared::http::reqwest_client_builder()
            .timeout(timeout)
            .build()
            .map_err(|e| {
                AppError::Network(format!("Failed to create HTTP client: {}", WithCauses(&e)))
            })?;

        Ok(Self {
            model_dir,
            client,
            bytes_downloaded: Arc::new(AtomicU64::new(0)),
            download_current_bytes: Arc::new(AtomicU64::new(0)),
            download_total_bytes: Arc::new(AtomicU64::new(0)),
            is_cancelled: Arc::new(AtomicBool::new(false)),
            app_handle: None,
            download_start_time: Arc::new(Mutex::new(None)),
        })
    }

    pub fn with_app_handle(mut self, app_handle: tauri::AppHandle) -> Self {
        self.app_handle = Some(app_handle);
        self
    }

    fn emit_progress(&self, model_id: &str, progress: DownloadProgress) {
        if let Some(handle) = &self.app_handle {
            let bytes_per_second = progress.speed_mbps * 1_048_576.0;

            let payload = DownloadProgressPayload {
                id: model_id.to_string(),
                bytes_downloaded: progress.bytes_downloaded,
                total_bytes: if progress.total_bytes > 0 {
                    Some(progress.total_bytes)
                } else {
                    None
                },
                bytes_per_second,
                percentage: Some(progress.percentage),
                eta_seconds: Some(progress.eta_seconds),
            };

            let _ = handle.emit_to(tauri::EventTarget::Any, "download:progress", payload);
        }
    }

    fn emit_complete(&self, model_id: &str) {
        if let Some(handle) = &self.app_handle {
            let elapsed_seconds = self
                .download_start_time
                .lock()
                .ok()
                .and_then(|guard| guard.as_ref().map(|start| start.elapsed().as_secs()));

            let total_bytes = self.bytes_downloaded.load(Ordering::Relaxed);

            let payload = DownloadCompletedPayload {
                id: model_id.to_string(),
                total_bytes_completed: Some(total_bytes),
                elapsed_seconds,
            };

            let _ = handle.emit_to(tauri::EventTarget::Any, "download:completed", payload);
        }
    }

    fn emit_error(&self, model_id: &str, error: &str) {
        if let Some(handle) = &self.app_handle {
            let payload = DownloadFailedPayload {
                id: model_id.to_string(),
                error: error.to_string(),
            };

            let _ = handle.emit_to(tauri::EventTarget::Any, "download:failed", payload);
        }
    }

    fn reset_download_progress(&self) {
        self.download_current_bytes.store(0, Ordering::Relaxed);
        self.download_total_bytes.store(0, Ordering::Relaxed);
    }

    async fn download_with_retry(
        &self,
        url: &str,
        target_path: &Path,
        filename: &str,
        expected_checksum: Option<&str>,
        model_id: &str,
    ) -> Result<()> {
        let mut attempts = 0;
        let mut last_error = None;

        while attempts < MAX_RETRIES {
            match self
                .download_file(url, target_path, filename, model_id)
                .await
            {
                Ok(_) => {
                    if let Some(checksum) = expected_checksum {
                        if self.verify_checksum(target_path, checksum).await? {
                            tracing::info!("Checksum verified for {}", filename);
                            return Ok(());
                        } else {
                            tracing::warn!("Checksum mismatch for {}, retrying...", filename);
                            let _ = fs::remove_file(target_path).await;
                            attempts += 1;
                            tokio::time::sleep(std::time::Duration::from_millis(
                                RETRY_DELAY_MS * 2_u64.pow(attempts),
                            ))
                            .await;
                            continue;
                        }
                    }
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!("Download attempt {} failed: {}", attempts + 1, e);
                    last_error = Some(e);
                    attempts += 1;

                    if attempts < MAX_RETRIES {
                        let delay = RETRY_DELAY_MS * 2_u64.pow(attempts);
                        tracing::info!("Retrying in {} ms...", delay);
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    }
                }
            }
        }

        self.reset_download_progress();
        Err(last_error.unwrap_or_else(|| AppError::Network("Max retries exceeded".to_string())))
    }

    async fn download_file(
        &self,
        url: &str,
        target_path: &Path,
        filename: &str,
        model_id: &str,
    ) -> Result<()> {
        let temp_path = target_path.with_extension("tmp");

        let mut downloaded: u64 = 0;
        let resume_supported = if temp_path.exists() {
            if let Ok(metadata) = fs::metadata(&temp_path).await {
                downloaded = metadata.len();
                tracing::info!(
                    "Found partial download of {} ({} MB), attempting to resume",
                    filename,
                    downloaded / 1_048_576
                );
                true
            } else {
                false
            }
        } else {
            false
        };

        let mut request = self.client.get(url);

        if resume_supported && downloaded > 0 {
            request = request.header("Range", format!("bytes={}-", downloaded));
        }

        let response = request
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Network error: Failed to connect to download server for {}. Please check your internet connection. Error: {}", filename, WithCauses(&e))))?;

        let status = response.status();
        if !status.is_success() && status.as_u16() != 206 {
            return Err(AppError::Network(format!(
                "Download failed for {}: Server returned status {}. The model file may be temporarily unavailable.",
                filename,
                status
            )));
        }

        // If server doesn't support resume (200 instead of 206), start fresh
        if status.as_u16() == 200 && downloaded > 0 {
            tracing::info!("Server doesn't support resume, starting download from beginning");
            downloaded = 0;
        }

        let total_size = if status.as_u16() == 206 {
            // For partial content, we need to add what we already have
            response.content_length().unwrap_or(0) + downloaded
        } else {
            response.content_length().unwrap_or(0)
        };

        self.download_total_bytes
            .store(total_size, Ordering::Relaxed);
        self.download_current_bytes
            .store(downloaded, Ordering::Relaxed);

        tracing::info!(
            "Downloading {} ({}/{} MB)",
            filename,
            downloaded / 1_048_576,
            total_size / 1_048_576
        );

        let mut file = if resume_supported && downloaded > 0 && status.as_u16() == 206 {
            // Append to existing file
            fs::OpenOptions::new()
                .write(true)
                .append(true)
                .open(&temp_path)
                .await
                .context(format!(
                    "Failed to open partial file for {}. Starting fresh download.",
                    filename
                ))?
        } else {
            fs::File::create(&temp_path)
                .await
                .context(format!(
                    "Failed to create file for {}. Check that the directory is writable and you have sufficient permissions.",
                    filename
                ))?
        };

        let initial_downloaded = downloaded;
        let mut stream = response.bytes_stream();
        let start_time = std::time::Instant::now();

        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            if self.is_cancelled.load(Ordering::Relaxed) {
                let _ = fs::remove_file(&temp_path).await;
                return Err(AppError::Other("Download cancelled".to_string()));
            }

            let chunk = chunk.map_err(|e| {
                AppError::Network(format!(
                    "Network error while downloading {}: Connection interrupted. Error: {}",
                    filename,
                    WithCauses(&e)
                ))
            })?;
            file.write_all(&chunk).await.context(format!(
                "Disk write error for {}: Failed to write data. Check available disk space.",
                filename
            ))?;

            downloaded += chunk.len() as u64;
            self.bytes_downloaded
                .fetch_add(chunk.len() as u64, Ordering::Relaxed);
            self.download_current_bytes
                .store(downloaded, Ordering::Relaxed);

            let elapsed = start_time.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                let bytes_in_session = downloaded - initial_downloaded;
                let speed_bps = bytes_in_session as f64 / elapsed;
                let speed_mbps = speed_bps / 1_048_576.0;
                let percentage = if total_size > 0 {
                    (downloaded as f64 / total_size as f64) * 100.0
                } else {
                    0.0
                };
                let eta = if speed_bps > 0.0 && total_size > 0 {
                    ((total_size - downloaded) as f64 / speed_bps) as u64
                } else {
                    0
                };

                self.emit_progress(
                    model_id,
                    DownloadProgress {
                        bytes_downloaded: downloaded,
                        total_bytes: total_size,
                        percentage,
                        speed_mbps,
                        eta_seconds: eta,
                        current_file: filename.to_string(),
                    },
                );

                tracing::debug!(
                    "{}: {:.2}% ({}/{} MB) @ {:.2} MB/s, ETA: {}s",
                    filename,
                    percentage,
                    downloaded / 1_048_576,
                    total_size / 1_048_576,
                    speed_mbps,
                    eta
                );
            }
        }

        file.flush()
            .await
            .context(format!("Failed to finalize {}: Disk write error", filename))?;
        drop(file);

        fs::rename(&temp_path, target_path).await.context(format!(
            "Failed to finalize {}: Could not move temporary file to final location",
            filename
        ))?;

        tracing::info!("Successfully downloaded {}", filename);
        Ok(())
    }

    async fn verify_checksum(&self, file_path: &Path, expected: &str) -> Result<bool> {
        let hash_hex = compute_file_sha256_async(file_path).await?;
        Ok(hash_hex == expected)
    }

    pub fn cancel_download(&self) {
        self.is_cancelled.store(true, Ordering::Relaxed);
    }

    /// Download the reranker GGUF from its pinned commit unless it is already
    /// here, and verify its SHA-256.
    pub async fn ensure_reranker_available(&self) -> Result<PathBuf> {
        let gguf = self.get_reranker_path();
        if self.is_reranker_ready().await {
            tracing::info!("Reranker model already exists at {:?}", gguf);
            return Ok(gguf);
        }

        tracing::info!("Reranker model not found, initiating download...");
        match self.download_reranker_model(&gguf).await {
            Ok(()) => {
                self.emit_complete("qwen3-reranker-0.6b");
                self.reset_download_progress();
                Ok(gguf)
            }
            Err(e) => {
                self.emit_error("qwen3-reranker-0.6b", &e.to_string());
                self.reset_download_progress();
                Err(e)
            }
        }
    }

    async fn download_reranker_model(&self, gguf: &Path) -> Result<()> {
        if let Some(dir) = gguf.parent() {
            fs::create_dir_all(dir)
                .await
                .context("Failed to create reranker directory")?;
        }
        if let Ok(mut guard) = self.download_start_time.lock() {
            *guard = Some(std::time::Instant::now());
        }
        tracing::info!("Downloading reranker {}...", CURATED_RERANKER.file);
        // A file of another size is a stale or foreign one; the download
        // finishes by renaming over it.
        self.download_with_retry(
            &CURATED_RERANKER.url(),
            gguf,
            CURATED_RERANKER.file,
            Some(CURATED_RERANKER.sha256),
            "qwen3-reranker-0.6b",
        )
        .await?;

        if self.is_cancelled.load(Ordering::Relaxed) {
            return Err(AppError::Other("Download cancelled by user".to_string()));
        }
        Ok(())
    }

    pub fn get_reranker_path(&self) -> PathBuf {
        CURATED_RERANKER.path_in(&self.model_dir)
    }

    /// The reranker GGUF is here at its pinned size. Its checksum was
    /// verified when it was downloaded.
    pub async fn is_reranker_ready(&self) -> bool {
        tokio::fs::metadata(self.get_reranker_path())
            .await
            .is_ok_and(|metadata| metadata.len() == CURATED_RERANKER.bytes)
    }
}

#[async_trait]
impl ModelManagerTrait for ModelManager {
    async fn ensure_reranker_available(&self) -> Result<PathBuf> {
        self.ensure_reranker_available().await
    }

    async fn is_reranker_ready(&self) -> bool {
        self.is_reranker_ready().await
    }

    fn get_reranker_path(&self) -> PathBuf {
        self.get_reranker_path()
    }

    fn cancel_download(&self) {
        self.cancel_download()
    }

    fn get_download_progress(&self) -> Option<f32> {
        let total = self.download_total_bytes.load(Ordering::Relaxed);
        if total == 0 {
            return None;
        }
        let current = self.download_current_bytes.load(Ordering::Relaxed);
        Some(current as f32 / total as f32)
    }
}

#[cfg(test)]
mod tests;
