//! Stack Exchange API 2.3 search finder for community questions and answers.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::DateTime;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identity,
    KeyService, Lead, NeedKind, Privacy, Quota, RatePolicy, Reach, Role, SourceNeed, Terms,
    WaveSlot,
};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const API: &str = "https://api.stackexchange.com";

pub(crate) const ID: FinderId = FinderId::from_static("stack_exchange");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Stack Exchange",
    finds: &[Role {
        kind: NeedKind::Community,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Community,
    auth: Auth::OptionalKey {
        service: KeyService::StackExchange,
    },
    privacy: Privacy::QueryLogged {
        by: "Stack Exchange",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: Some(Quota {
            requests: 300,
            per: Duration::from_secs(86_400),
        }),
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://api.stackexchange.com/docs",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("api.stackexchange.com"),
};

pub(crate) struct StackExchange {
    http: FinderHttp,
    api: String,
    api_key: Option<String>,
    descriptor: FinderDescriptor,
}

impl StackExchange {
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
    let backoff = payload.get("backoff").and_then(serde_json::Value::as_u64);
    let error_id = payload.get("error_id").and_then(serde_json::Value::as_i64);
    let items = payload.get("items").and_then(serde_json::Value::as_array);

    if let Some(secs) = backoff {
        let is_empty = items.is_none_or(|arr| arr.is_empty());
        if is_empty || error_id.is_some() {
            return Err(FailureCause::Status {
                code: 429,
                retry_after: Some(Duration::from_secs(secs)),
            });
        }
    }
    if let Some(id) = error_id {
        let name = payload
            .get("error_name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("error");
        let msg = payload
            .get("error_message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        return Err(if name == "throttle_violation" {
            FailureCause::Status {
                code: 429,
                retry_after: None,
            }
        } else {
            FailureCause::Parse(format!("API error {id}: {name} {msg}").trim().to_string())
        });
    }

    let Some(items) = items else {
        return Err(FailureCause::Parse("no items field".to_string()));
    };

    let mut leads = Vec::new();
    for item in items {
        let Some(link) = item.get("link").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Ok(url) = Url::parse(link) else {
            continue;
        };
        let title = item
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let author = item
            .get("owner")
            .and_then(|o| o.get("display_name"))
            .and_then(serde_json::Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let published = item
            .get("creation_date")
            .and_then(serde_json::Value::as_i64)
            .and_then(|ts| DateTime::from_timestamp(ts, 0));

        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
        leads.push(Lead {
            title: (!title.is_empty()).then_some(title.to_string()),
            published,
            terms: Some(Terms::CcBySa {
                post_url: url.clone(),
                author,
            }),
            ..Lead::page(url, ID, rank)
        });
    }
    Ok(leads)
}

#[async_trait]
impl SourceFinder for StackExchange {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let query = urlencoding::encode(need.query.trim());
        let mut url = format!(
            "{}/2.3/search/advanced?order=desc&sort=relevance&site=stackoverflow&pagesize={RESULTS_PER_PAGE}&page={page}&q={query}",
            self.api
        );
        if let Some(key) = &self.api_key {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                url.push_str("&key=");
                url.push_str(&urlencoding::encode(trimmed));
            }
        }

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
