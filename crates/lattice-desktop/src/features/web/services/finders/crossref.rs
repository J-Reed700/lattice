//! Crossref finder: scholarly metadata retrieval and citation reference search.
//!
//! Reaches Crossref's REST API; supports DOI lookups and reference extraction.

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

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp, GetOptions};
use crate::features::web::services::identity;

/// Crossref limits list requests (queries and filters) to 1 a second in the
/// public pool and 3 a second in the polite pool.
const PUBLIC_LIST_INTERVAL: Duration = Duration::from_secs(1);
const POLITE_LIST_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 3);

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const API: &str = "https://api.crossref.org";

pub(crate) const ID: FinderId = FinderId::from_static("crossref");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Crossref",
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
    auth: Auth::OptionalKey {
        service: KeyService::CrossrefPlus,
    },
    privacy: Privacy::QueryLogged { by: "Crossref" },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: PUBLIC_LIST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://community.crossref.org/t/refining-rest-api-limits-for-improved-stability-and-reliability/16137",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("api.crossref.org"),
};

pub(crate) struct Crossref {
    http: FinderHttp,
    api: String,
    plus_token: Option<String>,
    /// The email contact that puts requests in the polite pool.
    mailto: Option<String>,
    descriptor: FinderDescriptor,
}

impl Crossref {
    pub(crate) fn new(http: FinderHttp, plus_token: Option<String>) -> Self {
        Self::with_api_token(http, API.to_string(), plus_token)
    }

    #[cfg(test)]
    pub(crate) fn with_api(http: FinderHttp, api: String) -> Self {
        Self::with_api_token(http, api, None)
    }

    pub(crate) fn with_api_token(
        http: FinderHttp,
        api: String,
        plus_token: Option<String>,
    ) -> Self {
        let mailto = email_contact(http.contact.as_deref()).map(str::to_string);
        let mut descriptor = DESCRIPTOR;
        if mailto.is_some() {
            descriptor.rate.min_interval = POLITE_LIST_INTERVAL;
        }
        Self {
            http,
            api,
            plus_token: plus_token
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty()),
            mailto,
            descriptor,
        }
    }

    /// `url` with the polite pool's `mailto` when the user's contact is an
    /// email address.
    fn with_mailto(&self, url: String) -> String {
        match &self.mailto {
            Some(email) => {
                let separator = if url.contains('?') { '&' } else { '?' };
                format!("{url}{separator}mailto={}", urlencoding::encode(email))
            }
            None => url,
        }
    }
}

/// The contact when it is an email address (one `@`, a dotted domain, no
/// spaces or slashes), the only form Crossref's polite pool reads.
fn email_contact(contact: Option<&str>) -> Option<&str> {
    let contact = contact?.trim();
    let (local, domain) = contact.split_once('@')?;
    let plain = !contact.chars().any(|c| c.is_whitespace() || c == '/');
    (plain
        && !local.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && domain.split('.').all(|label| !label.is_empty()))
    .then_some(contact)
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

fn strip_tags(text: &str) -> String {
    let re = lazy_regex::regex!(r"<[^>]*>");
    re.replace_all(text, "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_date_parts(item: &serde_json::Value) -> Option<DateTime<Utc>> {
    let date_parts = item
        .get("published-print")
        .or_else(|| item.get("published-online"))
        .or_else(|| item.get("created"))
        .and_then(|p| p.get("date-parts"))
        .and_then(|parts| parts.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.as_array())?;

    let year = date_parts.first()?.as_i64()? as i32;
    let month = date_parts
        .get(1)
        .and_then(|m| m.as_u64())
        .map(|m| m as u32)
        .unwrap_or(1);
    let day = date_parts
        .get(2)
        .and_then(|d| d.as_u64())
        .map(|d| d as u32)
        .unwrap_or(1);

    chrono::NaiveDate::from_ymd_opt(year, month, day)?
        .and_hms_opt(0, 0, 0)
        .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
}

fn parse_work_item(
    item: &serde_json::Value,
    page: usize,
    need_kind: NeedKind,
    leads: &mut Vec<Lead>,
) {
    let raw_doi = item.get("DOI").and_then(|v| v.as_str()).map(clean_doi);
    let clean_doi_str = raw_doi.filter(|d| !d.is_empty());

    let raw_url = item.get("URL").and_then(|v| v.as_str()).map(str::trim);
    let landing_url = raw_url.and_then(|u| Url::parse(u).ok()).or_else(|| {
        clean_doi_str
            .as_ref()
            .and_then(|d| Url::parse(&format!("https://doi.org/{d}")).ok())
    });

    let Some(url) = landing_url else {
        return;
    };

    let title = item
        .get("title")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(|t| t.trim().to_string());

    let snippet = item
        .get("abstract")
        .and_then(|v| v.as_str())
        .map(strip_tags)
        .filter(|s| !s.is_empty());

    let published = parse_date_parts(item);

    let kind = match &clean_doi_str {
        Some(doi) => LeadKind::Document(Identifier::Doi(doi.clone())),
        None => LeadKind::Page,
    };

    let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
    leads.push(Lead {
        title,
        snippet,
        published,
        kind,
        ..Lead::page(url, ID, rank)
    });

    if need_kind == NeedKind::Leads {
        if let Some(refs) = item.get("reference").and_then(|v| v.as_array()) {
            for ref_item in refs {
                let Some(ref_doi_raw) = ref_item.get("DOI").and_then(|v| v.as_str()) else {
                    continue;
                };
                let clean_ref = clean_doi(ref_doi_raw);
                if clean_ref.is_empty() {
                    continue;
                }
                if let Ok(ref_url) = Url::parse(&format!("https://doi.org/{clean_ref}")) {
                    if leads.iter().all(|l| l.url != ref_url) {
                        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                        leads.push(Lead {
                            title: None,
                            snippet: None,
                            published: None,
                            kind: LeadKind::Document(Identifier::Doi(clean_ref)),
                            ..Lead::page(ref_url, ID, rank)
                        });
                    }
                }
            }
        }
    }
}

fn leads_from(
    payload: &serde_json::Value,
    page: usize,
    need_kind: NeedKind,
) -> Result<Vec<Lead>, FailureCause> {
    let mut leads = Vec::new();
    let message = match payload.get("message") {
        Some(msg) => msg,
        None => {
            return Err(FailureCause::Parse(
                "missing Crossref message field".to_string(),
            ))
        }
    };

    if let Some(items) = message.get("items").and_then(|v| v.as_array()) {
        for item in items {
            parse_work_item(item, page, need_kind, &mut leads);
        }
    } else if message.is_object() {
        parse_work_item(message, page, need_kind, &mut leads);
    } else {
        return Err(FailureCause::Parse(
            "unexpected Crossref message shape".to_string(),
        ));
    }

    Ok(leads)
}

#[async_trait]
impl SourceFinder for Crossref {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let url = match doi_from(need) {
            Some(doi) if page == 1 => {
                let encoded_doi = doi
                    .split('/')
                    .map(|part| urlencoding::encode(part).into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                format!("{}/works/{encoded_doi}", self.api)
            }
            _ => {
                let offset = rank_of(page, RESULTS_PER_PAGE, 0).saturating_sub(1);
                let query = urlencoding::encode(need.query.trim());
                format!(
                    "{}/works?query={query}&rows={RESULTS_PER_PAGE}&offset={offset}",
                    self.api
                )
            }
        };

        let url = self.with_mailto(url);

        let mut headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");
        if let Some(token) = &self.plus_token {
            if let Ok(mut value) = HeaderValue::from_str(&format!("Bearer {token}")) {
                value.set_sensitive(true);
                headers.insert(HeaderName::from_static("crossref-plus-api-token"), value);
            }
        }
        let options = GetOptions {
            min_interval: Some(self.descriptor.rate.min_interval),
            ..GetOptions::default()
        };
        let body = match self
            .http
            .get_with(&url, headers, MAX_BODY_BYTES, options)
            .await
        {
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
                );
            }
        };

        match leads_from(&payload, page, need.kind) {
            Ok(leads) => answered(&self.descriptor, leads, started),
            Err(cause) => failed(&self.descriptor, cause, started),
        }
    }
}
