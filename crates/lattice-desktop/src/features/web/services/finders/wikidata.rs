//! Wikidata entity and property search for owner resolution (P856 official
//! website, P1324 source repository, P2078 user manual). Design §3.4, §3.10.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identifier,
    Identity, Lead, LeadKind, NeedKind, Origin, Privacy, RatePolicy, Reach, Role, SourceNeed,
    SubjectHint, SubjectKind, WaveSlot,
};
use url::Url;

use super::{answered, cause_of, failed, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const ID: FinderId = FinderId::from_static("wikidata");
pub(crate) const RESULTS_PER_PAGE: usize = 5;

const API: &str = "https://www.wikidata.org/w/api.php";
const MAX_BODY_BYTES: usize = 1024 * 1024;

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Wikidata",
    finds: &[Role {
        kind: NeedKind::Encyclopedia,
        wave: WaveSlot::Second,
    }],
    reaches: Reach::Encyclopedia,
    auth: Auth::None,
    privacy: Privacy::QueryLogged {
        by: "the Wikimedia Foundation",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://www.mediawiki.org/wiki/API:Etiquette",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("www.wikidata.org"),
};

pub(crate) struct Wikidata {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl Wikidata {
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

    /// Resolves a subject to its owner origins by searching Wikidata
    /// entities and reading claims for P856, P2078, and P1324.
    pub(crate) async fn resolve_subject(
        &self,
        subject: &SubjectHint,
    ) -> (Vec<Origin>, FinderOutcome) {
        let started = Instant::now();
        let direct_qid = subject.identifiers.iter().find_map(|id| match id {
            Identifier::WikidataQid(qid) => Some(qid.clone()),
            _ => None,
        });

        let qids = match direct_qid {
            Some(qid) => vec![qid],
            None => {
                let name = subject.name.trim();
                if name.is_empty() {
                    return (Vec::new(), answered(&self.descriptor, Vec::new(), started));
                }
                let search_url = format!(
                    "{}?action=wbsearchentities&format=json&language=en&type=item&limit=5&search={}",
                    self.api,
                    urlencoding::encode(name)
                );
                let headers = identity::declared_client_headers(
                    self.http.contact.as_deref(),
                    "application/json",
                );
                let body = match self.http.get(&search_url, headers, MAX_BODY_BYTES).await {
                    Fetched::Body(body) => body,
                    other => {
                        return (
                            Vec::new(),
                            failed(&self.descriptor, cause_of(other), started),
                        )
                    }
                };
                let payload = match serde_json::from_str::<serde_json::Value>(&body) {
                    Ok(p) => p,
                    Err(e) => {
                        return (
                            Vec::new(),
                            failed(
                                &self.descriptor,
                                FailureCause::Parse(e.to_string()),
                                started,
                            ),
                        )
                    }
                };
                if let Some(cause) = api_error(&payload) {
                    return (Vec::new(), failed(&self.descriptor, cause, started));
                }
                let ids: Vec<String> = payload
                    .get("search")
                    .and_then(serde_json::Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|it| {
                                it.get("id")
                                    .and_then(serde_json::Value::as_str)
                                    .map(String::from)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if ids.is_empty() {
                    return (Vec::new(), answered(&self.descriptor, Vec::new(), started));
                }
                ids
            }
        };

        let ids_param = qids.join("|");
        let entities_url = format!(
            "{}?action=wbgetentities&format=json&props=claims|labels|descriptions&ids={}",
            self.api,
            urlencoding::encode(&ids_param)
        );
        let headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");
        let body = match self.http.get(&entities_url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(body) => body,
            other => {
                return (
                    Vec::new(),
                    failed(&self.descriptor, cause_of(other), started),
                )
            }
        };
        let payload = match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(p) => p,
            Err(e) => {
                return (
                    Vec::new(),
                    failed(
                        &self.descriptor,
                        FailureCause::Parse(e.to_string()),
                        started,
                    ),
                )
            }
        };
        if let Some(cause) = api_error(&payload) {
            return (Vec::new(), failed(&self.descriptor, cause, started));
        }

        let Some(entities) = payload
            .get("entities")
            .and_then(serde_json::Value::as_object)
        else {
            return (Vec::new(), answered(&self.descriptor, Vec::new(), started));
        };

        let mut origins = Vec::new();
        let mut leads = Vec::new();
        let props = [
            ("P856", LeadKind::OfficialSite),
            ("P2078", LeadKind::OfficialSite),
            ("P1324", LeadKind::Repository),
        ];

        for qid in &qids {
            let Some(entity) = entities.get(qid) else {
                continue;
            };
            let label = entity
                .get("labels")
                .and_then(|l| l.get("en"))
                .and_then(|v| v.get("value"))
                .and_then(serde_json::Value::as_str)
                .map(String::from);
            let description = entity
                .get("descriptions")
                .and_then(|d| d.get("en"))
                .and_then(|v| v.get("value"))
                .and_then(serde_json::Value::as_str)
                .map(String::from);
            let Some(claims) = entity.get("claims").and_then(serde_json::Value::as_object) else {
                continue;
            };

            for (prop, kind) in &props {
                let Some(statements) = claims.get(*prop).and_then(serde_json::Value::as_array)
                else {
                    continue;
                };
                for statement in statements {
                    let Some(val_str) = statement
                        .get("mainsnak")
                        .and_then(|m| m.get("datavalue"))
                        .and_then(|d| d.get("value"))
                        .and_then(serde_json::Value::as_str)
                    else {
                        continue;
                    };
                    let Ok(url) = Url::parse(val_str) else {
                        continue;
                    };
                    if url.scheme() != "http" && url.scheme() != "https" {
                        continue;
                    }
                    let Some(origin) = origin_from_url(&url) else {
                        continue;
                    };
                    if !origins.contains(&origin) {
                        origins.push(origin.clone());
                    }
                    let rank = leads.len() + 1;
                    leads.push(Lead {
                        url,
                        title: label.clone(),
                        snippet: description.clone(),
                        published: None,
                        kind: kind.clone(),
                        owner: Some(origin),
                        rel_canonical: None,
                        terms: None,
                        found_by: vec![(ID, rank)],
                    });
                }
            }
        }

        (origins, answered(&self.descriptor, leads, started))
    }
}

fn api_error(payload: &serde_json::Value) -> Option<FailureCause> {
    let error = payload.get("error")?;
    let code = error
        .get("code")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    Some(if code == "ratelimited" {
        FailureCause::Status {
            code: 429,
            retry_after: None,
        }
    } else {
        FailureCause::Parse(format!("API error {code}"))
    })
}

/// Derives an origin from an official or repository URL.
fn origin_from_url(url: &Url) -> Option<Origin> {
    let host = url.host_str()?;
    let host_lower = host.to_ascii_lowercase();
    if host_lower == "github.com" || host_lower == "gitlab.com" {
        let segments: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
        match segments.as_slice() {
            [owner, repo_raw, ..] => {
                let repo = repo_raw.trim_end_matches(".git");
                if repo.is_empty() {
                    Some(Origin::new(host, Some(owner)))
                } else {
                    Some(Origin::new(host, Some(&format!("{owner}/{repo}"))))
                }
            }
            [owner] => Some(Origin::new(host, Some(owner))),
            [] => Some(Origin::new(host, None)),
        }
    } else {
        Some(Origin::new(host, None))
    }
}

#[async_trait]
impl SourceFinder for Wikidata {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        if page > 1 {
            return answered(&self.descriptor, Vec::new(), started);
        }
        let subject = match &need.subject {
            Some(subject) => subject.clone(),
            None => SubjectHint {
                name: need.query.clone(),
                kind: SubjectKind::Other,
                identifiers: Vec::new(),
            },
        };
        let (_origins, outcome) = self.resolve_subject(&subject).await;
        outcome
    }
}

#[cfg(test)]
#[path = "wikidata_tests.rs"]
mod tests;
