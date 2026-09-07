// BGE-M3 Model Manager
//
// This module handles downloading the BGE-M3 embedding model from HuggingFace.
//
// ## Architecture Note: Why Hardcoded URLs?
//
// While we have a domain-driven multi-file download infrastructure (see
// `domain/modules/model_metadata.rs` with `ModelFile` and `CuratedModel`), this manager
// uses hardcoded constants for BGE-M3. This is intentional and follows the
// "ruthless simplicity" principle:
//
// - **Current**: BGE-M3 is our only embedding model and requires 5 specific files
// - **Simple**: Direct URL constants are easy to understand and debug
// - **Working**: This approach is proven and tested
//
// The domain-driven infrastructure exists for:
// - Future model additions (when we support multiple embedding models)
// - User-provided models (when that feature is needed)
// - Complex model configurations (versioning, variants, etc.)
//
// ## BGE-M3 File Structure
//
// BGE-M3 ONNX model requires 5 files:
// 1. `model.onnx` - Main ONNX model graph (724 KB)
// 2. `model.onnx_data` - External tensor data (2.27 GB) **REQUIRED**
// 3. `tokenizer.json` - Tokenizer configuration
// 4. `vocab.txt` - Vocabulary file
// 5. `config.json` - Model configuration
//
// IMPORTANT: model.onnx_data is NOT optional - ONNX Runtime loads it automatically
// when the main model.onnx file references external data. Missing this file
// causes "failed to load model" errors.
//
// ## When to Use Domain Infrastructure
//
// Consider migrating to domain-driven approach when:
// - We support multiple embedding models (e.g., e5-large, instructor-xl)
// - Users can choose/add custom models
// - Models have complex version/variant management
// - We need dynamic model discovery from HuggingFace API
//
// Until then, keep it simple with hardcoded constants.

use crate::infrastructure::services::traits::ModelManagerTrait;
use crate::shared::error::{AppError, Result, ResultExt};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use tauri::Emitter;
use tokio::fs;
use tokio::io::AsyncWriteExt;

// BGE-M3 embedding model URLs
const MODEL_URL: &str = "https://huggingface.co/BAAI/bge-m3/resolve/main/onnx/model.onnx";
const MODEL_DATA_URL: &str = "https://huggingface.co/BAAI/bge-m3/resolve/main/onnx/model.onnx_data";
const TOKENIZER_URL: &str = "https://huggingface.co/BAAI/bge-m3/resolve/main/tokenizer.json";
const VOCAB_URL: &str = "https://huggingface.co/BAAI/bge-m3/resolve/main/vocab.txt";
const CONFIG_URL: &str = "https://huggingface.co/BAAI/bge-m3/resolve/main/config.json";

const RERANKER_MODEL_URL: &str =
    "https://huggingface.co/cross-encoder/ms-marco-MiniLM-L-6-v2/resolve/main/model.safetensors";
const RERANKER_TOKENIZER_URL: &str =
    "https://huggingface.co/cross-encoder/ms-marco-MiniLM-L-6-v2/resolve/main/tokenizer.json";
const RERANKER_CONFIG_URL: &str =
    "https://huggingface.co/cross-encoder/ms-marco-MiniLM-L-6-v2/resolve/main/config.json";

// NOTE: Update this checksum after downloading the actual model
// To calculate: sha256sum model.onnx or certUtil -hashfile model.onnx SHA256
const MODEL_CHECKSUM: Option<&str> = None; // Set to Some("actual_hash") when known
const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 1000;
const MIN_REQUIRED_SPACE_MB: u64 = 3000; // 3000MB minimum for BGE-M3 model + external data (2.27 GB total)

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
        let disable_system_proxy =
            cfg!(test) || std::env::var("RECALL_DISABLE_SYSTEM_PROXY").is_ok();

        if disable_system_proxy {
            let client = Client::builder()
                .timeout(timeout)
                .no_proxy()
                .build()
                .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

            return Ok(Self {
                model_dir,
                client,
                bytes_downloaded: Arc::new(AtomicU64::new(0)),
                download_current_bytes: Arc::new(AtomicU64::new(0)),
                download_total_bytes: Arc::new(AtomicU64::new(0)),
                is_cancelled: Arc::new(AtomicBool::new(false)),
                app_handle: None,
                download_start_time: Arc::new(Mutex::new(None)),
            });
        }

        let client = match std::panic::catch_unwind(|| Client::builder().timeout(timeout).build()) {
            Ok(Ok(client)) => client,
            Ok(Err(e)) => {
                tracing::warn!(
                    "Failed to create HTTP client with system proxy ({}); retrying without proxy",
                    e
                );
                Client::builder()
                    .timeout(timeout)
                    .no_proxy()
                    .build()
                    .map_err(|e| {
                        AppError::Network(format!("Failed to create HTTP client: {}", e))
                    })?
            }
            Err(_) => {
                tracing::warn!(
                    "HTTP client build panicked while resolving system proxy; retrying without proxy"
                );
                Client::builder()
                    .timeout(timeout)
                    .no_proxy()
                    .build()
                    .map_err(|e| {
                        AppError::Network(format!("Failed to create HTTP client: {}", e))
                    })?
            }
        };

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

    fn emit_progress(&self, progress: DownloadProgress) {
        if let Some(handle) = &self.app_handle {
            // Convert speed from MB/s to bytes/s for schema compliance
            let bytes_per_second = progress.speed_mbps * 1_048_576.0;

            let payload = DownloadProgressPayload {
                id: "bge-m3".to_string(),
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

    fn emit_complete(&self) {
        if let Some(handle) = &self.app_handle {
            let elapsed_seconds = self
                .download_start_time
                .lock()
                .ok()
                .and_then(|guard| guard.as_ref().map(|start| start.elapsed().as_secs()));

            let total_bytes = self.bytes_downloaded.load(Ordering::Relaxed);

            let payload = DownloadCompletedPayload {
                id: "bge-m3".to_string(),
                total_bytes_completed: Some(total_bytes),
                elapsed_seconds,
            };

            let _ = handle.emit_to(tauri::EventTarget::Any, "download:completed", payload);
        }
    }

    fn emit_error(&self, error: &str) {
        if let Some(handle) = &self.app_handle {
            let payload = DownloadFailedPayload {
                id: "bge-m3".to_string(),
                error: error.to_string(),
            };

            let _ = handle.emit_to(tauri::EventTarget::Any, "download:failed", payload);
        }
    }

    fn reset_download_progress(&self) {
        self.download_current_bytes.store(0, Ordering::Relaxed);
        self.download_total_bytes.store(0, Ordering::Relaxed);
    }

    pub async fn ensure_model_available(&self) -> Result<PathBuf> {
        let model_path = self.model_dir.join("model.onnx");
        let tokenizer_path = self.model_dir.join("tokenizer.json");

        if model_path.exists() && tokenizer_path.exists() {
            tracing::info!("Models already exist at {:?}", self.model_dir);
            return Ok(model_path);
        }

        tracing::info!("Models not found, initiating download...");
        match self.download_all_models().await {
            Ok(_) => {
                self.emit_complete();
                self.reset_download_progress();
                Ok(model_path)
            }
            Err(e) => {
                self.emit_error(&e.to_string());
                self.reset_download_progress();
                Err(e)
            }
        }
    }

    async fn download_all_models(&self) -> Result<()> {
        fs::create_dir_all(&self.model_dir)
            .await
            .context("Failed to create model directory")?;

        // Check available disk space
        self.check_disk_space().await?;

        // Track download start time for elapsed_seconds calculation
        if let Ok(mut guard) = self.download_start_time.lock() {
            *guard = Some(std::time::Instant::now());
        }

        // BGE-M3 requires these 5 files - hardcoded for simplicity
        // See module-level docs for when to migrate to domain-driven approach
        let files = vec![
            ("model.onnx", MODEL_URL, MODEL_CHECKSUM),
            ("model.onnx_data", MODEL_DATA_URL, None), // REQUIRED: 2.27 GB external data
            ("tokenizer.json", TOKENIZER_URL, None),
            ("vocab.txt", VOCAB_URL, None),
            ("config.json", CONFIG_URL, None),
        ];

        for (filename, url, checksum) in files {
            let target_path = self.model_dir.join(filename);

            if target_path.exists() {
                tracing::info!("{} already exists, skipping", filename);
                continue;
            }

            tracing::info!("Downloading {}...", filename);
            self.download_with_retry(url, &target_path, filename, checksum)
                .await?;

            if self.is_cancelled.load(Ordering::Relaxed) {
                return Err(AppError::Other("Download cancelled by user".to_string()));
            }
        }

        Ok(())
    }

    async fn download_with_retry(
        &self,
        url: &str,
        target_path: &Path,
        filename: &str,
        expected_checksum: Option<&str>,
    ) -> Result<()> {
        let mut attempts = 0;
        let mut last_error = None;

        while attempts < MAX_RETRIES {
            match self.download_file(url, target_path, filename).await {
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

    async fn download_file(&self, url: &str, target_path: &Path, filename: &str) -> Result<()> {
        let temp_path = target_path.with_extension("tmp");

        // Check for partial download and attempt to resume
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

        // Add Range header for resume support
        if resume_supported && downloaded > 0 {
            request = request.header("Range", format!("bytes={}-", downloaded));
        }

        let response = request
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Network error: Failed to connect to download server for {}. Please check your internet connection. Error: {}", filename, e)))?;

        // Check if resume is supported (206) or starting fresh (200)
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
            // Create new file
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
                    filename, e
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

                self.emit_progress(DownloadProgress {
                    bytes_downloaded: downloaded,
                    total_bytes: total_size,
                    percentage,
                    speed_mbps,
                    eta_seconds: eta,
                    current_file: filename.to_string(),
                });

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
        let contents = fs::read(file_path)
            .await
            .context("Failed to read file for checksum")?;

        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let hash = hasher.finalize();
        let hash_hex = hex::encode(hash);

        Ok(hash_hex == expected)
    }

    pub fn cancel_download(&self) {
        self.is_cancelled.store(true, Ordering::Relaxed);
    }

    pub fn get_model_path(&self) -> PathBuf {
        self.model_dir.join("model.onnx")
    }

    pub fn get_tokenizer_path(&self) -> PathBuf {
        self.model_dir.join("tokenizer.json")
    }

    pub async fn is_model_ready(&self) -> bool {
        let model_path = self.get_model_path();
        let tokenizer_path = self.get_tokenizer_path();

        tokio::fs::metadata(&model_path).await.is_ok()
            && tokio::fs::metadata(&tokenizer_path).await.is_ok()
    }

    pub async fn ensure_reranker_available(&self) -> Result<PathBuf> {
        let reranker_dir = self.model_dir.join("reranker");
        let model_path = reranker_dir.join("model.safetensors");
        let tokenizer_path = reranker_dir.join("tokenizer.json");
        let config_path = reranker_dir.join("config.json");

        if model_path.exists() && tokenizer_path.exists() && config_path.exists() {
            tracing::info!("Reranker model already exists at {:?}", reranker_dir);
            return Ok(model_path);
        }

        tracing::info!("Reranker model not found, initiating download...");
        match self.download_reranker_models().await {
            Ok(_) => {
                self.emit_complete();
                self.reset_download_progress();
                Ok(model_path)
            }
            Err(e) => {
                self.emit_error(&e.to_string());
                self.reset_download_progress();
                Err(e)
            }
        }
    }

    async fn download_reranker_models(&self) -> Result<()> {
        let reranker_dir = self.model_dir.join("reranker");
        fs::create_dir_all(&reranker_dir)
            .await
            .context("Failed to create reranker directory")?;

        // Sprint 5: candle's BertModel needs config.json at load time
        // (drives hidden_size, num_attention_heads, etc.). The old ORT
        // path embedded those in the .onnx graph, so we didn't need a
        // separate config download. Now we do.
        let files = vec![
            ("model.safetensors", RERANKER_MODEL_URL, None),
            ("tokenizer.json", RERANKER_TOKENIZER_URL, None),
            ("config.json", RERANKER_CONFIG_URL, None),
        ];

        for (filename, url, checksum) in files {
            let target_path = reranker_dir.join(filename);

            if target_path.exists() {
                tracing::info!("{} already exists, skipping", filename);
                continue;
            }

            tracing::info!("Downloading reranker {}...", filename);
            self.download_with_retry(url, &target_path, filename, checksum)
                .await?;

            if self.is_cancelled.load(Ordering::Relaxed) {
                return Err(AppError::Other("Download cancelled by user".to_string()));
            }
        }

        let legacy_onnx = reranker_dir.join("model.onnx");
        if legacy_onnx.exists() {
            tracing::info!("Pruning legacy reranker model.onnx after migration");
            if let Err(err) = fs::remove_file(&legacy_onnx).await {
                tracing::warn!("Failed to prune legacy model.onnx: {err}");
            }
        }

        Ok(())
    }

    pub fn get_reranker_path(&self) -> PathBuf {
        self.model_dir.join("reranker/model.safetensors")
    }

    pub fn get_reranker_tokenizer_path(&self) -> PathBuf {
        self.model_dir.join("reranker/tokenizer.json")
    }

    pub async fn is_reranker_ready(&self) -> bool {
        let model_path = self.get_reranker_path();
        let tokenizer_path = self.get_reranker_tokenizer_path();
        let config_path = self.model_dir.join("reranker/config.json");

        tokio::fs::metadata(&model_path).await.is_ok()
            && tokio::fs::metadata(&tokenizer_path).await.is_ok()
            && tokio::fs::metadata(&config_path).await.is_ok()
    }

    async fn check_disk_space(&self) -> Result<()> {
        let mut disks = sysinfo::Disks::new_with_refreshed_list();
        disks.refresh();

        let model_dir_canonical = match self.model_dir.canonicalize() {
            Ok(path) => path,
            Err(err) => {
                tracing::warn!(
                    "Failed to canonicalize model_dir for disk-space check, proceeding: {err}"
                );
                return Ok(());
            }
        };

        let matched = disks
            .list()
            .iter()
            .filter(|disk| model_dir_canonical.starts_with(disk.mount_point()))
            .max_by_key(|disk| disk.mount_point().as_os_str().len());

        let Some(disk) = matched else {
            tracing::warn!(
                "No disk found for model_dir {:?}; skipping disk-space check",
                model_dir_canonical
            );
            return Ok(());
        };

        let free_mb = disk.available_space() / (1024 * 1024);
        if free_mb < MIN_REQUIRED_SPACE_MB {
            return Err(AppError::Other(format!(
                "Insufficient disk space: {} MB available, {} MB required. \
                 Please free up disk space and try again.",
                free_mb, MIN_REQUIRED_SPACE_MB
            )));
        }

        tracing::info!(
            "Disk space check passed: {} MB available on {:?}",
            free_mb,
            disk.mount_point()
        );
        Ok(())
    }
}

pub async fn ensure_model_downloaded(model_dir: impl AsRef<Path>) -> Result<PathBuf> {
    let manager = ModelManager::new(model_dir.as_ref().to_path_buf())?;
    manager.ensure_model_available().await
}

#[async_trait]
impl ModelManagerTrait for ModelManager {
    async fn ensure_model_available(&self) -> Result<PathBuf> {
        self.ensure_model_available().await
    }

    async fn is_model_ready(&self) -> bool {
        self.is_model_ready().await
    }

    fn get_model_path(&self) -> PathBuf {
        self.get_model_path()
    }

    fn get_tokenizer_path(&self) -> PathBuf {
        self.get_tokenizer_path()
    }

    async fn ensure_reranker_available(&self) -> Result<PathBuf> {
        self.ensure_reranker_available().await
    }

    async fn is_reranker_ready(&self) -> bool {
        self.is_reranker_ready().await
    }

    fn get_reranker_path(&self) -> PathBuf {
        self.get_reranker_path()
    }

    fn get_reranker_tokenizer_path(&self) -> PathBuf {
        self.get_reranker_tokenizer_path()
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

    fn get_model_info(&self) -> crate::infrastructure::services::traits::ModelInfo {
        crate::infrastructure::services::traits::ModelInfo {
            name: "BAAI/bge-m3".to_string(),
            version: "1.0.0".to_string(),
            size_bytes: None,
            is_downloaded: self.get_model_path().exists(),
        }
    }
}
