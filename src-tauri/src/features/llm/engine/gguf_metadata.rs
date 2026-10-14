//! A minimal reader for the GGUF header, enough to learn what a model was
//! trained for and what its KV cache costs.
//!
//! Sizing a context window is guesswork without this. Ask for more than the
//! model was trained on and llama.cpp silently rope-scales, degrading quality
//! in a way nothing reports; ask for a flat conservative default and a 28 GB
//! GPU runs an 8 K window it could carry four times over. Both failures are
//! invisible, so the honest input is the model's own `context_length`.
//!
//! Only the key/value header is parsed. Tensor data is never touched, and an
//! unreadable or unfamiliar file yields `None` rather than an error — the
//! caller falls back to sizing from hardware alone.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

const GGUF_MAGIC: u32 = 0x4655_4747; // "GGUF", little-endian

/// GGUF metadata value types, as written in the header.
mod value_type {
    pub const UINT8: u32 = 0;
    pub const INT8: u32 = 1;
    pub const UINT16: u32 = 2;
    pub const INT16: u32 = 3;
    pub const UINT32: u32 = 4;
    pub const INT32: u32 = 5;
    pub const FLOAT32: u32 = 6;
    pub const BOOL: u32 = 7;
    pub const STRING: u32 = 8;
    pub const ARRAY: u32 = 9;
    pub const UINT64: u32 = 10;
    pub const INT64: u32 = 11;
    pub const FLOAT64: u32 = 12;
}

/// A header value that is either one number for every layer or one number per
/// layer. llama.cpp accepts both for head counts, and Gemma 4 writes its KV-head
/// count per layer because its global layers have fewer heads than its
/// sliding ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PerLayer {
    Uniform(u32),
    Layers(Vec<u32>),
}

impl PerLayer {
    /// The value for one layer, or `None` past the end of a per-layer list.
    fn at(&self, layer: usize) -> Option<u32> {
        match self {
            Self::Uniform(value) => Some(*value),
            Self::Layers(values) => values.get(layer).copied(),
        }
    }
}

impl From<u32> for PerLayer {
    fn from(value: u32) -> Self {
        Self::Uniform(value)
    }
}

/// What the header says about a model, as far as context sizing cares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GgufModelInfo {
    /// `general.architecture`, e.g. `llama`, `qwen3`.
    pub architecture: Option<String>,
    /// `{architecture}.context_length`: the window the model was trained for.
    pub trained_context_length: Option<u32>,
    /// Transformer layers, counting any multi-token-prediction blocks at the
    /// end (`nextn_predict_layers`).
    pub block_count: Option<u32>,
    /// Key/value heads, for every layer or per layer. Grouped-query models
    /// have far fewer of these than attention heads, which is most of why
    /// their caches are affordable.
    pub head_count_kv: Option<PerLayer>,
    /// Attention heads, used to derive the head dimension when the header does
    /// not state key/value lengths outright.
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
    /// Which layers slide, one flag per layer. Only the per-layer list form is
    /// read; a model whose layers cannot be told apart is sized as if none of
    /// them slid.
    pub sliding_window_pattern: Option<Vec<bool>>,
    /// Hybrid models (Qwen 3.5, 3.6, 3.8): one layer in this many attends to
    /// the whole window. The others are linear-attention layers whose
    /// recurrent state does not grow with the window.
    pub full_attention_interval: Option<u32>,
    /// Multi-token-prediction blocks at the end of `block_count`.
    pub nextn_predict_layers: Option<u32>,
    /// Trailing layers that reuse an earlier layer's cache rather than keep
    /// their own (Gemma 4 E2B and E4B).
    pub shared_kv_layers: Option<u32>,
}

/// llama.cpp stores keys and values as f16 by default: two bytes an element.
const KV_CACHE_BYTES_PER_ELEMENT: u64 = 2;

/// Slots llama-server opens when it is launched without `--parallel`, as the
/// sidecar is: four sequences over one unified cache. A sliding-window layer
/// keeps a window for each of them.
const SERVER_SLOTS: u64 = 4;

/// llama.cpp's default micro-batch. A sliding-window layer also holds the
/// batch being decoded past its window.
const MICRO_BATCH_TOKENS: u64 = 512;

/// llama.cpp rounds a sliding-window cache up to a multiple of this many cells.
const SLIDING_CACHE_ALIGNMENT: u64 = 256;

/// One layer that keeps a KV cache of its own.
struct LayerCache {
    /// Key plus value bytes for one cached token.
    bytes_per_token: u64,
    /// Whether the cache stops growing at the sliding window.
    sliding: bool,
}

impl GgufModelInfo {
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
    pub fn kv_cache_bytes(&self, context_tokens: u32) -> Option<u64> {
        let layers = self.layer_caches()?;
        if layers.iter().all(|layer| layer.bytes_per_token == 0) {
            return None;
        }

        let tokens = u64::from(context_tokens);
        // A layer is only marked sliding when the window is known.
        let window = u64::from(self.sliding_window.unwrap_or(0));
        let sliding_cap = (window * SERVER_SLOTS + MICRO_BATCH_TOKENS)
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
    fn layer_caches(&self) -> Option<Vec<LayerCache>> {
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
                    .checked_mul(KV_CACHE_BYTES_PER_ELEMENT)?;
                Some(LayerCache {
                    bytes_per_token,
                    sliding,
                })
            })
            .collect()
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

/// Read what the GGUF header says about a model.
///
/// Returns `None` when the file cannot be read or is not GGUF. A truncated or
/// unfamiliar header yields whatever was learned before the reader gave up,
/// because a partial answer is still better than a guess.
pub fn read_model_info(path: &Path) -> Option<GgufModelInfo> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);

    if read_u32(&mut reader)? != GGUF_MAGIC {
        return None;
    }
    let version = read_u32(&mut reader)?;
    if !(2..=3).contains(&version) {
        // v1 wrote 32-bit lengths. No model ships it any more, and guessing at
        // the layout would be worse than declining to answer.
        return None;
    }

    let _tensor_count = read_u64(&mut reader)?;
    let kv_count = read_u64(&mut reader)?;

    let mut architecture = None;
    let mut context_lengths: Vec<(String, u32)> = Vec::new();
    let mut info = GgufModelInfo::default();

    for _ in 0..kv_count {
        let Some(key) = read_string(&mut reader) else {
            break;
        };
        let Some(kind) = read_u32(&mut reader) else {
            break;
        };

        if key == "general.architecture" && kind == value_type::STRING {
            architecture = read_string(&mut reader);
            continue;
        }

        // Numbers and short lists of them are kept; everything else, the
        // tokenizer vocabularies above all, is skipped without being
        // materialised.
        let Some(value) = read_value(&mut reader, kind) else {
            break;
        };

        // Sizing keys are `{architecture}.{name}`. A longer prefix, such as a
        // vision tower's `{architecture}.vision.block_count`, is not the
        // language model's.
        let Some((_, name)) = key.split_once('.') else {
            continue;
        };
        match (name, value) {
            ("context_length", Value::Number(length)) => context_lengths.push((key, length)),
            ("block_count", Value::Number(count)) => info.block_count = Some(count),
            ("attention.head_count_kv", value) => info.head_count_kv = value.per_layer(),
            ("attention.head_count", value) => info.head_count = value.per_layer(),
            ("attention.key_length", Value::Number(length)) => info.key_length = Some(length),
            ("attention.value_length", Value::Number(length)) => {
                info.value_length = Some(length);
            }
            ("attention.key_length_swa", Value::Number(length)) => {
                info.key_length_swa = Some(length);
            }
            ("attention.value_length_swa", Value::Number(length)) => {
                info.value_length_swa = Some(length);
            }
            ("embedding_length", Value::Number(length)) => info.embedding_length = Some(length),
            ("attention.sliding_window", Value::Number(window)) => {
                info.sliding_window = Some(window);
            }
            ("attention.sliding_window_pattern", Value::Numbers(flags)) => {
                info.sliding_window_pattern = Some(flags.iter().map(|&flag| flag != 0).collect());
            }
            ("full_attention_interval", Value::Number(interval)) => {
                info.full_attention_interval = Some(interval);
            }
            ("nextn_predict_layers", Value::Number(count)) => {
                info.nextn_predict_layers = Some(count);
            }
            ("attention.shared_kv_layers", Value::Number(count)) => {
                info.shared_kv_layers = Some(count);
            }
            _ => {}
        }
    }

    // Prefer the key that belongs to this architecture; a file with exactly one
    // `.context_length` is unambiguous even when the architecture is missing.
    let trained_context_length = architecture
        .as_ref()
        .and_then(|arch| {
            let wanted = format!("{arch}.context_length");
            context_lengths
                .iter()
                .find(|(key, _)| *key == wanted)
                .map(|(_, length)| *length)
        })
        .or(match context_lengths.as_slice() {
            [(_, length)] => Some(*length),
            _ => None,
        });

    Some(GgufModelInfo {
        architecture,
        trained_context_length,
        ..info
    })
}

/// Inspect a bounded download prefix without touching tensor data.
/// Missing/truncated metadata is unknown, never evidence of compatibility.
pub(super) fn architecture_from_prefix(bytes: &[u8]) -> Option<String> {
    let mut reader = std::io::Cursor::new(bytes);
    if read_u32(&mut reader)? != GGUF_MAGIC || !(2..=3).contains(&read_u32(&mut reader)?) {
        return None;
    }
    read_u64(&mut reader)?;
    let count = read_u64(&mut reader)?;
    for _ in 0..count.min(100_000) {
        let key = read_string(&mut reader)?;
        let kind = read_u32(&mut reader)?;
        if key == "general.architecture" && kind == value_type::STRING {
            return read_string(&mut reader).filter(|value| !value.is_empty());
        }
        skip_value(&mut reader, kind)?;
        if reader.position() >= bytes.len() as u64 {
            return None;
        }
    }
    None
}

fn read_exact<const N: usize>(reader: &mut impl Read) -> Option<[u8; N]> {
    let mut buffer = [0u8; N];
    reader.read_exact(&mut buffer).ok()?;
    Some(buffer)
}

fn read_u32(reader: &mut impl Read) -> Option<u32> {
    read_exact::<4>(reader).map(u32::from_le_bytes)
}

fn read_u64(reader: &mut impl Read) -> Option<u64> {
    read_exact::<8>(reader).map(u64::from_le_bytes)
}

fn read_string(reader: &mut impl Read) -> Option<String> {
    let length = read_u64(reader)?;
    // A corrupt length must not turn into a multi-gigabyte allocation.
    if length > 1 << 20 {
        return None;
    }
    let mut bytes = vec![0u8; usize::try_from(length).ok()?];
    reader.read_exact(&mut bytes).ok()?;
    String::from_utf8(bytes).ok()
}

/// A header value, as far as sizing cares.
enum Value {
    Number(u32),
    /// A list of numbers short enough to be one per layer.
    Numbers(Vec<u32>),
    /// Anything else: text, a float, a number past `u32`, a long list. It has
    /// already been read past.
    Other,
}

impl Value {
    fn per_layer(self) -> Option<PerLayer> {
        match self {
            Self::Number(value) => Some(PerLayer::Uniform(value)),
            Self::Numbers(values) => Some(PerLayer::Layers(values)),
            Self::Other => None,
        }
    }
}

/// The longest list kept as per-layer values. llama.cpp allows 512 layers; a
/// tokenizer's token-type list runs to hundreds of thousands and is skipped.
const MAX_PER_LAYER_VALUES: u64 = 4096;

/// Read a value of the given type. `None` only when the header itself cannot
/// be read past, which ends the scan.
fn read_value(reader: &mut (impl Read + Seek), kind: u32) -> Option<Value> {
    if kind == value_type::ARRAY {
        let element_type = read_u32(reader)?;
        let count = read_u64(reader)?;
        if !is_integer(element_type) || count > MAX_PER_LAYER_VALUES {
            skip_elements(reader, element_type, count)?;
            return Some(Value::Other);
        }
        let mut values = Vec::new();
        let mut all_fit = true;
        for _ in 0..count {
            match read_integer(reader, element_type)? {
                Some(value) => values.push(value),
                None => all_fit = false,
            }
        }
        return Some(if all_fit {
            Value::Numbers(values)
        } else {
            Value::Other
        });
    }
    if is_integer(kind) {
        return Some(read_integer(reader, kind)?.map_or(Value::Other, Value::Number));
    }
    skip_value(reader, kind)?;
    Some(Value::Other)
}

fn is_integer(kind: u32) -> bool {
    matches!(
        kind,
        value_type::UINT8
            | value_type::INT8
            | value_type::UINT16
            | value_type::INT16
            | value_type::UINT32
            | value_type::INT32
            | value_type::UINT64
            | value_type::INT64
            | value_type::BOOL
    )
}

/// Read an integer of the given type, widened to `u32`. The outer `None` means
/// the header could not be read; the inner one a value no count or length
/// could be, negative or past `u32`, which has still been read past.
fn read_integer(reader: &mut impl Read, kind: u32) -> Option<Option<u32>> {
    let value = match kind {
        value_type::UINT8 | value_type::BOOL => i128::from(read_exact::<1>(reader)?[0]),
        value_type::INT8 => i128::from(i8::from_le_bytes(read_exact::<1>(reader)?)),
        value_type::UINT16 => i128::from(u16::from_le_bytes(read_exact::<2>(reader)?)),
        value_type::INT16 => i128::from(i16::from_le_bytes(read_exact::<2>(reader)?)),
        value_type::UINT32 => i128::from(read_u32(reader)?),
        value_type::INT32 => i128::from(i32::from_le_bytes(read_exact::<4>(reader)?)),
        value_type::UINT64 => i128::from(read_u64(reader)?),
        value_type::INT64 => i128::from(i64::from_le_bytes(read_exact::<8>(reader)?)),
        _ => return None,
    };
    Some(u32::try_from(value).ok())
}

/// Advance past a value the caller does not need.
fn skip_value(reader: &mut (impl Read + Seek), kind: u32) -> Option<()> {
    let width = match kind {
        value_type::UINT8 | value_type::INT8 | value_type::BOOL => 1,
        value_type::UINT16 | value_type::INT16 => 2,
        value_type::UINT32 | value_type::INT32 | value_type::FLOAT32 => 4,
        value_type::UINT64 | value_type::INT64 | value_type::FLOAT64 => 8,
        value_type::STRING => {
            let length = read_u64(reader)?;
            reader
                .seek(SeekFrom::Current(i64::try_from(length).ok()?))
                .ok()?;
            return Some(());
        }
        value_type::ARRAY => {
            let element_type = read_u32(reader)?;
            let count = read_u64(reader)?;
            return skip_elements(reader, element_type, count);
        }
        _ => return None,
    };
    reader.seek(SeekFrom::Current(width)).ok()?;
    Some(())
}

/// Advance past the elements of an array whose type and count are read.
fn skip_elements(reader: &mut (impl Read + Seek), element_type: u32, count: u64) -> Option<()> {
    // GGUF arrays are flat. Reject corrupt recursion/lengths before
    // inspecting an untrusted remote header or local model file.
    if element_type == value_type::ARRAY || count > 1_000_000 {
        return None;
    }
    for _ in 0..count {
        skip_value(reader, element_type)?;
    }
    Some(())
}

#[cfg(test)]
mod tests {
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
    /// whole window when the server drafts with it. The 9B MTP build (33
    /// blocks) is charged 8 + 1 layers; Qwen 3.8 27B (65 blocks, header read
    /// from the file on this machine) 16 + 1.
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

    /// The reader used to give up at Gemma 4's per-layer KV-head list and
    /// lose every key after it, the key widths and the sliding window among
    /// them.
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
}
