//! What a finder did with a need: answered, came back empty, failed with a
//! cause, or was skipped with a reason. Nothing degrades silently.

use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};

use super::descriptor::{FinderId, InstanceSetting, KeyService};
use super::lead::Lead;

/// One finder's result for one need and page.
#[derive(Debug, Clone, PartialEq)]
pub struct FinderOutcome {
    pub finder: FinderId,
    pub leads: Vec<Lead>,
    pub result: FinderResult,
    pub elapsed: Duration,
}

impl FinderOutcome {
    /// An outcome for a finder that never ran.
    pub fn skipped(finder: FinderId, reason: SkipReason) -> Self {
        Self {
            finder,
            leads: Vec::new(),
            result: FinderResult::Skipped(reason),
            elapsed: Duration::ZERO,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FinderResult {
    Answered,
    Empty,
    Failed(FailureCause),
    Skipped(SkipReason),
}

impl FinderResult {
    /// The wire name (`answered`, `empty`, `failed`, `skipped`).
    pub fn as_str(&self) -> &'static str {
        match self {
            FinderResult::Answered => "answered",
            FinderResult::Empty => "empty",
            FinderResult::Failed(_) => "failed",
            FinderResult::Skipped(_) => "skipped",
        }
    }

    /// The cause or reason, when there is one, as the user reads it.
    pub fn detail(&self) -> Option<String> {
        match self {
            FinderResult::Answered | FinderResult::Empty => None,
            FinderResult::Failed(cause) => Some(cause.to_string()),
            FinderResult::Skipped(reason) => Some(reason.to_string()),
        }
    }
}

/// Why a call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureCause {
    /// A non-success HTTP status, with the host's `Retry-After` when given.
    Status {
        code: u16,
        retry_after: Option<Duration>,
    },
    /// A bot challenge page (Turnstile, an "anomaly" page).
    Challenge,
    Transport(String),
    Parse(String),
    /// The host said when to come back.
    RateLimited {
        until: DateTime<Utc>,
    },
    /// The caller's budget ran out mid-call.
    Budget,
}

impl fmt::Display for FailureCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FailureCause::Status {
                code,
                retry_after: Some(wait),
            } => write!(f, "HTTP {code}, Retry-After {} s", wait.as_secs()),
            FailureCause::Status {
                code,
                retry_after: None,
            } => write!(f, "HTTP {code}"),
            FailureCause::Challenge => f.write_str("bot challenge"),
            FailureCause::Transport(detail) => write!(f, "transport: {detail}"),
            FailureCause::Parse(detail) => write!(f, "unreadable response: {detail}"),
            FailureCause::RateLimited { until } => {
                write!(f, "rate-limited until {}", until.format("%H:%M:%S UTC"))
            }
            FailureCause::Budget => f.write_str("out of time"),
        }
    }
}

/// Why a finder did not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The user turned it off.
    Disabled,
    NoKey {
        service: KeyService,
    },
    NoInstance {
        setting: InstanceSetting,
    },
    NoContact,
    CircuitOpen {
        until: DateTime<Utc>,
        last: Box<FailureCause>,
    },
    /// The service's documented allowance is spent until `until`.
    QuotaSpent {
        until: DateTime<Utc>,
    },
    NotApplicable,
    OutOfBudget,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SkipReason::Disabled => f.write_str("turned off"),
            SkipReason::NoKey { service } => write!(f, "no {} key", service.keyring_id()),
            SkipReason::NoInstance { .. } => f.write_str("no instance URL set"),
            SkipReason::NoContact => f.write_str("needs a contact"),
            SkipReason::CircuitOpen { until, last } => {
                write!(f, "paused until {} ({last})", until.format("%H:%M:%S UTC"))
            }
            SkipReason::QuotaSpent { until } => {
                write!(f, "allowance spent until {}", until.format("%H:%M:%S UTC"))
            }
            SkipReason::NotApplicable => f.write_str("not applicable"),
            SkipReason::OutOfBudget => f.write_str("out of budget"),
        }
    }
}

/// What a need found: fused leads and every finder's outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct Findings {
    /// Fused and deduplicated, best first.
    pub leads: Vec<Lead>,
    pub outcomes: Vec<FinderOutcome>,
    /// Every applicable finder answered, came back empty, failed or was
    /// skipped: there is nothing further to try for this need.
    pub exhausted: bool,
}
