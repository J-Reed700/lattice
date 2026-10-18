//! Go Module Proxy (`proxy.golang.org`).
//!
//! Queries Go module metadata and emits pkg.go.dev documentation and repository leads.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, Ecosystem, FinderDescriptor, FinderId, FinderOutcome, Identifier,
    Identity, Lead, LeadKind, NeedKind, Origin, Privacy, RatePolicy, Reach, Role, SourceNeed,
    WaveSlot,
};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 1;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const API: &str = "https://proxy.golang.org";
pub(crate) const ID: FinderId = FinderId::from_static("go_module");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Go Module Proxy",
    finds: &[Role {
        kind: NeedKind::Package,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Registry(Ecosystem::Go),
    auth: Auth::None,
    privacy: Privacy::QueryLogged { by: "Google" },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://proxy.golang.org/",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("proxy.golang.org"),
};

pub(crate) struct GoModule {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl GoModule {
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

fn escape_module_path(path: &str) -> String {
    let mut escaped = String::with_capacity(path.len());
    for ch in path.chars() {
        if ch.is_ascii_uppercase() {
            escaped.push('!');
            escaped.push(ch.to_ascii_lowercase());
        } else {
            escaped.push(ch);
        }
    }
    escaped
}

#[async_trait]
impl SourceFinder for GoModule {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        if page > 1 {
            return answered(&self.descriptor, Vec::new(), started);
        }

        let module_opt = need.subject.as_ref().and_then(|s| {
            s.identifiers.iter().find_map(|id| match id {
                Identifier::PackageName {
                    ecosystem: Ecosystem::Go,
                    name,
                } => Some(name.as_str()),
                _ => None,
            })
        });
        let module = module_opt
            .unwrap_or(need.query.trim())
            .trim()
            .trim_matches('/');

        if module.is_empty() {
            return answered(&self.descriptor, Vec::new(), started);
        }

        let escaped = escape_module_path(module);
        let url = format!("{}/{escaped}/@latest", self.api.trim_end_matches('/'));
        let headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");

        let body = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(body) => body,
            Fetched::Status {
                code: 404 | 410, ..
            } => {
                return answered(&self.descriptor, Vec::new(), started);
            }
            other => return failed(&self.descriptor, cause_of(other), started),
        };

        let version = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|payload| {
                payload
                    .get("Version")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string)
            });
        let snippet = version.map(|v| format!("version {v}"));

        let github_parts = if let Some(rest) = module.strip_prefix("github.com/") {
            let mut iter = rest.split('/');
            let owner = iter.next().filter(|s| !s.is_empty());
            let repo = iter.next().filter(|s| !s.is_empty());
            match (owner, repo) {
                (Some(o), Some(r)) => Some((o, r)),
                _ => None,
            }
        } else {
            None
        };

        let (owner, repo_url) = if let Some((owner_name, repo_name)) = github_parts {
            let repo_full_name = format!("{owner_name}/{repo_name}");
            (
                Some(Origin::new("github.com", Some(&repo_full_name))),
                Url::parse(&format!("https://github.com/{repo_full_name}")).ok(),
            )
        } else {
            (Origin::parse(&format!("https://{module}")), None)
        };

        let mut leads = Vec::new();

        // 1. OfficialSite: https://pkg.go.dev/{module}
        if let Ok(pkg_url) = Url::parse(&format!("https://pkg.go.dev/{module}")) {
            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                title: Some(module.to_string()),
                snippet: snippet.clone(),
                kind: LeadKind::OfficialSite,
                owner: owner.clone(),
                ..Lead::page(pkg_url, ID, rank)
            });
        }

        // 2. Repository: github repo URL if github module
        if let Some(repo_url) = repo_url {
            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                title: Some(format!("{module} repository")),
                snippet,
                kind: LeadKind::Repository,
                owner,
                ..Lead::page(repo_url, ID, rank)
            });
        }

        answered(&self.descriptor, leads, started)
    }
}
