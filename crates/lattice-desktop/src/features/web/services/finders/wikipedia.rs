//! English Wikipedia's search, through the MediaWiki Action API
//! (`list=search`). An API, so it is called as a declared client: the
//! Wikimedia User-Agent policy forbids bots from wearing a browser's agent.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identity,
    Lead, LeadKind, NeedKind, Privacy, RatePolicy, Reach, Role, SourceNeed, WaveSlot,
};
use url::Url;

use super::mediawiki::{self, MAX_BODY_BYTES};
use super::{answered, cause_of, failed, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) use mediawiki::RESULTS_PER_PAGE;

const API: &str = "https://en.wikipedia.org/w/api.php";
const ARTICLE_BASE: &str = "https://en.wikipedia.org/wiki/";

pub(crate) const ID: FinderId = FinderId::from_static("wikipedia");

/// The API etiquette sets no hard read limit and asks for requests in
/// series, which the per-host pacer already guarantees; the spacing is that
/// pacer's. It answers overload with `Retry-After`, which is honoured.
pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Wikipedia",
    finds: &[
        Role {
            kind: NeedKind::Encyclopedia,
            wave: WaveSlot::First,
        },
        Role {
            kind: NeedKind::Leads,
            wave: WaveSlot::First,
        },
    ],
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

    /// Extracts external links from the named page, returning them as
    /// citation leads with `LeadKind::ReferenceOf`.
    pub(crate) async fn external_links(&self, title_or_query: &str) -> (Vec<Lead>, FinderOutcome) {
        let started = Instant::now();
        let query = urlencoding::encode(title_or_query.trim()).into_owned();
        if query.is_empty() {
            return (Vec::new(), answered(&self.descriptor, Vec::new(), started));
        }
        let url = format!(
            "{}?action=query&format=json&utf8=1&prop=extlinks&ellimit=50&redirects=1&titles={query}",
            self.api
        );
        let headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");
        let body = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(body) => body,
            other => {
                return (
                    Vec::new(),
                    failed(&self.descriptor, cause_of(other), started),
                )
            }
        };
        let payload = match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(payload) => payload,
            Err(e) => {
                return (
                    Vec::new(),
                    failed(
                        &self.descriptor,
                        FailureCause::Parse(e.to_string()),
                        started,
                    ),
                )
            }
        };
        if let Some(error) = payload.get("error") {
            let code = error
                .get("code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            let cause = if code == "ratelimited" {
                FailureCause::Status {
                    code: 429,
                    retry_after: None,
                }
            } else {
                FailureCause::Parse(format!("API error {code}"))
            };
            return (Vec::new(), failed(&self.descriptor, cause, started));
        }
        let Some(pages) = payload
            .get("query")
            .and_then(|q| q.get("pages"))
            .and_then(serde_json::Value::as_object)
        else {
            return (
                Vec::new(),
                failed(
                    &self.descriptor,
                    FailureCause::Parse("no pages in query".into()),
                    started,
                ),
            );
        };

        let mut leads = Vec::new();
        for page in pages.values() {
            let title = page
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(title_or_query);
            let Ok(article_url) = Url::parse(&format!("{ARTICLE_BASE}{}", title.replace(' ', "_")))
            else {
                continue;
            };
            let Some(extlinks) = page.get("extlinks").and_then(serde_json::Value::as_array) else {
                continue;
            };
            for link in extlinks {
                let Some(raw_url) = link
                    .get("*")
                    .or_else(|| link.get("url"))
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let full_url = if raw_url.starts_with("//") {
                    format!("https:{raw_url}")
                } else {
                    raw_url.to_string()
                };
                let Ok(url) = Url::parse(&full_url) else {
                    continue;
                };
                let rank = leads.len() + 1;
                leads.push(Lead {
                    url,
                    title: None,
                    snippet: None,
                    published: None,
                    kind: LeadKind::ReferenceOf {
                        page: article_url.clone(),
                        place: None,
                    },
                    owner: None,
                    rel_canonical: None,
                    terms: None,
                    found_by: vec![(ID, rank)],
                });
            }
        }
        (leads.clone(), answered(&self.descriptor, leads, started))
    }
}

#[async_trait]
impl SourceFinder for Wikipedia {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        if need.kind == NeedKind::Leads {
            let (_leads, outcome) = self.external_links(&need.query).await;
            return outcome;
        }
        mediawiki::Search {
            http: &self.http,
            api: &self.api,
            article_base: ARTICLE_BASE,
            descriptor: &self.descriptor,
        }
        .run(&need.query, page)
        .await
    }
}
