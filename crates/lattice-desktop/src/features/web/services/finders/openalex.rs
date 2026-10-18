//! OpenAlex finder: open scholarly metadata catalogue (`works`).
//!
//! Reaches scholarly literature; answers with paper records and open access
//! locations.

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
const API: &str = "https://api.openalex.org";

pub(crate) const ID: FinderId = FinderId::from_static("openalex");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "OpenAlex",
    finds: &[
        Role {
            kind: NeedKind::Paper,
            wave: WaveSlot::First,
        },
        Role {
            kind: NeedKind::Leads,
            wave: WaveSlot::Second,
        },
    ],
    reaches: Reach::Scholarly,
    auth: Auth::None,
    privacy: Privacy::QueryLogged {
        by: "OurResearch (OpenAlex)",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://docs.openalex.org/how-to-use-the-api/rate-limits-and-authentication",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("api.openalex.org"),
};

pub(crate) struct OpenAlex {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl OpenAlex {
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

fn parse_date(date_str: &str) -> Option<DateTime<Utc>> {
    let date_str = date_str.trim();
    if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        return date
            .and_hms_opt(0, 0, 0)
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(&format!("{date_str}-01"), "%Y-%m-%d") {
        return date
            .and_hms_opt(0, 0, 0)
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    if let Ok(year) = date_str.parse::<i32>() {
        return chrono::NaiveDate::from_ymd_opt(year, 1, 1)?
            .and_hms_opt(0, 0, 0)
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    None
}

fn parse_work(item: &serde_json::Value, page: usize, leads: &mut Vec<Lead>) {
    let title = item.get("title").and_then(|v| v.as_str()).map(str::trim);
    let raw_doi = item.get("doi").and_then(|v| v.as_str()).map(str::trim);
    let clean_doi_str = raw_doi.map(clean_doi).filter(|d| !d.is_empty());

    let landing_url_str = item
        .get("primary_location")
        .and_then(|loc| loc.get("landing_page_url"))
        .and_then(|v| v.as_str())
        .map(str::trim);

    let landing_url = landing_url_str
        .and_then(|u| Url::parse(u).ok())
        .or_else(|| {
            clean_doi_str
                .as_ref()
                .and_then(|d| Url::parse(&format!("https://doi.org/{d}")).ok())
        });

    let published = item
        .get("publication_date")
        .and_then(|v| v.as_str())
        .and_then(parse_date);

    let kind = match &clean_doi_str {
        Some(doi) => LeadKind::Document(Identifier::Doi(doi.clone())),
        None => LeadKind::Page,
    };

    if let Some(url) = landing_url {
        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
        leads.push(Lead {
            title: title.map(str::to_string),
            snippet: None,
            published,
            kind: kind.clone(),
            ..Lead::page(url, ID, rank)
        });
    }

    let oa_url_str = item
        .get("open_access")
        .and_then(|oa| oa.get("oa_url"))
        .and_then(|v| v.as_str())
        .map(str::trim);

    if let Some(oa_str) = oa_url_str {
        if let Ok(oa_url) = Url::parse(oa_str) {
            let already_added = leads.iter().any(|l| l.url == oa_url);
            if !already_added {
                let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                leads.push(Lead {
                    title: title.map(str::to_string),
                    snippet: Some("Open access copy".to_string()),
                    published,
                    kind,
                    ..Lead::page(oa_url, ID, rank)
                });
            }
        }
    }
}

fn leads_from(payload: &serde_json::Value, page: usize) -> Result<Vec<Lead>, FailureCause> {
    if let Some(error) = payload.get("error") {
        let message = if let Some(msg) = error.as_str() {
            msg
        } else {
            error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown OpenAlex error")
        };
        return Err(FailureCause::Parse(message.to_string()));
    }

    let mut leads = Vec::new();
    if let Some(results) = payload.get("results").and_then(|r| r.as_array()) {
        for item in results {
            parse_work(item, page, &mut leads);
        }
    } else if payload.is_object()
        && (payload.get("id").is_some()
            || payload.get("title").is_some()
            || payload.get("doi").is_some())
    {
        parse_work(payload, page, &mut leads);
    } else {
        return Err(FailureCause::Parse(
            "unexpected OpenAlex response format".to_string(),
        ));
    }
    Ok(leads)
}

#[async_trait]
impl SourceFinder for OpenAlex {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let url = if let Some(doi) = doi_from(need) {
            if page == 1 {
                format!("{}/works/https://doi.org/{doi}", self.api)
            } else {
                let query = urlencoding::encode(need.query.trim());
                format!(
                    "{}/works?search={query}&per-page={RESULTS_PER_PAGE}&page={page}",
                    self.api
                )
            }
        } else {
            let query = urlencoding::encode(need.query.trim());
            format!(
                "{}/works?search={query}&per-page={RESULTS_PER_PAGE}&page={page}",
                self.api
            )
        };

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
