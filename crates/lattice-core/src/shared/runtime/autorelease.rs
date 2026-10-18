//! Objective-C autorelease pool scoping for Candle's Metal backend.
//!
//! Every Metal dispatch autoreleases objects: the command buffer, its compute
//! context, the `MTLArchitecture` and name string `candle-metal-kernels`
//! queries per kernel. On a thread that never pops an autorelease pool (any
//! tokio worker or blocking thread) the runtime parks them on an implicit pool
//! that is drained only when the thread exits, so they accumulate for the life
//! of the process. Measured on a 16k-chunk import: 10 million strings, 350k
//! command buffers, 1.5 GB of heap, plus the GPU buffers those command buffers
//! still referenced. Wrapping each inference step in its own pool releases all
//! of it when the step returns.

/// Run `f` inside a fresh autorelease pool. Everything autoreleased by `f`
/// is released when it returns.
#[cfg(target_os = "macos")]
pub fn with_autorelease_pool<T>(f: impl FnOnce() -> T) -> T {
    objc::rc::autoreleasepool(f)
}

/// Autorelease pools are a macOS runtime concept; elsewhere this is `f()`.
#[cfg(not(target_os = "macos"))]
pub fn with_autorelease_pool<T>(f: impl FnOnce() -> T) -> T {
    f()
}
