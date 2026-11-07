//! The MediaWiki Action API's search (`list=search`), shared by the finders of
//! the Wikimedia wikis. A wiki differs by its API root, its article address
//! and its descriptor; the request, paging, address encoding and snippet
//! cleaning are the same.

use std::time::Instant;

use lattice_core::domain::sourcing::{
    FailureCause, FinderDescriptor, FinderId, FinderOutcome, Lead,
};
use url::Url;

use super::{answered, cause_of, failed, rank_of, Fetched, FinderHttp};
use crate::features::web::services::identity;

/// Results asked for per page: MediaWiki's own default `srlimit`.
pub(crate) const RESULTS_PER_PAGE: usize = 10;

/// Largest decoded API response accepted.
pub(crate) const MAX_BODY_BYTES: usize = 1024 * 1024;

/// One wiki's search: its API root, the address its articles live under, and
/// the finder that asks it.
pub(crate) struct Search<'a> {
    pub(crate) http: &'a FinderHttp,
    pub(crate) api: &'a str,
    pub(crate) article_base: &'a str,
    pub(crate) descriptor: &'a FinderDescriptor,
}

impl Search<'_> {
    pub(crate) async fn run(&self, query: &str, page: usize) -> FinderOutcome {
        let started = Instant::now();
        let query = urlencoding::encode(query.trim()).into_owned();
        let offset = rank_of(page, RESULTS_PER_PAGE, 0).saturating_sub(1);
        let url = format!(
            "{}?action=query&format=json&utf8=1&list=search&srprop=snippet&srsearch={query}&srlimit={RESULTS_PER_PAGE}&sroffset={offset}",
            self.api
        );
        let headers =
            identity::declared_client_headers(self.http.contact.as_deref(), "application/json");
        let body = match self.http.get(&url, headers, MAX_BODY_BYTES).await {
            Fetched::Body(body) => body,
            other => return failed(self.descriptor, cause_of(other), started),
        };
        let payload = match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(payload) => payload,
            Err(error) => {
                return failed(
                    self.descriptor,
                    FailureCause::Parse(error.to_string()),
                    started,
                )
            }
        };
        match leads_from(&payload, page, self.article_base, &self.descriptor.id) {
            Ok(leads) => answered(self.descriptor, leads, started),
            Err(cause) => failed(self.descriptor, cause, started),
        }
    }
}

/// What a search payload holds: article leads, or the API's own error.
fn leads_from(
    payload: &serde_json::Value,
    page: usize,
    article_base: &str,
    id: &FinderId,
) -> Result<Vec<Lead>, FailureCause> {
    if let Some(error) = payload.get("error") {
        let code = error
            .get("code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        // `ratelimited` is the API's 429 delivered in a 200.
        return Err(if code == "ratelimited" {
            FailureCause::Status {
                code: 429,
                retry_after: None,
            }
        } else {
            FailureCause::Parse(format!("API error {code}"))
        });
    }
    let Some(items) = payload
        .get("query")
        .and_then(|query| query.get("search"))
        .and_then(serde_json::Value::as_array)
    else {
        return Err(FailureCause::Parse("no search results field".to_string()));
    };
    let mut leads = Vec::new();
    for item in items {
        let title = item
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let Ok(url) = Url::parse(&format!("{article_base}{}", title.replace(' ', "_"))) else {
            continue;
        };
        if title.is_empty() {
            continue;
        }
        let snippet = item
            .get("snippet")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .replace("<span class=\"searchmatch\">", "")
            .replace("</span>", "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let rank = rank_of(page, RESULTS_PER_PAGE, leads.len());
        leads.push(Lead {
            title: Some(title.to_string()),
            snippet: (!snippet.is_empty()).then_some(snippet),
            ..Lead::page(url, id.clone(), rank)
        });
    }
    Ok(leads)
}
