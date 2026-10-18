//! SearXNG metasearch instance web finder.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identity,
    InstanceSetting, Lead, NeedKind, Privacy, RatePolicy, Reach, Role, SkipReason, SourceNeed,
    WaveSlot,
};
use url::Url;

use super::{answered, cause_of, failed, rank_of, scoped_query, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

pub(crate) const ID: FinderId = FinderId::from_static("searxng");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "SearXNG",
    finds: &[Role {
        kind: NeedKind::Web,
        wave: WaveSlot::Second,
    }],
    reaches: Reach::OpenWeb,
    auth: Auth::UserInstance {
        setting: InstanceSetting::SearxngUrl,
    },
    privacy: Privacy::SentToUserInstance,
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://docs.searxng.org",
    },
    supports_scope: true,
    identity: Identity::DeclaredClient,
    sends_to: Destination::UserInstance,
};

pub(crate) struct Searxng {
    http: FinderHttp,
    base_url: Option<String>,
    descriptor: FinderDescriptor,
}

impl Searxng {
    pub(crate) fn new(http: FinderHttp, base_url: Option<String>) -> Self {
        Self {
            http,
            base_url,
            descriptor: DESCRIPTOR,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_base_url(http: FinderHttp, base_url: String) -> Self {
        Self {
            http,
            base_url: Some(base_url),
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
        let Ok(url) = Url::parse(url_str) else {
            continue;
        };
        let title = item
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let snippet = item
            .get("content")
            .and_then(serde_json::Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
        leads.push(Lead {
            title: (!title.is_empty()).then_some(title.to_string()),
            snippet,
            ..Lead::page(url, ID, rank)
        });
    }
    Ok(leads)
}

#[async_trait]
impl SourceFinder for Searxng {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let Some(base) = self.base_url.as_deref().filter(|b| !b.trim().is_empty()) else {
            return FinderOutcome::skipped(
                self.descriptor.id.clone(),
                SkipReason::NoInstance {
                    setting: InstanceSetting::SearxngUrl,
                },
            );
        };

        let base_clean = base.trim().trim_end_matches('/');
        let scoped = scoped_query(need);
        let query = urlencoding::encode(&scoped);
        let url = format!("{base_clean}/search?q={query}&format=json&pageno={page}");

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
