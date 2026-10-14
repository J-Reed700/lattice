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
mod tests;
