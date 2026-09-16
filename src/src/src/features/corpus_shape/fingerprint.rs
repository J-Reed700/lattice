//! Cluster fingerprint + Jaccard similarity — the label-stability layer.
//!
//! See `CLUSTERING-BACKEND-PLAN.md §4` for the full rationale. Two
//! primitives only:
//!
//! 1. `fingerprint(cluster)` — SHA-256 over sorted member ids + centroid
//!    bucketed to 2 decimals. Exact match ⇒ zero LLM calls, identical label.
//! 2. `jaccard(a, b)` — |A ∩ B| / |A ∪ B| over doc id sets. Threshold ≥ 0.5
//!    means "more than half the docs are the same" ⇒ inherit label.
//!
//! Both operate on bare data (ids + centroid) — the caller (`RunClusteringUseCase`)
//! decides what gets inherited.

use std::collections::HashSet;

use sha2::{Digest, Sha256};

/// Stable fingerprint version. Bump when the hashing algorithm changes so old
/// fingerprints auto-invalidate.
const FINGERPRINT_VERSION: &str = "v1";

/// Decimal places the centroid is quantized to before hashing. More precision
/// means tiny numeric drift produces a new fingerprint and a new LLM call;
/// less precision means truly different clusters collide. 2 dp is enough
/// separation for unit-normalized vectors without being over-sensitive.
const CENTROID_DECIMAL_PLACES: i32 = 2;

/// Compute a cluster's stability fingerprint.
///
/// `member_doc_ids` need not be pre-sorted — the function sorts internally.
/// `centroid` is the per-document mean-pooled centroid. Any alteration to the
/// input (add a doc, change a centroid component) produces a new fingerprint.
pub fn fingerprint(member_doc_ids: &[String], centroid: &[f32]) -> String {
    let mut ids: Vec<&String> = member_doc_ids.iter().collect();
    ids.sort();

    let mut hasher = Sha256::new();
    hasher.update(FINGERPRINT_VERSION.as_bytes());
    hasher.update(b"|");
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            hasher.update(b",");
        }
        hasher.update(id.as_bytes());
    }
    hasher.update(b"|");

    let multiplier = 10_f32.powi(CENTROID_DECIMAL_PLACES);
    for (i, &v) in centroid.iter().enumerate() {
        if i > 0 {
            hasher.update(b",");
        }
        let rounded = (v * multiplier).round() / multiplier;
        // Format with a fixed precision so "1.0" and "1.00" hash identically.
        let s = format!("{:.1$}", rounded, CENTROID_DECIMAL_PLACES as usize);
        hasher.update(s.as_bytes());
    }

    hex::encode(hasher.finalize())
}

/// Jaccard similarity: |A ∩ B| / |A ∪ B|. Returns 0.0 when both sets are empty.
pub fn jaccard(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let set_a: HashSet<&String> = a.iter().collect();
    let set_b: HashSet<&String> = b.iter().collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 {
        0.0
    } else {
        intersection as f32 / union as f32
    }
}

/// Threshold above which a previous cluster's label is inherited by a new one.
/// Below, the cluster has drifted enough that reusing the label would be dishonest.
pub const JACCARD_INHERITANCE_THRESHOLD: f32 = 0.5;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn identical_input_produces_identical_fingerprint() {
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let centroid = vec![1.0, 2.0, 3.0];
        let f1 = fingerprint(&ids, &centroid);
        let f2 = fingerprint(&ids, &centroid);
        assert_eq!(f1, f2);
    }

    #[test]
    fn id_order_does_not_affect_fingerprint() {
        let ids_a = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let ids_b = vec!["c".to_string(), "b".to_string(), "a".to_string()];
        let centroid = vec![1.0, 2.0, 3.0];
        let f1 = fingerprint(&ids_a, &centroid);
        let f2 = fingerprint(&ids_b, &centroid);
        assert_eq!(f1, f2);
    }

    #[test]
    fn adding_member_changes_fingerprint() {
        let ids_a = vec!["a".to_string(), "b".to_string()];
        let ids_b = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let centroid = vec![1.0, 2.0, 3.0];
        assert_ne!(fingerprint(&ids_a, &centroid), fingerprint(&ids_b, &centroid));
    }

    #[test]
    fn tiny_centroid_drift_below_rounding_does_not_change_fingerprint() {
        let ids = vec!["a".to_string(), "b".to_string()];
        let c1 = vec![1.001, 2.002, 3.003];
        let c2 = vec![1.004, 2.001, 3.002];
        // Rounded to 2dp: both should be 1.00 (ish) — same.
        assert_eq!(fingerprint(&ids, &c1), fingerprint(&ids, &c2));
    }

    #[test]
    fn large_centroid_drift_changes_fingerprint() {
        let ids = vec!["a".to_string(), "b".to_string()];
        let c1 = vec![1.0, 2.0, 3.0];
        let c2 = vec![1.5, 2.5, 3.5];
        assert_ne!(fingerprint(&ids, &c1), fingerprint(&ids, &c2));
    }

    #[test]
    fn jaccard_identical_sets_is_one() {
        let a = vec!["x".to_string(), "y".to_string()];
        let b = vec!["y".to_string(), "x".to_string()];
        assert_eq!(jaccard(&a, &b), 1.0);
    }

    #[test]
    fn jaccard_disjoint_sets_is_zero() {
        let a = vec!["a".to_string(), "b".to_string()];
        let b = vec!["c".to_string(), "d".to_string()];
        assert_eq!(jaccard(&a, &b), 0.0);
    }

    #[test]
    fn jaccard_partial_overlap() {
        let a: Vec<String> = (0..10).map(|i| format!("doc-{}", i)).collect();
        let mut b: Vec<String> = (5..15).map(|i| format!("doc-{}", i)).collect();
        b.sort();
        // Intersection = 5, Union = 15, Jaccard = 5/15 ≈ 0.333
        let j = jaccard(&a, &b);
        assert!((j - 0.3333).abs() < 0.01, "got {}", j);
    }

    #[test]
    fn jaccard_both_empty_is_zero() {
        let a: Vec<String> = vec![];
        let b: Vec<String> = vec![];
        assert_eq!(jaccard(&a, &b), 0.0);
    }

    #[test]
    fn jaccard_preserves_after_small_churn() {
        // 30-doc cluster, 5 added, 2 dropped: intersection 28, union 33, 28/33 ~ 0.848.
        let prev: Vec<String> = (0..30).map(|i| format!("d{}", i)).collect();
        let mut next: Vec<String> = (2..30).map(|i| format!("d{}", i)).collect(); // dropped 0,1
        for i in 30..35 {
            next.push(format!("d{}", i));
        }
        let j = jaccard(&prev, &next);
        assert!(
            j > JACCARD_INHERITANCE_THRESHOLD,
            "small churn should stay above threshold, got {}",
            j
        );
    }
}
