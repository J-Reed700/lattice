//! GitHub search API (`/search/repositories`) and repository API (`/repos/{owner}/{name}`).
//!
//! Searches code repositories or resolves an exact repository subject.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identifier,
    Identity, KeyService, Lead, LeadKind, NeedKind, Origin, Privacy, Quota, RatePolicy, Reach,
    Role, SourceNeed, WaveSlot,
};
use reqwest::header::{HeaderValue, AUTHORIZATION};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const API: &str = "https://api.github.com";
pub(crate) const ID: FinderId = FinderId::from_static("github");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "GitHub",
    finds: &[Role {
        kind: NeedKind::Package,
        wave: WaveSlot::First,
    }],
    reaches: Reach::CodeHost,
    auth: Auth::OptionalKey {
        service: KeyService::GitHub,
    },
    privacy: Privacy::QueryLogged { by: "GitHub" },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: Duration::from_secs(1),
        quota: Some(Quota {
            requests: 60,
            per: Duration::from_secs(3600),
        }),
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("api.github.com"),
};

pub(crate) struct GitHub {
    http: FinderHttp,
    api: String,
    token: Option<String>,
    descriptor: FinderDescriptor,
}

impl GitHub {
    #[cfg(test)]
    pub(crate) fn new(http: FinderHttp) -> Self {
        Self::with_api(http, API.to_string(), None)
    }

    pub(crate) fn with_token(http: FinderHttp, token: Option<String>) -> Self {
        Self::with_api(http, API.to_string(), token)
    }

    pub(crate) fn with_api(http: FinderHttp, api: String, token: Option<String>) -> Self {
        let mut descriptor = DESCRIPTOR;
        if token.is_some() {
            descriptor.rate.quota = Some(Quota {
                requests: 5000,
                per: Duration::from_secs(3600),
            });
        }
        Self {
            http,
            api,
            token,
            descriptor,
        }
    }
}

fn clean_str(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

fn dedicated_domain_origin(homepage: &str) -> Option<Origin> {
    let origin = Origin::parse(homepage)?;
    let host = origin.host();
    if host.is_empty()
        || host == "github.com"
        || host.ends_with(".github.com")
        || host == "github.io"
        || host.ends_with(".github.io")
    {
        None
    } else {
        Some(origin)
    }
}

fn repos_from(payload: &serde_json::Value) -> Result<Vec<&serde_json::Value>, FailureCause> {
    if let Some(items) = payload.get("items").and_then(serde_json::Value::as_array) {
        return Ok(items.iter().collect());
    }
    if payload.get("full_name").is_some() {
        return Ok(vec![payload]);
    }
    if let Some(message) = payload.get("message").and_then(serde_json::Value::as_str) {
        if message.eq_ignore_ascii_case("not found") {
            return Ok(Vec::new());
        }
        return Err(FailureCause::Parse(format!("GitHub API error: {message}")));
    }
    Err(FailureCause::Parse(
        "unrecognized GitHub response shape".to_string(),
    ))
}

fn leads_from(payload: &serde_json::Value, page: usize) -> Result<Vec<Lead>, FailureCause> {
    let items = repos_from(payload)?;
    let mut leads = Vec::new();

    for repo in items {
        let Some(full_name) = clean_str(repo.get("full_name").and_then(serde_json::Value::as_str))
        else {
            continue;
        };
        let html_url = clean_str(repo.get("html_url").and_then(serde_json::Value::as_str));
        let description = clean_str(repo.get("description").and_then(serde_json::Value::as_str));
        let homepage = clean_str(repo.get("homepage").and_then(serde_json::Value::as_str));

        let owner = homepage
            .and_then(dedicated_domain_origin)
            .unwrap_or_else(|| Origin::new("github.com", Some(full_name)));
        let owner = Some(owner);

        // 1. OfficialSite: homepage
        if let Some(hp) = homepage {
            if let Ok(url) = Url::parse(hp) {
                let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                leads.push(Lead {
                    title: Some(format!("{full_name} homepage")),
                    snippet: description.map(ToString::to_string),
                    kind: LeadKind::OfficialSite,
                    owner: owner.clone(),
                    ..Lead::page(url, ID, rank)
                });
            }
        }

        // 2. Repository: html_url
        if let Some(repo_url) = html_url {
            if let Ok(url) = Url::parse(repo_url) {
                let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                leads.push(Lead {
                    title: Some(full_name.to_string()),
                    snippet: description.map(ToString::to_string),
                    kind: LeadKind::Repository,
                    owner,
                    ..Lead::page(url, ID, rank)
                });
            }
        }
    }
    Ok(leads)
}

#[async_trait]
impl SourceFinder for GitHub {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();

        let repo_id = need.subject.as_ref().and_then(|s| {
            s.identifiers.iter().find_map(|id| match id {
                Identifier::Repo { owner, name, host }
                    if host.is_empty() || host.eq_ignore_ascii_case("github.com") =>
                {
                    Some((owner.as_str(), name.as_str()))
                }
                _ => None,
            })
        });

        let url = if let (Some((owner, name)), 1) = (repo_id, page) {
            format!("{}/repos/{owner}/{name}", self.api.trim_end_matches('/'))
        } else {
            let query_term = if !need.query.trim().is_empty() {
                need.query.trim()
            } else if repo_id.is_some() {
                return answered(&self.descriptor, Vec::new(), started);
            } else {
                ""
            };
            let query = urlencoding::encode(query_term).into_owned();
            format!(
                "{}/search/repositories?q={query}&per_page={RESULTS_PER_PAGE}&page={page}",
                self.api.trim_end_matches('/')
            )
        };

        let mut headers = identity::declared_client_headers(
            self.http.contact.as_deref(),
            "application/vnd.github+json",
        );
        if let Some(token) = &self.token {
            if let Ok(val) = HeaderValue::from_str(&format!("Bearer {token}")) {
                headers.insert(AUTHORIZATION, val);
            }
        }

        let body = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(body) => body,
            Fetched::Status { code: 404, .. } if repo_id.is_some() => {
                return answered(&self.descriptor, Vec::new(), started);
            }
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
