use super::*;
use crate::features::llm::engine::gguf_metadata::{read_model_info, GgufModelInfo};

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

    /// Context window in tokens. Maps to `--ctx-size`. The one the user
    /// saved for the model, else the one it was trained for: never sized to
    /// the machine. [`memory_hint`] says whether it fits, and nothing acts on
    /// that but the user.
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

    /// The vision projector to load beside the model (`--mmproj`), when the
    /// model may see and its catalog projector is downloaded.
    pub projector_path: Option<PathBuf>,

    /// `Some` for an embedding server of this GGUF rather than a chat one.
    /// Part of the key, so a chat and an embedding server of one file would
    /// be two processes (no GGUF is both).
    pub embedding: Option<EmbeddingServing>,
}

/// How an embedding server is launched beyond its window and slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmbeddingServing {
    /// `-b` and `-ub`: the per-slot window, so a non-causal model sees each
    /// input in one micro-batch (llama.cpp asserts `n_ubatch >= n_tokens`).
    pub ubatch: u32,
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

/// What llama.cpp keeps free on each device when it fits a model by itself:
/// `--fit-target`, 1,024 MiB in the pinned build. The memory hint holds the
/// same back from a GPU's reported free memory and from available RAM.
/// Metal's working set needs nothing held back: Apple sizes it to leave the
/// system its share.
pub(crate) const FIT_TARGET_BYTES: u64 = 1024 * 1024 * 1024;

const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Memory a server of one model holds at one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    /// The GGUF, plus its catalog projector once that is downloaded.
    pub weights: u64,
    /// The KV cache of the layers that keep one.
    pub cache: u64,
    /// A hybrid's recurrent state, which does not grow with the window.
    pub state: u64,
    /// Scratch for computing one micro-batch.
    pub compute: u64,
    /// The window, in tokens, it is priced at.
    pub window: u32,
}

impl Footprint {
    pub fn total(self) -> u64 {
        self.weights
            .saturating_add(self.cache)
            .saturating_add(self.state)
            .saturating_add(self.compute)
    }
}

/// The weights a server of `model_path` holds: the GGUF, plus its catalog
/// vision projector once that is downloaded. The projector is counted whether
/// or not a start loads it, so switching Vision never changes what a model is
/// priced at. `None` when the model file cannot be read.
fn resident_weight_bytes(model_path: &std::path::Path) -> Option<u64> {
    let weights = std::fs::metadata(model_path).ok()?.len();
    Some(weights + crate::features::llm::engine::vision::projector_bytes_on_disk(model_path))
}

/// What a server holds at a window of `window` tokens: its `weights`, and what
/// its header, `info`, says the rest costs. Whatever the header cannot say
/// counts as nothing.
pub(super) fn footprint_from(
    weights: u64,
    info: Option<&GgufModelInfo>,
    window: u32,
    kv_cache_quantization: bool,
) -> Footprint {
    Footprint {
        weights,
        cache: info
            .and_then(|info| info.kv_cache_bytes_with_quantization(window, kv_cache_quantization))
            .unwrap_or(0),
        state: info.map_or(0, GgufModelInfo::recurrent_state_bytes),
        compute: info.map_or(0, |info| info.compute_buffer_bytes(window)),
        window,
    }
}

/// What a server of `model_path` holds at a window of `context_size` tokens.
/// A GGUF whose header cannot be read still counts its weights.
pub(crate) fn footprint_of(
    model_path: &std::path::Path,
    context_size: u32,
    kv_cache_quantization: bool,
) -> Footprint {
    footprint_from(
        resident_weight_bytes(model_path).unwrap_or(0),
        read_model_info(model_path).as_ref(),
        context_size,
        kv_cache_quantization,
    )
}

/// Memory a running server already holds, at the window it was launched with.
pub(crate) fn resident_bytes_of(
    model_path: &std::path::Path,
    context_size: u32,
    kv_cache_quantization: bool,
) -> u64 {
    footprint_of(model_path, context_size, kv_cache_quantization).total()
}

/// Where the memory a new server could have is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemorySource {
    /// Metal's `recommendedMaxWorkingSetSize`, which `llama-server
    /// --list-devices` reports as the device's total.
    MetalWorkingSet,
    /// The free memory a CUDA or Vulkan device reported to the same probe.
    GpuFree,
    /// The RAM the OS reports available, for a server on the CPU.
    SystemRam,
}

/// The memory this machine reports a new bundled server could have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRoom {
    pub source: MemorySource,
    /// What the platform reports, less [`FIT_TARGET_BYTES`] where llama.cpp
    /// would keep that free.
    pub reported: u64,
    /// What other models hold of it: other servers, and models running inside
    /// the app on the same device. Available RAM already leaves them out, so
    /// it is 0 there.
    pub held: u64,
}

impl MemoryRoom {
    pub fn spare(self) -> u64 {
        self.reported.saturating_sub(self.held)
    }
}

/// The memory a server could have beside the `resident_bytes` other models
/// hold on its device: the GPU's when it is `offloaded`, else the system's.
/// `None` when the platform does not report it.
pub(crate) fn memory_room(
    capabilities: &SystemCapabilities,
    offloaded: bool,
    resident_bytes: u64,
) -> Option<MemoryRoom> {
    use crate::features::llm::engine::system::GPUVendor;

    let gpu = capabilities
        .gpu
        .as_ref()
        .filter(|gpu| offloaded && gpu.vendor.is_accelerated());
    let room = match gpu {
        Some(gpu) if gpu.vendor == GPUVendor::Apple => MemoryRoom {
            source: MemorySource::MetalWorkingSet,
            reported: gib_to_bytes(gpu.vram_gb?),
            held: resident_bytes,
        },
        Some(gpu) => MemoryRoom {
            source: MemorySource::GpuFree,
            reported: gib_to_bytes(gpu.free_vram_gb?).saturating_sub(FIT_TARGET_BYTES),
            held: resident_bytes,
        },
        None => MemoryRoom {
            source: MemorySource::SystemRam,
            reported: capabilities
                .available_memory_bytes?
                .saturating_sub(FIT_TARGET_BYTES),
            held: 0,
        },
    };
    Some(room)
}

fn gib_to_bytes(gib: f64) -> u64 {
    (gib * BYTES_PER_GIB) as u64
}

/// The largest window at which a server of `weights` and `info` fits
/// `spare_bytes`: the trained length when that fits, else the largest whole
/// thousand of tokens below it that does; 0 when none does. `None` when the
/// header says neither what the cache costs nor how long the model was
/// trained for.
fn largest_window_within(
    weights: u64,
    info: &GgufModelInfo,
    spare_bytes: u64,
    kv_cache_quantization: bool,
) -> Option<u32> {
    let trained = info.trained_context_length?;
    info.kv_cache_bytes_with_quantization(trained, kv_cache_quantization)?;
    let fits = |window: u32| {
        footprint_from(weights, Some(info), window, kv_cache_quantization).total() <= spare_bytes
    };
    if fits(trained) {
        return Some(trained);
    }
    Some(
        (1..=trained / 1024)
            .rev()
            .map(|thousands| thousands * 1024)
            .find(|&window| fits(window))
            .unwrap_or(0),
    )
}

/// What a server would ask of this machine's memory. A hint for the user: it
/// never changes a window, and never stops a server from starting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryHint {
    /// What the server takes at the window it is launched with.
    pub needed: Footprint,
    /// What this machine reports it could have.
    pub room: MemoryRoom,
    /// The largest window that fits the room, up to the trained length;
    /// `None` when the header cannot say.
    pub fits_window: Option<u32>,
}

impl MemoryHint {
    /// Whether the server fits the room this machine reports.
    pub fn fits(self) -> bool {
        self.needed.total() <= self.room.spare()
    }

    /// The numbers, for a server that failed to start or ran degraded, and
    /// what the user can do about them.
    pub fn describe(self) -> String {
        let needed = self.needed;
        let state = if needed.state > 0 {
            format!(" + {} recurrent state", gigabytes(needed.state))
        } else {
            String::new()
        };
        let room = match self.room.source {
            MemorySource::MetalWorkingSet => format!(
                "this Mac's GPU working set is {}",
                gigabytes(self.room.reported)
            ),
            MemorySource::GpuFree => format!(
                "the GPU reports {} free, after the 1 GB llama.cpp keeps free",
                gigabytes(self.room.reported)
            ),
            MemorySource::SystemRam => format!(
                "the system reports {} of RAM available, after the 1 GB llama.cpp keeps free",
                gigabytes(self.room.reported)
            ),
        };
        let held = if self.room.held > 0 {
            format!(
                ", and other models hold {} of it",
                gigabytes(self.room.held)
            )
        } else {
            String::new()
        };
        let fit = match self.fits_window {
            Some(0) => " Not even its weights fit.".to_owned(),
            Some(window) if window < needed.window => {
                format!(" About {} tokens fit.", tokens(window))
            }
            _ => String::new(),
        };
        format!(
            "At {window} tokens this model needs about {total} ({weights} weights + {cache} \
             context{state} + {compute} compute); {room}{held}.{fit} Lower its context length \
             in Settings → Models.",
            window = tokens(needed.window),
            total = gigabytes(needed.total()),
            weights = gigabytes(needed.weights),
            cache = gigabytes(needed.cache),
            compute = gigabytes(needed.compute),
        )
    }
}

/// The memory hint for a server started for `config`, beside the
/// `resident_bytes` other models hold on its device, from `info`, its
/// model's header as the caller read it. `None` when the platform does not
/// report its memory.
pub(crate) fn memory_hint(
    config: &SidecarConfig,
    capabilities: &SystemCapabilities,
    resident_bytes: u64,
    info: Option<&GgufModelInfo>,
) -> Option<MemoryHint> {
    let room = memory_room(capabilities, config.n_gpu_layers > 0, resident_bytes)?;
    let weights = resident_weight_bytes(&config.model_path).unwrap_or(0);
    Some(hint_for(
        weights,
        info,
        config.context_size,
        config.kv_cache_quantization,
        room,
    ))
}

/// The arithmetic behind [`memory_hint`], separated from reading the disk so
/// it can be checked against known models.
pub(super) fn hint_for(
    weights: u64,
    info: Option<&GgufModelInfo>,
    window: u32,
    kv_cache_quantization: bool,
    room: MemoryRoom,
) -> MemoryHint {
    MemoryHint {
        needed: footprint_from(weights, info, window, kv_cache_quantization),
        room,
        fits_window: info.and_then(|info| {
            largest_window_within(weights, info, room.spare(), kv_cache_quantization)
        }),
    }
}

/// Bytes in gigabytes to one decimal.
fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / BYTES_PER_GIB)
}

/// A token count with its thousands grouped: `60,416`.
fn tokens(count: u32) -> String {
    let digits = count.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

impl SidecarConfig {
    /// `model_path` with everything offloaded to the GPU, at the window its
    /// header says it was trained for; 0, which llama-server reads as the
    /// same, when the header does not say. Use `from_capabilities()` in
    /// production code paths so the offload matches the user's hardware.
    pub fn for_model(model_path: PathBuf) -> Self {
        let context_size = read_model_info(&model_path)
            .and_then(|info| info.trained_context_length)
            .unwrap_or(0);
        Self {
            model_path,
            n_gpu_layers: 99,
            context_size,
            mtp_capable: false,
            speculative_decoding: SpeculativeDecoding::Off,
            parallel_slots: None,
            seed: None,
            kv_cache_quantization: false,
            projector_path: None,
            embedding: None,
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

    /// The same model with GPU offload disabled, for fallback attempts. The
    /// window is the user's, so it stays.
    pub fn without_gpu_offload(&self) -> Self {
        Self {
            n_gpu_layers: 0,
            ..self.clone()
        }
    }

    /// `model_path` at a window of `context_size` tokens, every layer
    /// offloaded when the machine has an accelerator and none when it does
    /// not. The window is never Lattice's choice: it is the one the user
    /// saved, else the one the model was trained for.
    pub fn from_capabilities(
        model_path: PathBuf,
        capabilities: &SystemCapabilities,
        context_size: u32,
    ) -> Self {
        let has_accelerator = capabilities
            .gpu
            .as_ref()
            .map(|gpu| gpu.vendor.is_accelerated())
            .unwrap_or(false);

        Self {
            model_path,
            n_gpu_layers: if has_accelerator { 99 } else { 0 },
            context_size,
            mtp_capable: false,
            speculative_decoding: SpeculativeDecoding::Off,
            parallel_slots: None,
            seed: None,
            kv_cache_quantization: false,
            projector_path: None,
            embedding: None,
        }
    }
}
