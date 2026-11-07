use super::shape::{ModelShape, MICRO_BATCH_TOKENS};

/// What llama.cpp keeps free on each device when it fits a model by itself (`--fit-target`).
pub const FIT_TARGET_BYTES: u64 = 1024 * 1024 * 1024;

/// The smallest window the app opens a chat model with.
pub const MINIMUM_WINDOW_TOKENS: u32 = 8_192;

/// The window the app opens a chat model with when memory allows.
pub const COMFORTABLE_WINDOW_TOKENS: u32 = 32_768;

/// What sizes a server's buffers beside its window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerBuffers {
    pub kv_cache_quantization: bool,
    /// Tokens in a micro-batch (`-ub`).
    pub micro_batch: u32,
}

impl ServerBuffers {
    /// A chat server's, at llama.cpp's default micro-batch.
    pub fn chat(kv_cache_quantization: bool) -> Self {
        Self {
            kv_cache_quantization,
            micro_batch: MICRO_BATCH_TOKENS,
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

/// What a server holds at `window` tokens: `gpu_weight_bytes`, and what its header, `shape`, says
/// the rest costs. Whatever the header cannot say counts as nothing.
pub fn footprint(
    gpu_weight_bytes: u64,
    shape: Option<&ModelShape>,
    window: u32,
    buffers: ServerBuffers,
) -> Footprint {
    Footprint {
        weights: gpu_weight_bytes,
        cache: shape
            .and_then(|shape| shape.kv_cache_bytes(window, buffers.kv_cache_quantization))
            .unwrap_or(0),
        state: shape.map_or(0, ModelShape::recurrent_state_bytes),
        compute: shape.map_or(0, |shape| {
            shape.compute_buffer_bytes(window, buffers.micro_batch)
        }),
        window,
    }
}
