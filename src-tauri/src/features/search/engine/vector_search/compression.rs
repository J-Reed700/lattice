//! Opt-in vector compression for the USearch index.
//!
//! Two orthogonal levers, both off by default:
//!
//!  - **Matryoshka truncation.** Models trained with Matryoshka Representation
//!    Learning (Qwen3-Embedding-0.6B, for one) pack the most informative
//!    directions into the leading coordinates, so the first `dims` components
//!    of a 1024-d vector — renormalized — are still a usable embedding. Models
//!    without MRL training (all-MiniLM-L6-v2) do *not* survive this, which is
//!    why the choice is a per-index configuration rather than a global one.
//!  - **Scalar quantization.** USearch stores each component as `i8` instead of
//!    `f32`, quantizing on insert. Inputs are L2-normalized, so every component
//!    already lives in `[-1, 1]` and the cast is a plain scale to `±127`.
//!
//! Both are lossy, so the index becomes a *candidate generator*: a search
//! over-fetches `top_k * rescore_factor` neighbours and the caller rescores
//! them with exact cosine against full-precision vectors held in a side store.
//! That keeps the returned ordering identical to an uncompressed index for all
//! but pathological corpora, while the HNSW graph itself shrinks by the product
//! of both factors.
//!
//! # Compatibility
//!
//! Vectors written under one configuration are not interchangeable with vectors
//! written under another: the dimensionality and the scalar type both differ.
//! [`VectorIndexCompression::layout_token`] is the stable name for "which vector
//! space is on disk here", and it feeds both the index filename and the
//! persisted sidecar metadata so a configuration change rebuilds instead of
//! silently mixing generations.

use serde::{Deserialize, Serialize};

use crate::shared::error::AppError;
use crate::shared::result::Result;

/// Candidates fetched per requested result before exact rescoring.
///
/// Four is the usual recommendation for int8 HNSW: enough headroom that the
/// quantized ranking rarely drops a true top-k neighbour, cheap enough that the
/// rescore stays a rounding error next to the graph traversal.
pub const DEFAULT_RESCORE_FACTOR: usize = 4;

const fn default_rescore_factor() -> usize {
    DEFAULT_RESCORE_FACTOR
}

/// Scalar type USearch stores each vector component as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorQuantization {
    /// Full 32-bit floats — no quantization loss, 4 bytes per component.
    F32,
    /// Signed 8-bit integers — 1 byte per component. USearch quantizes on
    /// insert by scaling the (already unit-length) vector to `±127`.
    I8,
}

impl VectorQuantization {
    /// Bytes USearch spends per stored component.
    pub const fn bytes_per_component(self) -> usize {
        match self {
            Self::F32 => 4,
            Self::I8 => 1,
        }
    }

    /// Short stable token for layout identity.
    pub const fn token(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::I8 => "i8",
        }
    }
}

/// How vectors are stored in the USearch index.
///
/// `None` is the default and reproduces the historical behaviour exactly:
/// full-dimension `f32`, no side store, no rescoring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VectorIndexCompression {
    /// Store the embedding as produced: full dimension, `f32`.
    #[default]
    None,
    /// Store a renormalized `dims`-length prefix of the embedding, optionally
    /// quantized, and rescore over-fetched candidates against full vectors.
    Truncated {
        /// Leading components kept. Must be in `1..=dimension`.
        dims: usize,
        /// Scalar type of the stored components.
        quantization: VectorQuantization,
        /// Candidates fetched per requested result before rescoring.
        ///
        /// Defaulted on read so metadata written before this knob existed (and
        /// hand-edited files that omit it) still load.
        #[serde(default = "default_rescore_factor")]
        rescore_factor: usize,
    },
}

impl VectorIndexCompression {
    /// Matryoshka truncation to `dims` with the default rescore factor.
    pub const fn truncated(dims: usize, quantization: VectorQuantization) -> Self {
        Self::Truncated {
            dims,
            quantization,
            rescore_factor: DEFAULT_RESCORE_FACTOR,
        }
    }

    /// True when vectors are stored lossily and searches must rescore.
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Truncated { .. })
    }

    /// Dimension USearch is configured with, given the embedding dimension.
    pub const fn stored_dimension(&self, embedding_dimension: usize) -> usize {
        match self {
            Self::None => embedding_dimension,
            Self::Truncated { dims, .. } => *dims,
        }
    }

    /// Scalar type USearch stores components as.
    pub const fn quantization(&self) -> VectorQuantization {
        match self {
            Self::None => VectorQuantization::F32,
            Self::Truncated { quantization, .. } => *quantization,
        }
    }

    /// Candidates to fetch per requested result before rescoring.
    ///
    /// Always at least 1 so a corrupt or hand-written `0` cannot collapse the
    /// candidate window to nothing.
    pub const fn rescore_factor(&self) -> usize {
        match self {
            Self::None => 1,
            Self::Truncated { rescore_factor, .. } => {
                if *rescore_factor == 0 {
                    1
                } else {
                    *rescore_factor
                }
            }
        }
    }

    /// Bytes USearch spends per vector for a given embedding dimension.
    ///
    /// Excludes the HNSW graph links, which are unaffected by compression.
    pub const fn bytes_per_vector(&self, embedding_dimension: usize) -> usize {
        self.stored_dimension(embedding_dimension) * self.quantization().bytes_per_component()
    }

    /// Stable name for the vector space this configuration produces.
    ///
    /// `None` deliberately has no token so existing index filenames and
    /// metadata keep their exact spelling — an upgrade must not look like a
    /// generation change to a user who never turned compression on.
    pub fn layout_token(&self) -> Option<String> {
        match self {
            Self::None => None,
            Self::Truncated {
                dims, quantization, ..
            } => Some(format!("mrl{}{}", dims, quantization.token())),
        }
    }

    /// True when two configurations produce interchangeable stored vectors.
    ///
    /// `rescore_factor` is a query-time knob, not a storage property, so
    /// changing it must not cost the user a rebuild.
    pub fn layout_matches(&self, other: &Self) -> bool {
        self.layout_token() == other.layout_token()
    }

    /// Reject configurations that cannot describe a real index.
    pub fn validate(&self, embedding_dimension: usize) -> Result<()> {
        let Self::Truncated { dims, .. } = self else {
            return Ok(());
        };
        if *dims == 0 {
            return Err(AppError::InvalidInput(
                "Vector compression dims must be greater than 0".to_string(),
            ));
        }
        if *dims > embedding_dimension {
            return Err(AppError::InvalidInput(format!(
                "Vector compression dims {} exceeds embedding dimension {}",
                dims, embedding_dimension
            )));
        }
        Ok(())
    }

    /// Project an embedding into the stored vector space.
    ///
    /// Truncation alone would leave a vector shorter than unit length, and
    /// cosine over a non-normalized prefix is not the cosine the Matryoshka
    /// objective optimized. Renormalizing restores that, and — because USearch's
    /// `f32 → i8` cast divides by the vector's own magnitude — also makes the
    /// quantization grid line up the same way for every vector.
    ///
    /// Returns a borrow when the configuration is `None`, so the uncompressed
    /// path copies nothing.
    pub fn project<'a>(&self, vector: &'a [f32]) -> Result<std::borrow::Cow<'a, [f32]>> {
        let Self::Truncated { dims, .. } = self else {
            return Ok(std::borrow::Cow::Borrowed(vector));
        };
        let head = vector.get(..*dims).ok_or_else(|| {
            AppError::InvalidInput(format!(
                "Cannot truncate a {}-dimension vector to {} dimensions",
                vector.len(),
                dims
            ))
        })?;

        let norm = head.iter().map(|v| v * v).sum::<f32>().sqrt();
        if !norm.is_finite() || norm <= 0.0 {
            // A prefix can legitimately be all zeros for a degenerate vector.
            // Renormalizing would produce NaNs, which USearch turns into
            // undefined neighbours; the raw prefix at least stays comparable.
            return Ok(std::borrow::Cow::Owned(head.to_vec()));
        }
        Ok(std::borrow::Cow::Owned(
            head.iter().map(|v| v / norm).collect(),
        ))
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;

    fn unit(values: &[f32]) -> Vec<f32> {
        let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
        values.iter().map(|v| v / norm).collect()
    }

    #[test]
    fn none_is_the_default_and_borrows_the_input() {
        let config = VectorIndexCompression::default();
        assert_eq!(config, VectorIndexCompression::None);
        assert!(!config.is_active());
        assert_eq!(config.stored_dimension(1024), 1024);
        assert!(config.layout_token().is_none());

        let vector = unit(&[1.0, 2.0, 3.0, 4.0]);
        let projected = config.project(&vector).unwrap();
        assert!(matches!(projected, std::borrow::Cow::Borrowed(_)));
        assert_eq!(projected.as_ref(), vector.as_slice());
    }

    #[test]
    fn truncation_keeps_the_leading_components_and_renormalizes() {
        let config = VectorIndexCompression::truncated(2, VectorQuantization::I8);
        let vector = unit(&[3.0, 4.0, 12.0, 0.0]);

        let projected = config.project(&vector).unwrap();
        assert_eq!(projected.len(), 2);

        // Direction of the prefix is preserved …
        let ratio = projected.first().unwrap() / projected.get(1).unwrap();
        assert!((ratio - 3.0 / 4.0).abs() < 1e-6, "prefix direction changed");

        // … and the result is unit length, which plain truncation would not be.
        let norm = projected.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6, "projection norm was {norm}");

        let truncated_only = vector.get(..2).unwrap();
        let raw_norm = truncated_only.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(
            (raw_norm - 1.0).abs() > 1e-3,
            "precondition: the raw prefix is not already unit length"
        );
    }

    #[test]
    fn projecting_an_all_zero_prefix_does_not_produce_nans() {
        let config = VectorIndexCompression::truncated(2, VectorQuantization::F32);
        let projected = config.project(&[0.0, 0.0, 1.0, 0.0]).unwrap();
        assert!(projected.iter().all(|v| v.is_finite()));
        assert_eq!(projected.as_ref(), &[0.0, 0.0]);
    }

    #[test]
    fn projecting_a_too_short_vector_is_an_error() {
        let config = VectorIndexCompression::truncated(8, VectorQuantization::I8);
        assert!(config.project(&[1.0, 0.0]).is_err());
    }

    #[test]
    fn validate_rejects_impossible_dims() {
        assert!(VectorIndexCompression::None.validate(384).is_ok());
        assert!(
            VectorIndexCompression::truncated(384, VectorQuantization::I8)
                .validate(384)
                .is_ok()
        );
        assert!(VectorIndexCompression::truncated(0, VectorQuantization::I8)
            .validate(384)
            .is_err());
        assert!(
            VectorIndexCompression::truncated(512, VectorQuantization::I8)
                .validate(384)
                .is_err()
        );
    }

    #[test]
    fn layout_identity_ignores_the_rescore_factor_but_not_storage() {
        let base = VectorIndexCompression::truncated(512, VectorQuantization::I8);
        let wider = VectorIndexCompression::Truncated {
            dims: 512,
            quantization: VectorQuantization::I8,
            rescore_factor: 16,
        };
        assert!(base.layout_matches(&wider), "rescore factor is query-time");

        let other_dims = VectorIndexCompression::truncated(256, VectorQuantization::I8);
        let other_scalar = VectorIndexCompression::truncated(512, VectorQuantization::F32);
        assert!(!base.layout_matches(&other_dims));
        assert!(!base.layout_matches(&other_scalar));
        assert!(!base.layout_matches(&VectorIndexCompression::None));

        assert_eq!(base.layout_token().as_deref(), Some("mrl512i8"));
    }

    #[test]
    fn rescore_factor_never_collapses_the_candidate_window() {
        let corrupt = VectorIndexCompression::Truncated {
            dims: 64,
            quantization: VectorQuantization::I8,
            rescore_factor: 0,
        };
        assert_eq!(corrupt.rescore_factor(), 1);
        assert_eq!(VectorIndexCompression::None.rescore_factor(), 1);
    }

    #[test]
    fn storage_cost_shrinks_by_truncation_times_quantization() {
        assert_eq!(VectorIndexCompression::None.bytes_per_vector(1024), 4096);
        assert_eq!(
            VectorIndexCompression::truncated(512, VectorQuantization::I8).bytes_per_vector(1024),
            512
        );
        assert_eq!(
            VectorIndexCompression::truncated(512, VectorQuantization::F32).bytes_per_vector(1024),
            2048
        );
    }

    #[test]
    fn metadata_json_defaults_the_rescore_factor_when_absent() {
        let parsed: VectorIndexCompression =
            serde_json::from_str(r#"{"kind":"truncated","dims":512,"quantization":"i8"}"#).unwrap();
        assert_eq!(
            parsed,
            VectorIndexCompression::truncated(512, VectorQuantization::I8)
        );
        assert_eq!(parsed.rescore_factor(), DEFAULT_RESCORE_FACTOR);
    }

    #[test]
    fn config_round_trips_through_json() {
        for config in [
            VectorIndexCompression::None,
            VectorIndexCompression::truncated(256, VectorQuantization::I8),
            VectorIndexCompression::Truncated {
                dims: 768,
                quantization: VectorQuantization::F32,
                rescore_factor: 8,
            },
        ] {
            let json = serde_json::to_string(&config).unwrap();
            let back: VectorIndexCompression = serde_json::from_str(&json).unwrap();
            assert_eq!(back, config, "round trip lost information: {json}");
        }
    }
}
