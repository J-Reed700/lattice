//! Per-finder health: a circuit breaker plus the counters the health panel
//! and the metrics snapshot show. The pacer stays per host; the breaker is
//! per finder.

use chrono::{DateTime, Utc};

use super::descriptor::FinderId;
use super::outcome::{FailureCause, SkipReason};

/// The breaker's state machine: `Closed` counts consecutive failures, `Open`
/// refuses calls until a time, `HalfOpen` lets one probe through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BreakerState {
    Closed {
        /// Consecutive refusals (403, 429, challenge).
        refusals: u32,
        /// Consecutive transport failures (including 5xx).
        transport: u32,
    },
    Open {
        until: DateTime<Utc>,
        cause: FailureCause,
    },
    HalfOpen {
        /// A probe is in flight; further calls wait for its result.
        probing: bool,
        /// When the open window ended.
        since: DateTime<Utc>,
        last: FailureCause,
    },
}

/// A finder's circuit breaker and the allowance window of its quota.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breaker {
    state: BreakerState,
    /// Consecutive openings without a success between them; the derived
    /// backoff doubles with each.
    trips: u32,
    quota: Option<QuotaWindow>,
}

/// The fixed window a documented allowance is counted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaWindow {
    pub started: DateTime<Utc>,
    pub used: u32,
}

/// What the breaker says about a call about to be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    Go,
    /// The one call allowed while half-open.
    Probe,
    Refuse(SkipReason),
}

/// The health panel's summary state for a finder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthState {
    Healthy,
    Open {
        until: DateTime<Utc>,
        cause: FailureCause,
    },
    HalfOpen,
    /// Not configured or turned off (no key, no instance, no contact).
    Unavailable(SkipReason),
}

/// One finder's health, for the Settings → Sources panel and
/// `MetricsPort::snapshot`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinderHealth {
    pub id: FinderId,
    pub state: HealthState,
    pub calls: u64,
    pub answered: u64,
    pub empty: u64,
    pub failed: u64,
    pub skipped: u64,
    pub last_cause: Option<FailureCause>,
    pub next_allowed_at: Option<DateTime<Utc>>,
    pub median_ms: Option<u64>,
}

impl Default for Breaker {
    fn default() -> Self {
        Self::new()
    }
}

impl Breaker {
    /// A closed breaker with no history.
    pub fn new() -> Self {
        Self {
            state: BreakerState::Closed {
                refusals: 0,
                transport: 0,
            },
            trips: 0,
            quota: None,
        }
    }

    pub fn state(&self) -> &BreakerState {
        &self.state
    }

    /// Consecutive openings without a success between them.
    pub fn trips(&self) -> u32 {
        self.trips
    }
}
