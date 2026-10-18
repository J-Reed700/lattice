//! Bing's HTML results page: a keyless web engine with a native `site:`.
//! Microsoft has retired the Bing Search API, so this page is the only way
//! to Bing without a key, and the first thing likely to break.

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

/// Results asked for per page (`count=`); Bing's `first=` steps by this.
pub(crate) const RESULTS_PER_PAGE: usize = 10;

/// Largest decoded result page accepted.
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

const ENDPOINT: &str = "https://www.bing.com/search";

/// Where the request says it came from: Bing's own front page.
const REFERER: &str = "https://www.bing.com/";

pub(crate) const ID: FinderId = FinderId::from_static("bing_html");

/// Bing publishes no rate for its HTML page, so the spacing is this app's
/// own per-host pacing. A refusal or a challenge opens the breaker at once,
/// as for every scraped engine.
pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Bing",
    finds: &[Role {
        kind: NeedKind::Web,
        wave: WaveSlot::First,
    }],
    reaches: Reach::OpenWeb,
    auth: Auth::None,
    privacy: Privacy::QueryLogged {
        by: "Microsoft Bing",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: true,
        source: "https://www.microsoft.com/servicesagreement",
    },
    supports_scope: true,
    identity: Identity::BrowserLike,
    sends_to: Destination::Host("bing.com"),
};

pub(crate) struct BingHtml {
    http: FinderHttp,
    endpoint: String,
    descriptor: FinderDescriptor,
}

impl BingHtml {
    pub(crate) fn new(http: FinderHttp) -> Self {
        Self::with_endpoint(http, ENDPOINT.to_string())
    }

    /// The finder against another endpoint (a fixture server in tests).
    pub(crate) fn with_endpoint(http: FinderHttp, endpoint: String) -> Self {
        Self {
            http,
            endpoint,
            descriptor: DESCRIPTOR,
        }
    }
}

#[async_trait]
impl SourceFinder for BingHtml {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let query = urlencoding::encode(&scoped_query(need)).into_owned();
        let first = rank_of(page, RESULTS_PER_PAGE, 0);
        let url = format!(
            "{}?q={query}&count={RESULTS_PER_PAGE}&first={first}",
            self.endpoint
        );
        let headers = identity::search_headers(identity::random_profile(), Some(REFERER));
        let html = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(html) => html,
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
        answered(&self.descriptor, leads, started)
    }
}
