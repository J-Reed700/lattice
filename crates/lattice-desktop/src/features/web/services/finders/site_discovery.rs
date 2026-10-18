//! Owner site discovery: crawling robots.txt, sitemaps and llms.txt on an
//! owner's origin to discover and score official documentation leads. Design §3.4, §3.10.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FinderDescriptor, FinderId, FinderOutcome, Identity, Lead, LeadKind,
    NeedKind, Origin, Privacy, RatePolicy, Reach, Role, SourceNeed, WaveSlot,
};
use url::Url;

use super::{answered, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const ID: FinderId = FinderId::from_static("site_discovery");
pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 1024 * 1024;

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "Owner site discovery",
    finds: &[
        Role {
            kind: NeedKind::OfficialDocs,
            wave: WaveSlot::First,
        },
        Role {
            kind: NeedKind::Package,
            wave: WaveSlot::Second,
        },
    ],
    reaches: Reach::OwnerSites,
    auth: Auth::None,
    privacy: Privacy::AnonymousQuery,
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://www.rfc-editor.org/rfc/rfc9309",
    },
    supports_scope: true,
    identity: Identity::DeclaredClient,
    sends_to: Destination::SubjectOwner,
};

pub(crate) struct SiteDiscovery {
    http: FinderHttp,
    scheme: &'static str,
    descriptor: FinderDescriptor,
}

impl SiteDiscovery {
    pub(crate) fn new(http: FinderHttp) -> Self {
        Self {
            http,
            scheme: "https",
            descriptor: DESCRIPTOR,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_scheme_for_test(http: FinderHttp, scheme: &'static str) -> Self {
        Self {
            http,
            scheme,
            descriptor: DESCRIPTOR,
        }
    }
}

struct RobotsTxt {
    disallow_rules: Vec<String>,
    sitemaps: Vec<String>,
}

impl RobotsTxt {
    fn parse(text: &str) -> Self {
        let mut disallow_rules = Vec::new();
        let mut sitemaps = Vec::new();
        let mut applies_to_us = false;

        for line in text.lines() {
            let line = line.split_once('#').map_or(line, |(b, _)| b).trim();
            if line.is_empty() {
                continue;
            }
            if let Some((key, val)) = line.split_once(':') {
                let key = key.trim().to_ascii_lowercase();
                let val = val.trim();
                if key == "user-agent" {
                    let agent = val.to_ascii_lowercase();
                    applies_to_us = agent == "*" || agent == "lattice";
                } else if key == "disallow" && applies_to_us && !val.is_empty() {
                    disallow_rules.push(val.to_string());
                } else if key == "sitemap" && !val.is_empty() {
                    sitemaps.push(val.to_string());
                }
            }
        }
        Self {
            disallow_rules,
            sitemaps,
        }
    }

    fn is_disallowed(&self, path: &str) -> bool {
        self.disallow_rules.iter().any(|r| path.starts_with(r))
    }
}

#[derive(Clone)]
struct Candidate {
    url: Url,
    title: Option<String>,
}

fn is_covered(scope: &Origin, url: &Url) -> bool {
    if scope.covers(url) {
        return true;
    }
    let host_no_port = scope.host().split(':').next().unwrap_or(scope.host());
    if host_no_port != scope.host() {
        return Origin::new(host_no_port, scope.path_prefix()).covers(url);
    }
    false
}

fn parse_llms_txt(body: &str, base_url: &Url) -> Vec<(Url, Option<String>)> {
    let mut links = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((_, after_open)) = line.split_once('[') {
            if let Some((title_raw, after_close)) = after_open.split_once("](") {
                if let Some((target_raw, _)) = after_close.split_once(')') {
                    let title = title_raw.trim().to_string();
                    let target = target_raw.trim();
                    if let Ok(resolved) = base_url.join(target) {
                        links.push((resolved, (!title.is_empty()).then_some(title)));
                    }
                }
            }
        }
        for word in line.split_whitespace() {
            let clean = word.trim_matches(|c: char| {
                !c.is_alphanumeric() && c != '/' && c != ':' && c != '.' && c != '-' && c != '_'
            });
            if clean.starts_with("http://") || clean.starts_with("https://") {
                if let Ok(parsed) = Url::parse(clean) {
                    if !links.iter().any(|(u, _)| u == &parsed) {
                        links.push((parsed, None));
                    }
                }
            }
        }
    }
    links
}

fn parse_sitemap_xml(xml: &str) -> Vec<Url> {
    let mut urls = Vec::new();
    let mut rem = xml;
    while let Some((_, after)) = rem.split_once("<loc>") {
        if let Some((loc, rest)) = after.split_once("</loc>") {
            if let Ok(parsed) = Url::parse(loc.trim()) {
                urls.push(parsed);
            }
            rem = rest;
        } else {
            break;
        }
    }
    urls
}

#[async_trait]
impl SourceFinder for SiteDiscovery {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let Some(scope) = &need.scope else {
            return answered(&self.descriptor, Vec::new(), started);
        };
        let headers = identity::declared_client_headers(self.http.contact.as_deref(), "*/*");

        // 1. Fetch robots.txt
        let robots_url = format!("{}://{}/robots.txt", self.scheme, scope.host());
        let robots = match self
            .http
            .get(&robots_url, headers.clone(), MAX_BODY_BYTES)
            .await
        {
            Fetched::Body(body) => RobotsTxt::parse(&body),
            _ => RobotsTxt {
                disallow_rules: Vec::new(),
                sitemaps: Vec::new(),
            },
        };

        let mut candidates: Vec<Candidate> = Vec::new();
        let Ok(base_url) = Url::parse(&format!("{}://{}/", self.scheme, scope.host())) else {
            return answered(&self.descriptor, Vec::new(), started);
        };

        // 2. Try llms.txt if allowed
        if !robots.is_disallowed("/llms.txt") {
            let llms_url = format!("{}://{}/llms.txt", self.scheme, scope.host());
            if let Fetched::Body(body) = self
                .http
                .get(&llms_url, headers.clone(), MAX_BODY_BYTES)
                .await
            {
                for (url, title) in parse_llms_txt(&body, &base_url) {
                    if is_covered(scope, &url) {
                        candidates.push(Candidate { url, title });
                    }
                }
            }
        }

        // 3. Try sitemap
        let sitemap_target = robots
            .sitemaps
            .first()
            .cloned()
            .unwrap_or_else(|| format!("{}://{}/sitemap.xml", self.scheme, scope.host()));

        if let Ok(sitemap_url) = Url::parse(&sitemap_target) {
            if !robots.is_disallowed(sitemap_url.path()) {
                if let Fetched::Body(xml) = self
                    .http
                    .get(&sitemap_target, headers, MAX_BODY_BYTES)
                    .await
                {
                    for url in parse_sitemap_xml(&xml) {
                        if is_covered(scope, &url) && !candidates.iter().any(|c| c.url == url) {
                            candidates.push(Candidate { url, title: None });
                        }
                    }
                }
            }
        }

        // 4. Score against need.query terms
        let query_tokens: Vec<String> = need
            .query
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();

        struct Scored {
            candidate: Candidate,
            score: i64,
        }
        let mut scored = Vec::new();
        for candidate in candidates {
            let title_lower = candidate.title.as_deref().unwrap_or("").to_lowercase();
            let path_lower = candidate.url.path().to_lowercase();
            let hits = query_tokens
                .iter()
                .filter(|t| title_lower.contains(*t) || path_lower.contains(*t))
                .count();
            if query_tokens.is_empty() || hits > 0 {
                let score = (hits as i64 * 1000) - (candidate.url.path().len() as i64);
                scored.push(Scored { candidate, score });
            }
        }
        scored.sort_by_key(|b| std::cmp::Reverse(b.score));

        let offset = page.saturating_sub(1).saturating_mul(RESULTS_PER_PAGE);
        let paged: Vec<_> = scored
            .into_iter()
            .skip(offset)
            .take(RESULTS_PER_PAGE)
            .collect();

        let mut leads = Vec::with_capacity(paged.len());
        for (i, item) in paged.into_iter().enumerate() {
            let rank = rank_of(page, RESULTS_PER_PAGE, i);
            leads.push(Lead {
                url: item.candidate.url,
                title: item.candidate.title,
                snippet: None,
                published: None,
                kind: LeadKind::OfficialSite,
                owner: Some(scope.clone()),
                rel_canonical: None,
                terms: None,
                found_by: vec![(ID, rank)],
            });
        }
        answered(&self.descriptor, leads, started)
    }
}

#[cfg(test)]
#[path = "site_discovery_tests.rs"]
mod tests;
