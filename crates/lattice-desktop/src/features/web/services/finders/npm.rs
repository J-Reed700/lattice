//! npm registry finder: package search and repository/homepage lead discovery.
//!
//! Searches the npm registry via `/-/v1/search` with pagination.
//! An honest declared-client identity is used for API requests.

use std::collections::HashSet;
use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, Ecosystem, FailureCause, FinderDescriptor, FinderId, FinderOutcome,
    Identifier, Identity, Lead, LeadKind, NeedKind, Origin, PackageRef, Privacy, RatePolicy, Reach,
    Role, SourceNeed, WaveSlot,
};
use serde::Deserialize;
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const API: &str = "https://registry.npmjs.org";

pub(crate) const ID: FinderId = FinderId::from_static("npm");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "npm",
    finds: &[Role {
        kind: NeedKind::Package,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Registry(Ecosystem::Npm),
    auth: Auth::None,
    privacy: Privacy::QueryLogged { by: "npm, Inc." },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://github.com/npm/registry/blob/main/docs/REGISTRY-API.md",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("registry.npmjs.org"),
};

pub(crate) struct Npm {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl Npm {
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
struct NpmSearchResponse {
    #[serde(default)]
    objects: Vec<NpmObject>,
}

#[derive(Deserialize)]
struct NpmObject {
    package: NpmPackage,
}

#[derive(Deserialize)]
struct NpmPackage {
    name: String,
    description: Option<String>,
    links: Option<NpmLinks>,
}

#[derive(Deserialize)]
struct NpmLinks {
    npm: Option<String>,
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

fn sanitize_repo_url(s: &str) -> Option<Url> {
    let s = s.trim();
    let s = s.strip_prefix("git+").unwrap_or(s);
    if let Some(rest) = s.strip_prefix("git://") {
        Url::parse(&format!("https://{rest}")).ok()
    } else {
        Url::parse(s).ok()
    }
}

/// Leads for each package in `objects`; when the need names a package, only
/// its record (a search also returns packages that merely share words).
fn leads_from_objects(
    objects: Vec<NpmObject>,
    page: usize,
    named: Option<PackageRef<'_>>,
) -> Vec<Lead> {
    let mut leads = Vec::new();
    for obj in objects {
        let pkg = obj.package;
        let name = pkg.name.trim();
        if name.is_empty() {
            continue;
        }
        if named.is_some_and(|package| !package.is_record(Ecosystem::Npm, name)) {
            continue;
        }

        let mut homepage_url = pkg
            .links
            .as_ref()
            .and_then(|l| l.homepage.as_deref())
            .and_then(|u| Url::parse(u.trim()).ok());

        let repo_url = pkg
            .links
            .as_ref()
            .and_then(|l| l.repository.as_deref())
            .and_then(sanitize_repo_url);

        if homepage_url.is_some() && homepage_url == repo_url {
            homepage_url = None;
        }

        let owner = homepage_url
            .as_ref()
            .and_then(origin_from_url)
            .or_else(|| repo_url.as_ref().and_then(origin_from_url));

        let npm_url = pkg
            .links
            .as_ref()
            .and_then(|l| l.npm.as_deref())
            .and_then(|u| Url::parse(u.trim()).ok())
            .or_else(|| Url::parse(&format!("https://www.npmjs.com/package/{name}")).ok());

        let mut seen = HashSet::new();

        if let Some(url) = homepage_url {
            if seen.insert(url.clone()) {
                let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                leads.push(Lead {
                    title: Some(name.to_string()),
                    snippet: pkg.description.clone(),
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
                    snippet: pkg.description.clone(),
                    kind: LeadKind::Repository,
                    owner: owner.clone(),
                    ..Lead::page(url, ID, rank)
                });
            }
        }

        if let Some(url) = npm_url {
            if seen.insert(url.clone()) {
                let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                leads.push(Lead {
                    title: Some(name.to_string()),
                    snippet: pkg.description.clone(),
                    kind: LeadKind::Document(Identifier::PackageName {
                        ecosystem: Ecosystem::Npm,
                        name: name.to_string(),
                    }),
                    owner: owner.clone(),
                    ..Lead::page(url, ID, rank)
                });
            }
        }
    }
    leads
}

#[async_trait]
impl SourceFinder for Npm {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let offset = rank_of(page, RESULTS_PER_PAGE, 0).saturating_sub(1);
        let named = need.package();
        let query = urlencoding::encode(named.map_or(need.query.trim(), |package| package.name));
        let url = format!(
            "{}/-/v1/search?text={query}&size={RESULTS_PER_PAGE}&from={offset}",
            self.api
        );

        let headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");
        let body = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(body) => body,
            other => return failed(&self.descriptor, cause_of(other), started),
        };

        let payload = match serde_json::from_str::<NpmSearchResponse>(&body) {
            Ok(payload) => payload,
            Err(err) => {
                return failed(
                    &self.descriptor,
                    FailureCause::Parse(err.to_string()),
                    started,
                )
            }
        };

        let leads = leads_from_objects(payload.objects, page, named);
        answered(&self.descriptor, leads, started)
    }
}
