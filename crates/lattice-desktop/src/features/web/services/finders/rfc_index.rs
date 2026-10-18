//! RFC Editor index: resolves RFC numbers to canonical rfc-editor.org leads.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identifier,
    Identity, Lead, LeadKind, NeedKind, Origin, Privacy, RatePolicy, Reach, Role, SourceNeed,
    WaveSlot,
};
use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;

use super::{answered, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const DEFAULT_API: &str = "https://www.rfc-editor.org";

pub(crate) const ID: FinderId = FinderId::from_static("rfc_index");

static RFC_RE: Lazy<Regex> = Lazy::new(|| match Regex::new(r"(?i)\brfc[\s-]*(\d{1,5})\b") {
    Ok(re) => re,
    Err(_) => match Regex::new("$^") {
        Ok(empty) => empty,
        Err(_) => unreachable!(),
    },
});

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "RFC Editor",
    finds: &[Role {
        kind: NeedKind::Standard,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Standards,
    auth: Auth::None,
    privacy: Privacy::QueryLogged {
        by: "the RFC Editor",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://www.rfc-editor.org/",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("www.rfc-editor.org"),
};

pub(crate) struct RfcIndex {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl RfcIndex {
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

fn extract_rfc_numbers(need: &SourceNeed) -> Vec<u32> {
    let mut rfcs = Vec::new();

    if let Some(subject) = &need.subject {
        for id in &subject.identifiers {
            if let Identifier::Rfc(n) = id {
                if !rfcs.contains(n) {
                    rfcs.push(*n);
                }
            }
        }
        for caps in RFC_RE.captures_iter(&subject.name) {
            if let Some(m) = caps.get(1) {
                if let Ok(n) = m.as_str().parse::<u32>() {
                    if !rfcs.contains(&n) {
                        rfcs.push(n);
                    }
                }
            }
        }
    }

    for caps in RFC_RE.captures_iter(&need.query) {
        if let Some(m) = caps.get(1) {
            if let Ok(n) = m.as_str().parse::<u32>() {
                if !rfcs.contains(&n) {
                    rfcs.push(n);
                }
            }
        }
    }

    rfcs
}

#[async_trait]
impl SourceFinder for RfcIndex {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        if page > 1 {
            return answered(&self.descriptor, Vec::new(), started);
        }

        let rfcs = extract_rfc_numbers(need);
        if rfcs.is_empty() {
            return answered(&self.descriptor, Vec::new(), started);
        }

        let mut leads = Vec::new();
        let headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");

        for n in rfcs {
            let api_url = format!("{}/rfc/rfc{n}.json", self.api.trim_end_matches('/'));
            let fetched = self
                .http
                .get(&api_url, headers.clone(), MAX_BODY_BYTES)
                .await;

            let (title, snippet) = match fetched {
                Fetched::Body(body) => {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) {
                        let title = val
                            .get("title")
                            .and_then(|v| v.as_str())
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .or_else(|| {
                                val.get("doc_id")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.trim().to_string())
                            })
                            .or_else(|| Some(format!("RFC {n}")));
                        let snippet = val
                            .get("abstract")
                            .and_then(|v| v.as_str())
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty());
                        (title, snippet)
                    } else {
                        (Some(format!("RFC {n}")), None)
                    }
                }
                Fetched::Status {
                    code: 429,
                    retry_after,
                } => {
                    return failed(
                        &self.descriptor,
                        FailureCause::Status {
                            code: 429,
                            retry_after,
                        },
                        started,
                    );
                }
                _ => (Some(format!("RFC {n}")), None),
            };

            let Ok(url) = Url::parse(&format!("https://www.rfc-editor.org/rfc/rfc{n}.html")) else {
                continue;
            };

            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                url,
                title,
                snippet,
                published: None,
                kind: LeadKind::Document(Identifier::Rfc(n)),
                owner: Some(Origin::new("rfc-editor.org", None)),
                rel_canonical: None,
                terms: None,
                found_by: vec![(self.descriptor.id.clone(), rank)],
            });
        }

        answered(&self.descriptor, leads, started)
    }
}
