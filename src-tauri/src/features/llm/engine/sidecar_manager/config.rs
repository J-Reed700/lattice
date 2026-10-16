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

    /// Context window in tokens. Maps to `--ctx-size`. `Auto` sizes it
    /// from the model's trained length and the memory its KV cache can
    /// have ([`auto_context_size`]); the flat defaults are the fallback when
    /// either is unknown.
    pub context_size: u32,

    /// Whether the GGUF carries a multi-token-prediction head llama-server
    /// can draft with. Only an MTP build of a model has one.
    pub mtp_capable: bool,

    /// The speculative decoding the user opted into. It reaches the server
    /// only when the model is [`Self::mtp_capable`].
    pub speculative_decoding: SpeculativeDecoding,

    /// Optional explicit `--parallel` (`-np`) slot count (LM Studio "Max Concurrent Predictions").
    /// `None` uses llama-server's built-in default (4).
    pub parallel_slots: Option<u32>,

    /// Optional explicit RNG seed (`-s`). `None` uses a random seed.
    pub seed: Option<u32>,

    /// Whether to quantize the KV cache (`-ctk q8_0 -ctv q8_0`), halving KV cache memory.
    pub kv_cache_quantization: bool,
}

/// Speculative decoding a bundled server can be launched with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SpeculativeDecoding {
    #[default]
    Off,
    /// Draft up to `n_max` tokens a step with the model's own
    /// multi-token-prediction head.
    MtpDraft { n_max: u32 },
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

/// The smallest window worth starting. A card that cannot even afford this is
/// going to struggle, but a too-small window that loads still beats the larger
/// flat default that does not — and if it genuinely will not allocate, the
/// sidecar's own fallback reports a degraded start.
pub(super) const AUTO_MIN_CONTEXT_SIZE: u32 = 4_096;

/// Memory the machine keeps free beyond a model and its cache, so loading one
/// never tips it into swap: a tenth of RAM, and never under 2 GiB.
pub(super) fn swap_headroom_bytes(total_ram_bytes: u64) -> u64 {
    (total_ram_bytes / 10).max(2 * 1024 * 1024 * 1024)
}

/// The largest window the pre-growth sizing would choose: what it chose for
/// every machine before the window could grow past it. Auto never picks less
/// than that sizing did for the same machine and model.
pub(super) const AUTO_FLOOR_CEILING: u32 = 32_768;

/// System memory a new server may grow into, for a GPU that shares it.
#[derive(Debug, Clone, Copy)]
pub(super) struct SharedMemory {
    /// Available when the server is about to load, as the OS counts it.
    pub available_bytes: u64,
    /// Installed RAM, which sets the headroom kept back.
    pub total_bytes: u64,
}

impl SharedMemory {
    /// What is left for weights and cache once the headroom is kept back.
    fn spare_bytes(self) -> u64 {
        self.available_bytes
            .saturating_sub(swap_headroom_bytes(self.total_bytes))
    }
}

/// Size the context window from the model's own dimensions and the memory the
/// machine has.
///
/// Returns `None` when anything needed is unknown — an unreadable GGUF, a
/// header without its KV dimensions, a GPU that does not report its memory —
/// so the caller keeps the conservative flat default rather than acting on a
/// guess.
pub(super) fn auto_context_size(
    model_path: &std::path::Path,
    capabilities: &SystemCapabilities,
    resident_bytes: u64,
) -> Option<u32> {
    auto_context_size_with_quantization(model_path, capabilities, resident_bytes, false)
}

pub(super) fn auto_context_size_with_quantization(
    model_path: &std::path::Path,
    capabilities: &SystemCapabilities,
    resident_bytes: u64,
    quantized: bool,
) -> Option<u32> {
    use crate::features::llm::engine::gguf_metadata;

    let vram_gb = capabilities.gpu_vram_gb()?;
    let shared = capabilities
        .gpu
        .as_ref()
        .filter(|gpu| gpu.vendor.shares_system_memory())
        .and(capabilities.available_memory_bytes)
        .map(|available_bytes| SharedMemory {
            available_bytes,
            total_bytes: (capabilities.total_ram_gb * 1024.0 * 1024.0 * 1024.0) as u64,
        });
    let info = gguf_metadata::read_model_info(model_path)?;
    let weights_bytes = std::fs::metadata(model_path).ok()?.len();

    context_size_for_quantized(
        vram_gb,
        weights_bytes,
        resident_bytes,
        &info,
        shared,
        quantized,
    )
}

/// GPU memory a running server already holds: its weights plus the KV cache
/// for the window it was launched with. A GGUF whose header cannot be read
/// still counts its weights.
pub(super) fn resident_bytes_of(model_path: &std::path::Path, context_size: u32) -> u64 {
    let weights = std::fs::metadata(model_path).map_or(0, |m| m.len());
    let kv = crate::features::llm::engine::gguf_metadata::read_model_info(model_path)
        .and_then(|info| info.kv_cache_bytes(context_size))
        .unwrap_or(0);
    weights.saturating_add(kv)
}

/// The largest whole thousand of tokens, up to `ceiling`, whose KV cache fits
/// `spare_bytes`; 0 when none does. Each candidate is priced rather than
/// divided out because the cache is not always proportional to the window:
/// sliding layers stop growing at theirs.
fn largest_window_within(
    info: &crate::features::llm::engine::gguf_metadata::GgufModelInfo,
    ceiling: u32,
    spare_bytes: u64,
    quantized: bool,
) -> u32 {
    (1..=ceiling / 1024)
        .rev()
        .map(|thousands| thousands * 1024)
        .find(|&tokens| {
            info.kv_cache_bytes_with_quantization(tokens, quantized)
                .is_some_and(|bytes| bytes <= spare_bytes)
        })
        .unwrap_or(0)
}

/// The arithmetic behind [`auto_context_size`], separated from reading the disk
/// so the decision can be checked against known hardware and known models.
///
/// The window is a resource, not a content limit: what does not fit in it is
/// paged, never dropped. It has two parts:
///
/// - **The floor** is what the sizing before growth chose: the largest window
///   up to [`AUTO_FLOOR_CEILING`] (and the trained length) whose cache fits the
///   accelerator's usable share less the weights and what other running
///   servers hold (`resident_bytes`). Auto never picks less.
/// - **Above it** the window grows while its cache fits the memory available
///   now (`shared`, for a GPU that draws on system RAM) less headroom and the
///   weights, still within the usable share and never past the trained length.
///   llama.cpp allocates the whole cache at load, so growth sized to installed
///   memory rather than available memory would push a busy machine into swap.
#[cfg(test)]
pub(super) fn context_size_for(
    vram_gb: f64,
    weights_bytes: u64,
    resident_bytes: u64,
    info: &crate::features::llm::engine::gguf_metadata::GgufModelInfo,
    shared: Option<SharedMemory>,
) -> Option<u32> {
    context_size_for_quantized(vram_gb, weights_bytes, resident_bytes, info, shared, false)
}

pub(super) fn context_size_for_quantized(
    vram_gb: f64,
    weights_bytes: u64,
    resident_bytes: u64,
    info: &crate::features::llm::engine::gguf_metadata::GgufModelInfo,
    shared: Option<SharedMemory>,
    quantized: bool,
) -> Option<u32> {
    let floor_ceiling = info
        .trained_context_length
        .unwrap_or(AUTO_FLOOR_CEILING)
        .min(AUTO_FLOOR_CEILING);
    // A header without KV dimensions declines here, before memory is looked at.
    info.kv_cache_bytes_with_quantization(floor_ceiling, quantized)?;
    let minimum = AUTO_MIN_CONTEXT_SIZE.min(floor_ceiling);

    let usable_bytes = (vram_gb * VRAM_USABLE_FRACTION * 1024.0 * 1024.0 * 1024.0) as u64;
    // Weights past the usable share (a 9B on an 8 GB Mac) leave no room at
    // all. That is the minimum's case, not "unknown": `None` would hand the
    // caller the larger flat default, which is guaranteed not to fit.
    let Some(share_spare) = usable_bytes
        .checked_sub(resident_bytes)
        .and_then(|free| free.checked_sub(weights_bytes))
    else {
        return Some(minimum);
    };
    let floor = largest_window_within(info, floor_ceiling, share_spare, quantized).max(minimum);

    // Growth needs a trained length to stop at.
    let Some(trained) = info.trained_context_length else {
        return Some(floor);
    };
    // Other servers are already counted in what is available now.
    let growth_spare = shared.map_or(share_spare, |shared| {
        share_spare.min(shared.spare_bytes().saturating_sub(weights_bytes))
    });
    Some(largest_window_within(info, trained, growth_spare, quantized).max(floor))
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
            mtp_capable: false,
            speculative_decoding: SpeculativeDecoding::Off,
            parallel_slots: None,
            seed: None,
            kv_cache_quantization: false,
        }
    }

    /// Tokens to draft a step, when this server drafts with an MTP head:
    /// the user asked for it and the model has one.
    pub fn mtp_draft(&self) -> Option<u32> {
        match self.speculative_decoding {
            SpeculativeDecoding::MtpDraft { n_max } if self.mtp_capable => Some(n_max),
            _ => None,
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
            mtp_capable: self.mtp_capable,
            speculative_decoding: self.speculative_decoding,
            parallel_slots: self.parallel_slots,
            seed: self.seed,
            kv_cache_quantization: self.kv_cache_quantization,
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
            mtp_capable: false,
            speculative_decoding: SpeculativeDecoding::Off,
            parallel_slots: None,
            seed: None,
            kv_cache_quantization: false,
        }
    }

    /// Enable or disable `q8_0` KV cache quantization and resize the auto
    /// context window accordingly when running on an accelerator.
    pub fn with_kv_cache_quantization(
        mut self,
        enabled: bool,
        capabilities: &SystemCapabilities,
        resident_bytes: u64,
    ) -> Self {
        self.kv_cache_quantization = enabled;
        if enabled && self.n_gpu_layers > 0 && capabilities.total_ram_gb >= LOW_RAM_THRESHOLD_GB {
            if let Some(resized) = auto_context_size_with_quantization(
                &self.model_path,
                capabilities,
                resident_bytes,
                true,
            ) {
                self.context_size = resized;
            }
        }
        self
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

        if let Some(affordable) = auto_context_size_with_quantization(
            &self.model_path,
            capabilities,
            resident_bytes,
            self.kv_cache_quantization,
        ) {
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
