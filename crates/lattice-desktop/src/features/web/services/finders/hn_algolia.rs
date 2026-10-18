//! Hacker News search through the Algolia Search API.

use std::time::Instant;

use async_trait::async_trait;
use chrono::DateTime;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identity,
    Lead, NeedKind, Privacy, RatePolicy, Reach, Role, SourceNeed, WaveSlot,
};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const API: &str = "https://hn.algolia.com";

pub(crate) const ID: FinderId = FinderId::from_static("hn_algolia");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Hacker News",
    finds: &[Role {
        kind: NeedKind::Community,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Community,
    auth: Auth::None,
    privacy: Privacy::QueryLogged { by: "Algolia" },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://hn.algolia.com/api",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("hn.algolia.com"),
};

pub(crate) struct HnAlgolia {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl HnAlgolia {
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
    let Some(hits) = payload.get("hits").and_then(serde_json::Value::as_array) else {
        return Err(FailureCause::Parse("no hits field".to_string()));
    };

    let mut leads = Vec::new();
    for hit in hits {
        let title = hit
            .get("title")
            .or_else(|| hit.get("story_title"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let published = hit
            .get("created_at_i")
            .and_then(serde_json::Value::as_i64)
            .and_then(|ts| DateTime::from_timestamp(ts, 0));
        let snippet = hit
            .get("story_text")
            .or_else(|| hit.get("comment_text"))
            .and_then(serde_json::Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let object_id = hit
            .get("objectID")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();

        let mut item_added = false;
        if !object_id.is_empty() {
            if let Ok(item_url) =
                Url::parse(&format!("https://news.ycombinator.com/item?id={object_id}"))
            {
                let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                leads.push(Lead {
                    title: (!title.is_empty()).then_some(title.to_string()),
                    snippet: snippet.clone(),
                    published,
                    ..Lead::page(item_url, ID, rank)
                });
                item_added = true;
            }
        }

        if let Some(story_raw) = hit.get("url").and_then(serde_json::Value::as_str) {
            if let Ok(story_url) = Url::parse(story_raw.trim()) {
                let is_dup = item_added && object_id.contains(story_url.as_str());
                if !is_dup {
                    let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                    leads.push(Lead {
                        title: (!title.is_empty()).then_some(title.to_string()),
                        snippet: snippet.clone(),
                        published,
                        ..Lead::page(story_url, ID, rank)
                    });
                }
            }
        }
    }
    Ok(leads)
}

#[async_trait]
impl SourceFinder for HnAlgolia {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let query = urlencoding::encode(need.query.trim());
        let offset_page = page.saturating_sub(1);
        let url = format!(
            "{}/api/v1/search?query={query}&hitsPerPage={RESULTS_PER_PAGE}&page={offset_page}",
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
