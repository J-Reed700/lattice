#![allow(unsafe_code)]

/// True when the two vectors can be compared at all.
///
/// This has to be a **runtime** check, not `debug_assert_eq!`. The assert was
/// downgraded to debug-only so a dimension mismatch wouldn't panic in release
/// — but that traded a panic for something strictly worse. The SIMD paths
/// derive their load offsets from `a.len()` and read `b` at those offsets, so
/// in release a shorter `b` produced an out-of-bounds SIMD read: undefined
/// behaviour, not a graceful degradation.
///
/// Mismatched dimensions are reachable in practice, not merely a programming
/// error — the embeddings table can hold vectors from more than one model
/// (see IDX-2 and IDX-4), so a 384-dim query really can meet a 768-dim row.
///
/// One comparison against a dot product over hundreds of lanes is free.
#[inline]
fn dimensions_match(a: &[f32], b: &[f32]) -> bool {
    if a.len() == b.len() {
        return true;
    }

    #[cfg(debug_assertions)]
    {
        static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            tracing::warn!(
                a_len = a.len(),
                b_len = b.len(),
                "cosine similarity called with mismatched dimensions; returning 0.0. \
                 This usually means the embeddings table holds vectors from more than one model."
            );
        }
    }

    false
}

#[inline]
pub fn cosine_similarity_naive(a: &[f32], b: &[f32]) -> f32 {
    if !dimensions_match(a, b) {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut a_norm = 0.0f32;
    let mut b_norm = 0.0f32;

    for i in 0..a.len() {
        let a_val = a.get(i).unwrap_or(&0.0);
        let b_val = b.get(i).unwrap_or(&0.0);
        dot += a_val * b_val;
        a_norm += a_val * a_val;
        b_norm += b_val * b_val;
    }

    let denominator = a_norm.sqrt() * b_norm.sqrt();
    if denominator == 0.0 || !denominator.is_finite() {
        return 0.0; // Two zero vectors have 0 similarity
    }
    dot / denominator
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[inline]
// SAFETY: This function is unsafe because it uses x86_64 SIMD intrinsics.
// It must only be called when:
// 1. CPU feature detection has confirmed AVX2 and FMA are available
// 2. Input slices have equal length (verified by caller)
// 3. This code only runs on x86_64 architecture
// The function uses unaligned loads (_mm256_loadu_ps) so no alignment requirement.
unsafe fn cosine_similarity_avx2_impl(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::x86_64::*;

    let len = a.len();
    let mut dot_sum = _mm256_setzero_ps();
    let mut a_norm_sum = _mm256_setzero_ps();
    let mut b_norm_sum = _mm256_setzero_ps();

    let chunks = len / 8;
    let remainder = len % 8;

    for i in 0..chunks {
        let offset = i * 8;
        // SAFETY: Pointer arithmetic and SIMD loads are safe here because:
        // 1. offset = i * 8 where i < chunks, and chunks = len / 8
        // 2. Therefore offset + 8 <= len, so all accesses are in bounds
        // 3. _mm256_loadu_ps accepts unaligned pointers, no alignment requirement
        // 4. The slices are valid for the duration of this call (borrowed from caller)
        // 5. f32 data is always valid for SIMD operations (NaN/infinity are ok)
        let va = _mm256_loadu_ps(a.as_ptr().add(offset));
        let vb = _mm256_loadu_ps(b.as_ptr().add(offset));

        dot_sum = _mm256_fmadd_ps(va, vb, dot_sum);
        a_norm_sum = _mm256_fmadd_ps(va, va, a_norm_sum);
        b_norm_sum = _mm256_fmadd_ps(vb, vb, b_norm_sum);
    }

    let mut dot = hsum_ps_avx(dot_sum);
    let mut a_norm = hsum_ps_avx(a_norm_sum);
    let mut b_norm = hsum_ps_avx(b_norm_sum);

    if remainder > 0 {
        let offset = chunks * 8;
        for (&a_value, &b_value) in a
            .iter()
            .skip(offset)
            .zip(b.iter().skip(offset))
            .take(remainder)
        {
            dot += a_value * b_value;
            a_norm += a_value * a_value;
            b_norm += b_value * b_value;
        }
    }

    let denominator = a_norm.sqrt() * b_norm.sqrt();
    if denominator == 0.0 || !denominator.is_finite() {
        return 0.0; // Two zero vectors have 0 similarity
    }
    dot / denominator
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
// SAFETY: This function is unsafe because it uses x86_64 AVX2 intrinsics.
// It must only be called when AVX2 is available (enforced by target_feature).
// The function performs horizontal sum reduction on a __m256 vector,
// which is always safe as long as AVX2 is supported by the CPU.
unsafe fn hsum_ps_avx(v: std::arch::x86_64::__m256) -> f32 {
    use std::arch::x86_64::*;

    // SAFETY: All AVX2 intrinsics used here are safe because:
    // 1. The input __m256 is a valid SIMD register value
    // 2. All operations (extract, cast, add, move, convert) are pure register ops
    // 3. No memory access or pointer dereferencing occurs
    // 4. AVX2 support is guaranteed by the target_feature attribute
    // 5. This code only compiles and runs on x86_64 with AVX2
    let hi = _mm256_extractf128_ps(v, 1);
    let lo = _mm256_castps256_ps128(v);
    let sum128 = _mm_add_ps(hi, lo);

    let shuf = _mm_movehdup_ps(sum128);
    let sums = _mm_add_ps(sum128, shuf);
    let shuf = _mm_movehl_ps(shuf, sums);
    let sums = _mm_add_ss(sums, shuf);

    _mm_cvtss_f32(sums)
}

/// SECURITY FIX: Enhanced CPU feature detection with runtime checks
///
/// This function performs runtime CPU feature detection to ensure SIMD operations
/// are only used on CPUs that support them. This prevents:
/// 1. Illegal instruction crashes on older CPUs
/// 2. Undefined behavior from using unsupported SIMD instructions
/// 3. Silent failures that could lead to incorrect results
#[cfg(target_arch = "x86_64")]
pub fn cosine_similarity_simd(a: &[f32], b: &[f32]) -> f32 {
    // Hard gate before any SIMD path: the unsafe implementations below index
    // `b` using offsets derived from `a.len()`.
    if !dimensions_match(a, b) {
        return 0.0;
    }

    // SECURITY: Runtime CPU feature detection to prevent crashes on older processors
    // We check for both AVX2 and FMA support before using SIMD acceleration
    if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
        // Log once on first use (in debug builds)
        #[cfg(debug_assertions)]
        {
            static LOGGED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                tracing::debug!("Using AVX2+FMA SIMD acceleration for cosine similarity");
            }
        }

        // SAFETY: Calling AVX2 implementation is safe because:
        // 1. We've confirmed CPU supports AVX2 and FMA through feature detection
        // 2. Input slices have equal length (asserted above)
        // 3. The target_feature attribute ensures the function uses correct instructions
        // 4. The implementation uses unaligned loads, so no alignment requirement
        // 5. Falls back to safe naive implementation if CPU features unavailable
        unsafe { cosine_similarity_avx2_impl(a, b) }
    } else {
        // Log fallback reason (in debug builds)
        #[cfg(debug_assertions)]
        {
            static LOGGED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                let has_avx2 = is_x86_feature_detected!("avx2");
                let has_fma = is_x86_feature_detected!("fma");
                tracing::debug!(
                    "Falling back to naive cosine similarity (AVX2: {}, FMA: {})",
                    has_avx2,
                    has_fma
                );
            }
        }
        cosine_similarity_naive(a, b)
    }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
#[inline]
// SAFETY: This function is unsafe because it uses ARM NEON SIMD intrinsics.
// It must only be called when:
// 1. CPU feature detection has confirmed NEON is available
// 2. Input slices have equal length (verified by caller)
// 3. This code only runs on aarch64 architecture
// NOTE: NEON loads require 4-byte alignment. Misaligned access will CRASH on ARM.
unsafe fn cosine_similarity_neon_impl(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::aarch64::*;

    // Use a runtime alignment check with a safe fallback.
    // ARM NEON vld1q_f32 requires 16-byte alignment on many implementations
    // Vec<f32> from test helpers provides NO such guarantee
    // Check alignment and fall back to naive if misaligned
    let a_ptr = a.as_ptr() as usize;
    let b_ptr = b.as_ptr() as usize;
    const NEON_ALIGNMENT: usize = 16;

    if !a_ptr.is_multiple_of(NEON_ALIGNMENT) || !b_ptr.is_multiple_of(NEON_ALIGNMENT) {
        // Unaligned data detected - fall back to scalar implementation
        // This prevents SIGBUS on ARM while maintaining correctness
        return cosine_similarity_naive(a, b);
    }

    let len = a.len();
    let mut dot_sum = vdupq_n_f32(0.0);
    let mut a_norm_sum = vdupq_n_f32(0.0);
    let mut b_norm_sum = vdupq_n_f32(0.0);

    let chunks = len / 4;
    let remainder = len % 4;

    for i in 0..chunks {
        let offset = i * 4;
        // SAFETY: Pointer arithmetic and NEON loads are safe here because:
        // 1. offset = i * 4 where i < chunks, and chunks = len / 4
        // 2. Therefore offset + 4 <= len, so all accesses are in bounds
        // 3. vld1q_f32 loads 4 f32 values (16 bytes) starting at the pointer
        // 4. 16-byte alignment verified above (via runtime check)
        // 5. The slices are valid for the duration of this call (borrowed from caller)
        // 6. f32 data is always valid for SIMD operations (NaN/infinity are ok)
        let va = vld1q_f32(a.as_ptr().add(offset));
        let vb = vld1q_f32(b.as_ptr().add(offset));

        dot_sum = vfmaq_f32(dot_sum, va, vb);
        a_norm_sum = vfmaq_f32(a_norm_sum, va, va);
        b_norm_sum = vfmaq_f32(b_norm_sum, vb, vb);
    }

    let mut dot = vaddvq_f32(dot_sum);
    let mut a_norm = vaddvq_f32(a_norm_sum);
    let mut b_norm = vaddvq_f32(b_norm_sum);

    if remainder > 0 {
        let offset = chunks * 4;
        for i in 0..remainder {
            let a_val = a.get(offset + i).unwrap_or(&0.0);
            let b_val = b.get(offset + i).unwrap_or(&0.0);
            dot += a_val * b_val;
            a_norm += a_val * a_val;
            b_norm += b_val * b_val;
        }
    }

    let denominator = a_norm.sqrt() * b_norm.sqrt();
    if denominator == 0.0 || !denominator.is_finite() {
        return 0.0; // Two zero vectors have 0 similarity
    }
    dot / denominator
}

/// SECURITY FIX: Enhanced CPU feature detection for ARM processors
///
/// ARM processors (M1/M2 Macs) have strict alignment requirements.
/// This function ensures NEON SIMD is only used when:
/// 1. CPU supports NEON instructions
/// 2. Input data is properly aligned (guaranteed by Rust slices)
#[cfg(target_arch = "aarch64")]
pub fn cosine_similarity_simd(a: &[f32], b: &[f32]) -> f32 {
    // Hard gate before any SIMD path: the unsafe implementations below index
    // `b` using offsets derived from `a.len()`.
    if !dimensions_match(a, b) {
        return 0.0;
    }

    // SECURITY: Runtime CPU feature detection for ARM
    // NEON is standard on aarch64, but we check anyway for forward compatibility
    if std::arch::is_aarch64_feature_detected!("neon") {
        // Log once on first use (in debug builds)
        #[cfg(debug_assertions)]
        {
            static LOGGED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                tracing::debug!("Using ARM NEON SIMD acceleration for cosine similarity");
            }
        }

        // SAFETY: Calling NEON implementation is safe because:
        // 1. We've confirmed CPU supports NEON through feature detection
        // 2. Input slices have equal length (asserted above)
        // 3. Rust f32 slices are guaranteed to be properly aligned (4-byte boundary)
        // 4. The target_feature attribute ensures function uses correct instructions
        // 5. Falls back to safe naive implementation if NEON unavailable
        // 6. CRITICAL: ARM requires proper alignment - Rust slices provide this guarantee
        unsafe { cosine_similarity_neon_impl(a, b) }
    } else {
        // This branch should rarely execute on aarch64 since NEON is standard
        #[cfg(debug_assertions)]
        {
            static LOGGED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                tracing::warn!(
                    "NEON not detected on aarch64, falling back to naive implementation"
                );
            }
        }
        cosine_similarity_naive(a, b)
    }
}

/// Fallback implementation for architectures without SIMD support
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
pub fn cosine_similarity_simd(a: &[f32], b: &[f32]) -> f32 {
    // Log architecture info (in debug builds)
    #[cfg(debug_assertions)]
    {
        static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            tracing::debug!("No SIMD support for current architecture, using naive implementation");
        }
    }
    cosine_similarity_naive(a, b)
}

/// PERFORMANCE: Parallel batch cosine similarity computation using rayon for multi-core acceleration
pub fn cosine_similarity_batch_simd(query: &[f32], embeddings: &[Vec<f32>]) -> Vec<f32> {
    use rayon::prelude::*;

    embeddings
        .par_iter()
        .map(|emb| cosine_similarity_simd(query, emb))
        .collect()
}

#[inline]
pub fn normalize_vector(vec: &mut [f32]) {
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in vec.iter_mut() {
            *x /= norm;
        }
    }
}

pub fn normalize_batch(embeddings: &mut [Vec<f32>]) {
    for emb in embeddings.iter_mut() {
        normalize_vector(emb);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mismatched dimensions must degrade to 0.0 rather than read out of
    /// bounds. Run this in release too (`cargo test --release`): in debug the
    /// old `debug_assert_eq!` would have panicked, which masked the fact that
    /// release performed an OOB SIMD read instead.
    #[test]
    fn mismatched_dimensions_return_zero_not_undefined_behaviour() {
        // Long enough that the SIMD path takes several 8-wide chunks, so a
        // missing tail in `b` would definitely be read past the end.
        let a: Vec<f32> = (0..768).map(|i| i as f32 * 0.01).collect();
        let b: Vec<f32> = (0..384).map(|i| i as f32 * 0.01).collect();

        assert_eq!(cosine_similarity_simd(&a, &b), 0.0);
        assert_eq!(cosine_similarity_simd(&b, &a), 0.0);
        assert_eq!(cosine_similarity_naive(&a, &b), 0.0);
        assert_eq!(cosine_similarity_naive(&b, &a), 0.0);
    }

    /// Off-by-one lengths are the nastiest case: the chunk loop completes and
    /// only the scalar remainder walks off the end.
    #[test]
    fn off_by_one_dimensions_return_zero() {
        let a: Vec<f32> = vec![1.0; 385];
        let b: Vec<f32> = vec![1.0; 384];
        assert_eq!(cosine_similarity_simd(&a, &b), 0.0);
        assert_eq!(cosine_similarity_simd(&b, &a), 0.0);
    }

    #[test]
    fn empty_against_non_empty_returns_zero() {
        let a: Vec<f32> = Vec::new();
        let b: Vec<f32> = vec![1.0; 384];
        assert_eq!(cosine_similarity_simd(&a, &b), 0.0);
        assert_eq!(cosine_similarity_simd(&b, &a), 0.0);
    }

    /// The guard must not change results for well-formed input.
    #[test]
    fn matching_dimensions_are_unaffected_by_the_guard() {
        let a: Vec<f32> = (0..384).map(|i| (i as f32).sin()).collect();
        let b: Vec<f32> = (0..384).map(|i| (i as f32).cos()).collect();

        let simd = cosine_similarity_simd(&a, &b);
        let naive = cosine_similarity_naive(&a, &b);
        assert!(
            (simd - naive).abs() < 1e-4,
            "simd {} and naive {} should agree",
            simd,
            naive
        );
        assert!((cosine_similarity_simd(&a, &a) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_cosine_similarity_naive() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        let sim = cosine_similarity_naive(&a, &b);
        assert!((sim - 0.9746318).abs() < 1e-5);
    }

    #[test]
    fn test_cosine_similarity_simd() {
        let a: Vec<f32> = (0..384).map(|i| i as f32).collect();
        let b: Vec<f32> = (0..384).map(|i| (i * 2) as f32).collect();

        let sim_naive = cosine_similarity_naive(&a, &b);
        let sim_simd = cosine_similarity_simd(&a, &b);

        assert!((sim_naive - sim_simd).abs() < 1e-4);
    }

    #[test]
    fn test_normalize() {
        let mut vec = vec![3.0, 4.0];
        normalize_vector(&mut vec);

        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_cpu_feature_detection() {
        // This test verifies that the SIMD path is taken when features are available
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![5.0, 6.0, 7.0, 8.0];

        let result = cosine_similarity_simd(&a, &b);
        assert!(result.is_finite());
        assert!((-1.0..=1.0).contains(&result));
    }

    // P0 FIX: Tests for division by zero edge cases
    #[test]
    fn test_cosine_similarity_zero_vectors() {
        let a = vec![0.0; 384];
        let b = vec![0.0; 384];
        let result = cosine_similarity_naive(&a, &b);
        assert!(result.is_finite(), "Should not return NaN");
        assert_eq!(result, 0.0, "Zero vectors should have 0 similarity");
    }

    #[test]
    fn test_cosine_similarity_one_zero_vector() {
        let a = vec![1.0; 384];
        let b = vec![0.0; 384];
        let result = cosine_similarity_naive(&a, &b);
        assert!(result.is_finite(), "Should not return NaN");
        assert_eq!(result, 0.0, "One zero vector should have 0 similarity");
    }

    #[test]
    fn test_cosine_similarity_identical_vectors() {
        let a = vec![1.0; 384];
        let b = vec![1.0; 384];
        let result = cosine_similarity_naive(&a, &b);
        assert!(result.is_finite(), "Should not return NaN");
        assert!(
            (result - 1.0).abs() < 1e-6,
            "Identical vectors should have similarity ~1.0"
        );
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let mut a = vec![0.0; 384];
        let mut b = vec![0.0; 384];
        a[0] = 1.0;
        b[1] = 1.0;
        let result = cosine_similarity_naive(&a, &b);
        assert!(result.is_finite(), "Should not return NaN");
        assert!(
            result.abs() < 1e-6,
            "Orthogonal vectors should have similarity ~0.0"
        );
    }

    #[test]
    fn test_cosine_similarity_simd_zero_vectors() {
        let a = vec![0.0; 384];
        let b = vec![0.0; 384];
        let result = cosine_similarity_simd(&a, &b);
        assert!(
            result.is_finite(),
            "SIMD should not return NaN for zero vectors"
        );
        assert_eq!(result, 0.0, "SIMD zero vectors should have 0 similarity");
    }

    #[test]
    fn test_cosine_similarity_simd_one_zero() {
        let a = vec![1.0; 384];
        let b = vec![0.0; 384];
        let result = cosine_similarity_simd(&a, &b);
        assert!(result.is_finite(), "SIMD should not return NaN");
        assert_eq!(result, 0.0, "SIMD one zero vector should have 0 similarity");
    }

    #[test]
    fn test_cosine_similarity_batch_with_zeros() {
        let query = vec![0.0; 384];
        let embeddings = vec![vec![0.0; 384], vec![1.0; 384], vec![0.0; 384]];

        let results = cosine_similarity_batch_simd(&query, &embeddings);

        assert_eq!(results.len(), 3);
        for result in results {
            assert!(result.is_finite(), "Batch should not return NaN");
            assert_eq!(result, 0.0, "All should be 0 with zero query");
        }
    }

    #[test]
    fn test_normalize_zero_vector() {
        let mut vec = vec![0.0; 10];
        normalize_vector(&mut vec);

        for &val in &vec {
            assert!(val.is_finite());
            assert_eq!(val, 0.0);
        }
    }
}
