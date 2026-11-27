//! When a model request that failed on its way is sent again: one rule for
//! every backend. A backend makes each attempt and says how it failed; this
//! decides whether another attempt follows, and when.
use crate::features::llm::backend::RetrySink;
use lattice_core::shared::error::{AppError, Result};
use std::time::Duration;
use tokio::time::Instant;

/// Attempts in all, the first included.
pub(crate) const MAX_ATTEMPTS: usize = 5;

/// The longest wait before a resend, a server's `retry-after` included.
const LONGEST_WAIT: Duration = Duration::from_secs(30);

pub(crate) struct Resend<'a> {
    backend: &'a str,
    deadline: Option<Instant>,
    on_retry: Option<&'a RetrySink<'a>>,
    attempt: usize,
}

impl<'a> Resend<'a> {
    /// Attempts at one request to `backend`, all ending by `deadline`.
    /// `on_retry` hears of each resend before it goes; without it nothing is
    /// resent once part of the answer reached the caller.
    pub(crate) fn new(
        backend: &'a str,
        deadline: Option<Instant>,
        on_retry: Option<&'a RetrySink<'a>>,
    ) -> Self {
        Self {
            backend,
            deadline,
            on_retry,
            attempt: 1,
        }
    }

    /// The attempt being made, counted from 1.
    pub(crate) fn attempt(&self) -> usize {
        self.attempt
    }

    /// After the current attempt failed with `error`: waits and returns for
    /// the next attempt, or gives `error` back when none follows. `asked` is
    /// the wait the server asked for; `delivered` says part of the answer
    /// already reached the caller.
    pub(crate) async fn after(
        &mut self,
        error: AppError,
        asked: Option<Duration>,
        delivered: bool,
    ) -> Result<()> {
        if self.attempt >= MAX_ATTEMPTS || (delivered && self.on_retry.is_none()) {
            return Err(error);
        }
        let delay = wait(self.attempt, asked);
        // A wait the budget cannot pay for would be reported as an expiry,
        // hiding the cause the server already gave.
        if self
            .deadline
            .is_some_and(|deadline| deadline.saturating_duration_since(Instant::now()) <= delay)
        {
            return Err(error);
        }
        if let Some(reset) = self.on_retry {
            reset(self.attempt + 1)?;
        }
        tracing::warn!(
            backend = self.backend,
            attempt = self.attempt,
            max_attempts = MAX_ATTEMPTS,
            error = %error,
            "Retrying model request"
        );
        tokio::time::sleep(delay).await;
        self.attempt += 1;
        Ok(())
    }
}

/// Exponential backoff with half jitter, unless the server asked for a wait.
fn wait(attempt: usize, asked: Option<Duration>) -> Duration {
    let ceiling_ms = 500u64 << (attempt - 1);
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u64;
    asked
        .unwrap_or_else(|| Duration::from_millis(ceiling_ms / 2 + jitter % (ceiling_ms / 2 + 1)))
        .min(LONGEST_WAIT)
}
