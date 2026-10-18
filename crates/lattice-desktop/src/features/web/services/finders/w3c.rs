//! W3C Specifications: searches W3C standards and technical reports.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identity,
    Lead, LeadKind, NeedKind, Origin, Privacy, RatePolicy, Reach, Role, SourceNeed, WaveSlot,
};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_API: &str = "https://api.w3.org";

pub(crate) const ID: FinderId = FinderId::from_static("w3c");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "W3C Specifications",
    finds: &[Role {
        kind: NeedKind::Standard,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Standards,
    auth: Auth::None,
    privacy: Privacy::QueryLogged { by: "the W3C" },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://api.w3.org/doc",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("api.w3.org"),
};

pub(crate) struct W3c {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl W3c {
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

struct SpecCandidate {
    title: String,
    shortname: String,
    shortlink: Option<String>,
    score: u32,
}

fn score_spec(
    title: &str,
    shortname: &str,
    query_lower: &str,
    query_tokens: &[&str],
    subject_lower: &str,
    subject_tokens: &[&str],
) -> u32 {
    let t_lower = title.to_ascii_lowercase();
    let s_lower = shortname.to_ascii_lowercase();

    if query_lower.is_empty() && subject_lower.is_empty() {
        return 1;
    }

    let mut score = 0u32;
    if !query_lower.is_empty() {
        if s_lower == query_lower || t_lower == query_lower {
            score += 100;
        } else if t_lower.contains(query_lower) {
            score += 50;
        } else if s_lower.contains(query_lower) {
            score += 40;
        }
        for token in query_tokens {
            if s_lower.contains(token) {
                score += 15;
            }
            if t_lower.contains(token) {
                score += 10;
            }
        }
    }

    if !subject_lower.is_empty() {
        if s_lower == subject_lower || t_lower == subject_lower {
            score += 100;
        } else if t_lower.contains(subject_lower) {
            score += 50;
        } else if s_lower.contains(subject_lower) {
            score += 40;
        }
        for token in subject_tokens {
            if s_lower.contains(token) {
                score += 15;
            }
            if t_lower.contains(token) {
                score += 10;
            }
        }
    }

    score
}

fn extract_candidates(payload: &serde_json::Value, need: &SourceNeed) -> Vec<SpecCandidate> {
    let specs = payload
        .get("_links")
        .and_then(|l| l.get("specifications"))
        .and_then(serde_json::Value::as_array)
        .or_else(|| {
            payload
                .get("_embedded")
                .and_then(|e| e.get("specifications"))
                .and_then(serde_json::Value::as_array)
        })
        .or_else(|| {
            payload
                .get("specifications")
                .and_then(serde_json::Value::as_array)
        })
        .or_else(|| payload.as_array());

    let Some(items) = specs else {
        return Vec::new();
    };

    let query_lower = need.query.trim().to_ascii_lowercase();
    let subject_lower = need
        .subject
        .as_ref()
        .map(|s| s.name.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let query_tokens: Vec<&str> = query_lower.split_whitespace().collect();
    let subject_tokens: Vec<&str> = subject_lower.split_whitespace().collect();

    let mut candidates = Vec::new();
    for item in items {
        let title = item
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let shortname = item
            .get("shortname")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let shortlink = item
            .get("shortlink")
            .and_then(serde_json::Value::as_str)
            .or_else(|| item.get("href").and_then(serde_json::Value::as_str))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if title.is_empty() && shortname.is_empty() {
            continue;
        }

        let score = score_spec(
            title,
            shortname,
            &query_lower,
            &query_tokens,
            &subject_lower,
            &subject_tokens,
        );

        if score > 0 {
            candidates.push(SpecCandidate {
                title: title.to_string(),
                shortname: shortname.to_string(),
                shortlink,
                score,
            });
        }
    }

    candidates.sort_by_key(|b| std::cmp::Reverse(b.score));
    candidates
}

#[async_trait]
impl SourceFinder for W3c {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let url = format!(
            "{}/specifications?items=100&page=1",
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

        let candidates = extract_candidates(&payload, need);
        let offset = page.saturating_sub(1) * RESULTS_PER_PAGE;
        let mut leads = Vec::new();

        for candidate in candidates.into_iter().skip(offset).take(RESULTS_PER_PAGE) {
            let url_opt = candidate
                .shortlink
                .as_deref()
                .and_then(|link| Url::parse(link).ok())
                .or_else(|| {
                    if candidate.shortname.is_empty() {
                        None
                    } else {
                        Url::parse(&format!("https://www.w3.org/TR/{}/", candidate.shortname)).ok()
                    }
                });

            let Some(url) = url_opt else {
                continue;
            };

            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                url,
                title: (!candidate.title.is_empty()).then_some(candidate.title),
                snippet: None,
                published: None,
                kind: LeadKind::Page,
                owner: Some(Origin::new("w3.org", None)),
                rel_canonical: None,
                terms: None,
                found_by: vec![(ID, rank)],
            });
        }

        answered(&self.descriptor, leads, started)
    }
}
