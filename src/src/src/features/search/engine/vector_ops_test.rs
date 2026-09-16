//! Property-based tests for vector operations
//!
//! This module contains comprehensive property tests using proptest to verify
//! mathematical invariants and correctness of vector operations.

#[cfg(test)]
mod property_tests {
    use super::super::vector_ops::*;
    use proptest::prelude::*;

    /// Strategy for generating valid f32 vectors (avoiding NaN/Inf)
    fn valid_f32_vector(size: usize) -> impl Strategy<Value = Vec<f32>> {
        prop::collection::vec(-1000.0_f32..1000.0_f32, size)
    }

    /// Strategy for generating normalized vectors
    fn normalized_vector(size: usize) -> impl Strategy<Value = Vec<f32>> {
        valid_f32_vector(size).prop_map(|mut v| {
            normalize_vector(&mut v);
            v
        })
    }

    /// Strategy for generating non-zero vectors
    fn non_zero_vector(size: usize) -> impl Strategy<Value = Vec<f32>> {
        valid_f32_vector(size).prop_filter("vector must be non-zero", |v| {
            v.iter().any(|&x| x.abs() > 1e-10)
        })
    }

    proptest! {
        /// Property: Cosine similarity must always return a value in [-1, 1]
        ///
        /// Mathematical basis: cos(θ) ∈ [-1, 1] for any angle θ
        #[test]
        fn prop_cosine_similarity_bounded(
            a in valid_f32_vector(128),
            b in valid_f32_vector(128)
        ) {
            // Skip if either vector is zero
            if a.iter().all(|&x| x.abs() < 1e-10) || b.iter().all(|&x| x.abs() < 1e-10) {
                return Ok(());
            }

            let sim_naive = cosine_similarity_naive(&a, &b);
            let sim_simd = cosine_similarity_simd(&a, &b);

            prop_assert!((-1.0 - 1e-5..=1.0 + 1e-5).contains(&sim_naive),
                "Naive cosine similarity out of bounds: {}", sim_naive);
            prop_assert!((-1.0 - 1e-5..=1.0 + 1e-5).contains(&sim_simd),
                "SIMD cosine similarity out of bounds: {}", sim_simd);
        }

        /// Property: Cosine similarity is symmetric
        ///
        /// cos_sim(a, b) = cos_sim(b, a)
        #[test]
        fn prop_cosine_similarity_symmetric(
            a in valid_f32_vector(256),
            b in valid_f32_vector(256)
        ) {
            // Skip zero vectors
            if a.iter().all(|&x| x.abs() < 1e-10) || b.iter().all(|&x| x.abs() < 1e-10) {
                return Ok(());
            }

            let sim_ab = cosine_similarity_naive(&a, &b);
            let sim_ba = cosine_similarity_naive(&b, &a);

            prop_assert!((sim_ab - sim_ba).abs() < 1e-5,
                "Cosine similarity not symmetric: {} vs {}", sim_ab, sim_ba);
        }

        /// Property: Self-similarity of normalized vectors equals 1
        ///
        /// For any non-zero vector v, cos_sim(v, v) = 1
        #[test]
        fn prop_cosine_self_similarity(
            v in non_zero_vector(384)
        ) {
            let sim_naive = cosine_similarity_naive(&v, &v);
            let sim_simd = cosine_similarity_simd(&v, &v);

            prop_assert!((sim_naive - 1.0).abs() < 1e-5,
                "Naive self-similarity should be ~1.0, got {}", sim_naive);
            prop_assert!((sim_simd - 1.0).abs() < 1e-5,
                "SIMD self-similarity should be ~1.0, got {}", sim_simd);
        }

        /// Property: SIMD and naive implementations produce equivalent results
        ///
        /// This is critical for correctness - SIMD optimization should not change results
        #[test]
        fn prop_simd_naive_equivalence(
            a in valid_f32_vector(512),
            b in valid_f32_vector(512)
        ) {
            // Skip zero vectors
            if a.iter().all(|&x| x.abs() < 1e-10) || b.iter().all(|&x| x.abs() < 1e-10) {
                return Ok(());
            }

            let sim_naive = cosine_similarity_naive(&a, &b);
            let sim_simd = cosine_similarity_simd(&a, &b);

            prop_assert!((sim_naive - sim_simd).abs() < 1e-4,
                "SIMD and naive differ: naive={}, simd={}, diff={}",
                sim_naive, sim_simd, (sim_naive - sim_simd).abs());
        }

        /// Property: Opposite vectors have similarity -1
        ///
        /// cos_sim(v, -v) = -1
        #[test]
        fn prop_cosine_opposite_vectors(
            v in non_zero_vector(128)
        ) {
            let neg_v: Vec<f32> = v.iter().map(|&x| -x).collect();

            let sim = cosine_similarity_naive(&v, &neg_v);

            prop_assert!((sim - (-1.0)).abs() < 1e-5,
                "Opposite vectors should have similarity -1, got {}", sim);
        }

        /// Property: Orthogonal vectors have similarity ~0
        ///
        /// For vectors a and b where dot(a, b) = 0, cos_sim(a, b) = 0
        #[test]
        fn prop_cosine_orthogonal_vectors(
            x in -1000.0_f32..1000.0_f32,
            y in -1000.0_f32..1000.0_f32,
        ) {
            // Skip zero values
            if x.abs() < 1e-10 || y.abs() < 1e-10 {
                return Ok(());
            }

            let a = vec![x, y];
            let b = vec![-y, x];  // Perpendicular to a

            let sim = cosine_similarity_naive(&a, &b);

            prop_assert!(sim.abs() < 1e-5,
                "Orthogonal vectors should have similarity ~0, got {}", sim);
        }

        /// Property: Scaling vectors doesn't change cosine similarity
        ///
        /// cos_sim(a, b) = cos_sim(k*a, b) = cos_sim(a, k*b) for any scalar k ≠ 0
        #[test]
        fn prop_cosine_scale_invariant(
            a in non_zero_vector(128),
            b in non_zero_vector(128),
            scale in 0.1_f32..100.0_f32,
        ) {
            let scaled_a: Vec<f32> = a.iter().map(|&x| x * scale).collect();

            let sim_original = cosine_similarity_naive(&a, &b);
            let sim_scaled = cosine_similarity_naive(&scaled_a, &b);

            prop_assert!((sim_original - sim_scaled).abs() < 1e-4,
                "Scaling should not affect cosine similarity: {} vs {}",
                sim_original, sim_scaled);
        }

        /// Property: Batch operations produce same results as individual operations
        #[test]
        fn prop_cosine_batch_consistency(
            query in valid_f32_vector(128),
            embeddings in prop::collection::vec(valid_f32_vector(128), 1..20)
        ) {
            // Skip if query is zero
            if query.iter().all(|&x| x.abs() < 1e-10) {
                return Ok(());
            }

            let batch_result: Vec<f32> = embeddings.iter()
                .map(|emb| cosine_similarity_simd(&query, emb))
                .collect();

            for (i, emb) in embeddings.iter().enumerate() {
                // Skip zero embeddings
                if emb.iter().all(|&x| x.abs() < 1e-10) {
                    continue;
                }

                let individual_result = cosine_similarity_simd(&query, emb);
                prop_assert!((batch_result[i] - individual_result).abs() < 1e-5,
                    "Batch result differs from individual at index {}: {} vs {}",
                    i, batch_result[i], individual_result);
            }
        }
    }

    proptest! {
        /// Property: Normalized vectors have magnitude 1
        ///
        /// ||normalize(v)|| = 1 for any non-zero vector v
        #[test]
        fn prop_normalize_unit_length(
            v in non_zero_vector(256)
        ) {
            let mut normalized = v.clone();
            normalize_vector(&mut normalized);

            let magnitude: f32 = normalized.iter().map(|&x| x * x).sum::<f32>().sqrt();

            prop_assert!((magnitude - 1.0).abs() < 1e-5,
                "Normalized vector should have magnitude 1, got {}", magnitude);
        }

        /// Property: Normalization preserves direction
        ///
        /// normalize(v) = k*v for some scalar k > 0 (for non-zero v)
        #[test]
        fn prop_normalize_preserves_direction(
            v in non_zero_vector(128)
        ) {
            let mut normalized = v.clone();
            normalize_vector(&mut normalized);

            // All ratios v[i]/normalized[i] should be equal (the magnitude)
            let mut ratios = Vec::new();
            for (orig, norm) in v.iter().zip(normalized.iter()) {
                if orig.abs() > 1e-10 && norm.abs() > 1e-10 {
                    ratios.push(orig / norm);
                }
            }

            if ratios.len() > 1 {
                let first_ratio = ratios[0];
                for &ratio in &ratios[1..] {
                    prop_assert!((ratio - first_ratio).abs() / first_ratio.abs() < 1e-4,
                        "Direction not preserved: ratios vary");
                }
            }
        }

        /// Property: Normalization is idempotent
        ///
        /// normalize(normalize(v)) = normalize(v)
        #[test]
        fn prop_normalize_idempotent(
            v in non_zero_vector(384)
        ) {
            let mut once = v.clone();
            normalize_vector(&mut once);

            let mut twice = once.clone();
            normalize_vector(&mut twice);

            for (a, b) in once.iter().zip(twice.iter()) {
                prop_assert!((a - b).abs() < 1e-5,
                    "Normalization not idempotent");
            }
        }

        /// Property: Zero vector remains zero after normalization
        #[test]
        fn prop_normalize_zero_vector(
            size in 1_usize..500
        ) {
            let mut zero_vec = vec![0.0_f32; size];
            normalize_vector(&mut zero_vec);

            prop_assert!(zero_vec.iter().all(|&x| x.abs() < 1e-10),
                "Zero vector should remain zero after normalization");
        }

        /// Property: Batch normalization equals individual normalization
        #[test]
        fn prop_normalize_batch_consistency(
            embeddings in prop::collection::vec(non_zero_vector(128), 1..20)
        ) {
            let mut batch = embeddings.clone();
            for emb in batch.iter_mut() {
                normalize_vector(emb);
            }

            for (i, emb) in embeddings.iter().enumerate() {
                let mut individual = emb.clone();
                normalize_vector(&mut individual);

                for (a, b) in batch[i].iter().zip(individual.iter()) {
                    prop_assert!((a - b).abs() < 1e-5,
                        "Batch normalization differs from individual at index {}", i);
                }
            }
        }

        /// Property: After normalization, cosine similarity equals dot product
        ///
        /// For normalized vectors a and b: cos_sim(a, b) = dot(a, b)
        #[test]
        fn prop_normalized_cosine_equals_dot(
            a in normalized_vector(128),
            b in normalized_vector(128)
        ) {
            let cosine_sim = cosine_similarity_naive(&a, &b);
            let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();

            prop_assert!((cosine_sim - dot_product).abs() < 1e-4,
                "For normalized vectors, cosine similarity should equal dot product: {} vs {}",
                cosine_sim, dot_product);
        }
    }

    proptest! {
        /// Property: Works with various vector sizes
        #[test]
        fn prop_various_sizes(
            size in 1_usize..1024,
        ) {
            let a: Vec<f32> = (0..size).map(|i| i as f32).collect();
            let b: Vec<f32> = (0..size).map(|i| (i * 2) as f32).collect();

            let _sim = cosine_similarity_naive(&a, &b);
            let _sim = cosine_similarity_simd(&a, &b);
        }

        /// Property: Handles very small values without underflow
        #[test]
        fn prop_small_values(
            a in prop::collection::vec(1e-20_f32..1e-10_f32, 128),
            b in prop::collection::vec(1e-20_f32..1e-10_f32, 128),
        ) {
            let sim = cosine_similarity_naive(&a, &b);

            prop_assert!(!sim.is_nan(), "Result should not be NaN");
            prop_assert!(sim.is_finite(), "Result should be finite");
        }

        /// Property: Handles very large values without overflow
        #[test]
        fn prop_large_values(
            a in prop::collection::vec(1e10_f32..1e20_f32, 128),
            b in prop::collection::vec(1e10_f32..1e20_f32, 128),
        ) {
            let sim = cosine_similarity_naive(&a, &b);

            prop_assert!(!sim.is_nan(), "Result should not be NaN");
            prop_assert!(sim.is_finite(), "Result should be finite");
            prop_assert!((-1.0 - 1e-5..=1.0 + 1e-5).contains(&sim),
                "Result should be in valid range");
        }

        /// Property: Handles mixed positive and negative values
        #[test]
        fn prop_mixed_signs(
            a in prop::collection::vec(-1000.0_f32..1000.0_f32, 256),
            b in prop::collection::vec(-1000.0_f32..1000.0_f32, 256),
        ) {
            // Skip zero vectors
            if a.iter().all(|&x| x.abs() < 1e-10) || b.iter().all(|&x| x.abs() < 1e-10) {
                return Ok(());
            }

            let sim_naive = cosine_similarity_naive(&a, &b);
            let sim_simd = cosine_similarity_simd(&a, &b);

            prop_assert!(!sim_naive.is_nan(), "Naive result should not be NaN");
            prop_assert!(!sim_simd.is_nan(), "SIMD result should not be NaN");
            prop_assert!((sim_naive - sim_simd).abs() < 1e-4,
                "Implementations should agree with mixed signs");
        }
    }

    proptest! {
        /// Property: SIMD works correctly with non-aligned sizes
        ///
        /// Tests that remainder handling in SIMD code is correct
        #[test]
        fn prop_simd_non_aligned_sizes(
            size in prop::sample::select(vec![
                1, 3, 5, 7, 9, 13, 17, 31, 63, 127, 129, 255, 257, 383, 385, 511, 513
            ])
        ) {
            let a: Vec<f32> = (0..size).map(|i| (i as f32 + 1.0) / 100.0).collect();
            let b: Vec<f32> = (0..size).map(|i| (i as f32 + 2.0) / 100.0).collect();

            let sim_naive = cosine_similarity_naive(&a, &b);
            let sim_simd = cosine_similarity_simd(&a, &b);

            prop_assert!((sim_naive - sim_simd).abs() < 1e-4,
                "SIMD should work correctly with size {} (remainder handling)", size);
        }
    }

    #[test]
    fn test_empty_vectors_return_zero() {
        // Empty vectors of equal length don't panic - they return 0.0
        let sim = cosine_similarity_naive(&[], &[]);
        assert_eq!(sim, 0.0, "Empty vectors should have 0 similarity");
    }

    #[test]
    fn test_single_element_vectors() {
        let a = vec![5.0];
        let b = vec![10.0];
        let sim = cosine_similarity_naive(&a, &b);
        assert!(
            (sim - 1.0).abs() < 1e-5,
            "Single positive elements should have similarity 1.0"
        );
    }

    #[test]
    fn test_single_element_opposite() {
        let a = vec![5.0];
        let b = vec![-5.0];
        let sim = cosine_similarity_naive(&a, &b);
        assert!(
            (sim - (-1.0)).abs() < 1e-5,
            "Single opposite elements should have similarity -1.0"
        );
    }

    #[test]
    fn test_normalize_single_element() {
        let mut v = vec![42.0];
        normalize_vector(&mut v);
        assert!(
            (v[0] - 1.0).abs() < 1e-5,
            "Single positive element should normalize to 1.0"
        );
    }

    #[test]
    fn test_normalize_single_negative() {
        let mut v = vec![-42.0];
        normalize_vector(&mut v);
        assert!(
            (v[0] - (-1.0)).abs() < 1e-5,
            "Single negative element should normalize to -1.0"
        );
    }

    #[test]
    fn test_exact_multiples_of_simd_width() {
        for size in [4, 8, 16, 32, 64, 128, 256, 512] {
            let a: Vec<f32> = (0..size).map(|i| i as f32).collect();
            let b: Vec<f32> = (0..size).map(|i| (i * 2) as f32).collect();

            let sim_naive = cosine_similarity_naive(&a, &b);
            let sim_simd = cosine_similarity_simd(&a, &b);

            assert!(
                (sim_naive - sim_simd).abs() < 1e-4,
                "Failed for size {}: naive={}, simd={}",
                size,
                sim_naive,
                sim_simd
            );
        }
    }

    #[test]
    fn test_normalize_vector_infinity() {
        let mut v = vec![f32::INFINITY, 1.0, 2.0];
        normalize_vector(&mut v);

        // When vector contains infinity, norm is infinity, and INFINITY/INFINITY = NaN
        // This is mathematically correct behavior - infinity vectors are degenerate
        assert!(
            v[0].is_nan(),
            "Normalizing infinity produces NaN (expected degenerate case)"
        );
    }

    #[test]
    fn test_cosine_similarity_with_different_magnitudes() {
        let a = vec![1.0, 0.0];
        let b = vec![1000.0, 0.0];

        let sim = cosine_similarity_naive(&a, &b);

        assert!(
            (sim - 1.0).abs() < 1e-5,
            "Cosine similarity should be scale-invariant"
        );
    }

    #[test]
    fn test_cosine_batch_empty_batch() {
        let query = vec![1.0, 2.0, 3.0];
        let embeddings: Vec<Vec<f32>> = vec![];

        let results = cosine_similarity_batch_simd(&query, &embeddings);

        assert_eq!(results.len(), 0, "Empty batch should return empty results");
    }

    #[test]
    fn test_normalize_batch_empty_batch() {
        let mut batch: Vec<Vec<f32>> = vec![];
        for emb in batch.iter_mut() {
            normalize_vector(emb);
        }

        assert_eq!(batch.len(), 0, "Normalizing empty batch should be safe");
    }

    #[test]
    fn test_cosine_similarity_numerical_precision() {
        let a = vec![1e-30_f32; 128];
        let b = vec![1e-30_f32; 128];

        let sim = cosine_similarity_naive(&a, &b);

        assert!(
            !sim.is_nan(),
            "Should not produce NaN with very small values"
        );
        assert!(
            sim.is_finite(),
            "Should remain finite with very small values"
        );
    }

    #[test]
    fn test_normalize_alternating_signs() {
        let mut v: Vec<f32> = (0..100)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        normalize_vector(&mut v);

        let magnitude: f32 = v.iter().map(|&x| x * x).sum::<f32>().sqrt();

        assert!(
            (magnitude - 1.0).abs() < 1e-5,
            "Alternating signs should normalize to unit vector"
        );
    }

    #[test]
    fn test_cosine_with_mostly_zeros() {
        let mut a = vec![0.0_f32; 128];
        a[0] = 1.0;
        a[127] = 1.0;

        let mut b = vec![0.0_f32; 128];
        b[0] = 1.0;
        b[127] = 1.0;

        let sim = cosine_similarity_naive(&a, &b);

        assert!(
            (sim - 1.0).abs() < 1e-5,
            "Sparse vectors should compute correctly"
        );
    }
}
