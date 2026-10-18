//! The `SourceFinding` port over the registered finders: plan with the core
//! router, run, fuse, and report every finder's outcome (design §3.3, §3.6).

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::application::contracts::settings::SourcingSettingsDto;
use lattice_core::application::ports::{Budget, SourceFinding};
use lattice_core::application::services::sourcing::{plan, FinderGate};
use lattice_core::domain::sourcing::{
    fusion, FinderDescriptor, FinderHealth, FinderId, FinderOutcome, FinderResult, Findings,
    KeyService, Lead, Near, OwnerResolution, SourceNeed, SourcingPolicy, SubjectHint,
};
use lattice_core::shared::error::{AppError, Result};
use url::Url;

use super::cache::ProviderPageCache;
use super::health::HealthBook;
use super::runner::{Clock, Runner, SystemClock};
use crate::features::web::services::finders::FinderEntry;

/// What one search found, plus the rewordings it actually ran (the chat
/// tool's output lists them).
pub(crate) struct Search {
    pub findings: Findings,
    pub alternatives_run: Vec<String>,
}

/// Routed, fused, budgeted source finding over the registered finders.
/// One per process (held by the web service), so every caller shares the
/// finders' health and the provider-page cache.
pub(crate) struct SourcingService {
    registry: Vec<FinderEntry>,
    settings: SourcingSettingsDto,
    keys: BTreeSet<KeyService>,
    policy: SourcingPolicy,
    health: HealthBook,
    cache: ProviderPageCache,
    clock: Arc<dyn Clock>,
}

impl SourcingService {
    /// The service over `registry`, gated by the user's settings and the
    /// services that have a key in the keyring.
    pub(crate) fn new(
        registry: Vec<FinderEntry>,
        settings: SourcingSettingsDto,
        keys: BTreeSet<KeyService>,
    ) -> Self {
        Self::with_clock(registry, settings, keys, Arc::new(SystemClock))
    }

    pub(crate) fn with_clock(
        registry: Vec<FinderEntry>,
        settings: SourcingSettingsDto,
        keys: BTreeSet<KeyService>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let policy =
            SourcingPolicy::default().with_finder_concurrency(settings.finder_concurrency());
        Self {
            registry,
            settings,
            keys,
            policy,
            health: HealthBook::new(policy),
            cache: ProviderPageCache::new(policy),
            clock,
        }
    }

    /// Finds sources for `need`, among the finders in `only` when the caller
    /// names some (the chat tool's `providers`). Errs only when nothing was
    /// found and every finder that applied failed or was skipped, naming
    /// each one's cause.
    pub(crate) async fn find_with(
        &self,
        need: SourceNeed,
        budget: Budget,
        only: Option<&[FinderId]>,
    ) -> Result<Search> {
        let entries: Vec<FinderEntry> = self
            .registry
            .iter()
            .filter(|entry| only.is_none_or(|ids| ids.contains(&entry.descriptor().id)))
            .cloned()
            .collect();
        let descriptors: Vec<FinderDescriptor> = entries
            .iter()
            .map(|entry| entry.descriptor().clone())
            .collect();
        let refused = self.health.refused(self.clock.now(), budget.remaining());
        let gate = FinderGate {
            settings: &self.settings,
            keys: &self.keys,
            refused: &refused,
        };
        let plan = plan(&need, &descriptors, &gate);
        let runner = Runner::new(
            &entries,
            &self.health,
            &self.cache,
            self.clock.as_ref(),
            self.policy,
            &need,
            &budget,
        );
        let report = runner.run(&plan).await;

        let lists: Vec<fusion::RankedList<'_>> = report
            .steps
            .iter()
            .map(|step| fusion::RankedList {
                finder: &step.entry.descriptor().id,
                wave: step.wave,
                leads: &step.leads,
            })
            .collect();
        let mut leads = fusion::diversify(fusion::rrf(&lists, self.policy.rrf_k, None), None);
        if let Some(scope) = &need.scope {
            leads.retain(|lead| scope.covers(&lead.url));
        }
        let outcomes = per_finder(report.outcomes);
        if leads.is_empty() && outcomes.iter().all(|outcome| !answered_or_empty(outcome)) {
            return Err(AppError::Network(nothing_answered(&outcomes)));
        }
        Ok(Search {
            findings: Findings {
                leads,
                outcomes,
                exhausted: report.exhausted,
            },
            alternatives_run: report.alternatives_run,
        })
    }
}

fn answered_or_empty(outcome: &FinderOutcome) -> bool {
    matches!(outcome.result, FinderResult::Answered | FinderResult::Empty)
}

/// Every finder's cause, one per line, for the all-failed error.
fn nothing_answered(outcomes: &[FinderOutcome]) -> String {
    if outcomes.is_empty() {
        return "No source finder applies to this search".to_string();
    }
    let lines: Vec<String> = outcomes
        .iter()
        .map(|outcome| match outcome.result.detail() {
            Some(detail) => format!("{}: {} ({detail})", outcome.finder, outcome.result.as_str()),
            None => format!("{}: {}", outcome.finder, outcome.result.as_str()),
        })
        .collect();
    format!(
        "Every search provider failed or was skipped:\n{}",
        lines.join("\n")
    )
}

/// One outcome per finder, in the order finders first settled: its leads
/// across pages and queries, its time, and its result. A failure or skip is
/// never hidden by a page that answered (the latest problem is the one
/// named); otherwise a finder that answered anywhere answered.
fn per_finder(outcomes: Vec<FinderOutcome>) -> Vec<FinderOutcome> {
    let mut order: Vec<FinderId> = Vec::new();
    let mut merged: HashMap<FinderId, FinderOutcome> = HashMap::new();
    for outcome in outcomes {
        match merged.get_mut(&outcome.finder) {
            None => {
                order.push(outcome.finder.clone());
                merged.insert(outcome.finder.clone(), outcome);
            }
            Some(so_far) => {
                so_far.leads.extend(outcome.leads);
                so_far.elapsed = so_far.elapsed.saturating_add(outcome.elapsed);
                so_far.result = merge(&so_far.result, outcome.result);
            }
        }
    }
    order
        .into_iter()
        .filter_map(|finder| merged.remove(&finder))
        .collect()
}

fn merge(so_far: &FinderResult, next: FinderResult) -> FinderResult {
    let is_problem = |result: &FinderResult| {
        matches!(result, FinderResult::Failed(_) | FinderResult::Skipped(_))
    };
    if is_problem(&next) {
        return next;
    }
    if is_problem(so_far) {
        return so_far.clone();
    }
    if matches!(so_far, FinderResult::Answered) {
        return FinderResult::Answered;
    }
    next
}

#[async_trait]
impl SourceFinding for SourcingService {
    async fn find(&self, need: SourceNeed, budget: Budget) -> Result<Findings> {
        self.find_with(need, budget, None)
            .await
            .map(|search| search.findings)
    }

    /// No finder resolves owners yet: Wikidata and the registries arrive in
    /// Wave 1 (F1). Said, rather than answered with an empty resolution.
    async fn resolve_owner(
        &self,
        subject: &SubjectHint,
        _budget: Budget,
    ) -> Result<OwnerResolution> {
        Err(AppError::NotFound(format!(
            "No owner finder is registered yet, so the owner of \"{}\" cannot be resolved",
            subject.name
        )))
    }

    /// Following a page's links arrives with the chat tools (T1).
    async fn leads_from_page(
        &self,
        url: &Url,
        _near: Option<Near>,
        _budget: Budget,
    ) -> Result<Vec<Lead>> {
        Err(AppError::NotFound(format!(
            "Following the links of {url} is not available yet"
        )))
    }

    fn health(&self) -> Vec<FinderHealth> {
        let descriptors: Vec<&FinderDescriptor> =
            self.registry.iter().map(FinderEntry::descriptor).collect();
        self.health
            .snapshot(&descriptors, &self.settings, &self.keys)
    }
}
