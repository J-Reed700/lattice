//! Semantic Scholar finder: AI2's Academic Graph API.
//!
//! Reaches Semantic Scholar paper index and open access PDFs.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identifier,
    Identity, KeyService, Lead, LeadKind, NeedKind, Privacy, RatePolicy, Reach, Role, SourceNeed,
    WaveSlot,
};
use reqwest::header::{HeaderName, HeaderValue};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const API: &str = "https://api.semanticscholar.org";

pub(crate) const ID: FinderId = FinderId::from_static("semantic_scholar");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Semantic Scholar",
    finds: &[Role {
        kind: NeedKind::Paper,
        wave: WaveSlot::Second,
    }],
    reaches: Reach::Scholarly,
    auth: Auth::OptionalKey {
        service: KeyService::SemanticScholar,
    },
    privacy: Privacy::QueryLogged {
        by: "the Allen Institute for AI",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: Duration::from_secs(1),
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://www.semanticscholar.org/product/api",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("api.semanticscholar.org"),
};

pub(crate) struct SemanticScholar {
    http: FinderHttp,
    api: String,
    api_key: Option<String>,
    descriptor: FinderDescriptor,
}

impl SemanticScholar {
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
    if let Some(error) = payload.get("error") {
        let msg = error.as_str().unwrap_or("Semantic Scholar error");
        return Err(FailureCause::Parse(msg.to_string()));
    }

    let Some(items) = payload.get("data").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };

    let mut leads = Vec::new();
    for item in items {
        let title = item.get("title").and_then(|v| v.as_str()).map(str::trim);
        let snippet = item.get("abstract").and_then(|v| v.as_str()).map(str::trim);
        let published = item
            .get("year")
            .and_then(|v| v.as_i64())
            .and_then(|y| chrono::NaiveDate::from_ymd_opt(y as i32, 1, 1))
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));

        let doi = item
            .get("externalIds")
            .and_then(|e| e.get("DOI"))
            .and_then(|v| v.as_str())
            .map(clean_doi)
            .filter(|d| !d.is_empty());

        let kind = match &doi {
            Some(d) => LeadKind::Document(Identifier::Doi(d.clone())),
            None => LeadKind::Page,
        };

        let raw_url = item.get("url").and_then(|v| v.as_str()).map(str::trim);
        let paper_url = raw_url.and_then(|u| Url::parse(u).ok()).or_else(|| {
            doi.as_ref()
                .and_then(|d| Url::parse(&format!("https://doi.org/{d}")).ok())
        });

        if let Some(url) = paper_url {
            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                title: title.map(str::to_string),
                snippet: snippet.map(str::to_string),
                published,
                kind: kind.clone(),
                ..Lead::page(url, ID, rank)
            });
        }

        let oa_pdf_url = item
            .get("openAccessPdf")
            .and_then(|oa| oa.get("url"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .and_then(|u| Url::parse(u).ok());

        if let Some(pdf_url) = oa_pdf_url {
            let already_added = leads.iter().any(|l| l.url == pdf_url);
            if !already_added {
                let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                leads.push(Lead {
                    title: title.map(str::to_string),
                    snippet: Some("Open Access PDF".to_string()),
                    published,
                    kind,
                    ..Lead::page(pdf_url, ID, rank)
                });
            }
        }
    }

    Ok(leads)
}

#[async_trait]
impl SourceFinder for SemanticScholar {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let offset = rank_of(page, RESULTS_PER_PAGE, 0).saturating_sub(1);
        let query = urlencoding::encode(need.query.trim());
        let url = format!(
            "{}/graph/v1/paper/search?query={query}&limit={RESULTS_PER_PAGE}&offset={offset}&fields=title,url,abstract,year,externalIds,openAccessPdf",
            self.api
        );

        let mut headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");
        if let Some(ref key) = self.api_key {
            let clean_key = key.trim();
            if !clean_key.is_empty() {
                if let Ok(val) = HeaderValue::from_str(clean_key) {
                    headers.insert(HeaderName::from_static("x-api-key"), val);
                }
            }
        }

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
