//! Runs a plan (design §3.4, §3.6): waves of finder steps, pages within a
//! step, the provider-page cache, a wait for a host's `Retry-After`, and
//! transport retries. Every bound comes from the caller's budget, the
//! finder's descriptor, its breaker, or the need's `want`.
//!
//! - **Pages.** A step's first page always runs (from the cache when it
//!   holds it). A further page is fetched only while the fused pool is short
//!   of `want`, the step's last page was full (a short page means the
//!   service has no more), and that page brought something new; one finder
//!   is never asked for more pages than it would need to fill `want` alone.
//! - **Refusals.** A challenge, 403 or 429 ends the finder's part in this
//!   call: it is never asked again within it (pressing on against a
//!   challenge is what gets an address blocked), and its breaker counts it.
//! - **A host's instruction.** A `Retry-After` from a finder that honours it
//!   opens the breaker for exactly that long. The call waits it out, once,
//!   when it still needs leads and the wait ends before the deadline;
//!   otherwise the outcome names the instruction and later calls skip the
//!   finder until then.
//! - **Transport failures** are retried after the finder's own
//!   `min_interval` while its breaker stays closed and the deadline allows:
//!   the breaker's trip count is the retry bound.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use futures::StreamExt;
use lattice_core::application::ports::Budget;
use lattice_core::application::services::sourcing::{cache_policy, Plan, StepScope};
use lattice_core::domain::sourcing::{
    canonical, classify, Admission, FailureCause, FailureClass, FinderId, FinderOutcome,
    FinderResult, Lead, SkipReason, SourceNeed, SourcingPolicy,
};
use tracing::info;

use super::cache::ProviderPageCache;
use super::health::HealthBook;
use crate::features::web::services::finders::FinderEntry;

/// Wall time and waiting, injectable so a 60-second `Retry-After` can be
/// tested without waiting 60 seconds.
pub(crate) trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
    fn sleep(&self, wait: Duration) -> BoxFuture<'static, ()>;
}

/// The real clock.
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn sleep(&self, wait: Duration) -> BoxFuture<'static, ()> {
        Box::pin(tokio::time::sleep(wait))
    }
}

/// One finder searching one query: what its pages returned so far.
pub(super) struct StepRun {
    pub entry: FinderEntry,
    /// The wave the step ran in (0-based), for fusion's tie-break.
    pub wave: usize,
    /// The need as this step runs it: its query and scope.
    pub need: SourceNeed,
    pub leads: Vec<Lead>,
    /// Pages that answered.
    pub pages: usize,
    /// The last page was full and brought something new, so the service may
    /// hold more.
    pub more: bool,
}

/// What running a plan produced.
#[derive(Default)]
pub(super) struct RunReport {
    pub steps: Vec<StepRun>,
    /// One outcome per page call or skip, in the order they settled.
    pub outcomes: Vec<FinderOutcome>,
    /// The caller's rewordings that ran, in order.
    pub alternatives_run: Vec<String>,
    /// Every planned wave ran and no step may hold more.
    pub exhausted: bool,
}

/// One page's result, and whether the finder is held by an open window that
/// ends before the caller's deadline (so waiting for it is possible).
struct PageRun {
    outcome: FinderOutcome,
    held: bool,
}

impl PageRun {
    fn done(outcome: FinderOutcome) -> Self {
        Self {
            outcome,
            held: false,
        }
    }
}

pub(super) struct Runner<'a> {
    registry: &'a [FinderEntry],
    health: &'a HealthBook,
    cache: &'a ProviderPageCache,
    clock: &'a dyn Clock,
    policy: SourcingPolicy,
    need: &'a SourceNeed,
    budget: &'a Budget,
    /// Provider requests left under the caller's allowance, if it has one.
    requests: parking_lot::Mutex<Option<u32>>,
    /// Finders that refused this call, with the refusal.
    refused: parking_lot::Mutex<HashMap<FinderId, FailureCause>>,
}

impl<'a> Runner<'a> {
    pub(super) fn new(
        registry: &'a [FinderEntry],
        health: &'a HealthBook,
        cache: &'a ProviderPageCache,
        clock: &'a dyn Clock,
        policy: SourcingPolicy,
        need: &'a SourceNeed,
        budget: &'a Budget,
    ) -> Self {
        Self {
            registry,
            health,
            cache,
            clock,
            policy,
            need,
            budget,
            requests: parking_lot::Mutex::new(budget.requests),
            refused: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    pub(super) async fn run(&self, plan: &Plan) -> RunReport {
        let want = self.need.want.max(1);
        let mut report = RunReport::default();
        let mut fused = 0;
        let mut last_wave_new = 0;
        let mut ran_every_wave = true;
        for (index, wave) in plan.waves.iter().enumerate() {
            let later_steps = plan.waves[index..]
                .iter()
                .any(|wave| !wave.steps.is_empty());
            if index > 0 && !self.policy.wants_another_wave(fused, last_wave_new, want) {
                ran_every_wave = !later_steps;
                break;
            }
            for (finder, reason) in &wave.skipped {
                self.settle(
                    &mut report,
                    None,
                    FinderOutcome::skipped(finder.clone(), reason.clone()),
                );
            }
            if self.budget.is_spent() {
                for step in &wave.steps {
                    let skipped =
                        FinderOutcome::skipped(step.finder.clone(), SkipReason::OutOfBudget);
                    self.settle(&mut report, None, skipped);
                }
                ran_every_wave = !later_steps;
                break;
            }
            let before = fused;
            let queries = if wave.reformulated && !self.need.alternatives.is_empty() {
                self.need.alternatives.clone()
            } else {
                vec![self.need.query.clone()]
            };
            for query in queries {
                if wave.reformulated && fused >= want {
                    break;
                }
                let first = report.steps.len();
                for step in &wave.steps {
                    let Some(entry) = self.entry(&step.finder) else {
                        continue;
                    };
                    match self.step_need(step.scope, &query) {
                        Some(need) => report.steps.push(StepRun {
                            entry: entry.clone(),
                            wave: index,
                            need,
                            leads: Vec::new(),
                            pages: 0,
                            more: false,
                        }),
                        None => {
                            let skipped = FinderOutcome::skipped(
                                step.finder.clone(),
                                SkipReason::NotApplicable,
                            );
                            self.settle(&mut report, None, skipped);
                        }
                    }
                }
                if wave.reformulated {
                    report.alternatives_run.push(query);
                }
                let range = first..report.steps.len();
                self.first_pages(&mut report, range.clone(), want, &mut fused)
                    .await;
                self.deepen(&mut report, range, want, &mut fused).await;
            }
            last_wave_new = fused.saturating_sub(before);
        }
        report.exhausted = ran_every_wave && !report.steps.iter().any(|step| step.more);
        report
    }

    fn entry(&self, id: &FinderId) -> Option<&FinderEntry> {
        self.registry
            .iter()
            .find(|entry| &entry.descriptor().id == id)
    }

    /// The need as a step runs it. An owner-scoped step needs an owner; in
    /// Wave 0 that is the need's own scope (owner resolution is F1's).
    fn step_need(&self, scope: StepScope, query: &str) -> Option<SourceNeed> {
        let mut need = self.need.clone();
        need.query = query.to_string();
        match scope {
            StepScope::AsAsked => Some(need),
            StepScope::Owner => need.scope.is_some().then_some(need),
        }
    }

    /// Every step's first page, concurrently up to the wave's concurrency,
    /// then a wait for any finder a host asked to hold, while still short.
    async fn first_pages(
        &self,
        report: &mut RunReport,
        range: Range<usize>,
        want: usize,
        fused: &mut usize,
    ) {
        let destinations = report.steps[range.clone()]
            .iter()
            .map(|step| step.entry.descriptor().sends_to)
            .collect::<HashSet<_>>()
            .len();
        let concurrency = self.policy.wave_concurrency(destinations);
        // Boxed one by one rather than mapped by a closure: a closure over
        // the borrowed steps makes the future's `Send` depend on a
        // higher-ranked lifetime the compiler cannot prove.
        let mut pending: Vec<BoxFuture<'_, PageRun>> = Vec::with_capacity(range.len());
        for step in &report.steps[range.clone()] {
            pending.push(Box::pin(self.run_page(&step.entry, &step.need, 1)));
        }
        let runs: Vec<PageRun> = futures::stream::iter(pending)
            .buffered(concurrency)
            .collect()
            .await;
        let mut held = Vec::new();
        for (offset, run) in runs.into_iter().enumerate() {
            let index = range.start + offset;
            if run.held {
                held.push((index, run.outcome));
            } else {
                self.settle(report, Some(index), run.outcome);
            }
        }
        *fused = self.fused_len(&report.steps);
        for (index, outcome) in held {
            let outcome = if *fused < want {
                let step = &report.steps[index];
                self.after_wait(&step.entry, &step.need, 1, outcome).await
            } else {
                outcome
            };
            self.settle(report, Some(index), outcome);
            *fused = self.fused_len(&report.steps);
        }
    }

    /// Further pages, one at a time, from the step with the fewest pages
    /// that may hold more, while the pool is short of `want`.
    async fn deepen(
        &self,
        report: &mut RunReport,
        range: Range<usize>,
        want: usize,
        fused: &mut usize,
    ) {
        while *fused < want && !self.budget.is_spent() {
            let next = range
                .clone()
                .filter(|&index| {
                    let step = &report.steps[index];
                    step.more && step.pages < pages_to_fill(want, step.entry.page_size)
                })
                .min_by_key(|&index| (report.steps[index].pages, index));
            let Some(index) = next else {
                break;
            };
            let step = &report.steps[index];
            let page = step.pages + 1;
            let run = self.run_page(&step.entry, &step.need, page).await;
            let outcome = if run.held {
                self.after_wait(&step.entry, &step.need, page, run.outcome)
                    .await
            } else {
                run.outcome
            };
            let before = *fused;
            self.settle(report, Some(index), outcome);
            *fused = self.fused_len(&report.steps);
            if *fused == before {
                report.steps[index].more = false;
            }
        }
    }

    /// Wait out a host's window once, when it ends before the deadline, and
    /// ask again. If the host refuses again, that is the outcome.
    async fn after_wait(
        &self,
        entry: &FinderEntry,
        need: &SourceNeed,
        page: usize,
        outcome: FinderOutcome,
    ) -> FinderOutcome {
        let id = &entry.descriptor().id;
        let Some(wait) = self
            .health
            .wait_before(id, self.clock.now(), self.budget.remaining())
        else {
            return outcome;
        };
        if !self.pause(wait).await {
            return outcome;
        }
        let mut retried = self.run_page(entry, need, page).await.outcome;
        retried.elapsed = retried.elapsed.saturating_add(outcome.elapsed);
        retried
    }

    /// One page from one finder: from the cache, or through the breaker.
    async fn run_page(&self, entry: &FinderEntry, need: &SourceNeed, page: usize) -> PageRun {
        let descriptor = entry.descriptor();
        let id = &descriptor.id;
        let key =
            cache_policy::provider_page_key(id, self.need, &need.query, need.scope.as_ref(), page);
        if let Some(leads) = self.cache.get(&key, self.need.freshness, self.clock.now()) {
            return PageRun::done(FinderOutcome {
                finder: id.clone(),
                leads,
                result: FinderResult::Answered,
                elapsed: Duration::ZERO,
            });
        }
        if let Some(cause) = self.refused.lock().get(id).cloned() {
            let until = self
                .health
                .next_allowed_at(id)
                .unwrap_or_else(|| self.clock.now());
            let reason = SkipReason::CircuitOpen {
                until,
                last: Box::new(cause),
            };
            return PageRun::done(FinderOutcome::skipped(id.clone(), reason));
        }
        let mut last: Option<FinderOutcome> = None;
        loop {
            if self.budget.is_spent() || *self.requests.lock() == Some(0) {
                let spent = FinderOutcome::skipped(id.clone(), SkipReason::OutOfBudget);
                return PageRun::done(last.unwrap_or(spent));
            }
            if let Admission::Refuse(reason) = self.health.admit(descriptor, self.clock.now()) {
                // A retry the breaker ended reports the failure that ended it.
                if let Some(failure) = last {
                    return PageRun::done(failure);
                }
                return PageRun {
                    outcome: FinderOutcome::skipped(id.clone(), reason),
                    held: self.window_fits(id),
                };
            }
            if let Some(left) = self.requests.lock().as_mut() {
                *left = left.saturating_sub(1);
            }
            let mut outcome = self.call(entry, need, page).await;
            if let Some(previous) = &last {
                outcome.elapsed = outcome.elapsed.saturating_add(previous.elapsed);
            }
            let now = self.clock.now();
            self.health.record(descriptor, &outcome, now);
            let cause = match &outcome.result {
                FinderResult::Answered => {
                    self.cache.store(key, outcome.leads.clone(), now);
                    return PageRun::done(outcome);
                }
                FinderResult::Failed(cause) => cause.clone(),
                FinderResult::Empty | FinderResult::Skipped(_) => return PageRun::done(outcome),
            };
            match classify(&cause, &descriptor.rate, now) {
                FailureClass::Explicit { .. } => {
                    let held = self.window_fits(id);
                    return PageRun { outcome, held };
                }
                FailureClass::Refusal => {
                    self.refused.lock().insert(id.clone(), cause);
                    return PageRun::done(outcome);
                }
                FailureClass::Neutral => return PageRun::done(outcome),
                FailureClass::Transport => {
                    // The failure that trips the breaker is the last try.
                    let wait = descriptor.rate.min_interval;
                    if self.health.next_allowed_at(id).is_some() || wait >= self.budget.remaining()
                    {
                        return PageRun::done(outcome);
                    }
                    last = Some(outcome);
                    if !self.pause(wait).await {
                        return PageRun::done(last.unwrap_or_else(|| {
                            FinderOutcome::skipped(id.clone(), SkipReason::OutOfBudget)
                        }));
                    }
                }
            }
        }
    }

    /// The finder's own call, cut short by Stop or the deadline.
    async fn call(&self, entry: &FinderEntry, need: &SourceNeed, page: usize) -> FinderOutcome {
        let id = entry.descriptor().id.clone();
        let out_of_time = || FinderOutcome {
            finder: id.clone(),
            leads: Vec::new(),
            result: FinderResult::Failed(FailureCause::Budget),
            elapsed: Duration::ZERO,
        };
        tokio::select! {
            finished = tokio::time::timeout(
                self.budget.remaining(),
                entry.finder.find(need, page, self.budget),
            ) => finished.unwrap_or_else(|_| out_of_time()),
            () = self.budget.stop.cancelled() => out_of_time(),
        }
    }

    /// Sleep on the clock unless Stop comes first; false when stopped.
    async fn pause(&self, wait: Duration) -> bool {
        if wait.is_zero() {
            return !self.budget.stop.is_cancelled();
        }
        tokio::select! {
            () = self.clock.sleep(wait) => true,
            () = self.budget.stop.cancelled() => false,
        }
    }

    /// The finder is held by an open window that ends before the deadline.
    fn window_fits(&self, id: &FinderId) -> bool {
        self.health
            .wait_before(id, self.clock.now(), self.budget.remaining())
            .is_some_and(|wait| !wait.is_zero())
    }

    /// Record a settled page or skip: the step's leads and paging state, the
    /// outcome, the skip counter and the per-finder log line (§3.6).
    fn settle(&self, report: &mut RunReport, step: Option<usize>, outcome: FinderOutcome) {
        if let Some(step) = step.and_then(|index| report.steps.get_mut(index)) {
            match outcome.result {
                FinderResult::Answered => {
                    step.pages += 1;
                    step.more = outcome.leads.len() >= step.entry.page_size;
                    step.leads.extend(outcome.leads.iter().cloned());
                }
                _ => step.more = false,
            }
        }
        if matches!(outcome.result, FinderResult::Skipped(_)) {
            self.health.record_skip(&outcome.finder);
        }
        info!(
            finder = %outcome.finder,
            kind = self.need.kind.as_str(),
            result = outcome.result.as_str(),
            leads = outcome.leads.len(),
            ms = u64::try_from(outcome.elapsed.as_millis()).unwrap_or(u64::MAX),
            cause = outcome.result.detail().unwrap_or_default(),
            "Source finder outcome"
        );
        report.outcomes.push(outcome);
    }

    /// Distinct in-scope leads across every step: the pool's size.
    fn fused_len(&self, steps: &[StepRun]) -> usize {
        steps
            .iter()
            .flat_map(|step| step.leads.iter())
            .filter(|lead| {
                self.need
                    .scope
                    .as_ref()
                    .map_or(true, |scope| scope.covers(&lead.url))
            })
            .map(canonical::lead_key)
            .collect::<HashSet<_>>()
            .len()
    }
}

/// The pages one finder alone needs to fill `want`: past that, the pool is
/// short because the finders agree, and another page of the same engine
/// rarely changes that.
fn pages_to_fill(want: usize, page_size: usize) -> usize {
    want.div_ceil(page_size.max(1)).max(1)
}
