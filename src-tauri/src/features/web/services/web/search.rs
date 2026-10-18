//! `search_web` as a thin call to source finding: the chat tool's input
//! becomes a `Web` need, and the findings become the tool's output with
//! every finder's outcome in it.

use std::collections::HashSet;
use std::time::Instant;

use lattice_core::application::contracts::web::{
    FinderOutcomeDto, WebSearchInput, WebSearchOutput, WebSearchResult,
};
use lattice_core::application::ports::Budget;
use lattice_core::domain::sourcing::{FinderId, Freshness, NeedKind, SourceNeed, SourcingPolicy};
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::WebService;
use crate::features::web::services::finders::{bing_html, duckduckgo, wikipedia};
use crate::features::web::services::sourcing::Search;

/// The chat tool's own bounds on its arguments (its schema states them).
const MAX_RESULTS_PER_PAGE: usize = 50;
const MAX_DEPTH: usize = 4;
const MAX_BRANCH_QUERIES: usize = 4;

impl WebService {
    pub(super) async fn search_through_sourcing(
        &self,
        input: &WebSearchInput,
    ) -> Result<WebSearchOutput> {
        let query = input.query.trim();
        if query.is_empty() {
            return Err(AppError::InvalidInput(
                "Web search query cannot be empty".to_string(),
            ));
        }
        let max_results = input.max_results.clamp(1, MAX_RESULTS_PER_PAGE);
        let page = input.page.max(1);
        let offset = input
            .offset
            .saturating_add(page.saturating_sub(1).saturating_mul(max_results));
        let (need, only) = web_need(input, query, offset.saturating_add(max_results));
        let started = Instant::now();
        let search = self
            .sourcing
            .find_with(need, deadline_less_budget(), only.as_deref())
            .await?;
        let output = search_output(query, page, offset, max_results, search)?;
        info!(
            ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            results = output.result_count,
            pool = output.total_results,
            outcomes = ?output
                .outcomes
                .iter()
                .map(|outcome| format!("{}={}", outcome.finder, outcome.result))
                .collect::<Vec<_>>(),
            "Web search completed"
        );
        Ok(output)
    }
}

/// The budget of a `search_web` call. The port carries no deadline: the
/// tool loop bounds the call by the turn's deadline and Stop (it drops the
/// call), and the lesson loop by its job. So the search's own deadline is
/// the longest any breaker holds a host (`SourcingPolicy::breaker_ceiling`):
/// a host's `Retry-After` is waited out whenever the call still needs leads
/// and the caller is still waiting, and never past the caller's own end.
fn deadline_less_budget() -> Budget {
    let ceiling = SourcingPolicy::default().breaker_ceiling;
    let now = Instant::now();
    Budget {
        deadline: now.checked_add(ceiling).unwrap_or(now),
        stop: CancellationToken::new(),
        requests: None,
    }
}

/// The finder a provider name in the tool's `providers` means.
fn finder_for_provider(name: &str) -> Option<FinderId> {
    match name.trim().to_ascii_lowercase().as_str() {
        "ddg" | "duckduckgo" | "duckduckgo_html" => Some(duckduckgo::ID),
        "bing" | "bing_html" => Some(bing_html::ID),
        "wiki" | "wikipedia" => Some(wikipedia::ID),
        _ => None,
    }
}

/// The tool's input as a `Web` need: the query, the caller's follow-ups as
/// rewordings (as many as its depth and branching allow), Wikipedia when
/// asked, and `want` = everything up to the end of the requested page. The
/// finders it named, if any, limit which run.
fn web_need(
    input: &WebSearchInput,
    query: &str,
    want: usize,
) -> (SourceNeed, Option<Vec<FinderId>>) {
    let depth = input.depth.clamp(1, MAX_DEPTH);
    let branch_queries = input.branch_queries.clamp(1, MAX_BRANCH_QUERIES);
    let mut seen = HashSet::from([query.to_lowercase()]);
    let alternatives = input
        .followup_queries
        .iter()
        .map(|followup| followup.trim())
        .filter(|followup| !followup.is_empty() && seen.insert(followup.to_lowercase()))
        .take(depth.saturating_sub(1).saturating_mul(branch_queries))
        .map(str::to_string)
        .collect();

    let mut named: Vec<FinderId> = Vec::new();
    for finder in input
        .providers
        .iter()
        .filter_map(|name| finder_for_provider(name))
    {
        if !named.contains(&finder) {
            named.push(finder);
        }
    }
    let wants_wikipedia = input.include_wikipedia || named.contains(&wikipedia::ID);
    let only = if named.is_empty() {
        // No engine named: the default engines, plus Wikipedia when asked.
        let mut defaults = vec![duckduckgo::ID, bing_html::ID];
        if wants_wikipedia {
            defaults.push(wikipedia::ID);
        }
        defaults
    } else {
        if wants_wikipedia && !named.contains(&wikipedia::ID) {
            named.push(wikipedia::ID);
        }
        named
    };
    let need = SourceNeed {
        kind: NeedKind::Web,
        query: query.to_string(),
        subject: None,
        scope: None,
        freshness: Freshness::Any,
        want,
        alternatives,
        also: if wants_wikipedia {
            vec![NeedKind::Encyclopedia]
        } else {
            Vec::new()
        },
    };
    (need, Some(only))
}

/// The findings as the tool's output: the requested page sliced from the
/// fused pool, and one outcome per finder.
fn search_output(
    query: &str,
    page: usize,
    offset: usize,
    max_results: usize,
    search: Search,
) -> Result<WebSearchOutput> {
    let Search {
        findings,
        alternatives_run,
    } = search;
    let outcomes: Vec<FinderOutcomeDto> = findings
        .outcomes
        .iter()
        .map(FinderOutcomeDto::from)
        .collect();
    if findings.leads.is_empty() {
        let said: Vec<String> = outcomes
            .iter()
            .map(|outcome| format!("{}: {}", outcome.finder, outcome.result))
            .collect();
        return Err(AppError::Network(format!(
            "The search found nothing ({})",
            said.join(", ")
        )));
    }
    let total = findings.leads.len();
    let domains = findings
        .leads
        .iter()
        .filter_map(|lead| lead.url.host_str())
        .map(|host| host.trim_start_matches("www.").to_ascii_lowercase())
        .collect::<HashSet<_>>()
        .len();
    let start = offset.min(total);
    let end = start.saturating_add(max_results).min(total);
    let results: Vec<WebSearchResult> = findings.leads[start..end]
        .iter()
        .map(|lead| WebSearchResult {
            title: lead.title.clone().unwrap_or_else(|| lead.url.to_string()),
            url: lead.url.to_string(),
            snippet: lead.snippet.clone().unwrap_or_default(),
            published_date: lead.published,
            source: lead.found_by.first().map(|(finder, _)| finder.to_string()),
        })
        .collect();
    let providers_used = findings
        .outcomes
        .iter()
        .filter(|outcome| !outcome.leads.is_empty())
        .map(|outcome| outcome.finder.to_string())
        .collect();
    let has_more = end < total || !findings.exhausted;
    Ok(WebSearchOutput {
        result_count: results.len(),
        results,
        query: query.to_string(),
        page,
        offset,
        total_results: total,
        has_more,
        providers_used,
        unique_query_count: 1 + alternatives_run.len(),
        unique_url_count: total,
        unique_domain_count: domains,
        followup_queries: alternatives_run,
        outcomes,
    })
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
