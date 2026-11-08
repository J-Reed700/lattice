use super::*;
use std::io::Write;

struct GgufBuilder {
    version: u32,
    kv_count: u64,
    body: Vec<u8>,
    /// Name and offset of each tensor, and the bytes of data after the list.
    tensors: Vec<(String, u64)>,
    data_bytes: usize,
}

impl GgufBuilder {
    fn new(version: u32) -> Self {
        Self {
            version,
            kv_count: 0,
            body: Vec::new(),
            tensors: Vec::new(),
            data_bytes: 0,
        }
    }

    fn tensors(mut self, tensors: &[(&str, u64)], data_bytes: usize) -> Self {
        self.tensors = tensors
            .iter()
            .map(|(name, offset)| ((*name).to_owned(), *offset))
            .collect();
        self.data_bytes = data_bytes;
        self
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
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&GGUF_MAGIC.to_le_bytes());
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&(self.tensors.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&self.kv_count.to_le_bytes());
        bytes.extend_from_slice(&self.body);
        for (name, offset) in &self.tensors {
            bytes.extend_from_slice(&(name.len() as u64).to_le_bytes());
            bytes.extend_from_slice(name.as_bytes());
            bytes.extend_from_slice(&2u32.to_le_bytes()); // dimensions
            bytes.extend_from_slice(&4096u64.to_le_bytes());
            bytes.extend_from_slice(&248_320u64.to_le_bytes());
            bytes.extend_from_slice(&12u32.to_le_bytes()); // Q4_K
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        bytes.resize(bytes.len() + self.data_bytes, 0);
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
    assert_eq!(info.shape.embedding_length, Some(1024));
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

/// `bartowski/Meta-Llama-3.1-8B-Instruct-GGUF`
/// `Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf`, in the file's order: no key widths.
#[test]
fn a_plain_models_header_is_read_into_its_shape() {
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
        .u32_value("llama.rope.dimension_count", 128)
        .string_array("tokenizer.ggml.tokens", &["a", "b"])
        .read();

    assert_eq!(info.trained_context_length, Some(131_072));
    assert_eq!(info.shape.block_count, Some(32));
    assert_eq!(info.shape.embedding_length, Some(4096));
    assert_eq!(info.shape.head_count, Some(PerLayer::Uniform(32)));
    assert_eq!(info.shape.head_count_kv, Some(PerLayer::Uniform(8)));
    assert_eq!(info.shape.key_length, None);
    assert_eq!(info.shape.feed_forward_length, Some(14_336));
}

#[test]
fn a_qwen_hybrids_attention_interval_and_heads_are_read_into_its_shape() {
    let info = qwen35_9b_header().read();

    assert_eq!(info.shape.full_attention_interval, Some(4));
    assert_eq!(info.shape.head_count_kv, Some(PerLayer::Uniform(4)));
    assert_eq!(info.shape.key_length, Some(256));
    assert_eq!(info.shape.value_length, Some(256));
}

/// `unsloth/Qwen3.6-35B-A3B-GGUF` `Qwen3.6-35B-A3B-UD-Q4_K_XL.gguf`.
#[test]
fn the_mixture_of_experts_hybrids_header_is_read_into_its_shape() {
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

    assert_eq!(info.shape.block_count, Some(40));
    assert_eq!(info.shape.head_count_kv, Some(PerLayer::Uniform(2)));
    assert_eq!(info.shape.full_attention_interval, Some(4));
}

/// The reader keeps going past Gemma 4's per-layer KV-head list, so the key
/// widths and sliding window after it are read.
#[test]
fn a_per_layer_list_is_read_and_the_keys_after_it_still_count() {
    let info = gemma4_12b_header().read();

    assert_eq!(
        info.shape.head_count_kv,
        Some(PerLayer::Layers(gemma4_12b_kv_heads()))
    );
    assert_eq!(info.shape.key_length, Some(512));
    assert_eq!(info.shape.key_length_swa, Some(256));
    assert_eq!(info.shape.value_length_swa, Some(256));
    assert_eq!(info.shape.sliding_window, Some(1024));
    assert_eq!(info.shape.sliding_window_pattern, Some(gemma4_pattern(48)));
    assert_eq!(info.shape.shared_kv_layers, Some(0));
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

    assert_eq!(info.shape.head_count_kv, None);
    assert_eq!(info.shape.block_count, None);
    assert_eq!(info.shape.head_count, Some(PerLayer::Uniform(32)));
    assert_eq!(info.shape.embedding_length, Some(4096));
}

/// A vision tower's keys share the model's prefix but are not its layers.
#[test]
fn a_nested_key_is_not_mistaken_for_the_language_model_s() {
    let info = GgufBuilder::new(3)
        .string("general.architecture", "gemma4")
        .u32_value("gemma4.block_count", 48)
        .u32_value("gemma4.vision.block_count", 27)
        .read();

    assert_eq!(info.shape.block_count, Some(48));
}

/// Synthetic fixtures only prove the parser agrees with itself. This reads a
/// GGUF that actually exists on the machine, when one does.
#[test]
#[ignore = "requires LATTICE_GGUF_MODEL to point at a .gguf on this machine"]
fn a_real_gguf_on_this_machine_parses() {
    let path = std::env::var("LATTICE_GGUF_MODEL").unwrap();
    let info = read_model_info(Path::new(&path)).expect("a real GGUF must parse");

    println!("{info:#?}");
    println!("kv bytes at 1: {:?}", info.shape.kv_cache_bytes(1, false));
    println!(
        "kv bytes at 8192: {:?}",
        info.shape.kv_cache_bytes(8192, false)
    );
    println!(
        "kv bytes at 32768: {:?}",
        info.shape.kv_cache_bytes(32_768, false)
    );

    assert!(info.architecture.is_some());
    assert!(info.trained_context_length.is_some());
    assert!(info.shape.kv_cache_bytes(8192, false).is_some());
}

#[test]
fn the_header_gives_what_the_compute_buffer_and_recurrent_state_are_priced_from() {
    let info = qwen35_9b_header().read();

    assert_eq!(info.shape.feed_forward_length, Some(12_288));
    assert_eq!(info.shape.ssm_conv_kernel, Some(4));
    assert_eq!(info.shape.ssm_state_size, Some(128));
    assert_eq!(info.shape.ssm_group_count, Some(16));
    assert_eq!(info.shape.ssm_inner_size, Some(4096));
}

/// Qwen3.5-9B keeps its 545.62 MiB `token_embd.weight` in `CPU_Mapped` and sends
/// a separate `output.weight` to the GPU.
#[test]
fn an_untied_model_leaves_its_input_embedding_on_the_cpu() {
    let info = qwen35_9b_header()
        .tensors(
            &[
                ("output.weight", 0),
                ("token_embd.weight", 1_000),
                ("blk.0.attn_norm.weight", 4_000),
            ],
            5_000,
        )
        .read();

    assert_eq!(info.cpu_weight_bytes, 3_000);
}

/// The offsets need not run in listed order, and the last tensor ends where
/// the file does.
#[test]
fn an_input_embedding_runs_to_the_next_offset_or_the_end_of_the_data() {
    let last = qwen35_9b_header()
        .tensors(&[("token_embd.weight", 4_000), ("output.weight", 0)], 5_000)
        .read();

    assert_eq!(last.cpu_weight_bytes, 1_000);
}

/// The embedding and reranking models tie their output to the input
/// embedding: llama.cpp keeps a copy in RAM and the whole file on the GPU.
#[test]
fn a_tied_model_keeps_all_of_its_file_on_the_gpu() {
    let info = qwen35_9b_header()
        .tensors(
            &[("token_embd.weight", 0), ("blk.0.ffn_up.weight", 1_000)],
            2_000,
        )
        .read();

    assert_eq!(info.cpu_weight_bytes, 0);
}

#[test]
fn a_tensor_list_cut_short_counts_nothing_off_the_file() {
    let file = qwen35_9b_header()
        .tensors(&[("token_embd.weight", 0), ("output.weight", 1_000)], 0)
        .write();
    let length = std::fs::metadata(file.path()).unwrap().len();
    let cut = std::fs::OpenOptions::new()
        .write(true)
        .open(file.path())
        .unwrap();
    cut.set_len(length - 12).unwrap();

    assert_eq!(read_model_info(file.path()).unwrap().cpu_weight_bytes, 0);
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

#[test]
fn the_parser_fills_the_shape_the_core_prices_servers_from() {
    let info = qwen35_9b_header().read();

    assert_eq!(info.shape.block_count, Some(32));
    assert_eq!(info.shape.head_count_kv, Some(4.into()));
    assert_eq!(info.shape.key_length, Some(256));
    assert_eq!(info.shape.full_attention_interval, Some(4));
    assert_eq!(info.shape.feed_forward_length, Some(12_288));
    assert_eq!(info.shape.ssm_inner_size, Some(4096));

    let gemma = gemma4_12b_header().read();
    assert_eq!(
        gemma.shape.head_count_kv,
        Some(lattice_core::domain::models::fit::PerLayer::Layers(
            gemma4_12b_kv_heads()
        ))
    );
    assert_eq!(gemma.shape.sliding_window, Some(1024));
    assert_eq!(gemma.shape.sliding_window_pattern, Some(gemma4_pattern(48)));
}

#[test]
fn a_header_is_read_from_any_seekable_reader() {
    let file = qwen35_9b_header()
        .tensors(&[("output.weight", 0), ("token_embd.weight", 1_000)], 5_000)
        .write();
    let bytes = std::fs::read(file.path()).unwrap();

    let from_reader = read_model_info_from(std::io::Cursor::new(&bytes), bytes.len() as u64);

    assert_eq!(from_reader, read_model_info(file.path()));
    assert!(from_reader.unwrap().shape.block_count.is_some());
}

#[test]
fn the_runtime_prices_a_server_with_the_cores_footprint() {
    use crate::features::llm::engine::sidecar_manager::resident_bytes_of;
    use lattice_core::domain::models::fit::{footprint, ServerBuffers};

    let file = qwen35_9b_header()
        .tensors(&[("output.weight", 0), ("token_embd.weight", 1_000)], 5_000)
        .write();
    let length = std::fs::metadata(file.path()).unwrap().len();
    let info = read_model_info(file.path()).unwrap();
    let window = 32_768;

    let runtime = resident_bytes_of(file.path(), window, ServerBuffers::chat(false));
    let core = footprint(
        length - info.cpu_weight_bytes,
        Some(&info.shape),
        window,
        ServerBuffers::chat(false),
    )
    .total();

    assert_eq!(runtime, core);
    assert!(core > length - info.cpu_weight_bytes);
}

async fn header_of(client: &reqwest::Client, url: &str) -> Option<GgufModelInfo> {
    let mut want: u64 = 16 << 20;
    let mut last: Option<GgufModelInfo> = None;
    loop {
        let response = client
            .get(url)
            .header("Range", format!("bytes=0-{}", want - 1))
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        let total = response
            .headers()
            .get("content-range")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.rsplit('/').next())
            .and_then(|length| length.parse::<u64>().ok())?;
        let bytes = response.bytes().await.ok()?;
        let info = read_model_info_from(std::io::Cursor::new(bytes.to_vec()), total)?;
        if last.as_ref() == Some(&info) || want >= total {
            return Some(info);
        }
        last = Some(info);
        want *= 2;
    }
}

#[tokio::test]
#[ignore = "needs the network: reads each curated GGUF's header from Hugging Face"]
async fn the_catalogs_pinned_shapes_and_cpu_weights_are_what_the_runtime_reader_reads() {
    let client = reqwest::Client::new();

    for file in lattice_core::domain::models::curated::curated_gguf_files() {
        let info = header_of(&client, &file.url)
            .await
            .unwrap_or_else(|| panic!("could not read the header of {}", file.name));

        assert_eq!(info.shape, file.pinned.shape, "{}", file.name);
        assert_eq!(
            info.cpu_weight_bytes, file.pinned.cpu_weight_bytes,
            "{}",
            file.name
        );
    }
}
