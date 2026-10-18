//! Unpaywall finder: open access locator for known DOIs.
//!
//! Reaches api.unpaywall.org; requires a DOI and declared user contact.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identifier,
    Identity, Lead, LeadKind, NeedKind, Privacy, RatePolicy, Reach, Role, SourceNeed, WaveSlot,
};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const API: &str = "https://api.unpaywall.org";

pub(crate) const ID: FinderId = FinderId::from_static("unpaywall");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Unpaywall",
    finds: &[Role {
        kind: NeedKind::Paper,
        wave: WaveSlot::Third,
    }],
    reaches: Reach::Scholarly,
    auth: Auth::Contact,
    privacy: Privacy::IdentifiedByContact {
        by: "OurResearch (Unpaywall)",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://unpaywall.org/products/api",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("api.unpaywall.org"),
};

pub(crate) struct Unpaywall {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl Unpaywall {
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

fn clean_doi(doi: &str) -> String {
    let trimmed = doi.trim();
    let stripped = if let Some(s) = trimmed.strip_prefix("https://doi.org/") {
        s
    } else if let Some(s) = trimmed.strip_prefix("http://doi.org/") {
        s
    } else if let Some(s) = trimmed.strip_prefix("doi:") {
        s
    } else {
        trimmed
    };
    stripped
        .trim_end_matches(['.', ';', ',', ')', '>', ']'])
        .to_string()
}

fn doi_from(need: &SourceNeed) -> Option<String> {
    if let Some(subject) = &need.subject {
        for id in &subject.identifiers {
            if let Identifier::Doi(doi) = id {
                let clean = clean_doi(doi);
                if !clean.is_empty() {
                    return Some(clean);
                }
            }
        }
    }
    let re = lazy_regex::regex!(r"10\.\d{4,9}/\S+");
    re.find(&need.query).map(|m| clean_doi(m.as_str()))
}

fn leads_from(payload: &serde_json::Value, page: usize) -> Result<Vec<Lead>, FailureCause> {
    let title = payload.get("title").and_then(|v| v.as_str()).map(str::trim);
    let raw_doi = payload.get("doi").and_then(|v| v.as_str()).map(clean_doi);
    let Some(doi) = raw_doi.filter(|d| !d.is_empty()) else {
        return Ok(Vec::new());
    };

    let kind = LeadKind::Document(Identifier::Doi(doi.clone()));
    let mut leads = Vec::new();

    let doi_url_str = payload
        .get("doi_url")
        .and_then(|v| v.as_str())
        .map(str::trim);

    let landing_url = doi_url_str
        .and_then(|u| Url::parse(u).ok())
        .or_else(|| Url::parse(&format!("https://doi.org/{doi}")).ok());

    if let Some(url) = landing_url {
        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
        leads.push(Lead {
            title: title.map(str::to_string),
            snippet: None,
            published: None,
            kind: kind.clone(),
            ..Lead::page(url, ID, rank)
        });
    }

    if let Some(best_oa) = payload.get("best_oa_location") {
        if let Some(landing_str) = best_oa.get("url_for_landing_page").and_then(|v| v.as_str()) {
            if let Ok(oa_landing) = Url::parse(landing_str.trim()) {
                let already_added = leads.iter().any(|l| l.url == oa_landing);
                if !already_added {
                    let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                    leads.push(Lead {
                        title: title.map(str::to_string),
                        snippet: Some("Open access landing page".to_string()),
                        published: None,
                        kind: kind.clone(),
                        ..Lead::page(oa_landing, ID, rank)
                    });
                }
            }
        }

        if let Some(pdf_str) = best_oa.get("url_for_pdf").and_then(|v| v.as_str()) {
            if let Ok(oa_pdf) = Url::parse(pdf_str.trim()) {
                let already_added = leads.iter().any(|l| l.url == oa_pdf);
                if !already_added {
                    let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                    leads.push(Lead {
                        title: title.map(str::to_string),
                        snippet: Some("Open access PDF".to_string()),
                        published: None,
                        kind,
                        ..Lead::page(oa_pdf, ID, rank)
                    });
                }
            }
        }
    }

    Ok(leads)
}

#[async_trait]
impl SourceFinder for Unpaywall {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let Some(doi) = doi_from(need) else {
            return answered(&self.descriptor, Vec::new(), started);
        };
        let Some(contact) = self
            .http
            .contact
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
        else {
            return answered(&self.descriptor, Vec::new(), started);
        };
        if page > 1 {
            return answered(&self.descriptor, Vec::new(), started);
        }

        let encoded_doi = doi
            .split('/')
            .map(|part| urlencoding::encode(part).into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let encoded_contact = urlencoding::encode(contact);
        let url = format!("{}/v2/{encoded_doi}?email={encoded_contact}", self.api);

        let headers = identity::declared_client_headers(Some(contact), "application/json");
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
