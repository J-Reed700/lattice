//! The turn's one deadline, shared by its model rounds, its tool calls and any
//! condensing in between.

use lattice_core::shared::error::{AppError, Result};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub(super) struct TurnClock {
    deadline: Instant,
    budget_minutes: u64,
}

impl TurnClock {
    /// A resumed turn has only what its saved rounds left of the budget.
    pub(super) fn new(time_budget: Duration, spent: Duration) -> Self {
        Self {
            deadline: Instant::now() + time_budget.saturating_sub(spent),
            budget_minutes: time_budget.as_secs().div_ceil(60),
        }
    }

    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Whether the deadline has passed. A call that failed after it failed at
    /// it, whatever its error says: a provider's own budget, or the clock's.
    pub(super) fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    pub(super) fn exhausted(&self) -> AppError {
        AppError::ServiceNotAvailable(format!(
            "Generation exceeded this turn's {}-minute time budget.",
            self.budget_minutes
        ))
    }

    /// What is left of the turn. Each bounded await gets this, never a fresh
    /// window, so retries and tool calls cannot stretch the turn past its budget.
    pub(super) fn remaining(&self) -> Result<Duration> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(self.exhausted());
        }
        Ok(remaining)
    }
}
