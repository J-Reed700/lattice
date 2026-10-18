//! DuckDuckGo's HTML endpoint: a keyless web engine with a native `site:`.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identity,
    Lead, NeedKind, Privacy, RatePolicy, Reach, Role, SourceNeed, WaveSlot,
};

use super::{answered, cause_of, failed, rank_of, scoped_query, serp, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

/// Results the HTML endpoint serves per page; its `s=` offset steps by this.
pub(crate) const RESULTS_PER_PAGE: usize = 10;

/// Largest decoded result page accepted.
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

/// The endpoints in the order they are tried. A later one is tried only
/// when an earlier one answers as if it were gone (404, 410, or a server
/// error); a refusal or a challenge ends the call, because asking the same
/// host again by another door is what turns a challenge into a block.
const ENDPOINTS: [&str; 3] = [
    "https://html.duckduckgo.com/html/",
    "https://duckduckgo.com/html/",
    "https://lite.duckduckgo.com/lite/",
];

/// Where the request says it came from: DuckDuckGo's own front page.
const REFERER: &str = "https://duckduckgo.com/";

pub(crate) const ID: FinderId = FinderId::from_static("duckduckgo");

/// DuckDuckGo publishes no rate for its HTML pages, so the spacing is this
/// app's own per-host pacing. A refusal or a challenge opens the breaker at
/// once: the penalty for pressing on against a scraper's challenge is an
/// address block.
pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "DuckDuckGo",
    finds: &[Role {
        kind: NeedKind::Web,
        wave: WaveSlot::First,
    }],
    reaches: Reach::OpenWeb,
    auth: Auth::None,
    privacy: Privacy::AnonymousQuery,
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: true,
        source: "https://duckduckgo.com/terms",
    },
    supports_scope: true,
    identity: Identity::BrowserLike,
    sends_to: Destination::Host("duckduckgo.com"),
};

pub(crate) struct DuckDuckGo {
    http: FinderHttp,
    endpoints: Vec<String>,
    descriptor: FinderDescriptor,
}

impl DuckDuckGo {
    pub(crate) fn new(http: FinderHttp) -> Self {
        Self::with_endpoints(http, ENDPOINTS.iter().map(|e| (*e).to_string()).collect())
    }

    /// The finder against other endpoints (a fixture server in tests).
    pub(crate) fn with_endpoints(http: FinderHttp, endpoints: Vec<String>) -> Self {
        Self {
            http,
            endpoints,
            descriptor: DESCRIPTOR,
        }
    }
}

/// Whether a status means this endpoint is gone rather than that the host
/// refused us, so the next endpoint is worth one request.
fn endpoint_gone(code: u16) -> bool {
    matches!(code, 404 | 410) || (500..=599).contains(&code)
}

#[async_trait]
impl SourceFinder for DuckDuckGo {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let query = urlencoding::encode(&scoped_query(need)).into_owned();
        let offset = rank_of(page, RESULTS_PER_PAGE, 0).saturating_sub(1);
        let mut last = FailureCause::Transport("no endpoint configured".to_string());
        for endpoint in &self.endpoints {
            let url = format!("{endpoint}?q={query}&s={offset}");
            let headers = identity::search_headers(identity::random_profile(), Some(REFERER));
            let html = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
                Fetched::Body(html) => html,
                Fetched::Status { code, retry_after } if endpoint_gone(code) => {
                    last = FailureCause::Status { code, retry_after };
                    continue;
                }
                other => return failed(&self.descriptor, cause_of(other), started),
            };
            if serp::is_challenge_page(&html) {
                return failed(&self.descriptor, FailureCause::Challenge, started);
            }
            let hits = match self
                .http
                .parse(move || serp::parse_results(&html, RESULTS_PER_PAGE))
                .await
            {
                Ok(hits) => hits,
                Err(error) => {
                    return failed(
                        &self.descriptor,
                        FailureCause::Parse(error.to_string()),
                        started,
                    )
                }
            };
            let leads = hits
                .into_iter()
                .enumerate()
                .map(|(index, hit)| hit.into_lead(ID, rank_of(page, RESULTS_PER_PAGE, index)))
                .collect::<Vec<Lead>>();
            return answered(&self.descriptor, leads, started);
        }
        failed(&self.descriptor, last, started)
    }
}
