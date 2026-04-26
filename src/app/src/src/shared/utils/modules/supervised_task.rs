//! Supervised tokio task spawning.
//!
//! `tokio::spawn` lets a panicking task die silently — the JoinHandle
//! resolves to `Err(JoinError)` but if you don't await the handle,
//! the panic is invisible. For long-running background loops (sagas,
//! event bridges, watchers) that's the difference between "background
//! work stops with no telemetry" and "we know what happened".
//!
//! `supervise` wraps a future-producing closure so that:
//! - Panics inside the loop are caught, logged with the task name, and
//!   the task is restarted after exponential backoff (capped).
//! - Clean exit (the future returns) is logged and the task NOT
//!   restarted — this is normal shutdown.
//! - The supervisor itself runs forever as a tokio task.
//!
//! Backoff: 1s → 2s → 4s → 8s → 16s → 30s (cap), reset to 1s after a
//! restart that survives 60s without panicking.

use futures::FutureExt;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const RESET_HEALTHY_DURATION: Duration = Duration::from_secs(60);

/// Spawn a supervised tokio task that restarts on panic.
///
/// `name` is used for log lines; pick something searchable.
///
/// `make_future` is a closure that produces the task's future. It is
/// called once per restart, so the closure must be idempotent and
/// must capture all the state the task needs.
///
/// Returns a `JoinHandle` for the supervisor task itself. Dropping the
/// handle does NOT stop the task; that's a feature for fire-and-forget
/// background workers. Use `JoinHandle::abort()` to stop it.
pub fn supervise<F, Fut>(name: &'static str, mut make_future: F) -> JoinHandle<()>
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let mut backoff = INITIAL_BACKOFF;
        loop {
            let started_at = Instant::now();

            // AssertUnwindSafe: the future may capture mutable state but
            // we don't observe it after a panic — we're about to restart
            // from scratch by calling make_future() again.
            let result = AssertUnwindSafe(make_future()).catch_unwind().await;

            let ran_for = started_at.elapsed();

            match result {
                Ok(()) => {
                    // Normal completion — saga shut down cleanly, e.g.
                    // because the event bus closed. Don't restart.
                    info!(
                        task = name,
                        ran_for_secs = ran_for.as_secs(),
                        "Supervised task exited normally; supervisor stopping"
                    );
                    return;
                }
                Err(panic_payload) => {
                    let panic_msg = panic_payload
                        .downcast_ref::<&'static str>()
                        .map(|s| (*s).to_string())
                        .or_else(|| panic_payload.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "<non-string panic payload>".to_string());

                    // Reset backoff if the task ran long enough to count
                    // as 'healthy' before the panic.
                    if ran_for >= RESET_HEALTHY_DURATION {
                        backoff = INITIAL_BACKOFF;
                    }

                    error!(
                        task = name,
                        ran_for_secs = ran_for.as_secs(),
                        backoff_secs = backoff.as_secs(),
                        panic = %panic_msg,
                        "Supervised task panicked; restarting after backoff"
                    );

                    tokio::time::sleep(backoff).await;

                    // Exponential backoff up to MAX_BACKOFF.
                    backoff = (backoff * 2).min(MAX_BACKOFF);

                    warn!(task = name, "Restarting supervised task");
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// We use real wall-time waits in these tests rather than tokio's
    /// virtual time (`start_paused`) because the supervisor's
    /// `tokio::time::sleep(backoff)` is meant to test that a real
    /// backoff is applied — virtual time would skip the wait
    /// instantaneously. Tests cap the total wait at a small multiple
    /// of the initial backoff so they stay fast.
    ///
    /// We override INITIAL_BACKOFF behavior by writing tests that
    /// only need 1-2 restarts; total cost is ~3s.

    #[tokio::test]
    async fn does_not_restart_after_clean_exit() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let handle = supervise("test_clean_exit", move || {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            async {}
        });

        // Supervisor returns naturally on clean exit; await it.
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn restarts_on_panic_then_stops_on_clean_exit() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let handle = supervise("test_panic_then_exit", move || {
            let n = attempts_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                if n < 1 {
                    // First attempt panics.
                    panic!("intentional test panic");
                }
                // Second attempt exits cleanly.
            }
        });

        // First run panics → 1s backoff → second run clean-exits.
        // Allow generous slack for slow CI; total ~2-3s.
        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;

        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }
}
