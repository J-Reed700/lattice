//! IETF Datatracker: searches documents, RFCs, and internet-drafts.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identifier,
    Identity, Lead, LeadKind, NeedKind, Origin, Privacy, RatePolicy, Reach, Role, SourceNeed,
    WaveSlot,
};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const DEFAULT_API: &str = "https://datatracker.ietf.org";

pub(crate) const ID: FinderId = FinderId::from_static("ietf_datatracker");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "IETF Datatracker",
    finds: &[Role {
        kind: NeedKind::Standard,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Standards,
    auth: Auth::None,
    privacy: Privacy::QueryLogged { by: "the IETF" },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://datatracker.ietf.org/api/",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("datatracker.ietf.org"),
};

pub(crate) struct IetfDatatracker {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl IetfDatatracker {
    pub(crate) fn new(http: FinderHttp) -> Self {
        Self::with_api(http, DEFAULT_API.to_string())
    }

    pub(crate) fn with_api(http: FinderHttp, api: String) -> Self {
        Self {
            http,
            api,
            descriptor: DESCRIPTOR,
        }
    }
}

fn parse_leads(payload: &serde_json::Value, page: usize) -> Result<Vec<Lead>, FailureCause> {
    let Some(objects) = payload.get("objects").and_then(serde_json::Value::as_array) else {
        return Err(FailureCause::Parse(
            "no objects array in response".to_string(),
        ));
    };

    let mut leads = Vec::new();
    for item in objects {
        let name = item
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let title = item
            .get("title")
            .and_then(serde_json::Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let snippet = item
            .get("abstract")
            .and_then(serde_json::Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let rfc_num: Option<u32> = match item.get("rfc") {
            Some(serde_json::Value::Number(num)) => {
                num.as_u64().and_then(|n| u32::try_from(n).ok())
            }
            Some(serde_json::Value::String(s)) => s.parse::<u32>().ok(),
            _ => None,
        }
        .or_else(|| {
            name.strip_prefix("rfc")
                .and_then(|digits| digits.parse::<u32>().ok())
        });

        let (url, kind, owner) = match rfc_num {
            Some(n) => {
                let Ok(url) = Url::parse(&format!("https://www.rfc-editor.org/rfc/rfc{n}.html"))
                else {
                    continue;
                };
                (
                    url,
                    LeadKind::Document(Identifier::Rfc(n)),
                    Some(Origin::new("rfc-editor.org", None)),
                )
            }
            None => {
                if name.is_empty() {
                    continue;
                }
                let Ok(url) = Url::parse(&format!("https://datatracker.ietf.org/doc/{name}/"))
                else {
                    continue;
                };
                (url, LeadKind::Page, Some(Origin::new("ietf.org", None)))
            }
        };

        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
        leads.push(Lead {
            url,
            title,
            snippet,
            published: None,
            kind,
            owner,
            rel_canonical: None,
            terms: None,
            found_by: vec![(ID, rank)],
        });
    }

    Ok(leads)
}

#[async_trait]
impl SourceFinder for IetfDatatracker {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let query_str = if !need.query.trim().is_empty() {
            need.query.trim()
        } else if let Some(subject) = &need.subject {
            subject.name.trim()
        } else {
            ""
        };

        if query_str.is_empty() {
            return answered(&self.descriptor, Vec::new(), started);
        }

        let query = urlencoding::encode(query_str);
        let offset = rank_of(page, RESULTS_PER_PAGE, 0).saturating_sub(1);
        let url = format!(
            "{}/api/v1/doc/document/?format=json&limit={RESULTS_PER_PAGE}&offset={offset}&title__icontains={query}",
            self.api.trim_end_matches('/')
        );
        let headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");

        let body = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(body) => body,
            other => return failed(&self.descriptor, cause_of(other), started),
        };

        let payload = match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(p) => p,
            Err(error) => {
                return failed(
                    &self.descriptor,
                    FailureCause::Parse(error.to_string()),
                    started,
                )
            }
        };

        match parse_leads(&payload, page) {
            Ok(leads) => answered(&self.descriptor, leads, started),
            Err(cause) => failed(&self.descriptor, cause, started),
        }
    }
}
