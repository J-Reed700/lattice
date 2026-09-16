//! HDBSCAN clustering over per-document mean-pooled embeddings.
//!
//! This is the pure, deterministic stage of the pipeline — no LLM, no DB
//! access. Callers feed `(doc_id, vector)` pairs in, get back clusters +
//! outliers. Stability tests and synthetic fixtures live alongside the
//! labeling and fingerprint modules.
//!
//! # Tuning knobs
//!
//! - `min_cluster_size = 5` — smallest group we'll consider a cluster. Bigger
//!   means fewer, larger clusters + more noise.
//! - `min_samples = 3` — density threshold; smaller = looser clusters.
//!
//! These defaults can be tuned against the debug JSON output.

use ndarray::Array2;
use petal_clustering::{Fit, HDbscan};
use petal_neighbors::distance::Euclidean;

use crate::shared::error::{AppError, Result};

/// Hard cap on documents fed to one clustering run.
///
/// The pure stage is O(n²·d): at n = 4 000, d = 384 that is a few seconds on a
/// blocking thread; an unbounded vault would wedge the run. Callers take the
/// most recently indexed documents, which are the ones worth grouping.
pub const MAX_CLUSTERING_DOCS: usize = 4_000;

/// Default `min_cluster_size` — clusters with fewer members are relabelled as noise.
pub const DEFAULT_MIN_CLUSTER_SIZE: usize = 5;

/// Default `min_samples` — HDBSCAN's density threshold.
pub const DEFAULT_MIN_SAMPLES: usize = 3;

/// Input point: a document id + its mean-pooled embedding vector.
#[derive(Debug, Clone)]
pub struct ClusteringInput {
    pub doc_id: String,
    pub vector: Vec<f32>,
}

/// One cluster's worth of output — doc ids + centroid + membership probabilities.
#[derive(Debug, Clone)]
pub struct RawCluster {
    pub member_doc_ids: Vec<String>,
    pub centroid: Vec<f32>,
    /// Per-member membership probability (currently 1.0 for every member —
    /// petal-clustering returns outlier scores per point, not per-cluster
    /// soft memberships, so we treat HDBSCAN's hard assignment as 1.0 and
    /// leave room for a more nuanced value later).
    pub membership_probabilities: Vec<f32>,
    /// Up to 5 representative doc ids nearest the centroid (for LLM labeling).
    pub representatives: Vec<String>,
}

/// Result of a clustering call.
#[derive(Debug, Clone)]
pub struct ClusteringOutput {
    pub clusters: Vec<RawCluster>,
    pub noise_doc_ids: Vec<String>,
    pub embedding_dim: usize,
}

/// HDBSCAN parameters — kept separate so the caller (use case / test) can
/// override defaults explicitly.
#[derive(Debug, Clone, Copy)]
pub struct ClusteringParams {
    pub min_cluster_size: usize,
    pub min_samples: usize,
}

impl Default for ClusteringParams {
    fn default() -> Self {
        Self {
            min_cluster_size: DEFAULT_MIN_CLUSTER_SIZE,
            min_samples: DEFAULT_MIN_SAMPLES,
        }
    }
}

/// Cluster a set of per-document embeddings with HDBSCAN.
///
/// # Contract
///
/// - All input vectors must share the same dimension; mismatched dims return
///   `AppError::InvalidInput`.
/// - Fewer than `min_cluster_size` documents → returns zero clusters and
///   treats everything as noise (this is correct behavior, not failure).
/// - Output cluster order is not guaranteed stable across HDBSCAN versions;
///   downstream code must not depend on it.
pub fn cluster(inputs: Vec<ClusteringInput>, params: ClusteringParams) -> Result<ClusteringOutput> {
    if inputs.is_empty() {
        return Ok(ClusteringOutput {
            clusters: Vec::new(),
            noise_doc_ids: Vec::new(),
            embedding_dim: 0,
        });
    }

    let dim = inputs.first().map(|first| first.vector.len()).unwrap_or(0);

    if dim == 0 {
        return Err(AppError::InvalidInput(
            "Clustering input contains zero-dimensional vectors".to_string(),
        ));
    }

    for input in &inputs {
        if input.vector.len() != dim {
            return Err(AppError::InvalidInput(format!(
                "Embedding dimension mismatch: expected {}, got {} for doc {}",
                dim,
                input.vector.len(),
                input.doc_id
            )));
        }
    }

    let n = inputs.len();

    // petal-clustering 0.12 panics in `compute_core_distances` when there are
    // fewer points than `min_samples` (slice length mismatch on the k-NN
    // result). Guard explicitly: anything below the cluster-formation
    // threshold can't produce a real cluster anyway, so return all-noise.
    let min_points_required = params.min_cluster_size.max(params.min_samples);
    if n < min_points_required {
        let noise_doc_ids: Vec<String> = inputs.iter().map(|input| input.doc_id.clone()).collect();
        return Ok(ClusteringOutput {
            clusters: Vec::new(),
            noise_doc_ids,
            embedding_dim: dim,
        });
    }

    // Flatten (n × dim) into a row-major vector for ndarray.
    let mut flat: Vec<f64> = Vec::with_capacity(n * dim);
    for input in &inputs {
        for &v in &input.vector {
            flat.push(v as f64);
        }
    }

    let data = Array2::from_shape_vec((n, dim), flat)
        .map_err(|e| AppError::InvalidInput(format!("Failed to shape embedding matrix: {}", e)))?;

    let mut hdbscan = HDbscan::<f64, Euclidean> {
        alpha: 1.0,
        min_samples: params.min_samples,
        min_cluster_size: params.min_cluster_size,
        metric: Euclidean::default(),
        // Boruvka-based MST is faster at scale; correctness is the same.
        boruvka: true,
    };

    // Returns (clusters_map, outliers, outlier_scores). We drop the scores —
    // we're not surfacing per-doc outlierness in 5.3.
    let (cluster_map, outliers, _outlier_scores) = hdbscan.fit(&data, None);

    let mut clusters = Vec::with_capacity(cluster_map.len());
    for (_cluster_key, member_indices) in cluster_map {
        if member_indices.is_empty() {
            continue;
        }

        // Gather doc ids + vectors for this cluster.
        let mut member_doc_ids = Vec::with_capacity(member_indices.len());
        let mut vectors: Vec<&Vec<f32>> = Vec::with_capacity(member_indices.len());
        for &idx in &member_indices {
            let input = inputs.get(idx).ok_or_else(|| {
                AppError::InternalError(format!("HDBSCAN returned out-of-range index: {}", idx))
            })?;
            member_doc_ids.push(input.doc_id.clone());
            vectors.push(&input.vector);
        }

        // Arithmetic-mean centroid.
        let mut centroid = vec![0.0_f32; dim];
        for v in &vectors {
            for (i, &x) in v.iter().enumerate() {
                if let Some(slot) = centroid.get_mut(i) {
                    *slot += x;
                }
            }
        }
        let count = vectors.len() as f32;
        if count > 0.0 {
            for slot in centroid.iter_mut() {
                *slot /= count;
            }
        }

        // Top-5 representatives by Euclidean distance to centroid.
        let mut distances: Vec<(String, f32)> = vectors
            .iter()
            .zip(member_doc_ids.iter())
            .map(|(v, id)| (id.clone(), euclidean_distance(v, &centroid)))
            .collect();
        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let representatives: Vec<String> =
            distances.into_iter().take(5).map(|(id, _)| id).collect();

        let membership_probabilities = vec![1.0_f32; member_doc_ids.len()];

        clusters.push(RawCluster {
            member_doc_ids,
            centroid,
            membership_probabilities,
            representatives,
        });
    }

    let noise_doc_ids: Vec<String> = outliers
        .iter()
        .filter_map(|&idx| inputs.get(idx).map(|input| input.doc_id.clone()))
        .collect();

    Ok(ClusteringOutput {
        clusters,
        noise_doc_ids,
        embedding_dim: dim,
    })
}

/// Mean-pool a set of chunk-level embedding vectors into a single per-document
/// vector.
///
/// Returns `None` when `chunks` is empty — the caller should skip that doc.
pub fn mean_pool(chunks: &[Vec<f32>]) -> Option<Vec<f32>> {
    let first = chunks.first()?;
    let dim = first.len();
    if dim == 0 {
        return None;
    }

    let mut sum = vec![0.0_f32; dim];
    let mut n = 0_f32;
    for chunk in chunks {
        if chunk.len() != dim {
            // Mixed-dim chunks — don't silently corrupt the centroid.
            return None;
        }
        for (i, &x) in chunk.iter().enumerate() {
            if let Some(slot) = sum.get_mut(i) {
                *slot += x;
            }
        }
        n += 1.0;
    }
    if n == 0.0 {
        return None;
    }
    for slot in sum.iter_mut() {
        *slot /= n;
    }
    Some(sum)
}

fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut sum = 0.0_f32;
    let len = a.len().min(b.len());
    for i in 0..len {
        let av = a.get(i).copied().unwrap_or(0.0);
        let bv = b.get(i).copied().unwrap_or(0.0);
        let d = av - bv;
        sum += d * d;
    }
    sum.sqrt()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// Build a synthetic point near a given centroid with small jitter.
    ///
    /// Uses a multi-octave PRNG so the jitter looks uniformly random rather
    /// than a thin deterministic line — HDBSCAN picks up on low-rank noise
    /// structure and over-splits when every point lies on the same ray.
    fn jittered(center: &[f32], jitter: f32, seed: u64) -> Vec<f32> {
        let mut out = Vec::with_capacity(center.len());
        let mut state = seed
            .wrapping_add(0x9E37_79B9_7F4A_7C15)
            .wrapping_mul(0xBF58_476D_1CE4_E5B9);
        for &c in center.iter() {
            // xorshift-ish mixer per dim.
            state ^= state >> 30;
            state = state.wrapping_mul(0xBF58_476D_1CE4_E5B9);
            state ^= state >> 27;
            state = state.wrapping_mul(0x94D0_49BB_1331_11EB);
            state ^= state >> 31;
            let unit = (state as u32 as f32) / (u32::MAX as f32); // 0..=1
            let n = (unit - 0.5) * 2.0 * jitter;
            out.push(c + n);
        }
        out
    }

    #[test]
    fn empty_input_yields_empty_output() {
        let output = cluster(vec![], ClusteringParams::default()).unwrap();
        assert!(output.clusters.is_empty());
        assert!(output.noise_doc_ids.is_empty());
        assert_eq!(output.embedding_dim, 0);
    }

    #[test]
    fn single_doc_returns_all_noise_without_panic() {
        // Regression: petal-clustering 0.12 panics in compute_core_distances
        // when n < min_samples. Guard must catch this before HDBSCAN runs.
        let inputs = vec![ClusteringInput {
            doc_id: "lonely".into(),
            vector: vec![1.0, 2.0, 3.0],
        }];
        let output = cluster(inputs, ClusteringParams::default()).unwrap();
        assert!(output.clusters.is_empty());
        assert_eq!(output.noise_doc_ids, vec!["lonely".to_string()]);
        assert_eq!(output.embedding_dim, 3);
    }

    #[test]
    fn fewer_docs_than_min_cluster_size_returns_all_noise() {
        // 4 docs with min_cluster_size=5 → can't form a cluster, all noise.
        let inputs: Vec<ClusteringInput> = (0..4)
            .map(|i| ClusteringInput {
                doc_id: format!("doc-{}", i),
                vector: vec![i as f32, i as f32, i as f32],
            })
            .collect();
        let output = cluster(
            inputs,
            ClusteringParams {
                min_cluster_size: 5,
                min_samples: 3,
            },
        )
        .unwrap();
        assert!(output.clusters.is_empty());
        assert_eq!(output.noise_doc_ids.len(), 4);
    }

    #[test]
    fn mismatched_dim_is_rejected() {
        let inputs = vec![
            ClusteringInput {
                doc_id: "a".into(),
                vector: vec![1.0, 2.0, 3.0],
            },
            ClusteringInput {
                doc_id: "b".into(),
                vector: vec![1.0, 2.0],
            },
        ];
        let err = cluster(inputs, ClusteringParams::default()).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    #[test]
    fn three_obvious_blobs_are_detected() {
        // 3 well-separated gaussian blobs of ~35 points each + a few outliers.
        let centers = [
            vec![0.0_f32, 0.0, 0.0, 0.0],
            vec![10.0_f32, 10.0, 10.0, 10.0],
            vec![-10.0_f32, 10.0, -10.0, 10.0],
        ];
        let per_blob = 35;
        let jitter = 0.3;

        let mut inputs = Vec::new();
        for (bi, center) in centers.iter().enumerate() {
            for pi in 0..per_blob {
                let seed = ((bi * 1000) + pi) as u64;
                inputs.push(ClusteringInput {
                    doc_id: format!("doc-{}-{}", bi, pi),
                    vector: jittered(center, jitter, seed),
                });
            }
        }
        // A handful of scattered outliers — not enough to form a cluster.
        for oi in 0..4 {
            inputs.push(ClusteringInput {
                doc_id: format!("outlier-{}", oi),
                vector: jittered(&[50.0, -50.0, 50.0, -50.0], 5.0, oi as u64 + 999),
            });
        }

        let output = cluster(
            inputs,
            ClusteringParams {
                min_cluster_size: 5,
                min_samples: 3,
            },
        )
        .unwrap();

        assert_eq!(
            output.clusters.len(),
            3,
            "expected exactly 3 clusters, got {}",
            output.clusters.len()
        );
        // Every cluster should have close to per_blob members (allow small churn).
        for cluster in &output.clusters {
            assert!(
                cluster.member_doc_ids.len() >= per_blob - 3,
                "cluster too small: {}",
                cluster.member_doc_ids.len()
            );
            // Centroid has correct dimension.
            assert_eq!(cluster.centroid.len(), 4);
            // Representatives: at most 5, drawn from member set.
            assert!(cluster.representatives.len() <= 5);
            for rep in &cluster.representatives {
                assert!(cluster.member_doc_ids.contains(rep));
            }
        }

        // Scattered outliers should show up in noise.
        assert!(
            !output.noise_doc_ids.is_empty(),
            "expected some noise points"
        );
    }

    #[test]
    fn mean_pool_averages_chunks() {
        let chunks = vec![
            vec![1.0, 2.0, 3.0],
            vec![3.0, 4.0, 5.0],
            vec![5.0, 6.0, 7.0],
        ];
        let pooled = mean_pool(&chunks).unwrap();
        assert_eq!(pooled, vec![3.0, 4.0, 5.0]);
    }

    #[test]
    fn mean_pool_empty_returns_none() {
        assert!(mean_pool(&[]).is_none());
    }

    #[test]
    fn mean_pool_mismatched_dim_returns_none() {
        let chunks = vec![vec![1.0, 2.0], vec![1.0, 2.0, 3.0]];
        assert!(mean_pool(&chunks).is_none());
    }
}
