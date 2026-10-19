//! PyPI package registry finder: package metadata and documentation lookup.
//!
//! Retrieves JSON package records via PyPI's JSON API (`/pypi/{pkg}/json`).
//! An honest declared-client identity is used for API etiquette.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, Ecosystem, FailureCause, FinderDescriptor, FinderId, FinderOutcome,
    Identifier, Identity, Lead, LeadKind, NeedKind, Origin, Privacy, RatePolicy, Reach, Role,
    SourceNeed, WaveSlot,
};
use serde::Deserialize;
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const API: &str = "https://pypi.org";

pub(crate) const ID: FinderId = FinderId::from_static("pypi");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "PyPI",
    finds: &[Role {
        kind: NeedKind::Package,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Registry(Ecosystem::PyPi),
    auth: Auth::None,
    privacy: Privacy::QueryLogged {
        by: "the Python Software Foundation",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://docs.pypi.org/api/json/",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("pypi.org"),
};

pub(crate) struct PyPi {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl PyPi {
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

#[derive(Deserialize)]
struct PyPiPayload {
    info: PyPiInfo,
}

#[derive(Deserialize)]
struct PyPiInfo {
    name: String,
    summary: Option<String>,
    home_page: Option<String>,
    docs_url: Option<String>,
    package_url: Option<String>,
    project_urls: Option<HashMap<String, Option<String>>>,
}

fn origin_from_url(url: &Url) -> Option<Origin> {
    let host = url.host_str()?;
    let host_lower = host.to_ascii_lowercase();
    if host_lower == "github.com" || host_lower == "gitlab.com" {
        if let Some(segments) = url.path_segments() {
            let segs: Vec<&str> = segments.filter(|s| !s.is_empty()).collect();
            if let [owner, repo_raw, ..] = segs.as_slice() {
                let repo = repo_raw.trim_end_matches(".git");
                if !owner.is_empty() && !repo.is_empty() {
                    return Some(Origin::new(&host_lower, Some(&format!("{owner}/{repo}"))));
                }
            }
        }
    }
    Some(Origin::new(&host_lower, None))
}

fn leads_from_info(info: PyPiInfo, page: usize) -> Vec<Lead> {
    let name = info.name.trim();
    if name.is_empty() {
        return Vec::new();
    }

    let mut doc_url = info
        .docs_url
        .as_deref()
        .and_then(|u| Url::parse(u.trim()).ok());
    let mut homepage_url = info
        .home_page
        .as_deref()
        .and_then(|u| Url::parse(u.trim()).ok());
    let mut repo_url: Option<Url> = None;

    if let Some(urls) = &info.project_urls {
        for (label, url_opt) in urls {
            let Some(url_str) = url_opt.as_deref() else {
                continue;
            };
            let Ok(parsed) = Url::parse(url_str.trim()) else {
                continue;
            };
            let key = label.to_ascii_lowercase();
            if doc_url.is_none() && (key.contains("doc") || key.contains("documentation")) {
                doc_url = Some(parsed.clone());
            }
            if homepage_url.is_none() && (key.contains("home") || key.contains("homepage")) {
                homepage_url = Some(parsed.clone());
            }
            if repo_url.is_none()
                && (key.contains("source")
                    || key.contains("repo")
                    || key.contains("code")
                    || key.contains("github")
                    || key.contains("gitlab"))
            {
                repo_url = Some(parsed.clone());
            }
        }
    }

    if repo_url.is_none() {
        if let Some(ref hp) = homepage_url {
            let host = hp.host_str().unwrap_or("").to_ascii_lowercase();
            if host == "github.com" || host == "gitlab.com" {
                repo_url = homepage_url.take();
            }
        }
    }

    let owner = homepage_url
        .as_ref()
        .and_then(origin_from_url)
        .or_else(|| doc_url.as_ref().and_then(origin_from_url))
        .or_else(|| repo_url.as_ref().and_then(origin_from_url));

    let mut leads = Vec::new();
    let mut seen = HashSet::new();

    if let Some(url) = doc_url {
        if seen.insert(url.clone()) {
            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                title: Some(name.to_string()),
                snippet: info.summary.clone(),
                kind: LeadKind::OfficialSite,
                owner: owner.clone(),
                ..Lead::page(url, ID, rank)
            });
        }
    }

    if let Some(url) = homepage_url {
        if seen.insert(url.clone()) {
            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                title: Some(name.to_string()),
                snippet: info.summary.clone(),
                kind: LeadKind::OfficialSite,
                owner: owner.clone(),
                ..Lead::page(url, ID, rank)
            });
        }
    }

    if let Some(url) = repo_url {
        if seen.insert(url.clone()) {
            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                title: Some(name.to_string()),
                snippet: info.summary.clone(),
                kind: LeadKind::Repository,
                owner: owner.clone(),
                ..Lead::page(url, ID, rank)
            });
        }
    }

    let pkg_url = info
        .package_url
        .as_deref()
        .and_then(|u| Url::parse(u.trim()).ok())
        .or_else(|| Url::parse(&format!("https://pypi.org/project/{name}/")).ok());

    if let Some(url) = pkg_url {
        if seen.insert(url.clone()) {
            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                title: Some(name.to_string()),
                snippet: info.summary.clone(),
                kind: LeadKind::Document(Identifier::PackageName {
                    ecosystem: Ecosystem::PyPi,
                    name: name.to_string(),
                }),
                owner: owner.clone(),
                ..Lead::page(url, ID, rank)
            });
        }
    }

    leads
}

#[async_trait]
impl SourceFinder for PyPi {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        if page > 1 {
            return answered(&self.descriptor, Vec::new(), started);
        }

        // PyPI looks a project up by name and has no search, so a need that
        // names no package (or another registry's) has nothing to ask it.
        let pkg_name = need
            .package()
            .filter(|package| {
                package
                    .ecosystem
                    .is_none_or(|named| named == Ecosystem::PyPi)
            })
            .map(|package| package.name);
        let Some(pkg) = pkg_name else {
            return answered(&self.descriptor, Vec::new(), started);
        };

        let encoded = urlencoding::encode(pkg.trim());
        let url = format!("{}/pypi/{encoded}/json", self.api);
        let headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");

        let body = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(body) => body,
            Fetched::Status { code: 404, .. } => {
                return answered(&self.descriptor, Vec::new(), started);
            }
            other => return failed(&self.descriptor, cause_of(other), started),
        };

        let payload = match serde_json::from_str::<PyPiPayload>(&body) {
            Ok(payload) => payload,
            Err(err) => {
                return failed(
                    &self.descriptor,
                    FailureCause::Parse(err.to_string()),
                    started,
                )
            }
        };

        let leads = leads_from_info(payload.info, page);
        answered(&self.descriptor, leads, started)
    }
}
