//! Corpus-shape domain entities.
//!
//! Clustering runs over document embeddings, producing stable, LLM-labeled
//! clusters that act as a read-only overlay on the lattice.
//!
//! The entities here are intentionally "dumb" records — all mutation rules
//! live in `use_cases/run_clustering.rs` and the repository.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// How a cluster's label was produced.
///
/// Tracked so we can audit LLM cost and stability in the debug JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelSource {
    /// Fresh LLM call — first time we've seen this cluster.
    Llm,
    /// Reused from a prior run via fingerprint exact match.
    InheritedExact,
    /// Reused from a prior run via Jaccard ≥ threshold.
    InheritedJaccard,
    /// LLM was not available or produced nothing usable.
    Fallback,
}

impl LabelSource {
    /// Storage form — stable across versions; do not rename without a migration.
    pub fn as_str(self) -> &'static str {
        match self {
            LabelSource::Llm => "llm",
            LabelSource::InheritedExact => "inherited_exact",
            LabelSource::InheritedJaccard => "inherited_jaccard",
            LabelSource::Fallback => "fallback",
        }
    }

    /// Parse from storage form. Unknown values round-trip to `Fallback`
    /// so old data never crashes the UI.
    pub fn from_storage(s: &str) -> Self {
        match s {
            "llm" => LabelSource::Llm,
            "inherited_exact" => LabelSource::InheritedExact,
            "inherited_jaccard" => LabelSource::InheritedJaccard,
            _ => LabelSource::Fallback,
        }
    }
}

impl std::str::FromStr for LabelSource {
    type Err = std::convert::Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_storage(value))
    }
}

/// Metadata about a single clustering run. One row per `cluster_vault_run`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterRun {
    pub id: String,
    pub ran_at: DateTime<Utc>,
    pub doc_count: i64,
    pub cluster_count: i64,
    pub noise_count: i64,
    pub params_hash: String,
    pub duration_ms: i64,
    pub llm_calls: i64,
}

/// A single cluster produced by a run.
///
/// `centroid` is the arithmetic mean of member embedding vectors (the per-doc
/// mean-pooled embedding, not the chunk vectors).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cluster {
    pub id: String,
    pub run_id: String,
    pub label: String,
    pub description: Option<String>,
    pub member_doc_ids: Vec<String>,
    pub centroid: Vec<f32>,
    pub fingerprint: String,
    pub label_source: LabelSource,
    pub inherited_from_cluster_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Per-member detail used by the labeling + persistence paths.
#[derive(Debug, Clone)]
pub struct ClusterMember {
    pub document_id: String,
    pub membership_probability: f32,
    pub is_representative: bool,
}

#[cfg(test)]
mod tests {
    use super::LabelSource;

    #[test]
    fn standard_parser_preserves_storage_fallback() {
        for value in [
            "llm",
            "inherited_exact",
            "inherited_jaccard",
            "fallback",
            "future_value",
        ] {
            assert_eq!(
                value.parse::<LabelSource>().unwrap(),
                LabelSource::from_storage(value)
            );
        }
    }
}
