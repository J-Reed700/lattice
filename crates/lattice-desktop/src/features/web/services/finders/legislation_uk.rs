//! UK legislation search through legislation.gov.uk Atom feeds.

use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderDescriptor, FinderId, FinderOutcome, Identity,
    Lead, NeedKind, Origin, Privacy, RatePolicy, Reach, Role, SourceNeed, WaveSlot,
};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::web::MIN_SITE_REQUEST_INTERVAL;

pub(crate) const RESULTS_PER_PAGE: usize = 10;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const API: &str = "https://www.legislation.gov.uk";

pub(crate) const ID: FinderId = FinderId::from_static("legislation_uk");

pub(crate) const DESCRIPTOR: FinderDescriptor = FinderDescriptor {
    id: ID,
    name: "legislation.gov.uk",
    finds: &[Role {
        kind: NeedKind::Law,
        wave: WaveSlot::First,
    }],
    reaches: Reach::Law,
    auth: Auth::None,
    privacy: Privacy::QueryLogged {
        by: "legislation.gov.uk",
    },
    cost: Cost::Free,
    rate: RatePolicy {
        min_interval: MIN_SITE_REQUEST_INTERVAL,
        quota: None,
        honours_retry_after: true,
        trip_on_first_refusal: false,
        source: "https://www.legislation.gov.uk/developer",
    },
    supports_scope: false,
    identity: Identity::DeclaredClient,
    sends_to: Destination::Host("www.legislation.gov.uk"),
};

pub(crate) struct LegislationUk {
    http: FinderHttp,
    api: String,
    descriptor: FinderDescriptor,
}

impl LegislationUk {
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

fn unescape_xml(raw: &str) -> String {
    raw.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .trim()
        .to_string()
}

fn leads_from(xml: &str, page: usize) -> Result<Vec<Lead>, FailureCause> {
    let entry_re = match regex::Regex::new(r"(?s)<entry\b[^>]*>(.*?)</entry>") {
        Ok(re) => re,
        Err(e) => return Err(FailureCause::Parse(e.to_string())),
    };
    let id_re = match regex::Regex::new(r"(?s)<id\b[^>]*>(.*?)</id>") {
        Ok(re) => re,
        Err(e) => return Err(FailureCause::Parse(e.to_string())),
    };
    let title_re = match regex::Regex::new(r"(?s)<title\b[^>]*>(.*?)</title>") {
        Ok(re) => re,
        Err(e) => return Err(FailureCause::Parse(e.to_string())),
    };
    let summary_re = match regex::Regex::new(r"(?s)<summary\b[^>]*>(.*?)</summary>") {
        Ok(re) => re,
        Err(e) => return Err(FailureCause::Parse(e.to_string())),
    };

    let mut leads = Vec::new();
    for cap in entry_re.captures_iter(xml) {
        let entry_str = cap.get(1).map_or("", |m| m.as_str());
        let id_raw = id_re
            .captures(entry_str)
            .and_then(|c| c.get(1))
            .map_or("", |m| m.as_str())
            .trim();
        let Ok(url) = Url::parse(id_raw) else {
            continue;
        };

        let title_raw = title_re
            .captures(entry_str)
            .and_then(|c| c.get(1))
            .map_or("", |m| m.as_str());
        let title = unescape_xml(title_raw);

        let summary_raw = summary_re
            .captures(entry_str)
            .and_then(|c| c.get(1))
            .map_or("", |m| m.as_str());
        let summary = unescape_xml(summary_raw);

        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
        leads.push(Lead {
            title: (!title.is_empty()).then_some(title),
            snippet: (!summary.is_empty()).then_some(summary),
            owner: Some(Origin::new("legislation.gov.uk", None)),
            ..Lead::page(url, ID, rank)
        });
    }
    Ok(leads)
}

#[async_trait]
impl SourceFinder for LegislationUk {
    fn descriptor(&self) -> &FinderDescriptor {
        &self.descriptor
    }

    async fn find(&self, need: &SourceNeed, page: usize, _budget: &Budget) -> FinderOutcome {
        let started = Instant::now();
        let query = urlencoding::encode(need.query.trim());
        let url = format!("{}/all/data.feed?title={query}&page={page}", self.api);

        let headers = identity::declared_client_headers(
            self.http.contact.as_deref(),
            "application/atom+xml, application/xml;q=0.9, */*;q=0.8",
        );
        let body = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(body) => body,
            other => return failed(&self.descriptor, cause_of(other), started),
        };

        match leads_from(&body, page) {
            Ok(leads) => answered(&self.descriptor, leads, started),
            Err(cause) => failed(&self.descriptor, cause, started),
        }
    }
}
