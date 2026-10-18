//! Per-finder health: a circuit breaker plus the counters the health panel
//! and the metrics snapshot show. The pacer stays per host; the breaker is
//! per finder.

use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};

use super::descriptor::{FinderId, RatePolicy};
use super::outcome::{FailureCause, FinderResult, SkipReason};
use super::policy::SourcingPolicy;

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

/// How a failure bears on the breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    /// The host said when to come back (`Retry-After`, a rate-limit reset):
    /// open at once, until then.
    Explicit { until: DateTime<Utc> },
    /// 403, 429 or a challenge page.
    Refusal,
    /// A transport failure or a 5xx.
    Transport,
    /// Says nothing about the host's willingness (a 404, an unparsable page,
    /// the caller's own budget): neither counts nor resets a streak.
    Neutral,
}

/// Classifies a failure. A `Retry-After` counts only for a finder whose host
/// honours it.
pub fn classify(cause: &FailureCause, rate: &RatePolicy, now: DateTime<Utc>) -> FailureClass {
    match cause {
        FailureCause::Status {
            retry_after: Some(wait),
            ..
        } if rate.honours_retry_after => FailureClass::Explicit {
            until: after(now, *wait),
        },
        FailureCause::RateLimited { until } => FailureClass::Explicit { until: *until },
        FailureCause::Status {
            code: 403 | 429, ..
        }
        | FailureCause::Challenge => FailureClass::Refusal,
        FailureCause::Status {
            code: 500..=599, ..
        }
        | FailureCause::Transport(_) => FailureClass::Transport,
        FailureCause::Status { .. } | FailureCause::Parse(_) | FailureCause::Budget => {
            FailureClass::Neutral
        }
    }
}

/// How long the breaker stays open: the host's own instruction when it gave
/// one, in full; otherwise the finder's `min_interval × 2^trips` (`trips`
/// counts this opening, so the first doubles the interval), held to the
/// policy's ceiling.
///
/// The caller's deadline does not shorten the window: it bounds how long the
/// caller waits (see [`Breaker::wait_before`]), not when the next caller may
/// call a host that asked for an hour.
pub fn open_for(
    class: FailureClass,
    now: DateTime<Utc>,
    rate: &RatePolicy,
    trips: u32,
    ceiling: Duration,
) -> Duration {
    if let FailureClass::Explicit { until } = class {
        return (until - now).to_std().unwrap_or(Duration::ZERO);
    }
    2u32.checked_pow(trips)
        .and_then(|factor| rate.min_interval.checked_mul(factor))
        .map_or(ceiling, |window| window.min(ceiling))
}

/// `now + wait`, saturating at the end of time.
fn after(now: DateTime<Utc>, wait: Duration) -> DateTime<Utc> {
    TimeDelta::from_std(wait)
        .ok()
        .and_then(|delta| now.checked_add_signed(delta))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

impl Breaker {
    /// Asks whether a call may go now. An elapsed open window turns
    /// half-open and admits one probe; a documented allowance that is spent
    /// refuses until its window ends. An admitted call counts against the
    /// allowance.
    pub fn admit(&mut self, now: DateTime<Utc>, rate: &RatePolicy) -> Admission {
        if let BreakerState::Open { until, cause } = &self.state {
            if now < *until {
                return Admission::Refuse(SkipReason::CircuitOpen {
                    until: *until,
                    last: Box::new(cause.clone()),
                });
            }
            self.state = BreakerState::HalfOpen {
                probing: false,
                since: *until,
                last: cause.clone(),
            };
        }
        if let BreakerState::HalfOpen {
            probing: true,
            last,
            ..
        } = &self.state
        {
            // A probe is in flight; its result decides.
            return Admission::Refuse(SkipReason::CircuitOpen {
                until: now,
                last: Box::new(last.clone()),
            });
        }
        if let Some(quota) = rate.quota {
            let window = match self.quota {
                Some(window) if now < after(window.started, quota.per) => window,
                _ => QuotaWindow {
                    started: now,
                    used: 0,
                },
            };
            if window.used >= quota.requests {
                self.quota = Some(window);
                return Admission::Refuse(SkipReason::QuotaSpent {
                    until: after(window.started, quota.per),
                });
            }
            self.quota = Some(QuotaWindow {
                used: window.used.saturating_add(1),
                ..window
            });
        }
        match &mut self.state {
            BreakerState::HalfOpen { probing, .. } => {
                *probing = true;
                Admission::Probe
            }
            _ => Admission::Go,
        }
    }

    /// Records a call's result.
    pub fn record(
        &mut self,
        result: &FinderResult,
        now: DateTime<Utc>,
        rate: &RatePolicy,
        policy: &SourcingPolicy,
    ) {
        let cause = match result {
            FinderResult::Answered | FinderResult::Empty => {
                self.state = BreakerState::Closed {
                    refusals: 0,
                    transport: 0,
                };
                self.trips = 0;
                return;
            }
            FinderResult::Skipped(_) => {
                self.end_probe();
                return;
            }
            FinderResult::Failed(cause) => cause,
        };
        let class = classify(cause, rate, now);
        match (&self.state, class) {
            (_, FailureClass::Neutral) => self.end_probe(),
            (BreakerState::Open { until, .. }, FailureClass::Explicit { until: later })
                if later > *until =>
            {
                self.state = BreakerState::Open {
                    until: later,
                    cause: cause.clone(),
                };
            }
            (BreakerState::Open { .. }, _) => {}
            (BreakerState::HalfOpen { .. }, _)
            | (BreakerState::Closed { .. }, FailureClass::Explicit { .. }) => {
                self.open(cause, class, now, rate, policy);
            }
            (BreakerState::Closed { refusals, .. }, FailureClass::Refusal) => {
                let refusals = refusals.saturating_add(1);
                if refusals >= policy.breaker_trip.refusals || rate.trip_on_first_refusal {
                    self.open(cause, class, now, rate, policy);
                } else {
                    self.state = BreakerState::Closed {
                        refusals,
                        transport: 0,
                    };
                }
            }
            (BreakerState::Closed { transport, .. }, FailureClass::Transport) => {
                let transport = transport.saturating_add(1);
                if transport >= policy.breaker_trip.transport_failures {
                    self.open(cause, class, now, rate, policy);
                } else {
                    self.state = BreakerState::Closed {
                        refusals: 0,
                        transport,
                    };
                }
            }
        }
    }

    /// How long a caller with `remaining` time must wait before this finder
    /// may be called: zero when it is not open, the rest of the open window
    /// when that fits before the caller's deadline, `None` when it does not
    /// (the caller skips it as `CircuitOpen`).
    pub fn wait_before(&self, now: DateTime<Utc>, remaining: Duration) -> Option<Duration> {
        match &self.state {
            BreakerState::Open { until, .. } if *until > now => {
                let wait = (*until - now).to_std().unwrap_or(Duration::ZERO);
                (wait <= remaining).then_some(wait)
            }
            _ => Some(Duration::ZERO),
        }
    }

    /// When the open window ends, while it is open.
    pub fn next_allowed_at(&self) -> Option<DateTime<Utc>> {
        match &self.state {
            BreakerState::Open { until, .. } => Some(*until),
            _ => None,
        }
    }

    /// The panel's summary of this breaker.
    pub fn health_state(&self) -> HealthState {
        match &self.state {
            BreakerState::Closed { .. } => HealthState::Healthy,
            BreakerState::Open { until, cause } => HealthState::Open {
                until: *until,
                cause: cause.clone(),
            },
            BreakerState::HalfOpen { .. } => HealthState::HalfOpen,
        }
    }

    fn open(
        &mut self,
        cause: &FailureCause,
        class: FailureClass,
        now: DateTime<Utc>,
        rate: &RatePolicy,
        policy: &SourcingPolicy,
    ) {
        self.trips = self.trips.saturating_add(1);
        let window = open_for(class, now, rate, self.trips, policy.breaker_ceiling);
        self.state = BreakerState::Open {
            until: after(now, window),
            cause: cause.clone(),
        };
    }

    /// A probe that ended without a verdict lets the next call probe.
    fn end_probe(&mut self) {
        if let BreakerState::HalfOpen { probing, .. } = &mut self.state {
            *probing = false;
        }
    }
}

#[cfg(test)]
#[path = "health_tests.rs"]
mod tests;
