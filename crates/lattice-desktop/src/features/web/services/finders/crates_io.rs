//! crates.io registry finder: crate metadata and documentation lookup.
//!
//! Searches crates.io or retrieves exact crate records via the public v1 API.
//! An honest declared-client identity is required by the crates.io crawler policy.

use std::collections::HashSet;
use std::time::{Duration, Instant};

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

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const API: &str = "https://crates.io";

pub(crate) const ID: FinderId = FinderId::from_static("crates_io");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "crates.io",
    finds: &[Role {
        kind: NeedKind::Package,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Registry(Ecosystem::Crates),
    auth: Auth::None,
    privacy: Privacy::QueryLogged { by: "crates.io" },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: Duration::from_secs(1),
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://crates.io/data-access",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("crates.io"),
};

pub(crate) struct CratesIo {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl CratesIo {
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
struct SingleCratePayload {
    #[serde(rename = "crate")]
    krate: Option<CrateItem>,
}

#[derive(Deserialize)]
struct SearchPayload {
    #[serde(default)]
    crates: Vec<CrateItem>,
}

#[derive(Deserialize)]
struct CrateItem {
    name: String,
    description: Option<String>,
    documentation: Option<String>,
    homepage: Option<String>,
    repository: Option<String>,
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

fn leads_from_crates(crates: Vec<CrateItem>, page: usize) -> Vec<Lead> {
    let mut leads = Vec::new();
    for krate in crates {
        let name = krate.name.trim();
        if name.is_empty() {
            continue;
        }

        let doc_url = krate
            .documentation
            .as_deref()
            .and_then(|u| Url::parse(u.trim()).ok())
            .or_else(|| Url::parse(&format!("https://docs.rs/{name}")).ok());

        let homepage_url = krate
            .homepage
            .as_deref()
            .and_then(|u| Url::parse(u.trim()).ok());
        let repo_url = krate
            .repository
            .as_deref()
            .and_then(|u| Url::parse(u.trim()).ok());

        let owner = homepage_url
            .as_ref()
            .and_then(origin_from_url)
            .or_else(|| {
                doc_url
                    .as_ref()
                    .filter(|u| {
                        u.host_str().map(|h| h.to_ascii_lowercase()) != Some("docs.rs".to_string())
                    })
                    .and_then(origin_from_url)
            })
            .or_else(|| repo_url.as_ref().and_then(origin_from_url));

        let mut seen = HashSet::new();

        if let Some(url) = doc_url {
            if seen.insert(url.clone()) {
                let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                leads.push(Lead {
                    title: Some(name.to_string()),
                    snippet: krate.description.clone(),
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
                    snippet: krate.description.clone(),
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
                    snippet: krate.description.clone(),
                    kind: LeadKind::Repository,
                    owner: owner.clone(),
                    ..Lead::page(url, ID, rank)
                });
            }
        }

        if let Ok(crate_url) = Url::parse(&format!("https://crates.io/crates/{name}")) {
            if seen.insert(crate_url.clone()) {
                let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                leads.push(Lead {
                    title: Some(name.to_string()),
                    snippet: krate.description.clone(),
                    kind: LeadKind::Document(Identifier::PackageName {
                        ecosystem: Ecosystem::Crates,
                        name: name.to_string(),
                    }),
                    owner: owner.clone(),
                    ..Lead::page(crate_url, ID, rank)
                });
            }
        }
    }
    leads
}

#[async_trait]
impl SourceFinder for CratesIo {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let exact_crate = need.subject.as_ref().and_then(|subject| {
            subject.identifiers.iter().find_map(|id| match id {
                Identifier::PackageName {
                    ecosystem: Ecosystem::Crates,
                    name,
                } => Some(name.as_str()),
                _ => None,
            })
        });

        let (url, is_single) = match exact_crate {
            Some(name) if page == 1 => {
                let encoded = urlencoding::encode(name.trim());
                (format!("{}/api/v1/crates/{encoded}", self.api), true)
            }
            _ => {
                let query = urlencoding::encode(need.query.trim());
                (
                    format!(
                        "{}/api/v1/crates?q={query}&per_page={RESULTS_PER_PAGE}&page={page}",
                        self.api
                    ),
                    false,
                )
            }
        };

        let headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");
        let body = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(body) => body,
            Fetched::Status { code: 404, .. } if is_single => {
                return answered(&self.descriptor, Vec::new(), started);
            }
            other => return failed(&self.descriptor, cause_of(other), started),
        };

        let crates = if is_single {
            match serde_json::from_str::<SingleCratePayload>(&body) {
                Ok(payload) => payload.krate.into_iter().collect(),
                Err(err) => {
                    return failed(
                        &self.descriptor,
                        FailureCause::Parse(err.to_string()),
                        started,
                    )
                }
            }
        } else {
            match serde_json::from_str::<SearchPayload>(&body) {
                Ok(payload) => payload.crates,
                Err(err) => {
                    return failed(
                        &self.descriptor,
                        FailureCause::Parse(err.to_string()),
                        started,
                    )
                }
            }
        };

        let leads = leads_from_crates(crates, page);
        answered(&self.descriptor, leads, started)
    }
}
