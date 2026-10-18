//! Exa neural search API web finder.

use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identity,
    KeyPlan, KeyService, Lead, NeedKind, Privacy, RatePolicy, Reach, Role, SkipReason, SourceNeed,
    WaveSlot,
};
use reqwest::header::{HeaderName, HeaderValue};
use serde_json::json;
use url::Url;

use super::{answered, failed, rank_of, retry_after, scoped_query, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::{read_text_bounded, MIN_SITE_REQUEST_INTERVAL};

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const API: &str = "https://api.exa.ai";

pub(crate) const ID: FinderId = FinderId::from_static("exa");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Exa",
    finds: &[
        Role {
            kind: NeedKind::Web,
            wave: WaveSlot::Second,
        },
        Role {
            kind: NeedKind::Paper,
            wave: WaveSlot::Third,
        },
        Role {
            kind: NeedKind::Package,
            wave: WaveSlot::Second,
        },
    ],
    reaches: Reach::OpenWeb,
    auth: Auth::RequiredKey {
        service: KeyService::Exa,
        plan: KeyPlan::Credits,
    },
    privacy: Privacy::IdentifiedByKey { by: "Exa" },
    cost: Cost::Credits { per_request: 1 },
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://exa.ai/docs",
    },
    supports_scope: true,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("api.exa.ai"),
};

pub(crate) struct Exa {
    http: FinderHttp,
    api: String,
    api_key: Option<String>,
    descriptor: FinderDescriptor,
}

impl Exa {
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
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let published = item
            .get("publishedDate")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| {
                DateTime::parse_from_rfc3339(s.trim())
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            });

        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
        leads.push(Lead {
            title: (!title.is_empty()).then_some(title.to_string()),
            snippet,
            published,
            ..Lead::page(url, ID, rank)
        });
    }
    Ok(leads)
}

#[async_trait]
impl SourceFinder for Exa {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let Some(key) = self.api_key.as_deref().filter(|k| !k.trim().is_empty()) else {
            return FinderOutcome::skipped(
                self.descriptor.id.clone(),
                SkipReason::NoKey {
                    service: KeyService::Exa,
                },
            );
        };

        let url = format!("{}/search", self.api);
        let mut headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");
        if let Ok(val) = HeaderValue::from_str(key.trim()) {
            headers.insert(HeaderName::from_static("x-api-key"), val);
        }

        let payload = json!({
            "query": scoped_query(need),
            "numResults": RESULTS_PER_PAGE,
        });

        let _slot = self.http.pacer.acquire(&url).await;
        let response = match self
            .http
            .client
            .post(&url)
            .headers(headers)
            .json(&payload)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                return failed(
                    &self.descriptor,
                    FailureCause::Transport(e.to_string()),
                    started,
                );
            }
        };

        let status = response.status();
        if !status.is_success() {
            return failed(
                &self.descriptor,
                FailureCause::Status {
                    code: status.as_u16(),
                    retry_after: retry_after(response.headers(), Utc::now()),
                },
                started,
            );
        }

        let body = match read_text_bounded(response, MAX_BODY_BYTES).await {
            Ok(text) => text,
            Err(e) => return failed(&self.descriptor, FailureCause::Transport(e), started),
        };

        let json_payload = match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(p) => p,
            Err(e) => {
                return failed(
                    &self.descriptor,
                    FailureCause::Parse(e.to_string()),
                    started,
                );
            }
        };

        match leads_from(&json_payload, page) {
            Ok(leads) => answered(&self.descriptor, leads, started),
            Err(cause) => failed(&self.descriptor, cause, started),
        }
    }
}
