//! A minimal reader for the GGUF header, enough to learn what a model was
//! trained for.
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

/// What the header says about a model, as far as context sizing cares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GgufModelInfo {
    /// `general.architecture`, e.g. `llama`, `qwen3`.
    pub architecture: Option<String>,
    /// `{architecture}.context_length`: the window the model was trained for.
    pub trained_context_length: Option<u32>,
    /// Transformer layers; every one of them holds its own KV cache.
    pub block_count: Option<u32>,
    /// Key/value heads. Grouped-query models have far fewer of these than
    /// attention heads, which is most of why their caches are affordable.
    pub head_count_kv: Option<u32>,
    /// Attention heads, used to derive the head dimension when the header does
    /// not state key/value lengths outright.
    pub head_count: Option<u32>,
    pub key_length: Option<u32>,
    pub value_length: Option<u32>,
    pub embedding_length: Option<u32>,
}

/// Bytes one token occupies in a full-precision KV cache.
///
/// llama.cpp stores a key and a value per layer per KV head, f16 by default:
/// `blocks * kv_heads * (key_len + value_len) * 2`. For a 32-layer model with
/// 4 KV heads and 256-wide keys that is 128 KiB per token, which is why a
/// 256K-trained model cannot actually be run at 256K on consumer hardware.
const KV_CACHE_BYTES_PER_ELEMENT: u64 = 2;

impl GgufModelInfo {
    /// How many bytes of KV cache one token of context costs, when the header
    /// says enough to work it out.
    pub fn kv_cache_bytes_per_token(&self) -> Option<u64> {
        let blocks = u64::from(self.block_count?);
        let kv_heads = u64::from(self.head_count_kv.or(self.head_count)?);

        // Prefer the stated key/value widths. Otherwise every head splits the
        // embedding evenly, which is the usual arrangement.
        let head_dim = || {
            let embedding = u64::from(self.embedding_length?);
            let heads = u64::from(self.head_count?);
            (heads > 0).then(|| embedding / heads)
        };
        let key = match self.key_length {
            Some(length) => u64::from(length),
            None => head_dim()?,
        };
        let value = match self.value_length {
            Some(length) => u64::from(length),
            None => head_dim()?,
        };

        let per_token = blocks
            .checked_mul(kv_heads)?
            .checked_mul(key.checked_add(value)?)?
            .checked_mul(KV_CACHE_BYTES_PER_ELEMENT)?;
        (per_token > 0).then_some(per_token)
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

    // Suffixes worth stopping for. Everything else — tokenizer vocabularies
    // above all — is skipped without being materialised.
    const WANTED: [&str; 6] = [
        ".block_count",
        ".attention.head_count_kv",
        ".attention.head_count",
        ".attention.key_length",
        ".attention.value_length",
        ".embedding_length",
    ];

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
        if key.ends_with(".context_length") {
            if let Some(length) = read_scalar_as_u32(&mut reader, kind) {
                context_lengths.push((key, length));
                continue;
            }
            break;
        }
        if let Some(suffix) = WANTED.iter().find(|suffix| key.ends_with(*suffix)) {
            let Some(value) = read_scalar_as_u32(&mut reader, kind) else {
                break;
            };
            match *suffix {
                ".block_count" => info.block_count = Some(value),
                ".attention.head_count_kv" => info.head_count_kv = Some(value),
                ".attention.head_count" => info.head_count = Some(value),
                ".attention.key_length" => info.key_length = Some(value),
                ".attention.value_length" => info.value_length = Some(value),
                ".embedding_length" => info.embedding_length = Some(value),
                _ => {}
            }
            continue;
        }
        if skip_value(&mut reader, kind).is_none() {
            break;
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

/// Read a numeric value of the given type, widened to `u32`. A context length
/// is a small positive number in every type it is written as.
fn read_scalar_as_u32(reader: &mut (impl Read + Seek), kind: u32) -> Option<u32> {
    let value = match kind {
        value_type::UINT8 | value_type::BOOL => u64::from(read_exact::<1>(reader)?[0]),
        value_type::INT8 => u64::try_from(i8::from_le_bytes(read_exact::<1>(reader)?)).ok()?,
        value_type::UINT16 => u64::from(u16::from_le_bytes(read_exact::<2>(reader)?)),
        value_type::INT16 => u64::try_from(i16::from_le_bytes(read_exact::<2>(reader)?)).ok()?,
        value_type::UINT32 => u64::from(read_u32(reader)?),
        value_type::INT32 => u64::try_from(i32::from_le_bytes(read_exact::<4>(reader)?)).ok()?,
        value_type::UINT64 => read_u64(reader)?,
        value_type::INT64 => u64::try_from(i64::from_le_bytes(read_exact::<8>(reader)?)).ok()?,
        _ => {
            skip_value(reader, kind)?;
            return None;
        }
    };
    u32::try_from(value).ok()
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
            for _ in 0..count {
                skip_value(reader, element_type)?;
            }
            return Some(());
        }
        _ => return None,
    };
    reader.seek(SeekFrom::Current(width)).ok()?;
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

        fn write(self) -> tempfile::NamedTempFile {
            let mut file = tempfile::NamedTempFile::new().unwrap();
            let mut bytes = self.bytes;
            bytes.extend_from_slice(&self.kv_count.to_le_bytes());
            bytes.extend_from_slice(&self.body);
            file.write_all(&bytes).unwrap();
            file.flush().unwrap();
            file
        }
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

    /// The real numbers from the Ornith-1.5-9B header, cross-checked against an
    /// independent read of the same file: 32 blocks, 4 KV heads, 256-wide keys
    /// and values. 128 KiB per token is what makes a 262144-trained model
    /// impossible to actually run at 262144 on consumer hardware.
    #[test]
    fn a_grouped_query_cache_costs_what_its_dimensions_say() {
        let info = GgufModelInfo {
            block_count: Some(32),
            head_count_kv: Some(4),
            key_length: Some(256),
            value_length: Some(256),
            ..Default::default()
        };

        assert_eq!(info.kv_cache_bytes_per_token(), Some(128 * 1024));
    }

    /// Older headers state no key/value width, so the head dimension comes from
    /// the embedding split across the attention heads.
    #[test]
    fn a_header_without_key_widths_derives_them_from_the_embedding() {
        let info = GgufModelInfo {
            block_count: Some(32),
            head_count: Some(32),
            head_count_kv: Some(32),
            embedding_length: Some(4096),
            ..Default::default()
        };

        // 4096 / 32 = 128 per head, key and value: 32 * 32 * 256 * 2.
        assert_eq!(info.kv_cache_bytes_per_token(), Some(32 * 32 * 256 * 2));
    }

    /// Without the dimensions there is no honest answer, and a guess would size
    /// a cache that does not fit.
    #[test]
    fn an_incomplete_header_refuses_to_estimate() {
        let info = GgufModelInfo {
            block_count: Some(32),
            ..Default::default()
        };

        assert_eq!(info.kv_cache_bytes_per_token(), None);
    }

    #[test]
    fn the_full_set_of_sizing_keys_is_read_together() {
        let file = GgufBuilder::new(3)
            .string("general.architecture", "qwen35")
            .string_array("tokenizer.ggml.tokens", &["a", "b"])
            .u32_value("qwen35.block_count", 32)
            .u32_value("qwen35.context_length", 262_144)
            .u32_value("qwen35.embedding_length", 4096)
            .u32_value("qwen35.attention.head_count", 16)
            .u32_value("qwen35.attention.head_count_kv", 4)
            .u32_value("qwen35.attention.key_length", 256)
            .u32_value("qwen35.attention.value_length", 256)
            .write();

        let info = read_model_info(file.path()).unwrap();

        assert_eq!(info.trained_context_length, Some(262_144));
        assert_eq!(info.block_count, Some(32));
        assert_eq!(info.head_count_kv, Some(4));
        assert_eq!(info.kv_cache_bytes_per_token(), Some(128 * 1024));
    }

    /// Synthetic fixtures only prove the parser agrees with itself. This reads a
    /// GGUF that actually exists on the machine, when one does.
    #[test]
    #[ignore = "requires LATTICE_GGUF_MODEL to point at a .gguf on this machine"]
    fn a_real_gguf_on_this_machine_parses() {
        let path = std::env::var("LATTICE_GGUF_MODEL").unwrap();
        let info = read_model_info(Path::new(&path)).expect("a real GGUF must parse");

        println!("architecture: {:?}", info.architecture);
        println!("trained context: {:?}", info.trained_context_length);
        println!("blocks: {:?}", info.block_count);
        println!("kv heads: {:?}", info.head_count_kv);
        println!("kv bytes/token: {:?}", info.kv_cache_bytes_per_token());

        assert!(info.architecture.is_some());
        assert!(info.trained_context_length.is_some());
        assert!(info.kv_cache_bytes_per_token().is_some());
    }
}
