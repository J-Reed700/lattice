//! Content hashing shared by every crate.

use sha2::{Digest, Sha256};

/// Lower-case hex sha256 of `data`.
pub fn sha256_hex(data: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(data.as_ref()))
}

/// A short identity for content: the first eight bytes of its sha256.
pub fn sha256_prefix(data: impl AsRef<[u8]>) -> u64 {
    Sha256::digest(data.as_ref())
        .iter()
        .take(8)
        .fold(0, |prefix, byte| (prefix << 8) | u64::from(*byte))
}

#[cfg(test)]
mod tests {
    use super::{sha256_hex, sha256_prefix};

    #[test]
    fn a_prefix_is_the_first_eight_bytes_of_the_digest() {
        assert_eq!(sha256_prefix("abc"), 0xba78_16bf_8f01_cfea);
        assert_eq!(sha256_prefix(b"abc"), sha256_prefix("abc"));
    }

    #[test]
    fn hashes_text_and_bytes_to_the_same_lower_case_digest() {
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_hex("abc"), expected);
        assert_eq!(sha256_hex(b"abc"), expected);
        assert_eq!(sha256_hex(String::from("abc").as_bytes()), expected);
        assert_eq!(sha256_hex(vec![b'a', b'b', b'c']), expected);
    }
}
