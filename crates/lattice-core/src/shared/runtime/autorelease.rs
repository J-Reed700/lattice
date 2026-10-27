//! Objective-C autorelease pool scoping for Candle's Metal backend. Every
//! Metal dispatch autoreleases objects, and on a thread that never pops a
//! pool (any tokio worker or blocking thread) they accumulate until the
//! thread exits, holding heap and GPU buffers. Wrapping each inference step
//! in its own pool releases them when the step returns.

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
