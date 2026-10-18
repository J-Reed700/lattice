//! Europe PMC finder: life-sciences articles, PubMed and PMC links.
//!
//! Reaches Europe PMC REST API at www.ebi.ac.uk.

use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
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
const API: &str = "https://www.ebi.ac.uk";

pub(crate) const ID: FinderId = FinderId::from_static("europe_pmc");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Europe PMC",
    finds: &[Role {
        kind: NeedKind::Paper,
        wave: WaveSlot::Second,
    }],
    reaches: Reach::Scholarly,
    auth: Auth::None,
    privacy: Privacy::QueryLogged {
        by: "EMBL-EBI (Europe PMC)",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://europepmc.org/RestfulWebService",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("www.ebi.ac.uk"),
};

pub(crate) struct EuropePmc {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl EuropePmc {
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

fn leads_from(payload: &serde_json::Value, page: usize) -> Result<Vec<Lead>, FailureCause> {
    let Some(results) = payload
        .get("resultList")
        .and_then(|r| r.get("result"))
        .and_then(|v| v.as_array())
    else {
        return Ok(Vec::new());
    };

    let mut leads = Vec::new();
    for item in results {
        let title = item
            .get("title")
            .and_then(|v| v.as_str())
            .map(|t| t.trim_end_matches('.').trim());

        let raw_doi = item.get("doi").and_then(|v| v.as_str()).map(clean_doi);
        let clean_doi_str = raw_doi.filter(|d| !d.is_empty());

        let pmid = item
            .get("pmid")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|p| !p.is_empty());

        let pmcid = item
            .get("pmcid")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|p| !p.is_empty());

        let lead_url = if let Some(p) = pmid {
            Url::parse(&format!("https://europepmc.org/article/MED/{p}")).ok()
        } else if let Some(ref d) = clean_doi_str {
            Url::parse(&format!("https://doi.org/{d}")).ok()
        } else {
            None
        };

        let Some(url) = lead_url else {
            continue;
        };

        let published = item
            .get("pubYear")
            .and_then(|v| v.as_str())
            .and_then(|y| y.parse::<i32>().ok())
            .and_then(|y| chrono::NaiveDate::from_ymd_opt(y, 1, 1))
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));

        let kind = match &clean_doi_str {
            Some(d) => LeadKind::Document(Identifier::Doi(d.clone())),
            None => LeadKind::Page,
        };

        let snippet = pmcid.map(|p| format!("PMC: {p}"));

        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
        leads.push(Lead {
            title: title.map(str::to_string),
            snippet,
            published,
            kind,
            ..Lead::page(url, ID, rank)
        });
    }

    Ok(leads)
}

#[async_trait]
impl SourceFinder for EuropePmc {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let query = urlencoding::encode(need.query.trim());
        let url = format!(
            "{}/europepmc/webservices/rest/search?query={query}&format=json&pageSize={RESULTS_PER_PAGE}&page={page}",
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
