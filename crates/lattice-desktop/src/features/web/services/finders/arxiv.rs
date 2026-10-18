//! arXiv finder: pre-print repository search and identifier resolution.
//!
//! Reaches export.arxiv.org API; parses Atom XML results.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identifier,
    Identity, Lead, LeadKind, NeedKind, Origin, Privacy, RatePolicy, Reach, Role, SourceNeed,
    WaveSlot,
};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const API: &str = "https://export.arxiv.org";

pub(crate) const ID: FinderId = FinderId::from_static("arxiv");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "arXiv",
    finds: &[Role {
        kind: NeedKind::Paper,
        wave: WaveSlot::Second,
    }],
    reaches: Reach::Scholarly,
    auth: Auth::None,
    privacy: Privacy::QueryLogged {
        by: "Cornell University (arXiv)",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: Duration::from_secs(3),
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://info.arxiv.org/help/api/tou.html",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("export.arxiv.org"),
};

pub(crate) struct Arxiv {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl Arxiv {
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

fn arxiv_id_from(need: &SourceNeed) -> Option<String> {
    if let Some(subject) = &need.subject {
        for id in &subject.identifiers {
            if let Identifier::Arxiv(arxiv_id) = id {
                let clean = arxiv_id.trim();
                if !clean.is_empty() {
                    return Some(clean.to_string());
                }
            }
        }
    }
    None
}

fn leads_from(body: &str, page: usize) -> Result<Vec<Lead>, FailureCause> {
    let mut leads = Vec::new();
    let entry_re = lazy_regex::regex!(r"(?s)<entry>(.*?)</entry>");
    let id_re = lazy_regex::regex!(r"(?s)<id>(.*?)</id>");
    let title_re = lazy_regex::regex!(r"(?s)<title>(.*?)</title>");
    let summary_re = lazy_regex::regex!(r"(?s)<summary>(.*?)</summary>");
    let published_re = lazy_regex::regex!(r"(?s)<published>(.*?)</published>");

    for cap in entry_re.captures_iter(body) {
        let Some(entry_match) = cap.get(1) else {
            continue;
        };
        let entry_content = entry_match.as_str();

        let raw_id = id_re
            .captures(entry_content)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim());

        let Some(raw_id_str) = raw_id else {
            continue;
        };
        if raw_id_str.is_empty() {
            continue;
        }

        let clean_id = if let Some(s) = raw_id_str.strip_prefix("https://arxiv.org/abs/") {
            s
        } else if let Some(s) = raw_id_str.strip_prefix("http://arxiv.org/abs/") {
            s
        } else {
            raw_id_str
        };

        let lead_url = if let Ok(parsed) = Url::parse(raw_id_str) {
            parsed
        } else if let Ok(parsed) = Url::parse(&format!("https://arxiv.org/abs/{clean_id}")) {
            parsed
        } else {
            continue;
        };

        let title = title_re
            .captures(entry_content)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|s| !s.is_empty());

        let summary = summary_re
            .captures(entry_content)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|s| !s.is_empty());

        let published = published_re
            .captures(entry_content)
            .and_then(|c| c.get(1))
            .and_then(|m| DateTime::parse_from_rfc3339(m.as_str().trim()).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
        leads.push(Lead {
            title,
            snippet: summary,
            published,
            kind: LeadKind::Document(Identifier::Arxiv(clean_id.to_string())),
            owner: Some(Origin::new("arxiv.org", None)),
            ..Lead::page(lead_url, ID, rank)
        });
    }

    Ok(leads)
}

#[async_trait]
impl SourceFinder for Arxiv {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let url = if let Some(id) = arxiv_id_from(need) {
            if page > 1 {
                return answered(&self.descriptor, Vec::new(), started);
            }
            let encoded_id = urlencoding::encode(&id);
            format!("{}/api/query?id_list={encoded_id}", self.api)
        } else {
            let offset = rank_of(page, RESULTS_PER_PAGE, 0).saturating_sub(1);
            let query = urlencoding::encode(need.query.trim());
            format!(
                "{}/api/query?search_query=all:{query}&start={offset}&max_results={RESULTS_PER_PAGE}",
                self.api
            )
        };

        let headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/atom+xml");
        let body = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(body) => body,
            other => return failed(&self.descriptor, cause_of(other), started),
        };

        match leads_from(&body, page) {
            Ok(leads) => answered(&self.descriptor, leads, started),
            Err(cause) => failed(&self.descriptor, cause, started),
        }
    }
}
