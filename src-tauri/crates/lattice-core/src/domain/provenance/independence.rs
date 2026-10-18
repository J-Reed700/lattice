//! Independence (§7): corroboration counts origins, not pages. Copies,
//! mirrors, syndication and archives are folded into one origin cluster
//! before any "two independent sources" rule is applied.
//!
//! Copy detection is deterministic: resemblance and containment of exact
//! shingle sets. Embeddings stay out of it, because they score topical
//! similarity, which is exactly what two *independent* sources on one
//! subject share.

use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Values in a [`Sketch`]: one minimum per hash function.
pub const SKETCH_VALUES: usize = 128;

/// The shingles of one text, hashed. Exact: inside a course every pair is
/// compared on the full sets; the [`Sketch`] is kept for indexes across
/// courses and chat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShingleSet {
    pub(super) hashes: HashSet<u64>,
}

impl ShingleSet {
    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }
}

/// A MinHash sketch of a shingle set: [`SKETCH_VALUES`] minima, stored per
/// version as little-endian `u32`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sketch(pub [u32; SKETCH_VALUES]);

impl Sketch {
    /// The storage form: each value little-endian, in order.
    pub fn to_le_bytes(&self) -> Vec<u8> {
        self.0
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    /// Reads [`Sketch::to_le_bytes`]; `None` unless `bytes` is exactly
    /// [`SKETCH_VALUES`] little-endian `u32`s.
    pub fn from_le_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != SKETCH_VALUES * 4 {
            return None;
        }
        let mut values = [0u32; SKETCH_VALUES];
        for (value, chunk) in values.iter_mut().zip(bytes.chunks_exact(4)) {
            *value = u32::from_le_bytes(chunk.try_into().ok()?);
        }
        Some(Self(values))
    }
}

impl Serialize for Sketch {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter())
    }
}

impl<'de> Deserialize<'de> for Sketch {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<u32>::deserialize(deserializer)?;
        let values: [u32; SKETCH_VALUES] = values.try_into().map_err(|values: Vec<u32>| {
            serde::de::Error::invalid_length(values.len(), &"128 sketch values")
        })?;
        Ok(Self(values))
    }
}

/// An edge of the provenance graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// `from` is a copy of `to` (resemblance over the whole text); the newer
    /// version points at the older.
    CopyOf,
    /// `from` contains a copy of `to`'s text alongside other material.
    ContainsCopyOf,
    /// `from` is an archived copy of `to`.
    ArchiveOf,
    /// `from` names `to` as its canonical form on another domain
    /// (syndication).
    CanonicalOf,
    /// `from` links to `to` from a span of its text.
    Cites,
    /// `from` was cited for a claim and `to` was fetched to check it.
    CitedForClaim,
}

wire_names!(RelationKind {
    CopyOf => "copy_of",
    ContainsCopyOf => "contains_copy_of",
    ArchiveOf => "archive_of",
    CanonicalOf => "canonical_of",
    Cites => "cites",
    CitedForClaim => "cited_for_claim",
});

/// What a relation points at: a captured version, or a URL not captured.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationTarget {
    Version(String),
    Url(String),
}

/// Why a relation was recorded, by kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "evidence", rename_all = "snake_case")]
pub enum RelationEvidence {
    /// `CopyOf` / `ContainsCopyOf`: the measured overlap. `containment` is
    /// the share of `to`'s shingles found in `from`.
    Text { resemblance: f64, containment: f64 },
    /// `ArchiveOf`.
    Archive {
        archive_host: String,
        #[serde(default)]
        archived_at: Option<i64>,
    },
    /// `CanonicalOf`.
    Canonical { canonical_url: String },
    /// `Cites`: the link's bytes in `from`'s text and its anchor.
    Citation {
        start_byte: usize,
        end_byte: usize,
        anchor: String,
    },
    /// `CitedForClaim`: whether the cited page bore the claim out; `None`
    /// with `fetch_failed` when it could not be read.
    ClaimCitation {
        claim_id: String,
        #[serde(default)]
        borne_out: Option<bool>,
        #[serde(default)]
        fetch_failed: Option<String>,
    },
}

/// One edge: `from` (a version id) relates to `to` as `kind` says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceRelation {
    pub from: String,
    pub to: RelationTarget,
    pub kind: RelationKind,
    pub evidence: RelationEvidence,
}

/// An origin cluster's identity: its smallest member version id, so the
/// same supports always give the same id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClusterId(pub String);

/// Versions that count as one origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cluster {
    pub id: ClusterId,
    /// Member version ids, sorted.
    pub members: Vec<String>,
}
