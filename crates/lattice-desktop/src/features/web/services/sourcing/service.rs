//! The `SourceFinding` port over the registered finders: plan with the core
//! router, run, fuse, and report every finder's outcome (design §3.3, §3.6).

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::application::contracts::settings::SourcingSettingsDto;
use lattice_core::application::ports::{Budget, SourceFinding};
use lattice_core::application::services::sourcing::{plan, FinderGate};
use lattice_core::domain::provenance::{
    citation_candidates, markdown_links, LinkStanding, OutboundLink,
};
use lattice_core::domain::sourcing::{
    fusion, FinderDescriptor, FinderHealth, FinderId, FinderOutcome, FinderResult, Findings,
    KeyService, Lead, LeadKind, LinkPlace, Near, Origin, OwnerResolution, SourceNeed,
    SourcingPolicy, SubjectHint,
};
use lattice_core::shared::error::{AppError, Result};
use url::Url;

use super::cache::ProviderPageCache;
use super::health::HealthBook;
use super::owner::OwnerResolver;
use super::runner::{Clock, Runner, SystemClock};
use crate::features::web::services::finders::{Fetched, FinderEntry, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::reference_text;
use crate::features::web::services::web::MAX_PAGE_BODY_BYTES;

/// What one search found, plus the rewordings it actually ran (the chat
/// tool's output lists them) and the owner resolution if one ran.
pub(crate) struct Search {
    pub findings: Findings,
    pub alternatives_run: Vec<String>,
    pub owner_resolution: Option<OwnerResolution>,
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
    owner: Option<Arc<OwnerResolver>>,
    http: Option<FinderHttp>,
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
            owner: None,
            http: None,
        }
    }

    pub(crate) fn with_owner_and_http(
        mut self,
        owner: Arc<OwnerResolver>,
        http: FinderHttp,
    ) -> Self {
        self.owner = Some(owner);
        self.http = Some(http);
        self
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
        let mut owner_resolution: Option<OwnerResolution> = None;
        let mut initial_owner: Option<Origin> = need.scope.clone();
        if initial_owner.is_none() && plan.resolve_owner {
            if let (Some(resolver), Some(subject)) = (&self.owner, &need.subject) {
                let res = resolver.resolve(subject, &budget).await;
                initial_owner = res.origins.first().cloned();
                owner_resolution = Some(res);
            }
        }
        let runner = Runner::new(
            &entries,
            &self.health,
            &self.cache,
            self.clock.as_ref(),
            self.policy,
            &need,
            &budget,
            initial_owner,
        );
        let mut report = runner.run(&plan).await;
        if let Some(ref res) = owner_resolution {
            let mut combined = res.outcomes.clone();
            combined.extend(report.outcomes);
            report.outcomes = combined;
        }

        let lists: Vec<fusion::RankedList<'_>> = report
            .steps
            .iter()
            .map(|step| fusion::RankedList {
                finder: &step.entry.descriptor().id,
                wave: step.wave,
                leads: &step.leads,
            })
            .collect();
        let owner_ref = report.owner.as_ref();
        let mut leads =
            fusion::diversify(fusion::rrf(&lists, self.policy.rrf_k, owner_ref), owner_ref);
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
            owner_resolution,
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

/// Extracts the leads (`LeadKind::ReferenceOf`) from already-extracted
/// reference markdown of `page_url`, optionally ordered around `near`.
pub(crate) fn leads_from_markdown(
    page_url: &Url,
    markdown: &str,
    near: Option<&Near>,
) -> Vec<Lead> {
    let all_links = markdown_links(markdown);
    let ordered: Vec<OutboundLink> = match near {
        Some(Near::Bytes { start, end }) => {
            citation_candidates(markdown, (*start, *end), &all_links, |_| {
                LinkStanding::Unknown
            })
            .into_iter()
            .map(|c| OutboundLink {
                url: c.url,
                anchor: c.anchor,
                start_byte: c.start_byte,
                end_byte: c.end_byte,
            })
            .collect()
        }
        Some(Near::Quote(quote)) => {
            let trimmed = quote.trim();
            let span = if trimmed.is_empty() {
                None
            } else {
                markdown
                    .find(trimmed)
                    .map(|idx| (idx, idx + trimmed.len()))
                    .or_else(|| {
                        let lower_md = markdown.to_lowercase();
                        let lower_q = trimmed.to_lowercase();
                        lower_md
                            .find(&lower_q)
                            .map(|idx| (idx, (idx + lower_q.len()).min(markdown.len())))
                    })
            };
            match span {
                Some(span) => {
                    citation_candidates(markdown, span, &all_links, |_| LinkStanding::Unknown)
                        .into_iter()
                        .map(|c| OutboundLink {
                            url: c.url,
                            anchor: c.anchor,
                            start_byte: c.start_byte,
                            end_byte: c.end_byte,
                        })
                        .collect()
                }
                None => all_links,
            }
        }
        None => all_links,
    };

    let finder_id = FinderId::from_static("page_links");
    let mut leads = Vec::with_capacity(ordered.len());
    for (idx, link) in ordered.into_iter().enumerate() {
        let Ok(target) = Url::parse(&link.url) else {
            continue;
        };
        let context = sentence_around(markdown, link.start_byte, link.end_byte);
        let rank = idx + 1;
        leads.push(Lead {
            url: target,
            title: (!link.anchor.is_empty()).then(|| link.anchor.clone()),
            snippet: Some(context.clone()),
            published: None,
            kind: LeadKind::ReferenceOf {
                page: page_url.clone(),
                place: Some(LinkPlace {
                    anchor: link.anchor,
                    context,
                    offset: link.start_byte,
                }),
            },
            owner: None,
            rel_canonical: None,
            terms: None,
            found_by: vec![(finder_id.clone(), rank)],
        });
    }
    leads
}

fn sentence_around(text: &str, start: usize, end: usize) -> String {
    let start = start.min(text.len());
    let end = end.min(text.len());
    let line_start = text
        .get(..start)
        .and_then(|before| before.rfind('\n'))
        .map_or(0, |pos| pos + 1);
    let line_end = text
        .get(end..)
        .and_then(|after| after.find('\n'))
        .map_or(text.len(), |pos| end + pos);
    text.get(line_start..line_end)
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[async_trait]
impl SourceFinding for SourcingService {
    async fn find(&self, need: SourceNeed, budget: Budget) -> Result<Findings> {
        self.find_with(need, budget, None)
            .await
            .map(|search| search.findings)
    }

    async fn resolve_owner(
        &self,
        subject: &SubjectHint,
        budget: Budget,
    ) -> Result<OwnerResolution> {
        let Some(resolver) = &self.owner else {
            return Err(AppError::NotFound(format!(
                "No owner finder is registered yet, so the owner of \"{}\" cannot be resolved",
                subject.name
            )));
        };
        Ok(resolver.resolve(subject, &budget).await)
    }

    async fn leads_from_page(
        &self,
        url: &Url,
        near: Option<Near>,
        _budget: Budget,
    ) -> Result<Vec<Lead>> {
        let Some(http) = &self.http else {
            return Err(AppError::NotFound(format!(
                "Following the links of {url} is not available yet"
            )));
        };
        let headers = identity::browser_headers(identity::random_profile(), None);
        let html = match http.get(url.as_str(), headers, MAX_PAGE_BODY_BYTES).await {
            Fetched::Body(body) => body,
            Fetched::Status { code, .. } => {
                return Err(AppError::Network(format!("HTTP {code} fetching {url}")));
            }
            Fetched::Transport(err) => {
                return Err(AppError::Network(format!("Failed to fetch {url}: {err}")));
            }
        };
        let url_clone = url.clone();
        let (_title, markdown) = http
            .parse(move || reference_text::extract_with_url(&html, Some(url_clone.as_str())))
            .await??;
        Ok(leads_from_markdown(url, &markdown, near.as_ref()))
    }

    fn health(&self) -> Vec<FinderHealth> {
        let descriptors: Vec<&FinderDescriptor> =
            self.registry.iter().map(FinderEntry::descriptor).collect();
        self.health
            .snapshot(&descriptors, &self.settings, &self.keys)
    }
}
