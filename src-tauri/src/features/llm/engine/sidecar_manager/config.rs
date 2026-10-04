use super::*;

/// A bundled llama-server build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarBinary {
    /// The accelerated build: Metal on macOS, Vulkan on Windows and Linux.
    Primary,
    /// The CPU-only build, for machines where the primary build cannot load
    /// or cannot bring up its GPU backend. Windows and Linux only.
    Cpu,
}

impl SidecarBinary {
    /// Whether this platform bundles [`SidecarBinary::Cpu`].
    pub const CPU_BUILD_BUNDLED: bool = cfg!(any(target_os = "windows", target_os = "linux"));

    /// Installed sidecar name: what `sidecar()` spawns, and what logs,
    /// diagnostics and error messages call this build.
    pub fn label(self) -> &'static str {
        match self {
            Self::Primary => SIDECAR_BIN,
            Self::Cpu => SIDECAR_CPU_BIN,
        }
    }

    pub(super) fn bundled() -> &'static [SidecarBinary] {
        if Self::CPU_BUILD_BUNDLED {
            &[Self::Primary, Self::Cpu]
        } else {
            &[Self::Primary]
        }
    }
}

/// Configuration for spawning a llama-server sidecar.
///
/// Equality is identity for a running server: two roles whose configurations
/// compare equal are asking for the same process, and the pool in
/// [`sidecar_pool`](crate::features::llm::engine::sidecar_pool) hands them one. Any field added here
/// that changes what the server *is* must therefore be part of the key —
/// which deriving `Eq`/`Hash` over the whole struct takes care of.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SidecarConfig {
    /// Path to the GGUF model file.
    pub model_path: PathBuf,

    /// Number of model layers to offload to GPU. `99` means "all";
    /// `0` means CPU-only. Maps to llama-server's `-ngl` flag.
    pub n_gpu_layers: u32,

    /// Context window in tokens. Maps to `--ctx-size`. Default is
    /// constrained by the model's training context; we cap at 8192
    /// to avoid OOM on entry-level hardware.
    pub context_size: u32,
}

/// RAM threshold (GB) below which we reduce the context window to keep
/// memory pressure manageable. 8 GB is the entry point for "comfortable
/// 7B Q4_K_M with 8K context"; below that we squeeze ctx down.
pub(super) const LOW_RAM_THRESHOLD_GB: f64 = 8.0;

/// Context size for low-RAM machines. 2048 tokens is enough for most
/// chat turns and plays well with 4 GB systems running 3B/4B models.
pub(super) const LOW_RAM_CONTEXT_SIZE: u32 = 2048;

/// Default context for accelerated configurations. Most modern open
/// models (Llama 3, Qwen 2.5) train at >=8K natively; pushing past
/// that on consumer hardware is risky.
pub(super) const DEFAULT_GPU_CONTEXT_SIZE: u32 = 8192;

/// Default context for CPU-only configurations. CPU prefill scales
/// with context-squared, so 4K is a more honest UX target than 8K.
pub(super) const DEFAULT_CPU_CONTEXT_SIZE: u32 = 4096;

/// Share of reported accelerator memory the model and its KV cache may claim.
/// llama.cpp also allocates compute buffers, and on unified memory (Apple
/// Silicon) the same pool is the machine's RAM, so filling it would starve
/// everything else.
pub(super) const VRAM_USABLE_FRACTION: f64 = 0.7;

/// The largest window `Auto` will choose, however much memory is free. Prefill
/// cost climbs with context, and the retrieval pipeline never builds a prompt
/// near this size — past here the user is paying latency for nothing. Someone
/// who genuinely wants more can say so in settings.
pub(super) const AUTO_MAX_CONTEXT_SIZE: u32 = 32_768;

/// The smallest window worth starting. A card that cannot even afford this is
/// going to struggle, but a too-small window that loads still beats the larger
/// flat default that does not — and if it genuinely will not allocate, the
/// sidecar's own fallback reports a degraded start.
pub(super) const AUTO_MIN_CONTEXT_SIZE: u32 = 4_096;

/// Size the context window from the model's own dimensions and the memory the
/// accelerator reports.
///
/// Returns `None` when anything needed is unknown — an unreadable GGUF, a
/// header without KV dimensions, a GPU that does not report its memory — so the
/// caller keeps the conservative flat default rather than acting on a guess.
///
/// The trained length is a ceiling, never a target: a model trained at 262144
/// would need 34 GB of KV cache at full length, which no consumer card holds.
pub(super) fn auto_context_size(
    model_path: &std::path::Path,
    capabilities: &SystemCapabilities,
    resident_bytes: u64,
) -> Option<u32> {
    use crate::features::llm::engine::gguf_metadata;

    let vram_gb = capabilities.gpu_vram_gb()?;
    let info = gguf_metadata::read_model_info(model_path)?;
    let weights_bytes = std::fs::metadata(model_path).ok()?.len();

    context_size_for(vram_gb, weights_bytes, resident_bytes, &info)
}

/// GPU memory a running server already holds: its weights plus the KV cache
/// for the window it was launched with. A GGUF whose header cannot be read
/// still counts its weights.
pub(super) fn resident_bytes_of(model_path: &std::path::Path, context_size: u32) -> u64 {
    let weights = std::fs::metadata(model_path).map_or(0, |m| m.len());
    let kv = crate::features::llm::engine::gguf_metadata::read_model_info(model_path)
        .and_then(|info| info.kv_cache_bytes_per_token())
        .map_or(0, |per_token| {
            per_token.saturating_mul(u64::from(context_size))
        });
    weights.saturating_add(kv)
}

/// The arithmetic behind [`auto_context_size`], separated from reading the disk
/// so the decision can be checked against known hardware and known models.
///
/// `resident_bytes` is what other running servers already hold (weights and
/// KV). Each one used to be sized against the whole budget, so a chat model and
/// a different utility GGUF on a 16 GB Mac together claimed more than Metal's
/// working set and the machine swapped.
pub(super) fn context_size_for(
    vram_gb: f64,
    weights_bytes: u64,
    resident_bytes: u64,
    info: &crate::features::llm::engine::gguf_metadata::GgufModelInfo,
) -> Option<u32> {
    let bytes_per_token = info.kv_cache_bytes_per_token()?;

    let ceiling = info
        .trained_context_length
        .unwrap_or(AUTO_MAX_CONTEXT_SIZE)
        .min(AUTO_MAX_CONTEXT_SIZE);

    let usable_bytes = (vram_gb * VRAM_USABLE_FRACTION * 1024.0 * 1024.0 * 1024.0) as u64;
    // Weights past the usable share (a 9B on an 8 GB Mac) leave no room at
    // all. That is the floor's case, not "unknown": `None` would hand the
    // caller the larger flat default, which is guaranteed not to fit.
    let Some(spare_bytes) = usable_bytes
        .checked_sub(resident_bytes)
        .and_then(|free| free.checked_sub(weights_bytes))
    else {
        return Some(AUTO_MIN_CONTEXT_SIZE.min(ceiling));
    };
    let affordable = u32::try_from(spare_bytes / bytes_per_token).unwrap_or(u32::MAX);
    // Whole thousands read better in a log line than 31_417 does.
    let chosen = affordable.min(ceiling) / 1024 * 1024;

    // Never hand back less than the floor while the model could still be
    // trained for more: returning `None` here would hand the caller the larger
    // flat default, which is the one thing guaranteed not to fit.
    Some(chosen.max(AUTO_MIN_CONTEXT_SIZE.min(ceiling)))
}

impl SidecarConfig {
    /// A sensible default: GPU-offload everything, 8K context.
    /// Use `from_capabilities()` instead in production code paths so
    /// the config matches the user's hardware.
    pub fn for_model(model_path: PathBuf) -> Self {
        Self {
            model_path,
            n_gpu_layers: 99,
            context_size: DEFAULT_GPU_CONTEXT_SIZE,
        }
    }

    /// The same model with GPU offload disabled, for fallback attempts.
    /// Caps the context at the CPU default but keeps a smaller low-RAM
    /// window rather than growing it.
    pub fn without_gpu_offload(&self) -> Self {
        Self {
            model_path: self.model_path.clone(),
            n_gpu_layers: 0,
            context_size: self.context_size.min(DEFAULT_CPU_CONTEXT_SIZE),
        }
    }

    /// `resident_bytes` is the GPU memory other running servers already hold
    /// (see [`SidecarRegistry::resident_gpu_bytes`]); 0 when none are up.
    pub fn from_capabilities(
        model_path: PathBuf,
        capabilities: &SystemCapabilities,
        resident_bytes: u64,
    ) -> Self {
        let has_accelerator = capabilities
            .gpu
            .as_ref()
            .map(|gpu| gpu.vendor.is_accelerated())
            .unwrap_or(false);

        let n_gpu_layers = if has_accelerator { 99 } else { 0 };

        let base_context = if has_accelerator {
            // Size from the model and the hardware when both can be read; the
            // flat default is the fallback, not the plan. A 5 GB model on a
            // 28 GB card ran an 8K window purely because nothing had looked.
            auto_context_size(&model_path, capabilities, resident_bytes).unwrap_or_else(|| {
                tracing::debug!(
                    "Could not size the context window from the model and GPU; \
                     using the default {DEFAULT_GPU_CONTEXT_SIZE}"
                );
                DEFAULT_GPU_CONTEXT_SIZE
            })
        } else {
            // CPU prefill grows with context and there is no accelerator to
            // absorb it, so a bigger window would buy latency, not capability.
            DEFAULT_CPU_CONTEXT_SIZE
        };

        let context_size = if capabilities.total_ram_gb < LOW_RAM_THRESHOLD_GB {
            tracing::info!(
                total_ram_gb = capabilities.total_ram_gb,
                "Low-RAM system detected; reducing sidecar context window to {}",
                LOW_RAM_CONTEXT_SIZE
            );
            LOW_RAM_CONTEXT_SIZE
        } else {
            base_context
        };

        Self {
            model_path,
            n_gpu_layers,
            context_size,
        }
    }

    /// Replace the automatic choice with the one the user asked for.
    ///
    /// The request is honoured as written. Silently clamping it is how the
    /// existing `context_window` setting came to have no effect on the sidecar
    /// at all, so an unreachable request is logged loudly and then attempted —
    /// if it will not allocate, the sidecar's own fallback reports a degraded
    /// start rather than this quietly deciding for the user.
    pub fn with_context_override(
        mut self,
        requested: u32,
        capabilities: &SystemCapabilities,
        resident_bytes: u64,
    ) -> Self {
        if requested == 0 || requested == self.context_size {
            return self;
        }

        if let Some(affordable) = auto_context_size(&self.model_path, capabilities, resident_bytes)
        {
            if requested > affordable {
                tracing::warn!(
                    requested,
                    affordable,
                    "Configured context window is larger than this GPU and model can \
                     comfortably hold; starting anyway and falling back if it will not allocate"
                );
            }
        }
        if let Some(trained) =
            crate::features::llm::engine::gguf_metadata::read_model_info(&self.model_path)
                .and_then(|info| info.trained_context_length)
        {
            if requested > trained {
                tracing::warn!(
                    requested,
                    trained,
                    "Configured context window exceeds what this model was trained for; \
                     llama.cpp will extend it by rope scaling, which costs quality"
                );
            }
        }

        tracing::info!(
            from = self.context_size,
            to = requested,
            "Context window set from settings"
        );
        self.context_size = requested;
        self
    }
}
