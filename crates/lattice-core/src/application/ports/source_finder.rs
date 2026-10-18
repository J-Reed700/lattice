//! One finder: a port adapter with a descriptor. The router picks finders by
//! their descriptors and calls each for one need and one page.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::domain::sourcing::{FinderDescriptor, FinderOutcome, SourceNeed};

/// What a call may spend: the caller's deadline, its Stop, and the lesson's
/// research allowance when there is one.
#[derive(Debug, Clone)]
pub struct Budget {
    /// The turn's or the job's deadline.
    pub deadline: Instant,
    /// The user's Stop.
    pub stop: CancellationToken,
    /// Provider requests left (the lesson research allowance per claim and
    /// fingerprint); `None` when the caller has no such allowance.
    pub requests: Option<u32>,
}

impl Budget {
    /// Time left before the deadline (zero once it has passed).
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    /// Stopped, past the deadline, or out of requests.
    pub fn is_spent(&self) -> bool {
        self.stop.is_cancelled() || self.remaining().is_zero() || self.requests == Some(0)
    }
}

/// A single source finder (DuckDuckGo, Wikidata, crates.io, …).
#[async_trait]
pub trait SourceFinder: Send + Sync {
    /// What this finder finds, costs, needs and sends where.
    fn descriptor(&self) -> &FinderDescriptor;

    /// One finder, one need, one page (1-based). The need arrives with the
    /// scope and query the router chose for this step. Never retries across
    /// providers (the router does that) and never returns an error: a
    /// failure is a `FinderResult::Failed` with its cause.
    async fn find(&self, need: &SourceNeed, page: usize, budget: &Budget) -> FinderOutcome;
}
