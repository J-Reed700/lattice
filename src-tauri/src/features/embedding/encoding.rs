//! The one encoding for embedding vectors stored in `text_embeddings.embedding`.
//!
//! This module exists because the column previously had two writers that did
//! not agree. `EmbeddingRepository` wrote `bincode::serialize(&Vec<f32>)`,
//! which prefixes an 8-byte little-endian length header; the indexing engine
//! wrote bare little-endian `f32` bytes with no header. The readers were split
//! the same way, so each silently mishandled the other's rows: under the raw
//! decoder a bincode row appears to have `dim + 2` floats and gets filtered
//! out of the vector index, and under the bincode decoder a raw row simply
//! errors.
//!
//! The consequence was that documents indexed through one path vanished from
//! semantic search after an index rebuild, with nothing logged.
//!
//! Raw little-endian `f32` is the canonical format: it is what USearch wants,
//! it is compact, and it needs no framing. `decode_embedding` still accepts
//! the legacy bincode layout so rows written before this change keep working —
//! see `looks_like_bincode` for how the two are told apart.

use crate::shared::error::AppError;
use crate::shared::result::Result;

/// Width of one `f32` on disk.
const F32_BYTES: usize = 4;

/// Length of the `u64` element count that `bincode` writes ahead of a `Vec<f32>`.
const BINCODE_LEN_PREFIX: usize = 8;

/// The canonical identity of an embedding, used both as the
/// `text_embeddings.id` primary key and as the vector-index key.
///
/// There must be exactly one scheme. Previously the repository wrote
/// `emb_{chunk_id}` while the indexing engine wrote a bare `Uuid::new_v4()`,
/// and the startup rebuild keyed the vector index off whichever `id` it found
/// — but deletion always looked for `emb_{chunk_id}`. For engine-written rows
/// the two never matched, so removals silently missed (`remove_embedding`
/// returns `Ok` for an absent key) and deleted documents kept coming back in
/// semantic search forever.
///
/// Deriving the key from `chunk_id` also makes it deterministic, so a
/// re-index produces the same key for the same chunk instead of orphaning the
/// old one.
pub fn vector_key(chunk_id: &str) -> String {
    format!("emb_{}", chunk_id)
}

/// Serialize a vector to its canonical on-disk form.
pub fn encode_embedding(vector: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vector.len() * F32_BYTES);
    for value in vector {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// Deserialize a vector, accepting both the canonical layout and the legacy
/// bincode layout.
///
/// # Errors
///
/// Returns `AppError::Deserialization` if `bytes` matches neither layout.
pub fn decode_embedding(bytes: &[u8]) -> Result<Vec<f32>> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }

    if looks_like_bincode(bytes) {
        let payload = bytes.get(BINCODE_LEN_PREFIX..).ok_or_else(|| {
            AppError::Deserialization("Embedding bincode payload is truncated".to_string())
        })?;
        return decode_raw(payload);
    }

    decode_raw(bytes)
}

/// True when `bytes` is a bincode-encoded `Vec<f32>` rather than raw floats.
///
/// A bincode payload is `8 + 4n` bytes and opens with the element count `n`.
/// A raw payload is `4n`. The two are ambiguous whenever `8 + 4n == 4m` — that
/// is, whenever a raw vector happens to be exactly two elements longer — so
/// the length check alone is not enough, and we additionally require the
/// prefix to actually equal the element count implied by the remaining bytes.
/// A raw vector would have to contain that precise value in its first two
/// floats to be misread, which is vanishingly unlikely for real embeddings
/// (it requires a specific tiny denormal bit pattern).
fn looks_like_bincode(bytes: &[u8]) -> bool {
    if bytes.len() < BINCODE_LEN_PREFIX {
        return false;
    }

    let payload_len = bytes.len() - BINCODE_LEN_PREFIX;
    if !payload_len.is_multiple_of(F32_BYTES) {
        return false;
    }

    let mut prefix = [0u8; BINCODE_LEN_PREFIX];
    let Some(prefix_bytes) = bytes.get(..BINCODE_LEN_PREFIX) else {
        return false;
    };
    prefix.copy_from_slice(prefix_bytes);
    let declared = u64::from_le_bytes(prefix);

    declared == (payload_len / F32_BYTES) as u64
}

fn decode_raw(bytes: &[u8]) -> Result<Vec<f32>> {
    if !bytes.len().is_multiple_of(F32_BYTES) {
        return Err(AppError::Deserialization(format!(
            "Embedding blob length {} is not a multiple of {}",
            bytes.len(),
            F32_BYTES
        )));
    }

    Ok(bytes
        .chunks_exact(F32_BYTES)
        .map(|chunk| {
            let mut buf = [0u8; F32_BYTES];
            buf.copy_from_slice(chunk);
            f32::from_le_bytes(buf)
        })
        .collect())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;

    fn sample() -> Vec<f32> {
        (0..384).map(|i| (i as f32) * 0.001 - 0.19).collect()
    }

    /// Reproduce bincode 1.x's legacy `Vec<f32>` wire format without keeping
    /// the now-unmaintained crate in the production dependency graph.
    fn encode_legacy_bincode(values: &[f32]) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(BINCODE_LEN_PREFIX + values.len() * F32_BYTES);
        encoded.extend_from_slice(&(values.len() as u64).to_le_bytes());
        for value in values {
            encoded.extend_from_slice(&value.to_le_bytes());
        }
        encoded
    }

    #[test]
    fn round_trips_canonical_encoding() {
        let original = sample();
        let decoded = decode_embedding(&encode_embedding(&original)).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn encoded_length_has_no_framing_overhead() {
        assert_eq!(encode_embedding(&sample()).len(), 384 * F32_BYTES);
    }

    /// Rows written by the old `EmbeddingRepository` must keep decoding — a
    /// user's existing index should survive the change, not be discarded.
    #[test]
    fn reads_legacy_bincode_rows() {
        let original = sample();
        let legacy = encode_legacy_bincode(&original);
        assert_eq!(legacy.len(), BINCODE_LEN_PREFIX + 384 * F32_BYTES);

        let decoded = decode_embedding(&legacy).unwrap();
        assert_eq!(
            decoded.len(),
            384,
            "a legacy row must decode to its true dimension, not dim + 2"
        );
        assert_eq!(decoded, original);
    }

    /// The specific historic failure: a bincode row read by the raw decoder
    /// looked like `dim + 2` floats, so the rebuild filter dropped it.
    #[test]
    fn legacy_rows_no_longer_decode_two_elements_long() {
        let legacy = encode_legacy_bincode(&sample());
        let naive = legacy.len() / F32_BYTES;
        assert_eq!(naive, 386, "precondition: naive read sees dim + 2");
        assert_eq!(decode_embedding(&legacy).unwrap().len(), 384);
    }

    #[test]
    fn empty_blob_decodes_to_empty_vector() {
        assert!(decode_embedding(&[]).unwrap().is_empty());
    }

    #[test]
    fn rejects_a_truncated_blob() {
        assert!(decode_embedding(&[1, 2, 3]).is_err());
    }

    #[test]
    fn raw_vectors_are_not_mistaken_for_bincode() {
        // Ordinary embedding values must never satisfy the bincode heuristic.
        for dim in [128usize, 384, 768, 1024] {
            let vector: Vec<f32> = (0..dim).map(|i| (i as f32).sin()).collect();
            let encoded = encode_embedding(&vector);
            assert!(
                !looks_like_bincode(&encoded),
                "raw {}-dim vector misidentified as bincode",
                dim
            );
            assert_eq!(decode_embedding(&encoded).unwrap().len(), dim);
        }
    }
}
