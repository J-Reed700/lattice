//! NuGet package registry finder: package search and projectUrl lead discovery.
//!
//! Searches the NuGet catalog via Azure Search (`/query?q=...`).
//! An honest declared-client identity is used for API requests.

use std::collections::HashSet;
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
const API: &str = "https://azuresearch-usnc.nuget.org";

pub(crate) const ID: FinderId = FinderId::from_static("nuget");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "NuGet",
    finds: &[Role {
        kind: NeedKind::Package,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Registry(Ecosystem::NuGet),
    auth: Auth::None,
    privacy: Privacy::QueryLogged {
        by: "the .NET Foundation",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://learn.microsoft.com/en-us/nuget/api/search-query-service-resource",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("azuresearch-usnc.nuget.org"),
};

pub(crate) struct Nuget {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl Nuget {
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
struct NugetResponse {
    #[serde(default)]
    data: Vec<NugetItem>,
}

#[derive(Deserialize)]
struct NugetItem {
    id: String,
    description: Option<String>,
    #[serde(rename = "projectUrl")]
    project_url: Option<String>,
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

fn leads_from_items(data: Vec<NugetItem>, page: usize) -> Vec<Lead> {
    let mut leads = Vec::new();
    for item in data {
        let id = item.id.trim();
        if id.is_empty() {
            continue;
        }

        let project_url = item
            .project_url
            .as_deref()
            .and_then(|u| Url::parse(u.trim()).ok());

        let owner = project_url.as_ref().and_then(origin_from_url);

        let mut seen = HashSet::new();

        if let Some(url) = project_url {
            if seen.insert(url.clone()) {
                let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                leads.push(Lead {
                    title: Some(id.to_string()),
                    snippet: item.description.clone(),
                    kind: LeadKind::OfficialSite,
                    owner: owner.clone(),
                    ..Lead::page(url, ID, rank)
                });
            }
        }

        if let Ok(pkg_url) = Url::parse(&format!("https://www.nuget.org/packages/{id}")) {
            if seen.insert(pkg_url.clone()) {
                let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                leads.push(Lead {
                    title: Some(id.to_string()),
                    snippet: item.description.clone(),
                    kind: LeadKind::Document(Identifier::PackageName {
                        ecosystem: Ecosystem::NuGet,
                        name: id.to_string(),
                    }),
                    owner: owner.clone(),
                    ..Lead::page(pkg_url, ID, rank)
                });
            }
        }
    }
    leads
}

#[async_trait]
impl SourceFinder for Nuget {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let offset = rank_of(page, RESULTS_PER_PAGE, 0).saturating_sub(1);
        let query = urlencoding::encode(need.query.trim());
        let url = format!(
            "{}/query?q={query}&take={RESULTS_PER_PAGE}&skip={offset}&prerelease=false",
            self.api
        );

        let headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");
        let body = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(body) => body,
            other => return failed(&self.descriptor, cause_of(other), started),
        };

        let payload = match serde_json::from_str::<NugetResponse>(&body) {
            Ok(payload) => payload,
            Err(err) => {
                return failed(
                    &self.descriptor,
                    FailureCause::Parse(err.to_string()),
                    started,
                )
            }
        };

        let leads = leads_from_items(payload.data, page);
        answered(&self.descriptor, leads, started)
    }
}
