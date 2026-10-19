//! RubyGems search API (`/api/v1/search.json`).
//!
//! RubyGems asks bots to identify themselves honestly as declared clients.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, Ecosystem, FailureCause, FinderDescriptor, FinderId, FinderOutcome,
    Identifier, Identity, Lead, LeadKind, NeedKind, Origin, PackageRef, Privacy, RatePolicy, Reach,
    Role, SourceNeed, WaveSlot,
};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 30;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const API: &str = "https://rubygems.org";
pub(crate) const ID: FinderId = FinderId::from_static("rubygems");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "RubyGems",
    finds: &[Role {
        kind: NeedKind::Package,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Registry(Ecosystem::RubyGems),
    auth: Auth::None,
    privacy: Privacy::QueryLogged { by: "RubyGems.org" },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://guides.rubygems.org/rubygems-org-api/",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("rubygems.org"),
};

pub(crate) struct RubyGems {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl RubyGems {
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

fn clean_str(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

fn derive_owner(
    homepage: Option<&str>,
    source_code: Option<&str>,
    docs: Option<&str>,
    name: &str,
) -> Option<Origin> {
    homepage
        .and_then(Origin::parse)
        .or_else(|| source_code.and_then(Origin::parse))
        .or_else(|| docs.and_then(Origin::parse))
        .or_else(|| Origin::parse(&format!("https://rubygems.org/gems/{name}")))
}

/// Leads for each gem in `payload`; when the need names a package, only its
/// record (a search also returns gems that merely share words).
fn leads_from(
    payload: &serde_json::Value,
    page: usize,
    named: Option<PackageRef<'_>>,
) -> Result<Vec<Lead>, FailureCause> {
    let Some(items) = payload.as_array() else {
        return Err(FailureCause::Parse("expected array of gems".to_string()));
    };

    let mut leads = Vec::new();
    for gem in items {
        let Some(name) = clean_str(gem.get("name").and_then(serde_json::Value::as_str)) else {
            continue;
        };
        if named.is_some_and(|package| !package.is_record(Ecosystem::RubyGems, name)) {
            continue;
        }
        let info = clean_str(gem.get("info").and_then(serde_json::Value::as_str));
        let homepage_uri = clean_str(gem.get("homepage_uri").and_then(serde_json::Value::as_str));
        let documentation_uri = clean_str(
            gem.get("documentation_uri")
                .and_then(serde_json::Value::as_str),
        );
        let source_code_uri = clean_str(
            gem.get("source_code_uri")
                .and_then(serde_json::Value::as_str),
        );
        let project_uri = clean_str(gem.get("project_uri").and_then(serde_json::Value::as_str));

        let owner = derive_owner(homepage_uri, source_code_uri, documentation_uri, name);

        // 1. OfficialSite: homepage_uri falling back to documentation_uri
        if let Some(site_str) = homepage_uri.or(documentation_uri) {
            if let Ok(url) = Url::parse(site_str) {
                let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                leads.push(Lead {
                    title: Some(name.to_string()),
                    snippet: info.map(ToString::to_string),
                    kind: LeadKind::OfficialSite,
                    owner: owner.clone(),
                    ..Lead::page(url, ID, rank)
                });
            }
        }

        // 2. Repository: source_code_uri
        if let Some(repo_str) = source_code_uri {
            if let Ok(url) = Url::parse(repo_str) {
                let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
                leads.push(Lead {
                    title: Some(format!("{name} repository")),
                    snippet: info.map(ToString::to_string),
                    kind: LeadKind::Repository,
                    owner: owner.clone(),
                    ..Lead::page(url, ID, rank)
                });
            }
        }

        // 3. Document: project_uri or https://rubygems.org/gems/{name}
        let doc_target = project_uri
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("https://rubygems.org/gems/{name}"));
        if let Ok(url) = Url::parse(&doc_target) {
            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                title: Some(format!("{name} on RubyGems")),
                snippet: info.map(ToString::to_string),
                kind: LeadKind::Document(Identifier::PackageName {
                    ecosystem: Ecosystem::RubyGems,
                    name: name.to_string(),
                }),
                owner,
                ..Lead::page(url, ID, rank)
            });
        }
    }
    Ok(leads)
}

#[async_trait]
impl SourceFinder for RubyGems {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let named = need.package();
        let query_term = named.map_or(need.query.trim(), |package| package.name);
        let query = urlencoding::encode(query_term).into_owned();
        let url = format!("{}/api/v1/search.json?query={query}&page={page}", self.api);
        let headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");
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
        match leads_from(&payload, page, named) {
            Ok(leads) => answered(&self.descriptor, leads, started),
            Err(cause) => failed(&self.descriptor, cause, started),
        }
    }
}
