//! How many requests one cloud provider takes at once, learned from its own
//! answers the way TCP learns a path: one request until the provider has
//! answered, a doubling of the window per round of acceptances (slow start),
//! then one more per window of acceptances (additive increase), and a halving
//! when the provider refuses for load. Pure: it holds no clock, and every
//! rule that needs the time is given it.

use std::time::Duration;
use tokio::time::Instant;

/// The window of requests a provider is known to take, and when it may be
/// sent to again.
#[derive(Debug, Clone)]
pub(crate) struct CongestionWindow {
    window: usize,
    /// The window a throttle halved to; below it the window doubles per round,
    /// from it on it grows by one per round. `None` until the first throttle.
    threshold: Option<usize>,
    /// Acceptances since the window last grew, in additive increase.
    accepted_since_growth: usize,
    /// Admission is closed until then.
    open_at: Option<Instant>,
    /// Throttles since the provider last accepted a request.
    consecutive_throttles: u32,
}

impl CongestionWindow {
    pub(crate) fn new() -> Self {
        Self {
            window: 1,
            threshold: None,
            accepted_since_growth: 0,
            open_at: None,
            consecutive_throttles: 0,
        }
    }

    /// Requests allowed in flight.
    pub(crate) fn limit(&self) -> usize {
        self.window
    }

    /// The provider accepted a request. The window grows only when requests
    /// were held back by it (`demand_exceeds_window`): a window that was not
    /// filled has not been shown to be too small.
    pub(crate) fn accepted(&mut self, demand_exceeds_window: bool) {
        self.consecutive_throttles = 0;
        if !demand_exceeds_window {
            return;
        }
        match self.threshold {
            Some(threshold) if self.window >= threshold => {
                self.accepted_since_growth += 1;
                if self.accepted_since_growth >= self.window {
                    self.window += 1;
                    self.accepted_since_growth = 0;
                }
            }
            _ => self.window += 1,
        }
    }

    /// The provider refused a request for load. Returns when admission
    /// reopens: after the provider's `retry_after`, but never sooner than the
    /// backoff that consecutive refusals have earned (1, 2, 4 s, ...), so a
    /// provider that keeps answering `retry-after: 0` is not resent in a loop.
    /// Throttles that arrive while admission is closed answer requests that
    /// were already in flight together, so they halve the window once and
    /// only move the reopening later.
    pub(crate) fn throttled(&mut self, now: Instant, retry_after: Option<Duration>) -> Instant {
        if let Some(open_at) = self.open_at.filter(|open_at| now < *open_at) {
            let later = retry_after
                .and_then(|wait| now.checked_add(wait))
                .map_or(open_at, |reopen| reopen.max(open_at));
            self.open_at = Some(later);
            return later;
        }
        self.window = (self.window / 2).max(1);
        self.threshold = Some(self.window);
        self.accepted_since_growth = 0;
        let backoff = Duration::from_secs(
            1u64.checked_shl(self.consecutive_throttles)
                .unwrap_or(u64::MAX),
        );
        let wait = retry_after.map_or(backoff, |asked| asked.max(backoff));
        self.consecutive_throttles = self.consecutive_throttles.saturating_add(1);
        let reopen = now
            .checked_add(wait)
            .or_else(|| now.checked_add(backoff))
            .unwrap_or(now);
        self.open_at = Some(reopen);
        reopen
    }

    /// When admission reopens, while it is closed.
    pub(crate) fn open_at(&self) -> Option<Instant> {
        self.open_at
    }

    pub(crate) fn is_open(&self, now: Instant) -> bool {
        self.open_at.is_none_or(|open_at| now >= open_at)
    }
}

#[cfg(test)]
mod tests;
