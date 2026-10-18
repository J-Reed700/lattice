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

/// Parse a tool kind string into a `NeedKind`.
fn parse_need_kind(raw: &str) -> Option<NeedKind> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "web" => Some(NeedKind::Web),
        "official_docs" | "docs" | "specification" | "version_bound" => {
            Some(NeedKind::OfficialDocs)
        }
        "package" | "software" => Some(NeedKind::Package),
        "paper" | "scholarly" | "measurement" => Some(NeedKind::Paper),
        "standard" => Some(NeedKind::Standard),
        "law" => Some(NeedKind::Law),
        "community" => Some(NeedKind::Community),
        "encyclopedia" | "wiki" | "wikipedia" => Some(NeedKind::Encyclopedia),
        "archive" => Some(NeedKind::Archive),
        "leads" => Some(NeedKind::Leads),
        _ => None,
    }
}

/// Parse an identifier string such as `crates:tokio`, `npm:react`, `rfc:9110`,
/// `celex:32016R0679`, `arxiv:1706.03762`, `github:owner/repo`, or a DOI.
fn parse_identifier(raw: &str) -> Option<lattice_core::domain::sourcing::Identifier> {
    use lattice_core::domain::sourcing::{Ecosystem, Identifier};
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Some((prefix, rest)) = s.split_once(':') {
        let p = prefix.trim().to_ascii_lowercase();
        let val = rest.trim();
        if !val.is_empty() {
            match p.as_str() {
                "crates" | "cargo" | "crates.io" => {
                    return Some(Identifier::PackageName {
                        ecosystem: Ecosystem::Crates,
                        name: val.to_owned(),
                    })
                }
                "pypi" | "pip" | "python" => {
                    return Some(Identifier::PackageName {
                        ecosystem: Ecosystem::PyPi,
                        name: val.to_owned(),
                    })
                }
                "npm" | "node" => {
                    return Some(Identifier::PackageName {
                        ecosystem: Ecosystem::Npm,
                        name: val.to_owned(),
                    })
                }
                "nuget" | "dotnet" => {
                    return Some(Identifier::PackageName {
                        ecosystem: Ecosystem::NuGet,
                        name: val.to_owned(),
                    })
                }
                "rubygems" | "gem" | "ruby" => {
                    return Some(Identifier::PackageName {
                        ecosystem: Ecosystem::RubyGems,
                        name: val.to_owned(),
                    })
                }
                "maven" | "mvn" => {
                    return Some(Identifier::PackageName {
                        ecosystem: Ecosystem::Maven,
                        name: val.to_owned(),
                    })
                }
                "go" | "golang" => {
                    return Some(Identifier::PackageName {
                        ecosystem: Ecosystem::Go,
                        name: val.to_owned(),
                    })
                }
                "rfc" => {
                    if let Ok(n) = val.parse::<u32>() {
                        return Some(Identifier::Rfc(n));
                    }
                }
                "celex" => return Some(Identifier::Celex(val.to_owned())),
                "arxiv" => return Some(Identifier::Arxiv(val.to_owned())),
                "doi" => return Some(Identifier::Doi(val.to_owned())),
                "wikidata" | "qid" => return Some(Identifier::WikidataQid(val.to_owned())),
                "github" | "gh" => {
                    if let Some((owner, name)) = val.split_once('/') {
                        return Some(Identifier::Repo {
                            host: "github.com".to_owned(),
                            owner: owner.trim().to_owned(),
                            name: name.trim().to_owned(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    if s.starts_with("10.") && s.contains('/') {
        return Some(Identifier::Doi(s.to_owned()));
    }
    if let Some(rest) = s
        .to_ascii_lowercase()
        .strip_prefix("rfc")
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
        if let Ok(n) = rest.parse::<u32>() {
            return Some(Identifier::Rfc(n));
        }
    }
    if (s.starts_with("Q") || s.starts_with("q"))
        && s.len() > 1
        && s[1..].chars().all(|c| c.is_ascii_digit())
    {
        return Some(Identifier::WikidataQid(s.to_ascii_uppercase()));
    }
    if let Ok(url) = url::Url::parse(s) {
        return Some(Identifier::Url(url));
    }
    None
}

/// The tool's input as a `SourceNeed`: the query, optional `kind`, `subject`,
/// `identifier`, `site`, `after`, and rewordings (`alternatives` + `followup_queries`).
fn web_need(
    input: &WebSearchInput,
    query: &str,
    want: usize,
) -> (SourceNeed, Option<Vec<FinderId>>) {
    use lattice_core::domain::sourcing::{Origin, SubjectHint, SubjectKind};

    let depth = input.depth.clamp(1, MAX_DEPTH);
    let branch_queries = input.branch_queries.clamp(1, MAX_BRANCH_QUERIES);
    let mut seen = HashSet::from([query.to_lowercase()]);
    let max_alts = if !input.alternatives.is_empty() {
        input.alternatives.len().max(3)
    } else {
        depth.saturating_sub(1).saturating_mul(branch_queries)
    };
    let alternatives = input
        .alternatives
        .iter()
        .chain(input.followup_queries.iter())
        .map(|followup| followup.trim())
        .filter(|followup| !followup.is_empty() && seen.insert(followup.to_lowercase()))
        .take(max_alts)
        .map(str::to_string)
        .collect();

    let kind = input
        .kind
        .as_deref()
        .and_then(parse_need_kind)
        .unwrap_or(NeedKind::Web);

    let parsed_id = input.identifier.as_deref().and_then(parse_identifier);
    let subject_name = input
        .subject
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            parsed_id.as_ref().map(|id| match id {
                lattice_core::domain::sourcing::Identifier::PackageName { name, .. } => {
                    name.clone()
                }
                lattice_core::domain::sourcing::Identifier::Repo { name, .. } => name.clone(),
                _ => query.to_owned(),
            })
        });
    let subject = subject_name.map(|name| SubjectHint {
        name,
        kind: match kind {
            NeedKind::Package | NeedKind::OfficialDocs => SubjectKind::Software,
            NeedKind::Paper => SubjectKind::Paper,
            NeedKind::Standard => SubjectKind::Standard,
            NeedKind::Law => SubjectKind::Law,
            _ => SubjectKind::Other,
        },
        identifiers: parsed_id.into_iter().collect(),
    });

    let scope = input.site.as_deref().and_then(Origin::parse);
    let freshness = input
        .after
        .as_deref()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok())
        .map(Freshness::After)
        .unwrap_or(Freshness::Any);

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
    let only = if kind != NeedKind::Web && named.is_empty() {
        // Typed source-finding need (`find_sources` or learning `plan_needs`):
        // let the router choose across the full finder registry.
        None
    } else if named.is_empty() {
        // Plain `web_search` with no engine named: the default engines, plus
        // Wikipedia when asked.
        let mut defaults = vec![duckduckgo::ID, bing_html::ID];
        if wants_wikipedia {
            defaults.push(wikipedia::ID);
        }
        Some(defaults)
    } else {
        if wants_wikipedia && !named.contains(&wikipedia::ID) {
            named.push(wikipedia::ID);
        }
        Some(named)
    };
    let need = SourceNeed {
        kind,
        query: query.to_string(),
        subject,
        scope,
        freshness,
        want,
        alternatives,
        also: if wants_wikipedia && kind == NeedKind::Web {
            vec![NeedKind::Encyclopedia]
        } else {
            Vec::new()
        },
    };
    (need, only)
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
        owner_resolution,
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
    let results: Vec<WebSearchResult> = findings
        .leads
        .get(start..end)
        .unwrap_or_default()
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
    let resolved_owner = owner_resolution
        .and_then(|res| {
            res.origins.first().map(|origin| {
                lattice_core::application::contracts::web::ResolvedOwnerDto {
                    domain: origin.to_string(),
                    resolved_by: res.resolved_by.map(|b| b.as_str().to_string()),
                }
            })
        })
        .or_else(|| {
            findings.leads.iter().find_map(|lead| {
                lead.owner.as_ref().map(|origin| {
                    lattice_core::application::contracts::web::ResolvedOwnerDto {
                        domain: origin.to_string(),
                        resolved_by: Some("registry".to_string()),
                    }
                })
            })
        });
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
        owner: resolved_owner,
    })
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
