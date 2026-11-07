/// A header value that is either one number for every layer or one number per layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PerLayer {
    Uniform(u32),
    Layers(Vec<u32>),
}

impl From<u32> for PerLayer {
    fn from(value: u32) -> Self {
        Self::Uniform(value)
    }
}

/// llama.cpp's default micro-batch.
pub const MICRO_BATCH_TOKENS: u32 = 512;

/// What a GGUF header says about the memory a server of it holds. The one description the runtime
/// and the recommender price a model from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelShape {
    /// Transformer layers, counting multi-token-prediction blocks at the end.
    pub block_count: Option<u32>,
    pub head_count_kv: Option<PerLayer>,
    pub head_count: Option<PerLayer>,
    pub key_length: Option<u32>,
    pub value_length: Option<u32>,
    pub key_length_swa: Option<u32>,
    pub value_length_swa: Option<u32>,
    pub embedding_length: Option<u32>,
    pub sliding_window: Option<u32>,
    pub sliding_window_pattern: Option<Vec<bool>>,
    pub full_attention_interval: Option<u32>,
    pub nextn_predict_layers: Option<u32>,
    pub shared_kv_layers: Option<u32>,
    pub feed_forward_length: Option<u32>,
    pub ssm_conv_kernel: Option<u32>,
    pub ssm_state_size: Option<u32>,
    pub ssm_group_count: Option<u32>,
    pub ssm_inner_size: Option<u32>,
}

impl ModelShape {
    /// Bytes of KV cache llama-server allocates for a window of `context_tokens`, when the header
    /// says enough to work it out.
    pub fn kv_cache_bytes(&self, _context_tokens: u32, _quantized: bool) -> Option<u64> {
        todo!("stage 2")
    }

    /// Bytes of recurrent state a hybrid's linear-attention layers keep, which does not grow with
    /// the window. 0 for a model without them.
    pub fn recurrent_state_bytes(&self) -> u64 {
        todo!("stage 2")
    }

    /// Bytes of compute buffer for a window of `context_tokens` at a micro-batch of `micro_batch`.
    pub fn compute_buffer_bytes(&self, _context_tokens: u32, _micro_batch: u32) -> u64 {
        todo!("stage 2")
    }
}

/// The memory facts pinned for one GGUF file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedModel {
    pub shape: ModelShape,
    /// Bytes of the file that stay in system RAM however many layers are offloaded.
    pub cpu_weight_bytes: u64,
}
