use super::*;
use std::io::Write;

struct GgufBuilder {
    bytes: Vec<u8>,
    kv_count: u64,
    body: Vec<u8>,
}

impl GgufBuilder {
    fn new(version: u32) -> Self {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&GGUF_MAGIC.to_le_bytes());
        bytes.extend_from_slice(&version.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes()); // tensor count
        Self {
            bytes,
            kv_count: 0,
            body: Vec::new(),
        }
    }

    fn key(&mut self, key: &str) {
        self.body
            .extend_from_slice(&(key.len() as u64).to_le_bytes());
        self.body.extend_from_slice(key.as_bytes());
        self.kv_count += 1;
    }

    fn string(mut self, key: &str, value: &str) -> Self {
        self.key(key);
        self.body
            .extend_from_slice(&value_type::STRING.to_le_bytes());
        self.body
            .extend_from_slice(&(value.len() as u64).to_le_bytes());
        self.body.extend_from_slice(value.as_bytes());
        self
    }

    fn u32_value(mut self, key: &str, value: u32) -> Self {
        self.key(key);
        self.body
            .extend_from_slice(&value_type::UINT32.to_le_bytes());
        self.body.extend_from_slice(&value.to_le_bytes());
        self
    }

    fn u64_value(mut self, key: &str, value: u64) -> Self {
        self.key(key);
        self.body
            .extend_from_slice(&value_type::UINT64.to_le_bytes());
        self.body.extend_from_slice(&value.to_le_bytes());
        self
    }

    fn float(mut self, key: &str, value: f32) -> Self {
        self.key(key);
        self.body
            .extend_from_slice(&value_type::FLOAT32.to_le_bytes());
        self.body.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// A tokenizer vocabulary, the reason skipping has to be efficient and
    /// correct: it sits between the keys that matter.
    fn string_array(mut self, key: &str, values: &[&str]) -> Self {
        self.key(key);
        self.body
            .extend_from_slice(&value_type::ARRAY.to_le_bytes());
        self.body
            .extend_from_slice(&value_type::STRING.to_le_bytes());
        self.body
            .extend_from_slice(&(values.len() as u64).to_le_bytes());
        for value in values {
            self.body
                .extend_from_slice(&(value.len() as u64).to_le_bytes());
            self.body.extend_from_slice(value.as_bytes());
        }
        self
    }

    fn array_header(&mut self, key: &str, element_type: u32, count: usize) {
        self.key(key);
        self.body
            .extend_from_slice(&value_type::ARRAY.to_le_bytes());
        self.body.extend_from_slice(&element_type.to_le_bytes());
        self.body.extend_from_slice(&(count as u64).to_le_bytes());
    }

    fn i32_array(mut self, key: &str, values: &[i32]) -> Self {
        self.array_header(key, value_type::INT32, values.len());
        for value in values {
            self.body.extend_from_slice(&value.to_le_bytes());
        }
        self
    }

    fn bool_array(mut self, key: &str, values: &[bool]) -> Self {
        self.array_header(key, value_type::BOOL, values.len());
        for value in values {
            self.body.push(u8::from(*value));
        }
        self
    }

    fn float_array(mut self, key: &str, values: &[f32]) -> Self {
        self.array_header(key, value_type::FLOAT32, values.len());
        for value in values {
            self.body.extend_from_slice(&value.to_le_bytes());
        }
        self
    }

    fn write(self) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let mut bytes = self.bytes;
        bytes.extend_from_slice(&self.kv_count.to_le_bytes());
        bytes.extend_from_slice(&self.body);
        file.write_all(&bytes).unwrap();
        file.flush().unwrap();
        file
    }

    fn read(self) -> GgufModelInfo {
        let file = self.write();
        read_model_info(file.path()).unwrap()
    }
}

/// Gemma 4's layer layout: five sliding layers, then one global, repeated.
fn gemma4_pattern(layers: usize) -> Vec<bool> {
    (0..layers).map(|layer| layer % 6 != 5).collect()
}

/// The sizing keys of `unsloth/Qwen3.5-9B-GGUF` `Qwen3.5-9B-Q4_K_M.gguf`,
/// in the file's own order, read from its header by HTTP range request.
/// The tokenizer is cut down to two tokens.
fn qwen35_9b_header() -> GgufBuilder {
    GgufBuilder::new(3)
        .string("general.architecture", "qwen35")
        .u32_value("qwen35.block_count", 32)
        .u32_value("qwen35.context_length", 262_144)
        .u32_value("qwen35.embedding_length", 4096)
        .u32_value("qwen35.feed_forward_length", 12_288)
        .u32_value("qwen35.attention.head_count", 16)
        .u32_value("qwen35.attention.head_count_kv", 4)
        .i32_array("qwen35.rope.dimension_sections", &[11, 11, 10, 0])
        .float("qwen35.rope.freq_base", 10_000_000.0)
        .float("qwen35.attention.layer_norm_rms_epsilon", 1e-6)
        .u32_value("qwen35.attention.key_length", 256)
        .u32_value("qwen35.attention.value_length", 256)
        .u32_value("qwen35.ssm.conv_kernel", 4)
        .u32_value("qwen35.ssm.state_size", 128)
        .u32_value("qwen35.ssm.group_count", 16)
        .u32_value("qwen35.ssm.time_step_rank", 32)
        .u32_value("qwen35.ssm.inner_size", 4096)
        .u32_value("qwen35.full_attention_interval", 4)
        .u32_value("qwen35.rope.dimension_count", 64)
        .string_array("tokenizer.ggml.tokens", &["a", "b"])
}

/// Gemma 4 12B's KV heads: 8 on each sliding layer, 1 on each global one.
fn gemma4_12b_kv_heads() -> Vec<u32> {
    gemma4_pattern(48)
        .iter()
        .map(|&slides| if slides { 8 } else { 1 })
        .collect()
}

/// `unsloth/gemma-4-12b-it-GGUF` `gemma-4-12b-it-Q4_K_M.gguf`, in the
/// file's order. Its KV-head count is a per-layer list: 8 on sliding
/// layers, 1 on global ones.
fn gemma4_12b_header() -> GgufBuilder {
    let heads: Vec<i32> = gemma4_12b_kv_heads()
        .into_iter()
        .map(|h| h as i32)
        .collect();
    GgufBuilder::new(3)
        .string("general.architecture", "gemma4")
        .u32_value("gemma4.block_count", 48)
        .u32_value("gemma4.context_length", 262_144)
        .u32_value("gemma4.embedding_length", 3840)
        .u32_value("gemma4.feed_forward_length", 15_360)
        .u32_value("gemma4.attention.head_count", 16)
        .i32_array("gemma4.attention.head_count_kv", &heads)
        .float("gemma4.rope.freq_base", 1_000_000.0)
        .float("gemma4.rope.freq_base_swa", 10_000.0)
        .float("gemma4.attention.layer_norm_rms_epsilon", 1e-6)
        .u32_value("gemma4.attention.key_length", 512)
        .u32_value("gemma4.attention.value_length", 512)
        .float("gemma4.final_logit_softcapping", 30.0)
        .u32_value("gemma4.attention.sliding_window", 1024)
        .u32_value("gemma4.attention.shared_kv_layers", 0)
        .u32_value("gemma4.embedding_length_per_layer_input", 0)
        .bool_array(
            "gemma4.attention.sliding_window_pattern",
            &gemma4_pattern(48),
        )
        .u32_value("gemma4.attention.key_length_swa", 256)
        .u32_value("gemma4.attention.value_length_swa", 256)
        .u32_value("gemma4.rope.dimension_count", 512)
        .u32_value("gemma4.rope.dimension_count_swa", 256)
        .i32_array("tokenizer.ggml.suppress_tokens", &[258_883, 258_882])
        .string_array("tokenizer.ggml.tokens", &["a", "b"])
}

#[test]
fn the_trained_context_length_is_read_for_its_own_architecture() {
    let file = GgufBuilder::new(3)
        .string("general.architecture", "qwen3")
        .string_array("tokenizer.ggml.tokens", &["hello", "world", "<|im_start|>"])
        .u32_value("qwen3.block_count", 32)
        .u32_value("qwen3.context_length", 32768)
        .float("qwen3.attention.layer_norm_rms_epsilon", 1e-6)
        .write();

    let info = read_model_info(file.path()).unwrap();

    assert_eq!(info.architecture.as_deref(), Some("qwen3"));
    assert_eq!(info.trained_context_length, Some(32768));
}

/// The keys and values of the pinned `Qwen3-Embedding-0.6B-f16.gguf`
/// header (`general.file_type` 1 = F16, `qwen3.pooling_type` 3 = last),
/// confirmed against the file itself.
#[test]
fn an_embedding_header_names_its_pooling_and_precision() {
    let info = GgufBuilder::new(3)
        .string("general.architecture", "qwen3")
        .u32_value("qwen3.context_length", 32768)
        .u32_value("qwen3.embedding_length", 1024)
        .u32_value("general.file_type", 1)
        .u32_value("qwen3.pooling_type", 3)
        .read();

    assert_eq!(info.pooling_type, Some(3));
    assert_eq!(info.file_type, Some(1));
    assert_eq!(info.embedding_length, Some(1024));
}

#[test]
fn a_chat_header_has_no_pooling_type() {
    let info = qwen35_9b_header().read();
    assert_eq!(info.pooling_type, None);
}

/// Keys arrive in whatever order the converter wrote them, so the
/// architecture cannot be assumed to come first.
#[test]
fn the_context_length_is_found_before_the_architecture_that_names_it() {
    let file = GgufBuilder::new(3)
        .u32_value("llama.context_length", 4096)
        .string("general.architecture", "llama")
        .write();

    assert_eq!(
        read_model_info(file.path()).unwrap().trained_context_length,
        Some(4096)
    );
}

/// A context length written as a 64-bit value is still a context length.
#[test]
fn a_context_length_of_any_integer_width_is_understood() {
    let file = GgufBuilder::new(3)
        .string("general.architecture", "llama")
        .u64_value("llama.context_length", 8192)
        .write();

    assert_eq!(
        read_model_info(file.path()).unwrap().trained_context_length,
        Some(8192)
    );
}

/// Several architectures' keys in one file must not be guessed between.
#[test]
fn an_ambiguous_context_length_without_an_architecture_is_not_guessed() {
    let file = GgufBuilder::new(3)
        .u32_value("llama.context_length", 4096)
        .u32_value("qwen3.context_length", 32768)
        .write();

    assert_eq!(
        read_model_info(file.path()).unwrap().trained_context_length,
        None
    );
}

#[test]
fn a_file_that_is_not_gguf_is_declined_rather_than_misread() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"this is not a model").unwrap();
    file.flush().unwrap();

    assert!(read_model_info(file.path()).is_none());
}

#[test]
fn a_missing_file_is_declined() {
    assert!(read_model_info(Path::new("/nonexistent/model.gguf")).is_none());
}

/// A truncated download must not panic or hang; whatever was already read
/// is still usable.
#[test]
fn a_truncated_header_yields_what_was_read_so_far() {
    let file = GgufBuilder::new(3)
        .string("general.architecture", "llama")
        .u32_value("llama.context_length", 4096)
        .write();
    let mut bytes = std::fs::read(file.path()).unwrap();
    bytes.truncate(bytes.len() - 3);
    let truncated = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(truncated.path(), &bytes).unwrap();

    let info = read_model_info(truncated.path()).unwrap();

    assert_eq!(info.architecture.as_deref(), Some("llama"));
}

#[test]
fn a_version_one_file_is_declined_rather_than_misparsed() {
    let file = GgufBuilder::new(1)
        .string("general.architecture", "llama")
        .write();

    assert!(read_model_info(file.path()).is_none());
}

/// A plain model, every layer attending to the whole window: 32 blocks, 4
/// KV heads, 256-wide keys and values. 128 KiB per token, and the same for
/// every token however long the window, which is what makes a
/// 262144-trained model of this shape impossible to run at 262144 on
/// consumer hardware.
#[test]
fn a_grouped_query_cache_costs_what_its_dimensions_say() {
    let info = GgufModelInfo {
        block_count: Some(32),
        head_count_kv: Some(4.into()),
        key_length: Some(256),
        value_length: Some(256),
        ..Default::default()
    };

    assert_eq!(info.kv_cache_bytes(1), Some(128 * 1024));
    assert_eq!(info.kv_cache_bytes(8192), Some(8192 * 128 * 1024));
}

/// Older headers state no key/value width, so the head dimension comes from
/// the embedding split across the attention heads.
#[test]
fn a_header_without_key_widths_derives_them_from_the_embedding() {
    let info = GgufModelInfo {
        block_count: Some(32),
        head_count: Some(32.into()),
        head_count_kv: Some(32.into()),
        embedding_length: Some(4096),
        ..Default::default()
    };

    // 4096 / 32 = 128 per head, key and value: 32 * 32 * 256 * 2.
    assert_eq!(info.kv_cache_bytes(1), Some(32 * 32 * 256 * 2));
}

/// Without the dimensions there is no honest answer, and a guess would size
/// a cache that does not fit.
#[test]
fn an_incomplete_header_refuses_to_estimate() {
    let info = GgufModelInfo {
        block_count: Some(32),
        ..Default::default()
    };

    assert_eq!(info.kv_cache_bytes(8192), None);
}

/// `bartowski/Meta-Llama-3.1-8B-Instruct-GGUF`
/// `Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf`, in the file's order: no key
/// widths, so 4096 / 32 = 128 per head, and 32 * 8 * 256 * 2 = 128 KiB a
/// token at any window.
#[test]
fn a_plain_model_costs_the_same_for_every_token_of_its_window() {
    let info = GgufBuilder::new(3)
        .string("general.architecture", "llama")
        .string_array("general.tags", &["facebook", "meta"])
        .u32_value("llama.block_count", 32)
        .u32_value("llama.context_length", 131_072)
        .u32_value("llama.embedding_length", 4096)
        .u32_value("llama.feed_forward_length", 14_336)
        .u32_value("llama.attention.head_count", 32)
        .u32_value("llama.attention.head_count_kv", 8)
        .float("llama.rope.freq_base", 500_000.0)
        .float("llama.attention.layer_norm_rms_epsilon", 1e-5)
        .u32_value("general.file_type", 15)
        .u32_value("llama.vocab_size", 128_256)
        .u32_value("llama.rope.dimension_count", 128)
        .string_array("tokenizer.ggml.tokens", &["a", "b"])
        .read();

    assert_eq!(info.trained_context_length, Some(131_072));
    assert_eq!(info.kv_cache_bytes(1), Some(131_072));
    for window in [4096, 8192, 32_768, 131_072] {
        assert_eq!(
            info.kv_cache_bytes(window),
            Some(u64::from(window) * 131_072)
        );
    }
}

/// Qwen 3.5 9B has the shape of the 128 KiB model above, but only layers
/// 4, 8, ..., 32 attend to the whole window. The other 24 are Gated
/// DeltaNet layers with a fixed recurrent state, so the cache is 8 * 4 *
/// 512 * 2 = 32 KiB a token: a quarter of what counting every layer said.
#[test]
fn a_qwen_hybrid_is_charged_for_its_full_attention_layers_only() {
    let info = qwen35_9b_header().read();

    assert_eq!(info.full_attention_interval, Some(4));
    assert_eq!(info.head_count_kv, Some(PerLayer::Uniform(4)));
    assert_eq!(info.kv_cache_bytes(1), Some(32 * 1024));
    assert_eq!(info.kv_cache_bytes(32_768), Some(1 << 30));

    let every_layer = GgufModelInfo {
        full_attention_interval: None,
        ..info.clone()
    };
    assert_eq!(
        every_layer.kv_cache_bytes(32_768),
        info.kv_cache_bytes(32_768).map(|bytes| bytes * 4)
    );
}

/// `unsloth/Qwen3.6-35B-A3B-GGUF` `Qwen3.6-35B-A3B-UD-Q4_K_XL.gguf`: 40
/// blocks, 2 KV heads, so 10 full-attention layers at 2 * 512 * 2 bytes.
#[test]
fn the_mixture_of_experts_hybrid_counts_one_layer_in_four() {
    let info = GgufBuilder::new(3)
        .string("general.architecture", "qwen35moe")
        .u32_value("qwen35moe.block_count", 40)
        .u32_value("qwen35moe.context_length", 262_144)
        .u32_value("qwen35moe.embedding_length", 2048)
        .u32_value("qwen35moe.attention.head_count", 16)
        .u32_value("qwen35moe.attention.head_count_kv", 2)
        .i32_array("qwen35moe.rope.dimension_sections", &[11, 11, 10, 0])
        .u32_value("qwen35moe.expert_count", 256)
        .u32_value("qwen35moe.expert_used_count", 8)
        .u32_value("qwen35moe.attention.key_length", 256)
        .u32_value("qwen35moe.attention.value_length", 256)
        .u32_value("qwen35moe.full_attention_interval", 4)
        .read();

    assert_eq!(info.kv_cache_bytes(1), Some(10 * 2 * 512 * 2));
    assert_eq!(info.kv_cache_bytes(32_768), Some(671_088_640));
}

/// A multi-token-prediction block sits past the trunk and attends to the
/// whole window when the server drafts with it. The 9B MTP build (33 blocks)
/// is charged 8 + 1 layers; Qwen 3.8 27B (65 blocks) 16 + 1.
#[test]
fn multi_token_prediction_blocks_are_charged_as_full_attention() {
    let mtp_9b = GgufModelInfo {
        block_count: Some(33),
        nextn_predict_layers: Some(1),
        ..qwen35_9b_header().read()
    };
    let qwen38_27b = GgufModelInfo {
        architecture: Some("qwen35".into()),
        block_count: Some(65),
        nextn_predict_layers: Some(1),
        head_count: Some(24.into()),
        head_count_kv: Some(4.into()),
        key_length: Some(256),
        value_length: Some(256),
        embedding_length: Some(5120),
        full_attention_interval: Some(4),
        ..Default::default()
    };

    assert_eq!(mtp_9b.kv_cache_bytes(1), Some(9 * 4 * 512 * 2));
    assert_eq!(qwen38_27b.kv_cache_bytes(1), Some(17 * 4 * 512 * 2));
}

/// The reader keeps going past Gemma 4's per-layer KV-head list, so the key
/// widths and sliding window after it are read.
#[test]
fn a_per_layer_list_is_read_and_the_keys_after_it_still_count() {
    let info = gemma4_12b_header().read();

    assert_eq!(
        info.head_count_kv,
        Some(PerLayer::Layers(gemma4_12b_kv_heads()))
    );
    assert_eq!(info.key_length, Some(512));
    assert_eq!(info.key_length_swa, Some(256));
    assert_eq!(info.value_length_swa, Some(256));
    assert_eq!(info.sliding_window, Some(1024));
    assert_eq!(info.sliding_window_pattern, Some(gemma4_pattern(48)));
    assert_eq!(info.shared_kv_layers, Some(0));
}

/// Gemma 4 12B: 8 global layers of 1 KV head at 512 + 512 (16 KiB a token
/// between them) and 40 sliding layers of 8 heads at 256 + 256 (320 KiB a
/// token). The sliding layers stop at 1024 tokens for each of the
/// server's 4 slots plus a 512-token batch: 4608 cells.
#[test]
fn sliding_window_layers_stop_growing_at_their_window() {
    let info = gemma4_12b_header().read();
    let global = 8 * 1024 * 2;
    let sliding = 40 * 8 * 512 * 2;

    assert_eq!(info.kv_cache_bytes(1), Some(global + sliding));
    assert_eq!(info.kv_cache_bytes(4096), Some((global + sliding) * 4096));
    assert_eq!(
        info.kv_cache_bytes(32_768),
        Some(global * 32_768 + sliding * 4608)
    );
    assert_eq!(info.kv_cache_bytes(32_768), Some(2_046_820_352));
}

/// `unsloth/gemma-4-E4B-it-GGUF` `gemma-4-E4B-it-Q4_K_M.gguf`: the last 18
/// of 42 layers read earlier layers' caches. Of the 24 that keep one, 4
/// are global (2 heads at 512 + 512) and 20 slide over 512 tokens (2 heads
/// at 256 + 256), capped at 4 * 512 + 512 = 2560 cells.
#[test]
fn layers_that_share_an_earlier_cache_cost_nothing() {
    let info = GgufBuilder::new(3)
        .string("general.architecture", "gemma4")
        .u32_value("gemma4.block_count", 42)
        .u32_value("gemma4.context_length", 131_072)
        .u32_value("gemma4.embedding_length", 2560)
        .u32_value("gemma4.attention.head_count", 8)
        .u32_value("gemma4.attention.head_count_kv", 2)
        .u32_value("gemma4.attention.key_length", 512)
        .u32_value("gemma4.attention.value_length", 512)
        .u32_value("gemma4.attention.sliding_window", 512)
        .u32_value("gemma4.attention.shared_kv_layers", 18)
        .u32_value("gemma4.embedding_length_per_layer_input", 256)
        .bool_array(
            "gemma4.attention.sliding_window_pattern",
            &gemma4_pattern(42),
        )
        .u32_value("gemma4.attention.key_length_swa", 256)
        .u32_value("gemma4.attention.value_length_swa", 256)
        .read();
    let global = 4 * 2 * 1024 * 2;
    let sliding = 20 * 2 * 512 * 2;

    assert_eq!(info.kv_cache_bytes(1), Some(global + sliding));
    assert_eq!(
        info.kv_cache_bytes(32_768),
        Some(global * 32_768 + sliding * 2560)
    );
}

/// A list that does not cover every layer cannot say what the missing
/// layers cost.
#[test]
fn a_per_layer_list_shorter_than_the_model_refuses_to_estimate() {
    let info = GgufModelInfo {
        head_count_kv: Some(PerLayer::Layers(vec![8; 10])),
        ..gemma4_12b_header().read()
    };

    assert_eq!(info.kv_cache_bytes(8192), None);
}

/// A sizing key written in a type it should not have is passed over, and
/// the keys after it are still read.
#[test]
fn a_sizing_key_of_an_unexpected_type_is_passed_over() {
    let info = GgufBuilder::new(3)
        .string("general.architecture", "llama")
        .float_array("llama.attention.head_count_kv", &[8.0, 8.0])
        .float("llama.block_count", 32.0)
        .u32_value("llama.attention.head_count", 32)
        .u32_value("llama.embedding_length", 4096)
        .read();

    assert_eq!(info.head_count_kv, None);
    assert_eq!(info.block_count, None);
    assert_eq!(info.head_count, Some(PerLayer::Uniform(32)));
    assert_eq!(info.embedding_length, Some(4096));
}

/// A vision tower's keys share the model's prefix but are not its layers.
#[test]
fn a_nested_key_is_not_mistaken_for_the_language_model_s() {
    let info = GgufBuilder::new(3)
        .string("general.architecture", "gemma4")
        .u32_value("gemma4.block_count", 48)
        .u32_value("gemma4.vision.block_count", 27)
        .read();

    assert_eq!(info.block_count, Some(48));
}

/// Synthetic fixtures only prove the parser agrees with itself. This reads a
/// GGUF that actually exists on the machine, when one does.
#[test]
#[ignore = "requires LATTICE_GGUF_MODEL to point at a .gguf on this machine"]
fn a_real_gguf_on_this_machine_parses() {
    let path = std::env::var("LATTICE_GGUF_MODEL").unwrap();
    let info = read_model_info(Path::new(&path)).expect("a real GGUF must parse");

    println!("{info:#?}");
    println!("kv bytes at 1: {:?}", info.kv_cache_bytes(1));
    println!("kv bytes at 8192: {:?}", info.kv_cache_bytes(8192));
    println!("kv bytes at 32768: {:?}", info.kv_cache_bytes(32_768));

    assert!(info.architecture.is_some());
    assert!(info.trained_context_length.is_some());
    assert!(info.kv_cache_bytes(8192).is_some());
}

#[test]
fn the_header_gives_what_the_compute_buffer_and_recurrent_state_are_priced_from() {
    let info = qwen35_9b_header().read();

    assert_eq!(info.feed_forward_length, Some(12_288));
    assert_eq!(
        info.vocab_size,
        Some(2),
        "counted from tokenizer.ggml.tokens"
    );
    assert_eq!(info.ssm_conv_kernel, Some(4));
    assert_eq!(info.ssm_state_size, Some(128));
    assert_eq!(info.ssm_group_count, Some(16));
    assert_eq!(info.ssm_inner_size, Some(4096));
}

#[test]
fn a_stated_vocabulary_size_wins_over_the_token_count() {
    let info = qwen35_9b_header()
        .u32_value("qwen35.vocab_size", 248_320)
        .read();

    assert_eq!(info.vocab_size, Some(248_320));
}

/// Qwen 3.5 9B: 24 of its 32 layers are linear attention. Each keeps, for each
/// of the server's four slots, `3 * (4096 + 2 * 16 * 128)` convolution values
/// and `128 * 4096` recurrent ones, in f32.
#[test]
fn a_qwen_hybrid_s_recurrent_state_is_fixed_per_linear_layer_and_slot() {
    let info = qwen35_9b_header().read();

    let per_layer_and_slot = (3 * (4096 + 2 * 16 * 128) + 128 * 4096) * 4;
    assert_eq!(info.recurrent_state_bytes(), 24 * 4 * per_layer_and_slot);
}

#[test]
fn a_model_without_linear_attention_keeps_no_recurrent_state() {
    let hybrid = qwen35_9b_header().read();
    let plain = GgufModelInfo {
        full_attention_interval: None,
        ..hybrid.clone()
    };
    let undescribed = GgufModelInfo {
        ssm_inner_size: None,
        ..hybrid
    };

    assert_eq!(plain.recurrent_state_bytes(), 0);
    assert_eq!(undescribed.recurrent_state_bytes(), 0);
}

/// The output's logits outweigh a layer's scratch at a short window; the
/// attention mask over a long window outweighs the logits.
#[test]
fn the_compute_buffer_is_the_larger_of_the_output_and_a_layer() {
    let info = GgufModelInfo {
        vocab_size: Some(248_320),
        ..qwen35_9b_header().read()
    };

    let output = 512 * 248_320 * 4;
    assert_eq!(info.compute_buffer_bytes(8_192), output);
    let layer_at_full = 512 * (262_144 * 6 + 12_288 * 2 * 4);
    assert_eq!(info.compute_buffer_bytes(262_144), layer_at_full);
    assert!(layer_at_full > output);
}

#[test]
fn a_header_without_vocabulary_or_feed_forward_still_prices_the_mask() {
    let info = GgufModelInfo::default();

    assert_eq!(info.compute_buffer_bytes(1_024), 512 * 1_024 * 6);
}

/// Counts the reads that reach the file beneath a `BufReader`.
struct CountingReader<R> {
    inner: R,
    reads: usize,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reads += 1;
        self.inner.read(buf)
    }
}

fn vocabulary(tokens: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&value_type::STRING.to_le_bytes());
    bytes.extend_from_slice(&(tokens as u64).to_le_bytes());
    for index in 0..tokens {
        let token = format!("tok{index}");
        bytes.extend_from_slice(&(token.len() as u64).to_le_bytes());
        bytes.extend_from_slice(token.as_bytes());
    }
    bytes
}

/// Strings are read through a buffer rather than seeked past one by one, so
/// a large header takes a handful of system calls.
#[test]
fn a_vocabulary_is_skipped_a_buffer_at_a_time_not_a_read_per_token() {
    let mut bytes = vocabulary(100_000);
    bytes.extend_from_slice(&7u32.to_le_bytes());
    let mut reader = BufReader::new(CountingReader {
        inner: std::io::Cursor::new(bytes),
        reads: 0,
    });

    assert_eq!(skip_value(&mut reader, value_type::ARRAY), Some(()));
    assert_eq!(read_u32(&mut reader), Some(7));
    // About 1.6 MB: some 200 reads of 8 KiB, against 100,000 before.
    assert!(reader.get_ref().reads < 1_000, "{}", reader.get_ref().reads);
}

#[test]
fn a_value_longer_than_the_bytes_left_cannot_be_skipped() {
    let mut bytes = vocabulary(10);
    bytes.truncate(bytes.len() - 2);

    assert_eq!(
        skip_value(&mut std::io::Cursor::new(bytes), value_type::ARRAY),
        None
    );
}
