//! Tests for the download manager, grouped by the behaviour they exercise.
//!
//! Shared fixtures live here; each submodule owns one slice of the surface.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use tokio::time::Duration;

mod concurrency;
mod failure_modes;
mod resume_offset;
mod session_ops;
mod transfer;

/// Bounded database polling for asynchronous tests.
///
/// Uses a deadline so a failed condition cannot wait indefinitely.
async fn poll_until<F, T>(mut check_fn: F, timeout_duration: Duration) -> Result<T, String>
where
    F: FnMut() -> Pin<Box<dyn Future<Output = Option<T>> + Send>>,
{
    let deadline = tokio::time::Instant::now() + timeout_duration;

    loop {
        if let Some(result) = check_fn().await {
            return Ok(result);
        }

        if tokio::time::Instant::now() >= deadline {
            return Err("Timeout waiting for condition".to_string());
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn temp_root() -> PathBuf {
    std::env::temp_dir().join("lattice-download-tests")
}

fn temp_file(file_name: &str) -> PathBuf {
    temp_root().join(file_name)
}
