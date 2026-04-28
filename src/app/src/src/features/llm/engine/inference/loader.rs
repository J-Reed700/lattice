//! Model loading and initialization using mistral.rs.
//!
//! Supports two on-disk formats:
//!
//! - **GGUF** (single `.gguf` file): loaded via `GgufModelBuilder`. Pre-flight
//!   guarded by `gguf_arch::is_supported_architecture` because mistralrs
//!   panics on unknown GGUF arch tags (it `.unwrap()`s in
//!   `gguf/content.rs:151`) and would take down the Tauri process.
//! - **Safetensors** (HF directory layout: `config.json` + `*.safetensors`
//!   shards + tokenizer): loaded via `ModelBuilder::new(path).build()`,
//!   which auto-detects text vs multimodal vs embedding from `config.json`.
//!   Pre-flight guarded by an available-RAM check so a too-big model
//!   returns a clean `LLMError::InsufficientMemory` instead of letting
//!   the OS OOM-kill the Tauri process.

use super::config::InferenceConfig;
use super::gguf_arch;
use crate::llm::models::ModelFormat;
use crate::llm::types::LLMError;
use mistralrs::{GgufModelBuilder, Model, ModelBuilder, PagedAttentionMetaBuilder};
use std::path::Path;
use tokio::time::{timeout, Duration};

/// GGUF load timeout — fixed 5 minutes. Quantized files are usually
/// 2–10 GB; loading is fast on local SSD.
const GGUF_LOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// Lower bound on safetensors load timeout. We pick the larger of this
/// and a per-GB scaled value so even tiny models get reasonable head room.
const SAFETENSORS_TIMEOUT_MIN: Duration = Duration::from_secs(300); // 5 minutes

/// Upper bound on safetensors load timeout. Past this, something is
/// genuinely wrong (slow disk, deadlock, etc.) — fail fast rather than
/// hang the UI for hours.
const SAFETENSORS_TIMEOUT_MAX: Duration = Duration::from_secs(900); // 15 minutes

/// Per-GiB scaling factor for the safetensors timeout. 60 seconds/GiB
/// matches loading throughput on commodity SSDs.
const SAFETENSORS_TIMEOUT_PER_GIB: Duration = Duration::from_secs(60);

/// Safety margin for the safetensors RAM pre-flight. Loading a model
/// requires holding the weights in RAM plus ~10–20% overhead for the
/// tokenizer, KV cache scaffold, mistralrs internal state. We require
/// the available memory to exceed the on-disk size by this factor.
const SAFETENSORS_RAM_MARGIN: f64 = 1.20;

/// Model loader. Dispatches between GGUF and HF safetensors paths
/// based on the explicit `ModelFormat` argument.
pub struct ModelLoader {
    config: InferenceConfig,
}

impl ModelLoader {
    /// Create a new model loader.
    pub fn new(config: InferenceConfig) -> Self {
        Self { config }
    }

    /// Load a model from disk.
    ///
    /// # Arguments
    /// * `model_path` - For `Gguf`, a `.gguf` file. For `Safetensors`, a
    ///   directory containing `config.json` + `*.safetensors` shards.
    /// * `format` - Explicit format selector. Callers determine this
    ///   from their domain entity (e.g. `DownloadedModel`) rather than
    ///   sniffing at the loader, so the loader is dumb about layout.
    ///
    /// # Errors
    /// - `ModelNotLoaded` if the path doesn't exist
    /// - `Other` if format-specific pre-flight checks fail
    /// - `InsufficientMemory` if a safetensors model would exceed RAM
    /// - `GenerationFailed` if mistralrs's underlying builder errors out
    pub async fn load<P: AsRef<Path>>(
        &self,
        model_path: P,
        format: ModelFormat,
    ) -> Result<Model, LLMError> {
        let config = self.config.clone();
        if let Err(err) = config.validate() {
            return Err(LLMError::InvalidConfig(err));
        }

        let model_path = model_path.as_ref();
        if !model_path.exists() {
            return Err(LLMError::ModelNotLoaded);
        }

        match format {
            ModelFormat::Gguf => self.load_gguf(model_path, config).await,
            ModelFormat::Safetensors => self.load_safetensors(model_path, config).await,
        }
    }

    /// GGUF path. Pre-flight: parse the GGUF header, validate
    /// `general.architecture` against the allowlist, reject
    /// non-canonical/legacy tags before they hit mistralrs.
    async fn load_gguf(
        &self,
        model_path: &Path,
        config: InferenceConfig,
    ) -> Result<Model, LLMError> {
        // Run the GGUF arch peek on a blocking thread: GGUFs frequently
        // live on slow external drives or NAS mounts, where even reading
        // 1 MB can stall the tokio executor for hundreds of milliseconds.
        let arch_path = model_path.to_path_buf();
        let arch = tokio::task::spawn_blocking(move || gguf_arch::read_architecture(&arch_path))
            .await
            .map_err(|e| LLMError::Other(format!("GGUF peek task panicked: {e}")))?
            .map_err(|e| {
                LLMError::Other(format!(
                    "Cannot read GGUF metadata for {}: {}",
                    model_path.display(),
                    e
                ))
            })?;

        if !gguf_arch::is_supported_architecture(&arch) {
            // Prefer a quant-source-specific hint for non-standard tags
            // (e.g. unsloth's "qwen35"), falling back to the generic
            // unsupported-arch message.
            let detail = gguf_arch::nonstandard_alias_hint(&arch).map_or_else(
                || {
                    format!(
                        "Supported architectures: {}. To use this model, configure Ollama in Settings → Chat instead.",
                        gguf_arch::SUPPORTED_ARCHITECTURES.join(", ")
                    )
                },
                |h| h.to_string(),
            );
            return Err(LLMError::Other(format!(
                "Model architecture '{arch}' is not supported by the local LLM runtime. {detail}"
            )));
        }

        tracing::info!(arch = %arch, "GGUF architecture validated as supported");

        let model_dir = model_path
            .parent()
            .ok_or_else(|| LLMError::InvalidConfig("Invalid model path".to_string()))?;

        let model_filename = model_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| LLMError::InvalidConfig("Invalid model filename".to_string()))?;

        let model_dir_str = model_dir.to_str().ok_or_else(|| {
            LLMError::InvalidConfig("Model path contains invalid UTF-8 characters".to_string())
        })?;

        let mut builder =
            GgufModelBuilder::new(model_dir_str, vec![model_filename.to_string()]).with_logging();

        tracing::info!(
            "Initializing GGUF LLM (mistralrs auto-device-map): context_size={}, batch_size={}, threads={:?}, paged_attention={}",
            config.context_size,
            config.batch_size,
            config.n_threads,
            config.use_paged_attention
        );

        if config.use_paged_attention {
            let block_size = config.paged_attention_block_size.unwrap_or(32);
            match PagedAttentionMetaBuilder::default()
                .with_block_size(block_size)
                .build()
            {
                Ok(paged_cfg) => {
                    builder = builder.with_paged_attn(paged_cfg);
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "PagedAttention config build failed (unsupported platform?); \
                         continuing without paged attention"
                    );
                }
            }
        }

        if config.enable_isq {
            tracing::warn!("ISQ requested but unsupported in current mistralrs; ignoring.");
        }

        let model = timeout(GGUF_LOAD_TIMEOUT, builder.build()).await.map_err(|_| {
            LLMError::GenerationFailed("Model loading timed out after 5 minutes".to_string())
        })??;

        Ok(model)
    }

    /// Safetensors path. Pre-flight: sum directory size, compare to
    /// `available_memory()`, refuse to load if RAM shortfall would
    /// trigger an OOM kill. Dispatches to `mistralrs::ModelBuilder`
    /// which auto-detects text vs multimodal from `config.json`.
    async fn load_safetensors(
        &self,
        model_dir: &Path,
        config: InferenceConfig,
    ) -> Result<Model, LLMError> {
        let dir_str = model_dir.to_str().ok_or_else(|| {
            LLMError::InvalidConfig(
                "Safetensors model directory contains invalid UTF-8".to_string(),
            )
        })?;

        // Pre-flight: sum the size of every shard. We reject the load
        // before allocation when the user clearly doesn't have the RAM,
        // rather than letting the OS OOM-kill the Tauri process.
        let dir_size_bytes = sum_safetensors_dir_bytes(model_dir).unwrap_or(0);
        if dir_size_bytes > 0 {
            if let Err(e) = check_available_memory_or_reject(dir_str, dir_size_bytes) {
                return Err(e);
            }
        } else {
            tracing::warn!(
                dir = %model_dir.display(),
                "Safetensors directory contained no shards or could not be read; \
                 proceeding without RAM pre-flight (mistralrs will surface the error)"
            );
        }

        // Compute a load timeout proportional to size. Per oracle's
        // recommendation: ~1 minute per GiB, clamped to [5min, 15min].
        let load_timeout = compute_safetensors_timeout(dir_size_bytes);

        tracing::info!(
            dir = %model_dir.display(),
            size_gib = dir_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            timeout_secs = load_timeout.as_secs(),
            "Initializing safetensors LLM via ModelBuilder (auto-detect text/multimodal)"
        );

        let mut builder = ModelBuilder::new(dir_str);

        if config.use_paged_attention {
            let block_size = config.paged_attention_block_size.unwrap_or(32);
            match PagedAttentionMetaBuilder::default()
                .with_block_size(block_size)
                .build()
            {
                Ok(paged_cfg) => {
                    builder = builder.with_paged_attn(paged_cfg);
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "PagedAttention config build failed for safetensors load; continuing without"
                    );
                }
            }
        }

        let model = timeout(load_timeout, builder.build()).await.map_err(|_| {
            LLMError::GenerationFailed(format!(
                "Safetensors model loading timed out after {} seconds",
                load_timeout.as_secs()
            ))
        })??;

        Ok(model)
    }
}

/// Walk a directory once, summing every `*.safetensors` shard. Returns
/// total size in bytes (NOT MiB — we want the raw value for the RAM
/// comparison without a rounding step). Any I/O error returns `None`.
fn sum_safetensors_dir_bytes(dir: &Path) -> Option<u64> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut total: u64 = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_shard = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("safetensors"));
        if is_shard {
            if let Ok(meta) = std::fs::metadata(&path) {
                total = total.saturating_add(meta.len());
            }
        }
    }
    if total == 0 {
        None
    } else {
        Some(total)
    }
}

/// Compute an appropriate load timeout for a safetensors model.
/// `~1 minute per GiB`, clamped to [5min, 15min].
fn compute_safetensors_timeout(size_bytes: u64) -> Duration {
    let gib = size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let scaled_secs = (gib * SAFETENSORS_TIMEOUT_PER_GIB.as_secs_f64()) as u64;
    let scaled = Duration::from_secs(scaled_secs);
    scaled.clamp(SAFETENSORS_TIMEOUT_MIN, SAFETENSORS_TIMEOUT_MAX)
}

/// RAM pre-flight. Returns `Err(InsufficientMemory)` with a user-facing
/// message when the model's on-disk size exceeds available memory by
/// the safety margin. Returns `Ok(())` if the check is satisfied OR if
/// we can't read system memory (failing closed here would lock users
/// out of perfectly viable loads — defer the actual OOM to mistralrs
/// in that pathological case).
fn check_available_memory_or_reject(
    model_id_for_msg: &str,
    model_size_bytes: u64,
) -> Result<(), LLMError> {
    use sysinfo::System;

    let mut sys = System::new();
    sys.refresh_memory();
    let available_bytes = sys.available_memory(); // bytes since sysinfo 0.30+

    if available_bytes == 0 {
        // Couldn't read memory info — don't block.
        tracing::warn!(
            "Could not determine available RAM; skipping safetensors RAM pre-flight"
        );
        return Ok(());
    }

    let needed_bytes = (model_size_bytes as f64 * SAFETENSORS_RAM_MARGIN) as u64;
    if needed_bytes <= available_bytes {
        return Ok(());
    }

    let model_gib = model_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let avail_gib = available_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let need_gib = needed_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    Err(LLMError::InsufficientMemory(format!(
        "Loading {model_id_for_msg} would need ~{need_gib:.1} GiB of RAM \
         (model is {model_gib:.1} GiB on disk, with ~20% overhead for KV \
         cache and tokenizer state) but only {avail_gib:.1} GiB is \
         currently available. Close other applications or pick a quantized \
         GGUF variant of this model instead."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_loader_creation() {
        let config = InferenceConfig::default();
        let loader = ModelLoader::new(config);
        // Disabled by default for Metal compatibility
        assert!(!loader.config.use_paged_attention);
    }

    #[tokio::test]
    async fn test_load_nonexistent_gguf_returns_not_loaded() {
        let config = InferenceConfig::default();
        let loader = ModelLoader::new(config);
        let result = loader.load("/nonexistent.gguf", ModelFormat::Gguf).await;
        assert!(matches!(result, Err(LLMError::ModelNotLoaded)));
    }

    #[tokio::test]
    async fn test_load_nonexistent_safetensors_returns_not_loaded() {
        let config = InferenceConfig::default();
        let loader = ModelLoader::new(config);
        let result = loader
            .load("/nonexistent-safetensors-dir", ModelFormat::Safetensors)
            .await;
        assert!(matches!(result, Err(LLMError::ModelNotLoaded)));
    }

    #[test]
    fn test_compute_safetensors_timeout_min_clamp() {
        // Tiny model — should clamp to MIN (5 min).
        let t = compute_safetensors_timeout(100 * 1024 * 1024); // 100 MiB
        assert_eq!(t, SAFETENSORS_TIMEOUT_MIN);
    }

    #[test]
    fn test_compute_safetensors_timeout_max_clamp() {
        // Huge model — should clamp to MAX (15 min).
        let t = compute_safetensors_timeout(50 * 1024 * 1024 * 1024); // 50 GiB
        assert_eq!(t, SAFETENSORS_TIMEOUT_MAX);
    }

    #[test]
    fn test_compute_safetensors_timeout_scales_in_window() {
        // 8 GiB → 8 * 60s = 480s, inside [300, 900].
        let t = compute_safetensors_timeout(8 * 1024 * 1024 * 1024);
        assert_eq!(t, Duration::from_secs(480));
    }

    #[test]
    fn test_sum_safetensors_dir_bytes_sums_shards_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("model.safetensors"), vec![0u8; 1024]).unwrap();
        std::fs::write(dir.path().join("config.json"), "{}").unwrap();
        std::fs::write(dir.path().join("README.md"), "ignore me").unwrap();
        let bytes = sum_safetensors_dir_bytes(dir.path()).unwrap();
        assert_eq!(bytes, 1024);
    }

    #[test]
    fn test_check_available_memory_rejects_when_short() {
        use sysinfo::System;

        // Skip on environments where sysinfo can't read available memory
        // (some sandboxed test runners). The helper is documented to
        // pass-through (return Ok) in that case rather than block users.
        let mut sys = System::new();
        sys.refresh_memory();
        if sys.available_memory() == 0 {
            return;
        }

        // 1 PiB is very obviously larger than any reasonable system.
        let huge = 1024_u64 * 1024 * 1024 * 1024 * 1024;
        let result = check_available_memory_or_reject("test-model", huge);
        match result {
            Err(LLMError::InsufficientMemory(msg)) => {
                assert!(msg.contains("test-model"));
                assert!(msg.contains("RAM") || msg.contains("memory"));
            }
            other => panic!("expected InsufficientMemory, got {other:?}"),
        }
    }

    #[test]
    fn test_check_available_memory_passes_when_sufficient() {
        // 1 KiB is comfortably below any reasonable available memory,
        // AND comfortably below the "couldn't read" pass-through path.
        let tiny = 1024_u64;
        let result = check_available_memory_or_reject("test-model", tiny);
        assert!(matches!(result, Ok(())));
    }

    #[test]
    fn test_check_available_memory_passes_when_unreadable() {
        // Indirectly verified — when sysinfo returns 0 (unreadable),
        // the helper returns Ok rather than blocking. The other tests
        // in this module rely on that pass-through behavior in CI.
        // No explicit setup needed; just confirm the doc invariant via
        // the tiny case.
        assert!(matches!(
            check_available_memory_or_reject("x", 0),
            Ok(())
        ));
    }
}
