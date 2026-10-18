//! Content identity of an embedding model's artifacts.
//!
//! The identity names the vector space a model produces: it selects the
//! `usearch-<id>.usearch` file and the rows of `embedding_generation_vectors`
//! that belong to it. It is computed once, when a model is activated for
//! embedding, and stored on the model's row. Reads never rehash.
//!
//! Pure domain type: no I/O. Computation lives in
//! `crate::features::embedding::artifact_identity`.

use serde::{Deserialize, Serialize};
use std::fmt;

const SCHEME: &str = "sha256:";
const HEX_LEN: usize = 64;

/// Content identity of an embedding model's artifacts: `sha256:` + 64 lowercase hex.
/// Produced once, when a model is activated for embedding, and stored on its row.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ArtifactIdentity(String);

impl ArtifactIdentity {
    /// Parse a stored value. Rejects anything that is not `sha256:` + 64 lowercase hex.
    pub fn parse(value: &str) -> Result<Self, ArtifactIdentityError> {
        let well_formed = value.strip_prefix(SCHEME).is_some_and(|hex| {
            hex.len() == HEX_LEN
                && hex
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        });
        if well_formed {
            Ok(Self(value.to_string()))
        } else {
            Err(ArtifactIdentityError(value.to_string()))
        }
    }

    /// Build from a freshly finalized digest.
    pub fn from_digest(digest: &[u8; 32]) -> Self {
        Self(format!("{SCHEME}{}", hex::encode(digest)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ArtifactIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<String> for ArtifactIdentity {
    type Error = ArtifactIdentityError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<ArtifactIdentity> for String {
    fn from(identity: ArtifactIdentity) -> Self {
        identity.0
    }
}

#[derive(Debug, thiserror::Error)]
#[error("malformed artifact identity: {0}")]
pub struct ArtifactIdentityError(String);

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;

    fn hex_digits(len: usize) -> String {
        "0123456789abcdef".chars().cycle().take(len).collect()
    }

    #[test]
    fn parse_accepts_scheme_and_lowercase_hex() {
        let value = format!("sha256:{}", hex_digits(64));
        let identity = ArtifactIdentity::parse(&value).expect("well-formed identity");
        assert_eq!(identity.as_str(), value);
        assert_eq!(identity.to_string(), value);
    }

    #[test]
    fn parse_rejects_uppercase_hex() {
        let value = format!("sha256:{}", hex_digits(64).to_uppercase());
        assert!(ArtifactIdentity::parse(&value).is_err());
    }

    #[test]
    fn parse_rejects_wrong_length() {
        for len in [0, 63, 65] {
            let value = format!("sha256:{}", hex_digits(len));
            assert!(ArtifactIdentity::parse(&value).is_err(), "{len} hex digits");
        }
    }

    #[test]
    fn parse_rejects_missing_or_foreign_prefix() {
        for value in [
            hex_digits(64),
            format!("sha1:{}", hex_digits(64)),
            format!(":{}", hex_digits(64)),
        ] {
            assert!(ArtifactIdentity::parse(&value).is_err(), "{value}");
        }
    }

    #[test]
    fn from_digest_round_trips_through_parse() {
        let mut digest = [0u8; 32];
        for (value, byte) in (0u8..).zip(digest.iter_mut()) {
            *byte = value;
        }
        let identity = ArtifactIdentity::from_digest(&digest);
        assert_eq!(
            identity.as_str(),
            "sha256:000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
        );
        assert_eq!(
            ArtifactIdentity::parse(identity.as_str()).unwrap(),
            identity
        );
    }

    #[test]
    fn serde_uses_the_bare_string_and_rejects_malformed_values() {
        let identity = ArtifactIdentity::from_digest(&[0xab; 32]);
        let json = serde_json::to_string(&identity).unwrap();
        assert_eq!(json, format!("\"{}\"", identity.as_str()));
        assert_eq!(
            serde_json::from_str::<ArtifactIdentity>(&json).unwrap(),
            identity
        );

        let malformed = format!("\"sha256:{}\"", "AB".repeat(32));
        assert!(serde_json::from_str::<ArtifactIdentity>(&malformed).is_err());
    }
}
