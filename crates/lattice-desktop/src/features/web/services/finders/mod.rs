//! Finders: one adapter per service, each with its descriptor (design
//! §3.10). A finder answers one need for one page and never retries across
//! providers; the runner in `super::sourcing` decides what runs, when, and
//! what a failure means for the finder's breaker.

pub(crate) mod arxiv;
pub(crate) mod bing_html;
pub(crate) mod brave;
pub(crate) mod crates_io;
pub(crate) mod crossref;
pub(crate) mod discourse_json;
pub(crate) mod duckduckgo;
pub(crate) mod ecma_pattern;
pub(crate) mod eurlex_celex;
pub(crate) mod europe_pmc;
pub(crate) mod exa;
pub(crate) mod federal_register;
pub(crate) mod github;
pub(crate) mod go_module;
pub(crate) mod govinfo;
pub(crate) mod hn_algolia;
pub(crate) mod ietf_datatracker;
pub(crate) mod legislation_uk;
pub(crate) mod marginalia;
pub(crate) mod maven_pom;
pub(crate) mod npm;
pub(crate) mod nuget;
pub(crate) mod openalex;
pub(crate) mod pypi;
pub(crate) mod rfc_index;
pub(crate) mod rubygems;
pub(crate) mod searxng;
pub(crate) mod semantic_scholar;
pub(crate) mod serp;
pub(crate) mod site_discovery;
pub(crate) mod stack_exchange;
pub(crate) mod tavily;
pub(crate) mod unpaywall;
pub(crate) mod w3c;
pub(crate) mod whatwg_index;
pub(crate) mod wikidata;
pub(crate) mod wikipedia;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use lattice_core::application::contracts::settings::SourcingSettingsDto;
use lattice_core::application::ports::SourceFinder;
use lattice_core::domain::sourcing::{
    FailureCause, FinderDescriptor, FinderOutcome, FinderResult, KeyService, Lead, SourceNeed,
};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::Client;
use tokio::sync::Semaphore;

use super::web::{check_public_url, parse_html, read_text_bounded, SitePacer};

/// What every finder shares with the page reader: the guarded public client,
/// the per-host pacer, the HTML parser admission, and the user's optional
/// polite-pool contact for declared-client calls.
#[derive(Clone)]
pub(crate) struct FinderHttp {
    pub client: Client,
    pub pacer: Arc<SitePacer>,
    pub parsers: Arc<Semaphore>,
    pub contact: Option<String>,
    /// Whether [`FinderHttp::get`] refuses a URL whose host is local before
    /// sending it. Some of what is fetched comes from a model tool call or a
    /// site's robots.txt, and an IP-literal host never reaches the public
    /// client's resolver, so the address itself is checked. Off only for the
    /// user's own SearXNG instance, which may well be on their network.
    pub public_only: bool,
}

/// One response, as a finder needs it.
#[derive(Debug)]
pub(crate) enum Fetched {
    Body(String),
    /// A non-success status, with the host's `Retry-After` when it sent one.
    Status {
        code: u16,
        retry_after: Option<Duration>,
    },
    /// The request or its body read failed.
    Transport(String),
}

impl FinderHttp {
    /// One paced GET. The host's slot covers this request only, never a
    /// retry: what happens next is the runner's decision.
    pub(crate) async fn get(&self, url: &str, headers: HeaderMap, max_bytes: usize) -> Fetched {
        if self.public_only {
            if let Err(error) = check_public_url(url) {
                return Fetched::Transport(error.to_string());
            }
        }
        let _site_slot = self.pacer.acquire(url).await;
        let response = match self.client.get(url).headers(headers).send().await {
            Ok(response) => response,
            Err(error) => return Fetched::Transport(error.to_string()),
        };
        let status = response.status();
        if !status.is_success() {
            return Fetched::Status {
                code: status.as_u16(),
                retry_after: retry_after(response.headers(), Utc::now()),
            };
        }
        match read_text_bounded(response, max_bytes).await {
            Ok(text) => Fetched::Body(text),
            Err(error) => Fetched::Transport(error),
        }
    }
}

impl FinderHttp {
    /// Parse a result page under the shared parser admission.
    pub(crate) async fn parse<T, F>(&self, operation: F) -> lattice_core::shared::error::Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        parse_html(Arc::clone(&self.parsers), operation).await
    }
}

/// The host's `Retry-After`, in either form RFC 9110 §10.2.3 allows:
/// delay-seconds or an HTTP-date (a date already past means "now"). No
/// default and no ceiling: the breaker holds the finder for exactly what the
/// host asked, and the caller's deadline decides whether to wait for it.
pub(crate) fn retry_after(headers: &HeaderMap, now: DateTime<Utc>) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let date = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    Some((date - now).to_std().unwrap_or(Duration::ZERO))
}

/// A registered finder and how many results its service serves per page,
/// which tells the runner whether a page was full (a short page means the
/// service has no more).
#[derive(Clone)]
pub(crate) struct FinderEntry {
    pub finder: Arc<dyn SourceFinder>,
    pub page_size: usize,
}

impl FinderEntry {
    pub(crate) fn descriptor(&self) -> &FinderDescriptor {
        self.finder.descriptor()
    }
}

fn entry<F: SourceFinder + 'static>(finder: F, page_size: usize) -> FinderEntry {
    FinderEntry {
        finder: Arc::new(finder),
        page_size,
    }
}

/// The default registry (keyless finders on, plus keyed/instance finders that
/// report `Skipped(NoKey)` / `Skipped(NoInstance)` / `Skipped(NoContact)` until
/// configured), in the order their leads are listed when they tie.
pub(crate) fn keyless_registry(http: &FinderHttp) -> Vec<FinderEntry> {
    // The default settings name no SearXNG instance, so its client is never
    // used: the public one stands in.
    registry(
        http,
        &http.client,
        &SourcingSettingsDto::default(),
        &BTreeMap::new(),
    )
}

/// The full finder catalogue (§3.10), wired with any user-configured keys and
/// instance URLs. Every finder shares `http` and its public-only client,
/// except SearXNG, which reaches the user's own instance through `instance`.
pub(crate) fn registry(
    http: &FinderHttp,
    instance: &Client,
    settings: &SourcingSettingsDto,
    keys: &BTreeMap<KeyService, String>,
) -> Vec<FinderEntry> {
    let key = |service: KeyService| keys.get(&service).cloned();
    vec![
        // Open web engines
        entry(
            duckduckgo::DuckDuckGo::new(http.clone()),
            duckduckgo::RESULTS_PER_PAGE,
        ),
        entry(
            bing_html::BingHtml::new(http.clone()),
            bing_html::RESULTS_PER_PAGE,
        ),
        entry(
            brave::Brave::new(http.clone(), key(KeyService::Brave)),
            brave::RESULTS_PER_PAGE,
        ),
        entry(
            tavily::Tavily::new(http.clone(), key(KeyService::Tavily)),
            tavily::RESULTS_PER_PAGE,
        ),
        entry(
            exa::Exa::new(http.clone(), key(KeyService::Exa)),
            exa::RESULTS_PER_PAGE,
        ),
        entry(
            searxng::Searxng::new(
                FinderHttp {
                    client: instance.clone(),
                    public_only: false,
                    ..http.clone()
                },
                settings.searxng_url().map(|u| u.to_string()),
            ),
            searxng::RESULTS_PER_PAGE,
        ),
        entry(
            marginalia::Marginalia::new(http.clone()),
            marginalia::RESULTS_PER_PAGE,
        ),
        // Encyclopedia & owner sites
        entry(
            wikipedia::Wikipedia::new(http.clone()),
            wikipedia::RESULTS_PER_PAGE,
        ),
        entry(
            wikidata::Wikidata::new(http.clone()),
            wikidata::RESULTS_PER_PAGE,
        ),
        entry(
            site_discovery::SiteDiscovery::new(http.clone()),
            site_discovery::RESULTS_PER_PAGE,
        ),
        // Code hosts & software package registries
        entry(
            github::GitHub::with_token(http.clone(), key(KeyService::GitHub)),
            github::RESULTS_PER_PAGE,
        ),
        entry(
            crates_io::CratesIo::new(http.clone()),
            crates_io::RESULTS_PER_PAGE,
        ),
        entry(pypi::PyPi::new(http.clone()), pypi::RESULTS_PER_PAGE),
        entry(npm::Npm::new(http.clone()), npm::RESULTS_PER_PAGE),
        entry(nuget::Nuget::new(http.clone()), nuget::RESULTS_PER_PAGE),
        entry(
            rubygems::RubyGems::new(http.clone()),
            rubygems::RESULTS_PER_PAGE,
        ),
        entry(
            maven_pom::MavenPom::new(http.clone()),
            maven_pom::RESULTS_PER_PAGE,
        ),
        entry(
            go_module::GoModule::new(http.clone()),
            go_module::RESULTS_PER_PAGE,
        ),
        // Scholarly & OA
        entry(
            openalex::OpenAlex::new(http.clone()),
            openalex::RESULTS_PER_PAGE,
        ),
        entry(
            crossref::Crossref::new(http.clone()),
            crossref::RESULTS_PER_PAGE,
        ),
        entry(
            semantic_scholar::SemanticScholar::new(http.clone(), key(KeyService::SemanticScholar)),
            semantic_scholar::RESULTS_PER_PAGE,
        ),
        entry(arxiv::Arxiv::new(http.clone()), arxiv::RESULTS_PER_PAGE),
        entry(
            unpaywall::Unpaywall::new(http.clone()),
            unpaywall::RESULTS_PER_PAGE,
        ),
        entry(
            europe_pmc::EuropePmc::new(http.clone()),
            europe_pmc::RESULTS_PER_PAGE,
        ),
        // Standards bodies
        entry(
            rfc_index::RfcIndex::new(http.clone()),
            rfc_index::RESULTS_PER_PAGE,
        ),
        entry(
            ietf_datatracker::IetfDatatracker::new(http.clone()),
            ietf_datatracker::RESULTS_PER_PAGE,
        ),
        entry(w3c::W3c::new(http.clone()), w3c::RESULTS_PER_PAGE),
        entry(
            whatwg_index::WhatwgIndex::new(http.clone()),
            whatwg_index::RESULTS_PER_PAGE,
        ),
        entry(
            ecma_pattern::EcmaPattern::new(http.clone()),
            ecma_pattern::RESULTS_PER_PAGE,
        ),
        // Law & regulations
        entry(
            federal_register::FederalRegister::new(http.clone()),
            federal_register::RESULTS_PER_PAGE,
        ),
        entry(
            legislation_uk::LegislationUk::new(http.clone()),
            legislation_uk::RESULTS_PER_PAGE,
        ),
        entry(
            eurlex_celex::EurlexCelex::new(http.clone()),
            eurlex_celex::RESULTS_PER_PAGE,
        ),
        entry(
            govinfo::Govinfo::new(http.clone(), key(KeyService::ApiDataGov)),
            govinfo::RESULTS_PER_PAGE,
        ),
        // Community
        entry(
            stack_exchange::StackExchange::new(http.clone(), key(KeyService::StackExchange)),
            stack_exchange::RESULTS_PER_PAGE,
        ),
        entry(
            hn_algolia::HnAlgolia::new(http.clone()),
            hn_algolia::RESULTS_PER_PAGE,
        ),
        entry(
            discourse_json::DiscourseJson::new(http.clone()),
            discourse_json::RESULTS_PER_PAGE,
        ),
    ]
}

/// The query an engine with a native `site:` filter is sent: the need's
/// terms, scoped to the step's origin when it has one.
pub(crate) fn scoped_query(need: &SourceNeed) -> String {
    match &need.scope {
        Some(scope) => format!("site:{scope} {}", need.query.trim()),
        None => need.query.trim().to_string(),
    }
}

/// The 1-based rank of the `index`-th lead on `page` (1-based): a page-2
/// lead ranks after every page-1 lead, so fusion reads it as deeper.
pub(crate) fn rank_of(page: usize, page_size: usize, index: usize) -> usize {
    page.saturating_sub(1)
        .saturating_mul(page_size)
        .saturating_add(index)
        .saturating_add(1)
}

/// A finder's outcome for the leads it parsed: answered with leads, or empty.
pub(crate) fn answered(
    descriptor: &FinderDescriptor,
    leads: Vec<Lead>,
    started: Instant,
) -> FinderOutcome {
    let result = if leads.is_empty() {
        FinderResult::Empty
    } else {
        FinderResult::Answered
    };
    FinderOutcome {
        finder: descriptor.id.clone(),
        leads,
        result,
        elapsed: started.elapsed(),
    }
}

/// A finder's outcome for a failure.
pub(crate) fn failed(
    descriptor: &FinderDescriptor,
    cause: FailureCause,
    started: Instant,
) -> FinderOutcome {
    FinderOutcome {
        finder: descriptor.id.clone(),
        leads: Vec::new(),
        result: FinderResult::Failed(cause),
        elapsed: started.elapsed(),
    }
}

/// The failure a non-body response stands for.
pub(crate) fn cause_of(fetched: Fetched) -> FailureCause {
    match fetched {
        Fetched::Status { code, retry_after } => FailureCause::Status { code, retry_after },
        Fetched::Transport(detail) => FailureCause::Transport(detail),
        Fetched::Body(_) => FailureCause::Parse("unexpected body".to_string()),
    }
}

#[cfg(test)]
mod community_law_keyed_tests;
#[cfg(test)]
mod scholarly_tests;
#[cfg(test)]
mod software_registries_a_tests;
#[cfg(test)]
mod software_registries_b_tests;
#[cfg(test)]
mod standards_tests;
#[cfg(test)]
mod tests;
