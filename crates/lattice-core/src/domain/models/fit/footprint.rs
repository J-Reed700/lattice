use super::shape::{ModelShape, MICRO_BATCH_TOKENS};

/// What llama.cpp keeps free on each device when it fits a model by itself (`--fit-target`).
pub const FIT_TARGET_BYTES: u64 = 1024 * 1024 * 1024;

/// The smallest window the app opens a chat model with.
pub const MINIMUM_WINDOW_TOKENS: u32 = 8_192;

/// The window the app opens a chat model with when memory allows.
pub const COMFORTABLE_WINDOW_TOKENS: u32 = 32_768;

/// Prompt-cache checkpoints kept per slot for chat servers (`--ctx-checkpoints`).
/// Shared between the memory fitter and `llama-server` launch flags so they
/// cannot drift apart. Keeping 8 checkpoints per slot cuts hybrid
/// recurrent-state checkpoint RAM by 75% versus llama.cpp's default of 32 while
/// preserving end-of-prompt KV reuse and ~11k-token rollback spacing at 88k ctx.
pub const CHAT_CTX_CHECKPOINTS: u32 = 8;

/// What sizes a server's buffers beside its window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerBuffers {
    pub kv_cache_quantization: bool,
    /// Tokens in a micro-batch (`-ub`).
    pub micro_batch: u32,
    /// Prompt-cache checkpoints kept per slot (`--ctx-checkpoints`) that share
    /// the priced memory pool: [`CHAT_CTX_CHECKPOINTS`] for chat servers on
    /// unified/system RAM, `0` for pooling servers and for dedicated GPUs where
    /// checkpoints live in host RAM rather than VRAM.
    pub ctx_checkpoints: u32,
    /// Explicit parallel slot count (`-np`); `None` uses llama-server's default.
    pub slots: Option<u32>,
}

impl ServerBuffers {
    /// A chat server's, at llama.cpp's default micro-batch and slot count.
    pub fn chat(kv_cache_quantization: bool) -> Self {
        Self {
            kv_cache_quantization,
            micro_batch: MICRO_BATCH_TOKENS,
            ctx_checkpoints: CHAT_CTX_CHECKPOINTS,
            slots: None,
        }
    }

    /// A pooling (embedding or reranking) server's: unquantized cache, no
    /// prompt-cache checkpoints.
    pub fn pooling(micro_batch: u32) -> Self {
        Self {
            kv_cache_quantization: false,
            micro_batch,
            ctx_checkpoints: 0,
            slots: None,
        }
    }

    /// The same server priced against dedicated GPU VRAM, where prompt-cache
    /// checkpoints live in host RAM rather than on the card.
    pub fn for_dedicated_gpu(self) -> Self {
        Self {
            ctx_checkpoints: 0,
            ..self
        }
    }
}

/// Memory a server of one model holds at one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    /// What the device holds of the GGUF.
    pub weights: u64,
    /// The KV cache of the layers that keep one.
    pub cache: u64,
    /// A hybrid's recurrent state, which does not grow with the window.
    pub state: u64,
    /// Prompt-cache checkpoints of a hybrid's recurrent state held in the
    /// priced memory pool (`--ctx-checkpoints`); 0 for pure-attention models,
    /// pooling servers, and dedicated GPUs.
    pub checkpoints: u64,
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
            .saturating_add(self.checkpoints)
            .saturating_add(self.compute)
    }
}

/// What a server holds at `window` tokens: `gpu_weight_bytes`, and what its header, `shape`, says
/// the rest costs. Whatever the header cannot say counts as nothing.
pub fn footprint(
    gpu_weight_bytes: u64,
    shape: Option<&ModelShape>,
    window: u32,
    buffers: ServerBuffers,
) -> Footprint {
    let state = shape.map_or(0, |shape| match buffers.slots {
        Some(slots) => shape.recurrent_state_bytes_for_slots(slots),
        None => shape.recurrent_state_bytes(),
    });
    let checkpoints = state.saturating_mul(u64::from(buffers.ctx_checkpoints));
    Footprint {
        weights: gpu_weight_bytes,
        cache: shape
            .and_then(|shape| shape.kv_cache_bytes(window, buffers.kv_cache_quantization))
            .unwrap_or(0),
        state,
        checkpoints,
        compute: shape.map_or(0, |shape| {
            shape.compute_buffer_bytes(window, buffers.micro_batch)
        }),
        window,
    }
}
