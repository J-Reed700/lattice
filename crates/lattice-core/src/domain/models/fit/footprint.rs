/// Compute buffers, the recurrent state of hybrid layers and the server itself: added once to every
/// footprint.
pub const RUNTIME_OVERHEAD_BYTES: u64 = 1_000_000_000;

/// Bytes per cached key or value element: llama.cpp's default cache type is f16.
const F16_BYTES: u64 = 2;

/// Tokens llama.cpp adds to a sliding-window layer's cache for its batch.
const BATCH_TOKENS: u64 = 512;

/// The smallest window the app opens a chat model with.
pub const MINIMUM_WINDOW_TOKENS: u64 = 8_192;

/// The window the app opens a chat model with when memory allows.
pub const COMFORTABLE_WINDOW_TOKENS: u64 = 32_768;

/// The cache layout of one GGUF file, as read from its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GgufHeader {
    pub weights_bytes: u64,
    /// Layers that attend to the whole window.
    pub full_attention_layers: u64,
    pub full_attention_kv_heads: u64,
    pub full_attention_key_length: u64,
    pub full_attention_value_length: u64,
    /// Layers that attend to a sliding window only (zero when there are none).
    pub sliding_layers: u64,
    pub sliding_kv_heads: u64,
    pub sliding_key_length: u64,
    pub sliding_value_length: u64,
    pub sliding_window_tokens: u64,
    /// State of recurrent layers, which does not grow with the window.
    pub recurrent_state_bytes: u64,
}

/// What one GGUF file holds in accelerator memory: its weights and its context cache, at f16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    pub weights_bytes: u64,
    /// Key and value bytes per token across the full-attention layers.
    pub kv_per_token_bytes: u64,
    /// The sliding-window layers' cache, which stops growing at their window.
    pub windowed_kv_bytes: u64,
    pub recurrent_state_bytes: u64,
}

impl Footprint {
    /// Reads a footprint from a parsed header: the one rule for curated and downloaded files alike.
    pub fn from_header(header: &GgufHeader) -> Footprint {
        let per_token = |layers: u64, heads: u64, key: u64, value: u64| {
            layers * heads * (key + value) * F16_BYTES
        };
        Footprint {
            weights_bytes: header.weights_bytes,
            kv_per_token_bytes: per_token(
                header.full_attention_layers,
                header.full_attention_kv_heads,
                header.full_attention_key_length,
                header.full_attention_value_length,
            ),
            windowed_kv_bytes: per_token(
                header.sliding_layers,
                header.sliding_kv_heads,
                header.sliding_key_length,
                header.sliding_value_length,
            ) * (header.sliding_window_tokens + BATCH_TOKENS),
            recurrent_state_bytes: header.recurrent_state_bytes,
        }
    }

    /// Bytes the model holds while serving `window_tokens` of context, runtime overhead included.
    pub fn at(&self, window_tokens: u64) -> u64 {
        self.weights_bytes
            + self.windowed_kv_bytes
            + self.recurrent_state_bytes
            + self.kv_per_token_bytes * window_tokens
            + RUNTIME_OVERHEAD_BYTES
    }
}
