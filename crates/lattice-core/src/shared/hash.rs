//! Content hashing shared by every crate.

use sha2::{Digest, Sha256};

/// Lower-case hex sha256 of `data`.
pub fn sha256_hex(data: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(data.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;

    #[test]
    fn hashes_text_and_bytes_to_the_same_lower_case_digest() {
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_hex("abc"), expected);
        assert_eq!(sha256_hex(b"abc"), expected);
        assert_eq!(sha256_hex(String::from("abc").as_bytes()), expected);
        assert_eq!(sha256_hex(vec![b'a', b'b', b'c']), expected);
    }
}
