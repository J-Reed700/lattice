//! Discourse forum search through its public `search.json` endpoint.

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

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

pub(crate) const ID: FinderId = FinderId::from_static("discourse_json");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Discourse",
    finds: &[Role {
        kind: NeedKind::Community,
        wave: WaveSlot::Second,
    }],
    reaches: Reach::Community,
    auth: Auth::None,
    privacy: Privacy::QueryLogged {
        by: "the forum operator",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://docs.discourse.org",
    },
    supports_scope: true,
    identity: Identity::DeclaredClient,
    sends_to: Destination::SubjectOwner,
};

pub(crate) struct DiscourseJson {
    http: FinderHttp,
    descriptor: FinderDescriptor,
}

impl DiscourseJson {
    pub(crate) fn new(http: FinderHttp) -> Self {
        Self {
            http,
            descriptor: DESCRIPTOR,
        }
    }
}

fn leads_from(
    payload: &serde_json::Value,
    base_url: &str,
    page: usize,
) -> Result<Vec<Lead>, FailureCause> {
    let Some(topics) = payload.get("topics").and_then(serde_json::Value::as_array) else {
        return Err(FailureCause::Parse("no topics field".to_string()));
    };

    let mut leads = Vec::new();
    for topic in topics {
        let Some(id) = topic.get("id").and_then(serde_json::Value::as_i64) else {
            continue;
        };
        let slug = topic
            .get("slug")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("topic")
            .trim();
        let title = topic
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();

        let topic_url_str = format!("{base_url}/t/{slug}/{id}");
        let Ok(url) = Url::parse(&topic_url_str) else {
            continue;
        };
        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
        leads.push(Lead {
            title: (!title.is_empty()).then_some(title.to_string()),
            ..Lead::page(url, ID, rank)
        });
    }
    Ok(leads)
}

#[async_trait]
impl SourceFinder for DiscourseJson {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let Some(scope) = &need.scope else {
            return answered(&self.descriptor, Vec::new(), started);
        };

        let query = urlencoding::encode(need.query.trim());
        let base_url = format!("https://{}", scope.host());
        let url = format!("{base_url}/search.json?q={query}&page={page}");
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

        match leads_from(&payload, &base_url, page) {
            Ok(leads) => answered(&self.descriptor, leads, started),
            Err(cause) => failed(&self.descriptor, cause, started),
        }
    }
}
