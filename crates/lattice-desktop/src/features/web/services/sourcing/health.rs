//! Per-finder health (design §3.6): the core `Breaker` plus the counters the
//! health panel and the metrics snapshot read. The pacer stays per host; the
//! breaker is per finder. Owned by the sourcing service (and, for the archive
//! endpoints, by the Wayback client), never global.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::time::Duration;

use chrono::{DateTime, Utc};
use lattice_core::application::contracts::settings::SourcingSettingsDto;
use lattice_core::domain::sourcing::{
    Admission, Breaker, BreakerState, FailureCause, FinderDescriptor, FinderHealth, FinderId,
    FinderOutcome, FinderResult, HealthState, KeyService, SkipReason, SourcingPolicy,
};

pub(crate) struct HealthBook {
    policy: SourcingPolicy,
    entries: parking_lot::Mutex<HashMap<FinderId, Entry>>,
}

#[derive(Default)]
struct Entry {
    breaker: Breaker,
    calls: u64,
    answered: u64,
    empty: u64,
    failed: u64,
    skipped: u64,
    last_cause: Option<FailureCause>,
    /// When each recent call finished and how long it took, for the median.
    /// Kept for `search_page_ttl`: the same window a cached page is trusted
    /// for is the window a latency figure describes the finder as it is now.
    durations: VecDeque<(DateTime<Utc>, u64)>,
}

impl HealthBook {
    pub(crate) fn new(policy: SourcingPolicy) -> Self {
        Self {
            policy,
            entries: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    /// Asks the finder's breaker whether a call may go now (an admitted call
    /// counts against a documented allowance).
    pub(crate) fn admit(&self, descriptor: &FinderDescriptor, now: DateTime<Utc>) -> Admission {
        self.entries
            .lock()
            .entry(descriptor.id.clone())
            .or_default()
            .breaker
            .admit(now, &descriptor.rate)
    }

    /// Records a call's outcome in the breaker and the counters.
    pub(crate) fn record(
        &self,
        descriptor: &FinderDescriptor,
        outcome: &FinderOutcome,
        now: DateTime<Utc>,
    ) {
        let mut entries = self.entries.lock();
        let entry = entries.entry(descriptor.id.clone()).or_default();
        entry
            .breaker
            .record(&outcome.result, now, &descriptor.rate, &self.policy);
        match &outcome.result {
            FinderResult::Answered => entry.answered += 1,
            FinderResult::Empty => entry.empty += 1,
            FinderResult::Failed(cause) => {
                entry.failed += 1;
                entry.last_cause = Some(cause.clone());
            }
            FinderResult::Skipped(_) => {
                entry.skipped += 1;
                return;
            }
        }
        entry.calls += 1;
        let millis = u64::try_from(outcome.elapsed.as_millis()).unwrap_or(u64::MAX);
        entry.durations.push_back((now, millis));
        let window = self.policy.search_page_ttl;
        while entry
            .durations
            .front()
            .is_some_and(|(at, _)| (now - *at).to_std().is_ok_and(|age| age > window))
        {
            entry.durations.pop_front();
        }
    }

    /// Counts a finder that was not called.
    pub(crate) fn record_skip(&self, id: &FinderId) {
        self.entries.lock().entry(id.clone()).or_default().skipped += 1;
    }

    /// How long a caller with `remaining` time must wait before this finder
    /// may be called; `None` when its open window outlasts the caller.
    pub(super) fn wait_before(
        &self,
        id: &FinderId,
        now: DateTime<Utc>,
        remaining: Duration,
    ) -> Option<Duration> {
        self.entries
            .lock()
            .get(id)
            .map_or(Some(Duration::ZERO), |entry| {
                entry.breaker.wait_before(now, remaining)
            })
    }

    /// When the finder's open window ends, while it is open.
    pub(crate) fn next_allowed_at(&self, id: &FinderId) -> Option<DateTime<Utc>> {
        self.entries
            .lock()
            .get(id)
            .and_then(|entry| entry.breaker.next_allowed_at())
    }

    /// The finders a caller with `remaining` time cannot use: open breakers
    /// whose window outlasts the caller. A window that ends in time is not
    /// refused here; the runner waits it out if the call still needs leads.
    pub(super) fn refused(
        &self,
        now: DateTime<Utc>,
        remaining: Duration,
    ) -> BTreeMap<FinderId, SkipReason> {
        self.entries
            .lock()
            .iter()
            .filter_map(|(id, entry)| match entry.breaker.state() {
                BreakerState::Open { until, cause }
                    if entry.breaker.wait_before(now, remaining).is_none() =>
                {
                    Some((
                        id.clone(),
                        SkipReason::CircuitOpen {
                            until: *until,
                            last: Box::new(cause.clone()),
                        },
                    ))
                }
                _ => None,
            })
            .collect()
    }

    /// Every registered finder's health, with the settings' and keyring's
    /// reasons for the ones that cannot run.
    pub(super) fn snapshot(
        &self,
        descriptors: &[&FinderDescriptor],
        settings: &SourcingSettingsDto,
        keys: &BTreeSet<KeyService>,
    ) -> Vec<FinderHealth> {
        let entries = self.entries.lock();
        descriptors
            .iter()
            .map(|descriptor| {
                let entry = entries.get(&descriptor.id);
                let state = match settings.gate(descriptor, keys) {
                    Some(reason) => HealthState::Unavailable(reason),
                    None => {
                        entry.map_or(HealthState::Healthy, |entry| entry.breaker.health_state())
                    }
                };
                FinderHealth {
                    id: descriptor.id.clone(),
                    state,
                    calls: entry.map_or(0, |entry| entry.calls),
                    answered: entry.map_or(0, |entry| entry.answered),
                    empty: entry.map_or(0, |entry| entry.empty),
                    failed: entry.map_or(0, |entry| entry.failed),
                    skipped: entry.map_or(0, |entry| entry.skipped),
                    last_cause: entry.and_then(|entry| entry.last_cause.clone()),
                    next_allowed_at: entry.and_then(|entry| entry.breaker.next_allowed_at()),
                    median_ms: entry.and_then(|entry| median(&entry.durations)),
                }
            })
            .collect()
    }
}

fn median(durations: &VecDeque<(DateTime<Utc>, u64)>) -> Option<u64> {
    let mut millis: Vec<u64> = durations.iter().map(|(_, ms)| *ms).collect();
    millis.sort_unstable();
    millis.get(millis.len() / 2).copied()
}
