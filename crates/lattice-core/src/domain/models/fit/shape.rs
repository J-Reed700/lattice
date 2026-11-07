/// A header value that is either one number for every layer or one number per layer. llama.cpp
/// accepts both for head counts, and Gemma 4 writes its KV-head count per layer because its global
/// layers have fewer heads than its sliding ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PerLayer {
    Uniform(u32),
    Layers(Vec<u32>),
}

impl PerLayer {
    /// The value for one layer, or `None` past the end of a per-layer list.
    pub fn at(&self, layer: usize) -> Option<u32> {
        match self {
            Self::Uniform(value) => Some(*value),
            Self::Layers(values) => values.get(layer).copied(),
        }
    }

    /// The largest value of any layer.
    pub fn widest(&self) -> Option<u32> {
        match self {
            Self::Uniform(value) => Some(*value),
            Self::Layers(values) => values.iter().copied().max(),
        }
    }
}

impl From<u32> for PerLayer {
    fn from(value: u32) -> Self {
        Self::Uniform(value)
    }
}

/// llama.cpp stores keys and values as f16 by default: two bytes an element.
const KV_CACHE_BYTES_PER_ELEMENT: u64 = 2;
/// When KV cache quantization (`q8_0`) is enabled, each element takes ~1 byte.
const KV_CACHE_QUANTIZED_BYTES_PER_ELEMENT: u64 = 1;

/// Slots llama-server opens when it is launched without `--parallel`, as the
/// sidecar is: four sequences over one unified cache. A sliding-window layer
/// keeps a window for each of them.
const SERVER_SLOTS: u64 = 4;

/// llama.cpp's default micro-batch. A sliding-window layer also holds the
/// batch being decoded past its window.
pub const MICRO_BATCH_TOKENS: u32 = 512;

/// llama.cpp rounds a sliding-window cache up to a multiple of this many cells.
const SLIDING_CACHE_ALIGNMENT: u64 = 256;

/// llama.cpp keeps recurrent state and computes activations in f32.
const F32_BYTES: u64 = 4;

/// One cell of the attention mask: the f32 mask plus the f16 copy flash
/// attention reads.
const MASK_BYTES_PER_CELL: u64 = 6;

/// One layer that keeps a KV cache of its own.
struct LayerCache {
    /// Key plus value bytes for one cached token.
    bytes_per_token: u64,
    /// Whether the cache stops growing at the sliding window.
    sliding: bool,
}

/// What a GGUF header says about the memory a server of it holds. The one description the runtime
/// and the recommender price a model from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelShape {
    /// Transformer layers, counting any multi-token-prediction blocks at the end
    /// (`nextn_predict_layers`).
    pub block_count: Option<u32>,
    /// Key/value heads, for every layer or per layer.
    pub head_count_kv: Option<PerLayer>,
    /// Attention heads, used to derive the head dimension when the header does not state
    /// key/value lengths outright.
    pub head_count: Option<PerLayer>,
    /// Key and value widths of the layers that attend to the whole window.
    pub key_length: Option<u32>,
    pub value_length: Option<u32>,
    /// Key and value widths of sliding-window layers, where they differ.
    pub key_length_swa: Option<u32>,
    pub value_length_swa: Option<u32>,
    pub embedding_length: Option<u32>,
    /// Tokens a sliding-window layer attends to.
    pub sliding_window: Option<u32>,
    /// Which layers slide, one flag per layer.
    pub sliding_window_pattern: Option<Vec<bool>>,
    /// Hybrid models: one layer in this many attends to the whole window; the others are
    /// linear-attention layers whose recurrent state does not grow with the window.
    pub full_attention_interval: Option<u32>,
    /// Multi-token-prediction blocks at the end of `block_count`.
    pub nextn_predict_layers: Option<u32>,
    /// Trailing layers that reuse an earlier layer's cache rather than keep their own.
    pub shared_kv_layers: Option<u32>,
    /// Width of the feed-forward layer; the widest, where it varies by layer.
    pub feed_forward_length: Option<u32>,
    /// A hybrid's linear-attention layers (`{architecture}.ssm.*`): the convolution's width, the
    /// state each head keeps, the key groups and the inner width.
    pub ssm_conv_kernel: Option<u32>,
    pub ssm_state_size: Option<u32>,
    pub ssm_group_count: Option<u32>,
    pub ssm_inner_size: Option<u32>,
}

impl ModelShape {
    /// Bytes of KV cache llama-server allocates for a window of
    /// `context_tokens`, when the header says enough to work it out.
    ///
    /// Counted the way llama.cpp allocates it rather than as if every layer
    /// attended to the whole window:
    /// - a layer that reuses an earlier layer's cache (`shared_kv_layers`)
    ///   costs nothing;
    /// - a hybrid's linear-attention layers (all but one in
    ///   `full_attention_interval`) keep a fixed-size recurrent state instead,
    ///   which is not counted here;
    /// - a sliding-window layer holds at most its window for each server slot
    ///   plus one micro-batch, rounded up to 256 cells;
    /// - every other layer holds the whole window, at its own KV-head count
    ///   and key/value widths.
    ///
    /// A plain model therefore costs `blocks * kv_heads * (key_len +
    /// value_len) * 2` bytes per token, as before. For 32 layers with 4 KV
    /// heads and 256-wide keys that is 128 KiB per token; Qwen 3.5 9B has that
    /// shape but attends to the whole window in only 8 of its 32 layers, so it
    /// costs 32 KiB.
    ///
    /// Multi-token-prediction blocks are charged as full attention.
    /// llama-server allocates their cache only when it drafts with them, which
    /// the header cannot say, so the estimate errs by one layer towards a
    /// window that fits.
    pub fn kv_cache_bytes(&self, context_tokens: u32, quantized: bool) -> Option<u64> {
        let bytes_per_element = if quantized {
            KV_CACHE_QUANTIZED_BYTES_PER_ELEMENT
        } else {
            KV_CACHE_BYTES_PER_ELEMENT
        };
        let layers = self.layer_caches(bytes_per_element)?;
        if layers.iter().all(|layer| layer.bytes_per_token == 0) {
            return None;
        }

        let tokens = u64::from(context_tokens);
        // A layer is only marked sliding when the window is known.
        let window = u64::from(self.sliding_window.unwrap_or(0));
        let sliding_cap = (window * SERVER_SLOTS + u64::from(MICRO_BATCH_TOKENS))
            .div_ceil(SLIDING_CACHE_ALIGNMENT)
            * SLIDING_CACHE_ALIGNMENT;
        let sliding_tokens = tokens.min(sliding_cap);

        layers.iter().try_fold(0u64, |total, layer| {
            let cached = if layer.sliding {
                sliding_tokens
            } else {
                tokens
            };
            total.checked_add(layer.bytes_per_token.checked_mul(cached)?)
        })
    }

    /// Every layer that keeps a KV cache of its own, with what one token of
    /// it costs. `None` when a layer's heads or widths cannot be worked out.
    fn layer_caches(&self, bytes_per_element: u64) -> Option<Vec<LayerCache>> {
        let blocks = self.block_count?;
        let trunk = blocks.saturating_sub(self.nextn_predict_layers.unwrap_or(0));
        // Layers from here on read an earlier layer's cache.
        let with_own_cache = blocks
            .checked_sub(self.shared_kv_layers.unwrap_or(0))
            .unwrap_or(blocks);
        let kv_heads = self.head_count_kv.as_ref().or(self.head_count.as_ref())?;

        // Prefer the stated key/value widths. Otherwise every head splits the
        // embedding evenly, which is the usual arrangement.
        let head_dim = || {
            let embedding = self.embedding_length?;
            let heads = self.head_count.as_ref()?.at(0)?;
            (heads > 0).then(|| embedding / heads)
        };
        let full_key = || self.key_length.or_else(head_dim);
        let full_value = || self.value_length.or_else(head_dim);

        (0..with_own_cache)
            .filter(|&layer| !self.is_recurrent(layer, trunk))
            .map(|layer| {
                let index = usize::try_from(layer).ok()?;
                let sliding = self.is_sliding(index);
                let (key, value) = if sliding {
                    (
                        self.key_length_swa.or_else(full_key)?,
                        self.value_length_swa.or_else(full_value)?,
                    )
                } else {
                    (full_key()?, full_value()?)
                };
                let bytes_per_token = u64::from(kv_heads.at(index)?)
                    .checked_mul(u64::from(key).checked_add(u64::from(value))?)?
                    .checked_mul(bytes_per_element)?;
                Some(LayerCache {
                    bytes_per_token,
                    sliding,
                })
            })
            .collect()
    }

    /// Bytes of recurrent state llama-server keeps for a hybrid's
    /// linear-attention layers.
    ///
    /// Unlike the KV cache it does not grow with the window: each such layer
    /// holds, for every server slot, a convolution state of
    /// `(conv_kernel - 1) × (inner + 2 × groups × state)` values and a
    /// recurrent state of `state × inner`, in f32. Qwen 3.5 9B's 24 such
    /// layers hold about 0.2 GB. 0 for a model without them, or whose header
    /// does not describe them.
    pub fn recurrent_state_bytes(&self) -> u64 {
        let (Some(blocks), Some(kernel), Some(state), Some(inner)) = (
            self.block_count,
            self.ssm_conv_kernel,
            self.ssm_state_size,
            self.ssm_inner_size,
        ) else {
            return 0;
        };
        let trunk = blocks.saturating_sub(self.nextn_predict_layers.unwrap_or(0));
        let layers = (0..blocks)
            .filter(|&layer| self.is_recurrent(layer, trunk))
            .count() as u64;
        let (state, inner) = (u64::from(state), u64::from(inner));
        let groups = u64::from(self.ssm_group_count.unwrap_or(0));
        let convolution = u64::from(kernel.saturating_sub(1)) * (inner + 2 * groups * state);
        layers * SERVER_SLOTS * (convolution + state * inner) * F32_BYTES
    }

    /// Bytes of compute buffer llama-server reserves for a window of
    /// `context_tokens` at a micro-batch of `micro_batch` tokens: one
    /// micro-batch of a layer, which holds the attention mask over the whole
    /// window beside the feed-forward activations (gate and up).
    ///
    /// Logits are not in it: the graph is reserved for one output a slot, and
    /// they live in a host buffer of their own.
    ///
    /// Flash attention is assumed. llama.cpp's `-fa auto` turns it on wherever
    /// the backend supports it; without it, attention scratch grows with the
    /// window times the head count. A term the header does not give counts as
    /// nothing.
    pub fn compute_buffer_bytes(&self, context_tokens: u32, micro_batch: u32) -> u64 {
        let layer = u64::from(context_tokens) * MASK_BYTES_PER_CELL
            + u64::from(self.feed_forward_length.unwrap_or(0)) * 2 * F32_BYTES;
        u64::from(micro_batch) * layer
    }

    /// llama.cpp's rule for hybrids: counting from one, every
    /// `full_attention_interval`-th layer attends to the whole window and the
    /// rest are recurrent. Multi-token-prediction blocks past the trunk are
    /// attention layers.
    fn is_recurrent(&self, layer: u32, trunk: u32) -> bool {
        match self.full_attention_interval {
            Some(interval) if interval > 0 => {
                layer < trunk && !(layer + 1).is_multiple_of(interval)
            }
            _ => false,
        }
    }

    fn is_sliding(&self, layer: usize) -> bool {
        self.sliding_window.is_some_and(|window| window > 0)
            && self
                .sliding_window_pattern
                .as_ref()
                .and_then(|pattern| pattern.get(layer))
                .is_some_and(|&slides| slides)
    }
}

/// The memory facts pinned for one GGUF file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedModel {
    pub shape: ModelShape,
    /// Bytes of the file that stay in system RAM however many layers are offloaded.
    pub cpu_weight_bytes: u64,
}
