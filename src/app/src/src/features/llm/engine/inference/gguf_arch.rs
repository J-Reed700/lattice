//! Minimal GGUF header parser: extracts only `general.architecture`.
//!
//! GGUF is a tensor-bundle format used by llama.cpp / mistral.rs. The first
//! few KB contain the magic + version + metadata KV table. We need to know
//! the model architecture (e.g. "llama", "qwen2", "qwen35") BEFORE handing
//! the file to mistralrs, because mistralrs panics on unsupported archs and
//! takes the whole process down with it (see `gguf/content.rs:151`'s
//! internal `.unwrap()` we cannot patch).
//!
//! We hand-roll a tiny parser rather than pulling in a crate; we only need
//! to skip past KV pairs we don't care about until we find
//! `general.architecture`.
//!
//! GGUF v3 layout (little-endian throughout):
//! ```text
//! [4 bytes]   magic: "GGUF" = 0x46554747
//! [4 bytes]   version: u32 (1, 2, or 3)
//! [8 bytes]   tensor_count: u64
//! [8 bytes]   metadata_kv_count: u64
//! [then]      metadata_kv entries: (key_string, value_type, value)
//! ```
//! where `string` = u64 length + N bytes UTF-8 and `value_type` is a u32 enum.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Magic bytes at the start of every GGUF file: ASCII "GGUF".
const GGUF_MAGIC: [u8; 4] = *b"GGUF";

/// Cap header parsing at 1 MiB so a malformed file with absurd metadata
/// counts cannot make us read tensor data or stream the whole file.
const MAX_HEADER_BYTES: u64 = 1024 * 1024;

/// The GGUF metadata key whose value names the model architecture.
const ARCH_KEY: &str = "general.architecture";

/// Architectures recognised by `GGUFArchitecture::from_value` in mistralrs
/// rev `2d4ba4f` (verified 2026-04-28 against
/// `mistralrs-core/src/gguf/mod.rs`'s strum enum, lowercase serialization).
/// Anything outside this list will trigger the upstream `.unwrap()` panic at
/// `gguf/content.rs:151`, so we reject pre-flight.
///
/// Note: mistralrs supports more architectures (Gemma, Mistral, etc.) via its
/// non-GGUF (HF transformers) loaders, but the GGUF strum enum is stricter.
/// Models in the families below that ship as `gemma`/`mistral`-tagged GGUFs
/// will be rejected here — see `KNOWN_NONSTANDARD_ALIASES` for the user hint.
///
/// Update this list whenever the pinned `mistralrs` rev advances.
pub const SUPPORTED_ARCHITECTURES: &[&str] = &[
    "bloom",
    "falcon",
    "gpt2",
    "gptj",
    "gptneox",
    "llama",
    "mamba",
    "mistral3",
    "mpt",
    "phi2",
    "phi3",
    "qwen2",
    "qwen3",
    "qwen3moe",
    "rwkv",
    "starcoder2",
];

/// Architectures the user is likely to hit that are NOT in mistralrs's GGUF
/// strum enum, paired with a friendly explanation. Two categories:
///
/// 1. **Non-canonical quantizer tags** (unsloth's `qwen35`, etc.) — point the
///    user at a canonical-tag quant from bartowski/lmstudio-community.
/// 2. **GGUF-unsupported families** (`gemma`, `mistral`) — mistralrs supports
///    these via its HF-transformers loader path, but our GGUF entry point
///    can't handle them. Point the user at Ollama for now.
///
/// Format: `(bad_tag, hint_text)`.
pub const KNOWN_NONSTANDARD_ALIASES: &[(&str, &str)] = &[
    (
        "qwen35",
        "This GGUF tags itself 'qwen35' (non-standard). Try a quant from \
         'bartowski' or 'lmstudio-community' that uses the canonical 'qwen3' tag.",
    ),
    (
        "qwen3.5",
        "This GGUF tags itself 'qwen3.5' (non-standard). Try a quant from \
         'bartowski' or 'lmstudio-community' that uses the canonical 'qwen3' tag.",
    ),
    (
        "gemma",
        "Gemma models load via HF safetensors, not GGUF. The local LLM runtime \
         (mistralrs) supports Gemma but not via this code path. Run this model \
         through Ollama instead (Settings → Chat → Ollama Server).",
    ),
    (
        "gemma2",
        "Gemma 2 models load via HF safetensors, not GGUF. The local LLM \
         runtime (mistralrs) supports Gemma 2 but not via this code path. Run \
         this model through Ollama instead (Settings → Chat → Ollama Server).",
    ),
    (
        "gemma3",
        "Gemma 3 is a multimodal model that loads via HF safetensors, not \
         GGUF. Run this model through Ollama instead (Settings → Chat → \
         Ollama Server).",
    ),
    (
        "gemma4",
        "Gemma 4 is a multimodal model that loads via HF safetensors, not \
         GGUF. Run this model through Ollama instead (Settings → Chat → \
         Ollama Server).",
    ),
    (
        "mistral",
        "Mistral models load via HF safetensors, not GGUF. The GGUF loader \
         only accepts 'mistral3'. Run this model through Ollama instead, or \
         try a 'llama'-tagged quant (many Mistral derivatives are).",
    ),
];

/// Cap recursion depth in [`skip_value`]. Valid GGUF metadata is flat
/// (arrays of scalars or strings); deep nesting indicates a malformed or
/// adversarial file and could blow the stack.
const MAX_NESTING_DEPTH: u8 = 16;

/// Errors returned by [`read_architecture`].
#[derive(Debug, thiserror::Error)]
pub enum GgufReadError {
    #[error("I/O error reading GGUF: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not a GGUF file: bad magic bytes")]
    NotGguf,
    #[error("Unsupported GGUF version: {0}")]
    UnsupportedVersion(u32),
    #[error("Malformed GGUF metadata at offset {offset}: {reason}")]
    Malformed { offset: u64, reason: String },
    #[error("GGUF metadata does not contain general.architecture")]
    ArchitectureMissing,
}

/// Read just enough of a GGUF file to extract `general.architecture`.
///
/// Returns the architecture name as a string (e.g. `"llama"`, `"qwen2"`).
///
/// # Errors
/// - [`GgufReadError::Io`] on filesystem errors
/// - [`GgufReadError::NotGguf`] if the magic bytes don't match
/// - [`GgufReadError::UnsupportedVersion`] for version not in {1, 2, 3}
/// - [`GgufReadError::Malformed`] if the metadata table is truncated /
///   has implausibly large lengths / contains an unknown value type
/// - [`GgufReadError::ArchitectureMissing`] if no `general.architecture`
///   key is found within the first [`MAX_HEADER_BYTES`] bytes
pub fn read_architecture(path: &Path) -> Result<String, GgufReadError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // --- Magic + version -----------------------------------------------------
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if magic != GGUF_MAGIC {
        return Err(GgufReadError::NotGguf);
    }

    let version = read_u32(&mut reader)?;
    if !matches!(version, 1..=3) {
        return Err(GgufReadError::UnsupportedVersion(version));
    }

    // --- Counts --------------------------------------------------------------
    // GGUF v1 used u32 for these counts; v2/v3 use u64. We only support
    // v2/v3 in practice (modern files); v1 is mostly historical.
    let _tensor_count = read_u64(&mut reader)?;
    let kv_count = read_u64(&mut reader)?;

    // Sanity check: cap KV count so a corrupt file claiming 2^60 entries
    // doesn't make us spin forever. 1 million is far more than any real
    // model uses (typical: 20–50).
    if kv_count > 1_000_000 {
        return Err(GgufReadError::Malformed {
            offset: current_offset(&mut reader)?,
            reason: format!("implausible kv_count={kv_count}"),
        });
    }

    // --- Walk KV pairs -------------------------------------------------------
    for _ in 0..kv_count {
        let offset = current_offset(&mut reader)?;
        if offset > MAX_HEADER_BYTES {
            return Err(GgufReadError::Malformed {
                offset,
                reason: "exceeded header read cap before finding architecture".into(),
            });
        }

        let key = read_gguf_string(&mut reader)?;
        let value_type = read_u32(&mut reader)?;

        if key == ARCH_KEY {
            // Architecture must be a string (value_type == 8).
            if value_type != 8 {
                return Err(GgufReadError::Malformed {
                    offset: current_offset(&mut reader)?,
                    reason: format!("general.architecture has non-string type {value_type}"),
                });
            }
            return read_gguf_string(&mut reader);
        }

        skip_value(&mut reader, value_type, 0)?;
    }

    Err(GgufReadError::ArchitectureMissing)
}

/// Returns true iff `arch` appears in [`SUPPORTED_ARCHITECTURES`].
#[must_use]
pub fn is_supported_architecture(arch: &str) -> bool {
    SUPPORTED_ARCHITECTURES.contains(&arch)
}

/// Look up an unsupported arch in [`KNOWN_NONSTANDARD_ALIASES`] and return
/// the user-facing hint, if any. Used by callers to give richer error
/// messages than a generic "not supported".
#[must_use]
pub fn nonstandard_alias_hint(arch: &str) -> Option<&'static str> {
    KNOWN_NONSTANDARD_ALIASES
        .iter()
        .find(|(tag, _)| *tag == arch)
        .map(|(_, hint)| *hint)
}

// ---------------------------------------------------------------------------
// Internal binary readers
// ---------------------------------------------------------------------------

fn read_u32<R: Read>(r: &mut R) -> Result<u32, GgufReadError> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64<R: Read>(r: &mut R) -> Result<u64, GgufReadError> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

fn read_i64<R: Read>(r: &mut R) -> Result<i64, GgufReadError> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(i64::from_le_bytes(buf))
}

fn read_f32<R: Read>(r: &mut R) -> Result<f32, GgufReadError> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(f32::from_le_bytes(buf))
}

fn read_f64<R: Read>(r: &mut R) -> Result<f64, GgufReadError> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(f64::from_le_bytes(buf))
}

fn current_offset<R: Seek>(r: &mut R) -> Result<u64, GgufReadError> {
    Ok(r.stream_position()?)
}

/// Read a GGUF string: u64 length + N bytes UTF-8 (no trailing NUL).
fn read_gguf_string<R: Read + Seek>(r: &mut R) -> Result<String, GgufReadError> {
    let len = read_u64(r)?;
    if len > MAX_HEADER_BYTES {
        return Err(GgufReadError::Malformed {
            offset: current_offset(r)?,
            reason: format!("string length {len} exceeds cap"),
        });
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| GgufReadError::Malformed {
        offset: 0,
        reason: format!("invalid UTF-8 in string: {e}"),
    })
}

/// Skip past a GGUF metadata value of the given `value_type`. We don't
/// care about the contents — we just need the cursor at the next KV pair.
///
/// `depth` bounds recursion through nested arrays (value_type=9) to defend
/// against crafted files. Valid GGUF metadata is effectively flat.
fn skip_value<R: Read + Seek>(r: &mut R, value_type: u32, depth: u8) -> Result<(), GgufReadError> {
    if depth > MAX_NESTING_DEPTH {
        return Err(GgufReadError::Malformed {
            offset: current_offset(r)?,
            reason: format!("array nesting exceeds depth cap {MAX_NESTING_DEPTH}"),
        });
    }
    match value_type {
        // Fixed-width scalars: just seek past N bytes.
        0 | 1 | 7 => skip_bytes(r, 1)?,         // u8 / i8 / bool
        2 | 3 => skip_bytes(r, 2)?,             // u16 / i16
        4..=6 => skip_bytes(r, 4)?,             // u32 / i32 / f32
        10..=12 => skip_bytes(r, 8)?,           // u64 / i64 / f64
        // String: u64 length + N bytes
        8 => {
            let _ = read_gguf_string(r)?;
        }
        // Array: u32 element_type + u64 count + N elements (recursive)
        9 => {
            let elem_type = read_u32(r)?;
            let count = read_u64(r)?;
            if count > MAX_HEADER_BYTES {
                return Err(GgufReadError::Malformed {
                    offset: current_offset(r)?,
                    reason: format!("array count {count} exceeds cap"),
                });
            }
            for _ in 0..count {
                skip_value(r, elem_type, depth + 1)?;
            }
        }
        other => {
            return Err(GgufReadError::Malformed {
                offset: current_offset(r)?,
                reason: format!("unknown value_type {other}"),
            });
        }
    }
    Ok(())
}

fn skip_bytes<R: Seek>(r: &mut R, n: i64) -> Result<(), GgufReadError> {
    r.seek(SeekFrom::Current(n))?;
    Ok(())
}

// Silence unused-warning for helpers we keep available for future
// metadata extraction; clippy's dead_code lint is off in tests.
#[cfg(test)]
#[allow(dead_code)]
fn _keep_helpers_alive() {
    let _ = read_i64::<&[u8]>;
    let _ = read_f32::<&[u8]>;
    let _ = read_f64::<&[u8]>;
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Emit a GGUF string: u64 length (LE) + raw bytes.
    fn gguf_string(s: &str) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&(s.len() as u64).to_le_bytes());
        v.extend_from_slice(s.as_bytes());
        v
    }

    /// Build a minimal valid GGUF file with the given KV pairs (string-typed only).
    fn synth_gguf(kvs: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&GGUF_MAGIC);
        buf.extend_from_slice(&3u32.to_le_bytes()); // version 3
        buf.extend_from_slice(&0u64.to_le_bytes()); // tensor_count
        buf.extend_from_slice(&(kvs.len() as u64).to_le_bytes()); // kv_count
        for (k, v) in kvs {
            buf.extend(gguf_string(k));
            buf.extend_from_slice(&8u32.to_le_bytes()); // value_type = string
            buf.extend(gguf_string(v));
        }
        buf
    }

    fn write_temp(bytes: &[u8]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn test_reads_known_arch_from_real_gguf() {
        let bytes = synth_gguf(&[("general.architecture", "llama")]);
        let f = write_temp(&bytes);
        let arch = read_architecture(f.path()).unwrap();
        assert_eq!(arch, "llama");
    }

    #[test]
    fn test_finds_arch_after_other_kvs() {
        let bytes = synth_gguf(&[
            ("general.name", "MyModel"),
            ("general.author", "someone"),
            ("general.architecture", "qwen2"),
            ("general.license", "apache-2.0"),
        ]);
        let f = write_temp(&bytes);
        let arch = read_architecture(f.path()).unwrap();
        assert_eq!(arch, "qwen2");
    }

    #[test]
    fn test_rejects_non_gguf() {
        let bytes = b"NOPE\x00\x00\x00\x03\x00\x00\x00\x00\x00\x00\x00\x00";
        let f = write_temp(bytes);
        let err = read_architecture(f.path()).unwrap_err();
        assert!(matches!(err, GgufReadError::NotGguf), "got {err:?}");
    }

    #[test]
    fn test_rejects_unsupported_version() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&GGUF_MAGIC);
        bytes.extend_from_slice(&99u32.to_le_bytes()); // bogus version
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        let f = write_temp(&bytes);
        let err = read_architecture(f.path()).unwrap_err();
        assert!(
            matches!(err, GgufReadError::UnsupportedVersion(99)),
            "got {err:?}"
        );
    }

    #[test]
    fn test_missing_arch_key() {
        let bytes = synth_gguf(&[("general.name", "MyModel")]);
        let f = write_temp(&bytes);
        let err = read_architecture(f.path()).unwrap_err();
        assert!(
            matches!(err, GgufReadError::ArchitectureMissing),
            "got {err:?}"
        );
    }

    #[test]
    fn test_unsupported_arch_validates_false() {
        // Non-standard quantizer tags
        assert!(!is_supported_architecture("qwen35"));
        assert!(!is_supported_architecture("gemma3"));
        // Families mistralrs supports outside its GGUF strum enum
        assert!(!is_supported_architecture("gemma"));
        assert!(!is_supported_architecture("gemma2"));
        assert!(!is_supported_architecture("mistral"));
        // Brand-new architectures not yet in the pinned mistralrs enum
        assert!(!is_supported_architecture("deepseek3"));
        // Empty / nonsense
        assert!(!is_supported_architecture(""));
        assert!(!is_supported_architecture("not-a-real-arch"));
    }

    #[test]
    fn test_supported_arch_validates_true() {
        // Mirror the GGUFArchitecture enum from mistralrs HEAD (16 variants).
        for arch in [
            "llama", "mpt", "gptneox", "gptj", "gpt2", "bloom", "falcon",
            "mamba", "rwkv", "phi2", "phi3", "starcoder2", "qwen2", "qwen3",
            "qwen3moe", "mistral3",
        ] {
            assert!(
                is_supported_architecture(arch),
                "expected '{arch}' to be in the allowlist"
            );
        }
    }

    #[test]
    fn test_nonstandard_alias_hint_returns_helpful_message() {
        let qwen_hint = nonstandard_alias_hint("qwen35").expect("qwen35 should have a hint");
        assert!(qwen_hint.contains("bartowski"), "qwen35 hint should suggest a quant source: {qwen_hint}");

        let gemma_hint = nonstandard_alias_hint("gemma").expect("gemma should have a hint");
        assert!(gemma_hint.contains("Ollama"), "gemma hint should point to Ollama: {gemma_hint}");

        let mistral_hint = nonstandard_alias_hint("mistral").expect("mistral should have a hint");
        assert!(mistral_hint.contains("Ollama") || mistral_hint.contains("llama"),
            "mistral hint should suggest an alternative: {mistral_hint}");

        // Canonical archs that ARE supported should have no hint.
        assert!(nonstandard_alias_hint("llama").is_none());
        assert!(nonstandard_alias_hint("qwen3").is_none());
        assert!(nonstandard_alias_hint("not-real").is_none());
    }

    #[test]
    fn test_skip_value_handles_mixed_types() {
        // KV table: u32 scalar, then string arch — confirms skip_value
        // advances past a non-string value correctly.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&GGUF_MAGIC);
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes()); // tensors
        bytes.extend_from_slice(&2u64.to_le_bytes()); // 2 KV pairs
                                                      // KV 1: ("answer", u32 = 4, value 42)
        bytes.extend(gguf_string("answer"));
        bytes.extend_from_slice(&4u32.to_le_bytes()); // u32 type
        bytes.extend_from_slice(&42u32.to_le_bytes());
        // KV 2: ("general.architecture", string = 8, "phi3")
        bytes.extend(gguf_string("general.architecture"));
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend(gguf_string("phi3"));

        let f = write_temp(&bytes);
        let arch = read_architecture(f.path()).unwrap();
        assert_eq!(arch, "phi3");
    }

    #[test]
    fn test_truncated_file_returns_io_error() {
        // Magic + version only, no counts — read_exact on counts should fail.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&GGUF_MAGIC);
        bytes.extend_from_slice(&3u32.to_le_bytes());
        let f = write_temp(&bytes);
        let err = read_architecture(f.path()).unwrap_err();
        assert!(matches!(err, GgufReadError::Io(_)), "got {err:?}");
    }

    #[test]
    fn test_implausible_kv_count_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&GGUF_MAGIC);
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&u64::MAX.to_le_bytes()); // absurd
        let f = write_temp(&bytes);
        let err = read_architecture(f.path()).unwrap_err();
        assert!(matches!(err, GgufReadError::Malformed { .. }), "got {err:?}");
    }
}
