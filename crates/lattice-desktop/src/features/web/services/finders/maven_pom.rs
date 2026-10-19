//! Maven Central search API (`/solrsearch/select`).
//!
//! Sonatype Solr search over artifacts in Maven Central.

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

pub(crate) const RESULTS_PER_PAGE: usize = 20;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const API: &str = "https://search.maven.org";
pub(crate) const ID: FinderId = FinderId::from_static("maven_pom");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Maven Central",
    finds: &[Role {
        kind: NeedKind::Package,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Registry(Ecosystem::Maven),
    auth: Auth::None,
    privacy: Privacy::QueryLogged { by: "Sonatype" },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://central.sonatype.org/search/rest-api-guide/",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("search.maven.org"),
};

pub(crate) struct MavenPom {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl MavenPom {
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

/// Leads for each artifact in `payload`; when the need names a package, only
/// its record (a search also returns artifacts that merely share words).
fn leads_from(
    payload: &serde_json::Value,
    page: usize,
    named: Option<PackageRef<'_>>,
) -> Result<Vec<Lead>, FailureCause> {
    let Some(docs) = payload
        .get("response")
        .and_then(|r| r.get("docs"))
        .and_then(serde_json::Value::as_array)
    else {
        return Err(FailureCause::Parse(
            "no response.docs array in solr payload".to_string(),
        ));
    };

    let mut leads = Vec::new();
    for doc in docs {
        let g = clean_str(doc.get("g").and_then(serde_json::Value::as_str));
        let a = clean_str(doc.get("a").and_then(serde_json::Value::as_str));
        let (Some(g), Some(a)) = (g, a) else {
            continue;
        };
        if named.is_some_and(|package| !package.is_record(Ecosystem::Maven, &format!("{g}:{a}"))) {
            continue;
        }
        let latest_version =
            clean_str(doc.get("latestVersion").and_then(serde_json::Value::as_str));
        let snippet = latest_version.map(|v| format!("version {v}"));
        let owner = Some(Origin::new("javadoc.io", Some(&format!("doc/{g}/{a}"))));

        // 1. OfficialSite: https://javadoc.io/doc/{g}/{a}
        if let Ok(url) = Url::parse(&format!("https://javadoc.io/doc/{g}/{a}")) {
            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                title: Some(format!("{g}:{a}")),
                snippet: snippet.clone(),
                kind: LeadKind::OfficialSite,
                owner: owner.clone(),
                ..Lead::page(url, ID, rank)
            });
        }

        // 2. Document: https://central.sonatype.com/artifact/{g}/{a}
        if let Ok(url) = Url::parse(&format!("https://central.sonatype.com/artifact/{g}/{a}")) {
            let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
            leads.push(Lead {
                title: Some(format!("{g}:{a} on Maven Central")),
                snippet,
                kind: LeadKind::Document(Identifier::PackageName {
                    ecosystem: Ecosystem::Maven,
                    name: format!("{g}:{a}"),
                }),
                owner,
                ..Lead::page(url, ID, rank)
            });
        }
    }
    Ok(leads)
}

#[async_trait]
impl SourceFinder for MavenPom {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        // A named artifact is searched by its coordinates, so the search
        // finds it by name rather than by words.
        let named = need.package();
        let query_term = match named.map(|package| package.name) {
            Some(name) => match name.split_once(':') {
                Some((group, artifact)) => format!("g:\"{group}\" AND a:\"{artifact}\""),
                None => format!("a:\"{name}\""),
            },
            None => need.query.trim().to_string(),
        };
        let query = urlencoding::encode(&query_term).into_owned();
        let offset = rank_of(page, RESULTS_PER_PAGE, 0).saturating_sub(1);
        let url = format!(
            "{}/solrsearch/select?q={query}&rows={RESULTS_PER_PAGE}&start={offset}&wt=json",
            self.api
        );
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
