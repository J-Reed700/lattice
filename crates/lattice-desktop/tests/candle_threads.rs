//! In a test binary of its own, because candle sizes this pool once, on first use.
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn candle_runs_its_elementwise_work_on_the_inference_threads() {
    lattice::features::transcription::engine::limit_candle_threads();
    let threads = AtomicUsize::new(0);
    candle_core::utils::barrier_pool().execute(|_| {
        threads.fetch_add(1, Ordering::Relaxed);
    });
    assert_eq!(threads.into_inner(), 4);
}
