//! Brave Search API web finder.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identity,
    KeyPlan, KeyService, Lead, NeedKind, Privacy, RatePolicy, Reach, Role, SkipReason, SourceNeed,
    WaveSlot,
};
use reqwest::header::{HeaderName, HeaderValue};
use url::Url;

use super::{answered, cause_of, failed, rank_of, scoped_query, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const API: &str = "https://api.search.brave.com";

pub(crate) const ID: FinderId = FinderId::from_static("brave");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Brave Search",
    finds: &[Role {
        kind: NeedKind::Web,
        wave: WaveSlot::Second,
    }],
    reaches: Reach::OpenWeb,
    auth: Auth::RequiredKey {
        service: KeyService::Brave,
        plan: KeyPlan::Credits,
    },
    privacy: Privacy::IdentifiedByKey {
        by: "Brave Software",
    },
    cost: Cost::Credits { per_request: 1 },
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://brave.com/search/api",
    },
    supports_scope: true,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("api.search.brave.com"),
};

pub(crate) struct Brave {
    http: FinderHttp,
    api: String,
    api_key: Option<String>,
    descriptor: FinderDescriptor,
}

impl Brave {
    pub(crate) fn new(http: FinderHttp, api_key: Option<String>) -> Self {
        Self::with_api(http, API.to_string(), api_key)
    }

    pub(crate) fn with_api(http: FinderHttp, api: String, api_key: Option<String>) -> Self {
        Self {
            http,
            api,
            api_key,
            descriptor: DESCRIPTOR,
        }
    }
}

fn leads_from(payload: &serde_json::Value, page: usize) -> Result<Vec<Lead>, FailureCause> {
    let Some(results) = payload
        .get("web")
        .and_then(|w| w.get("results"))
        .and_then(serde_json::Value::as_array)
    else {
        return Err(FailureCause::Parse("no web.results field".to_string()));
    };

    let mut leads = Vec::new();
    for item in results {
        let Some(url_str) = item.get("url").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Ok(url) = Url::parse(url_str) else {
            continue;
        };
        let title = item
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let description = item
            .get("description")
            .and_then(serde_json::Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
        leads.push(Lead {
            title: (!title.is_empty()).then_some(title.to_string()),
            snippet: description,
            ..Lead::page(url, ID, rank)
        });
    }
    Ok(leads)
}

#[async_trait]
impl SourceFinder for Brave {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let Some(key) = self.api_key.as_deref().filter(|k| !k.trim().is_empty()) else {
            return FinderOutcome::skipped(
                self.descriptor.id.clone(),
                SkipReason::NoKey {
                    service: KeyService::Brave,
                },
            );
        };

        let offset = page.saturating_sub(1);
        let scoped = scoped_query(need);
        let query = urlencoding::encode(&scoped);
        let url = format!(
            "{}/res/v1/web/search?q={query}&count={RESULTS_PER_PAGE}&offset={offset}",
            self.api
        );

        let mut headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");
        if let Ok(val) = HeaderValue::from_str(key.trim()) {
            headers.insert(HeaderName::from_static("x-subscription-token"), val);
        }

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
