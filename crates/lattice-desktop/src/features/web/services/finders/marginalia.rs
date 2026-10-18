//! Marginalia Search API for the independent web.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identity,
    Lead, NeedKind, Privacy, RatePolicy, Reach, Role, SourceNeed, Terms, WaveSlot,
};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const API: &str = "https://api.marginalia.nu";

pub(crate) const ID: FinderId = FinderId::from_static("marginalia");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Marginalia",
    finds: &[Role {
        kind: NeedKind::Web,
        wave: WaveSlot::Second,
    }],
    reaches: Reach::OpenWeb,
    auth: Auth::None,
    privacy: Privacy::QueryLogged {
        by: "api.marginalia.nu",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://about.marginalia-search.com/article/api",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("api.marginalia.nu"),
};

pub(crate) struct Marginalia {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl Marginalia {
    pub(crate) fn new(http: FinderHttp) -> Self {
        Self::with_api(http, API.to_string())
    }

    pub(crate) fn with_api(http: FinderHttp, api: String) -> Self {
        Self {
            http,
            api,
            descriptor: DESCRIPTOR,
        }
    }
}

fn leads_from(payload: &serde_json::Value, page: usize) -> Result<Vec<Lead>, FailureCause> {
    let Some(results) = payload.get("results").and_then(serde_json::Value::as_array) else {
        return Err(FailureCause::Parse("no results field".to_string()));
    };

    let mut leads = Vec::new();
    for item in results {
        let Some(url_str) = item.get("url").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Ok(url) = Url::parse(url_str.trim()) else {
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
            terms: Some(Terms::NonCommercial),
            ..Lead::page(url, ID, rank)
        });
    }
    Ok(leads)
}

#[async_trait]
impl SourceFinder for Marginalia {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let query = urlencoding::encode(need.query.trim());
        let url = format!(
            "{}/public/search/{query}?count={RESULTS_PER_PAGE}",
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
