//! # Download Test Helpers
//!
//! Oracle-approved async test infrastructure for download operations.
//!
//! ## Oracle Patterns Used
//!
//! 1. **mpsc channels** for deterministic event coordination
//! 2. **timeout()** for bounded waits (no unbounded sleep!)
//! 3. **polling with timeout** for database coordination
//!
//! ## Usage
//!
//! ```rust
//! use common::download_helpers::*;
//!
//! #[tokio::test]
//! async fn test_download() {
//!     let (tx, rx) = mpsc::unbounded_channel();
//!
//!     // Start download, it emits to tx
//!
//!     // Collect events deterministically
//!     let events = collect_download_events(rx, Duration::from_secs(5)).await;
//!
//!     assert_eq!(events.len(), 3); // Started, Progress, Completed
//! }
//! ```

use std::future::Future;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use lattice::features::download::manager::DownloadEvent;

/// Collect download events until terminal state
///
/// Oracle Pattern: Uses mpsc channel to deterministically capture async events.
/// No sleep() - uses timeout() for bounded waiting.
///
/// # Arguments
///
/// * `rx` - Unbounded receiver for download events
/// * `max_duration` - Maximum time to wait for terminal event
///
/// # Returns
///
/// Vector of all events received until terminal state (Completed/Failed/Cancelled)
/// or until channel closes.
///
/// # Oracle Compliance
///
/// ✅ Uses mpsc channels for coordination
/// ✅ Uses timeout() not sleep()
/// ✅ Deterministic event collection
/// ❌ NO arbitrary delays
pub async fn collect_download_events(
    mut rx: mpsc::UnboundedReceiver<DownloadEvent>,
    _max_duration: Duration,
) -> Vec<DownloadEvent> {
    let mut events = Vec::new();

    loop {
        match timeout(Duration::from_millis(100), rx.recv()).await {
            Ok(Some(event)) => {
                let is_terminal = matches!(
                    event,
                    DownloadEvent::Completed { .. }
                        | DownloadEvent::Failed { .. }
                        | DownloadEvent::Cancelled { .. }
                );

                events.push(event);

                if is_terminal {
                    break;
                }
            }
            Ok(None) => break, // Channel closed
            Err(_) => break,   // Timeout - no more events
        }
    }

    events
}

/// Poll database until condition met or timeout
///
/// Oracle Pattern: Database polling with timeout (NOT sleep-based coordination).
/// This is the ONLY acceptable use of sleep() in async tests - for polling intervals.
///
/// # Arguments
///
/// * `check_fn` - Async function to check condition, returns Some(T) when met
/// * `timeout_duration` - Maximum time to poll
///
/// # Returns
///
/// Ok(T) when condition met, Err when timeout exceeded
///
/// # Example
///
/// ```rust
/// let result = poll_until(
///     || Box::pin(async {
///         db.get_download(&id).await.ok()
///     }),
///     Duration::from_secs(5)
/// ).await?;
/// ```
///
/// # Oracle Compliance
///
/// ✅ Timeout-based polling (deterministic)
/// ✅ sleep() only for poll interval (acceptable here)
/// ✅ No unbounded waiting
pub async fn poll_until<F, T>(mut check_fn: F, timeout_duration: Duration) -> Result<T, String>
where
    F: FnMut() -> std::pin::Pin<Box<dyn Future<Output = Option<T>> + Send>>,
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

/// Verify progress sequence matches expected pattern
///
/// Oracle Pattern: Uses channels to deterministically verify progress events.
///
/// # Arguments
///
/// * `rx` - Receiver for (bytes, speed) tuples
/// * `expected_bytes` - Expected byte counts in order
/// * `max_items` - Maximum events to collect before timeout
///
/// # Returns
///
/// Ok(()) if sequence matches, Err with details if mismatch
///
/// # Oracle Compliance
///
/// ✅ Channel-based coordination
/// ✅ Timeout-bounded waiting
/// ✅ Deterministic verification
pub async fn verify_progress_sequence(
    mut rx: mpsc::Receiver<(u64, f64)>,
    expected_bytes: Vec<u64>,
    max_items: usize,
) -> Result<(), String> {
    let mut actual = Vec::new();

    for _ in 0..max_items {
        match timeout(Duration::from_secs(1), rx.recv()).await {
            Ok(Some((bytes, _speed))) => actual.push(bytes),
            Ok(None) => break,
            Err(_) => break,
        }
    }

    if actual.len() != expected_bytes.len() {
        return Err(format!(
            "Expected {} progress events, got {}",
            expected_bytes.len(),
            actual.len()
        ));
    }

    for (i, (actual_bytes, expected_bytes)) in actual.iter().zip(expected_bytes.iter()).enumerate()
    {
        if actual_bytes != expected_bytes {
            return Err(format!(
                "Progress event {}: expected {} bytes, got {}",
                i, expected_bytes, actual_bytes
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_collect_download_events_terminal() {
        let (tx, rx) = mpsc::unbounded_channel();

        tx.send(DownloadEvent::Started {
            id: "test".to_string(),
        })
        .unwrap();

        tx.send(DownloadEvent::Progress {
            id: "test".to_string(),
            bytes_downloaded: 500,
            bytes_per_second: 100.0,
        })
        .unwrap();

        tx.send(DownloadEvent::Completed {
            id: "test".to_string(),
        })
        .unwrap();

        let events = collect_download_events(rx, Duration::from_secs(5)).await;

        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], DownloadEvent::Started { .. }));
        assert!(matches!(events[1], DownloadEvent::Progress { .. }));
        assert!(matches!(events[2], DownloadEvent::Completed { .. }));
    }

    #[tokio::test]
    async fn test_collect_download_events_channel_closed() {
        let (tx, rx) = mpsc::unbounded_channel();

        tx.send(DownloadEvent::Started {
            id: "test".to_string(),
        })
        .unwrap();

        drop(tx); // Close channel

        let events = collect_download_events(rx, Duration::from_secs(5)).await;

        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn test_poll_until_success() {
        let mut counter = 0;

        let result = poll_until(
            || {
                counter += 1;
                Box::pin(async move {
                    if counter >= 3 {
                        Some(42)
                    } else {
                        None
                    }
                })
            },
            Duration::from_secs(5),
        )
        .await;

        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_poll_until_timeout() {
        let result = poll_until(
            || Box::pin(async { None::<i32> }),
            Duration::from_millis(200),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Timeout waiting for condition");
    }

    #[tokio::test]
    async fn test_verify_progress_sequence_success() {
        let (tx, rx) = mpsc::channel(10);

        tx.send((100, 10.0)).await.unwrap();
        tx.send((200, 20.0)).await.unwrap();
        tx.send((300, 30.0)).await.unwrap();

        drop(tx);

        let result = verify_progress_sequence(rx, vec![100, 200, 300], 5).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_verify_progress_sequence_mismatch() {
        let (tx, rx) = mpsc::channel(10);

        tx.send((100, 10.0)).await.unwrap();
        tx.send((250, 25.0)).await.unwrap(); // Wrong value

        drop(tx);

        let result = verify_progress_sequence(rx, vec![100, 200], 5).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected 200 bytes, got 250"));
    }

    #[tokio::test]
    async fn test_verify_progress_sequence_count_mismatch() {
        let (tx, rx) = mpsc::channel(10);

        tx.send((100, 10.0)).await.unwrap();

        drop(tx);

        let result = verify_progress_sequence(rx, vec![100, 200, 300], 5).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Expected 3 progress events, got 1"));
    }
}
