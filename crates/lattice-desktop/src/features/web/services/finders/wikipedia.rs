//! English Wikipedia's search, through the MediaWiki Action API
//! (`list=search`). An API, so it is called as a declared client: the
//! Wikimedia User-Agent policy forbids bots from wearing a browser's agent.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identity,
    Lead, NeedKind, Privacy, RatePolicy, Reach, Role, SourceNeed, WaveSlot,
};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

/// Results asked for per page: MediaWiki's own default `srlimit`.
pub(crate) const RESULTS_PER_PAGE: usize = 10;

/// Largest decoded API response accepted.
const MAX_BODY_BYTES: usize = 1024 * 1024;

const API: &str = "https://en.wikipedia.org/w/api.php";
const ARTICLE_BASE: &str = "https://en.wikipedia.org/wiki/";

pub(crate) const ID: FinderId = FinderId::from_static("wikipedia");

/// The API etiquette sets no hard read limit and asks for requests in
/// series, which the per-host pacer already guarantees; the spacing is that
/// pacer's. It answers overload with `Retry-After`, which is honoured.
pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Wikipedia",
    finds: &[Role {
        kind: NeedKind::Encyclopedia,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Encyclopedia,
    auth: Auth::None,
    privacy: Privacy::QueryLogged {
        by: "the Wikimedia Foundation",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://www.mediawiki.org/wiki/API:Etiquette",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("en.wikipedia.org"),
};

pub(crate) struct Wikipedia {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl Wikipedia {
    pub(crate) fn new(http: FinderHttp) -> Self {
        Self::with_api(http, API.to_string())
    }

    /// The finder against another API root (a fixture server in tests).
    pub(crate) fn with_api(http: FinderHttp, api: String) -> Self {
        Self {
            http,
            api,
            descriptor: DESCRIPTOR,
        }
    }
}

/// What a search payload holds: article leads, or the API's own error.
fn leads_from(payload: &serde_json::Value, page: usize) -> Result<Vec<Lead>, FailureCause> {
    if let Some(error) = payload.get("error") {
        let code = error
            .get("code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        // `ratelimited` is the API's 429 delivered in a 200.
        return Err(if code == "ratelimited" {
            FailureCause::Status {
                code: 429,
                retry_after: None,
            }
        } else {
            FailureCause::Parse(format!("API error {code}"))
        });
    }
    let Some(items) = payload
        .get("query")
        .and_then(|query| query.get("search"))
        .and_then(serde_json::Value::as_array)
    else {
        return Err(FailureCause::Parse("no search results field".to_string()));
    };
    let mut leads = Vec::new();
    for item in items {
        let title = item
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let Ok(url) = Url::parse(&format!("{ARTICLE_BASE}{}", title.replace(' ', "_"))) else {
            continue;
        };
        if title.is_empty() {
            continue;
        }
        let snippet = item
            .get("snippet")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .replace("<span class=\"searchmatch\">", "")
            .replace("</span>", "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
        leads.push(Lead {
            title: Some(title.to_string()),
            snippet: (!snippet.is_empty()).then_some(snippet),
            ..Lead::page(url, ID, rank)
        });
    }
    Ok(leads)
}

#[async_trait]
impl SourceFinder for Wikipedia {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let query = urlencoding::encode(need.query.trim()).into_owned();
        let offset = rank_of(page, RESULTS_PER_PAGE, 0).saturating_sub(1);
        let url = format!(
            "{}?action=query&format=json&utf8=1&list=search&srprop=snippet&srsearch={query}&srlimit={RESULTS_PER_PAGE}&sroffset={offset}",
            self.api
        );
        let headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");
        let body = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(body) => body,
            other => return failed(&self.descriptor, cause_of(other), started),
        };
        let payload = match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(payload) => payload,
            Err(error) => {
                return failed(
                    &self.descriptor,
                    FailureCause::Parse(error.to_string()),
                    started,
                )
            }
        };
        match leads_from(&payload, page) {
            Ok(leads) => answered(&self.descriptor, leads, started),
            Err(cause) => failed(&self.descriptor, cause, started),
        }
    }
}
