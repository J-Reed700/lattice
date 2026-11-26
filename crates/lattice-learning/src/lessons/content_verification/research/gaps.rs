//! Gap metadata formatting, independent-source filtering, and alternative-query
//! fallback for lesson reference research.
use super::*;
use crate::lessons::content_verification::provenance_gate::FindingTrust;
use lattice_core::domain::provenance::{ClaimKind, Decision, Missing, NextStep, TemporalScope};
use std::collections::BTreeSet;

/// Builds the planner/selector JSON gap object for `finding`, including any
/// trust-profile metadata (claim kind, subject, owner hints, independence, and
/// staleness) so `web_research::plan_needs` can target primary sources directly.
pub(super) fn claim_gap(
    finding: &Finding,
    missing_reason: &str,
    sources: &[LearningSourceDto],
) -> Value {
    let mut gap = serde_json::Map::new();
    gap.insert("claim".into(), json!(finding.statement));
    gap.insert("missingEvidence".into(), json!(missing_reason));

    let Some(trust) = &finding.trust else {
        return Value::Object(gap);
    };

    gap.insert("kind".into(), json!(trust.profile.kind.as_str()));
    if let Some(subject) = &trust.profile.subject {
        gap.insert("subject".into(), json!(subject.name));
    }
    if trust.profile.kind == ClaimKind::Measurement {
        if let Some(measured_by) = &trust.profile.measurement_origin {
            let trimmed = measured_by.trim();
            if !trimmed.is_empty() {
                gap.insert("whoMeasured".into(), json!(trimmed));
            }
        }
    }

    let hints = owner_origin_hints(trust);
    if !hints.is_empty() {
        gap.insert("ownerOriginHints".into(), json!(hints));
    }

    if let Decision::Insufficient { missing, next, .. } = &trust.decision {
        let needs_independent = matches!(
            missing,
            Missing::NoIndependentCorroboration { .. } | Missing::OnlyTestimonial
        ) || next
            .iter()
            .any(|step| matches!(step, NextStep::SearchIndependent));
        if needs_independent {
            gap.insert("needsIndependentSource".into(), json!(true));
            let sites = support_hosts(trust, sources);
            if !sites.is_empty() {
                gap.insert("independentOf".into(), json!(sites));
            }
        }

        let is_stale = matches!(missing, Missing::OnlyStale { .. })
            || next
                .iter()
                .any(|step| matches!(step, NextStep::FetchNewer { .. }));
        if is_stale {
            if let Some(day) = newer_than_day(trust) {
                gap.insert("needsNewerThan".into(), json!(day));
            }
        }
    }

    Value::Object(gap)
}

/// Builds a topic-gap JSON object, preserving any planned keyword queries from
/// `evidence::plan` so `web_research::plan_needs` builds directly on them.
pub(super) fn topic_gap(topic: &str, planned_queries: &[String]) -> Value {
    let mut gap = serde_json::Map::new();
    gap.insert("topic".into(), json!(topic));
    gap.insert(
        "missingEvidence".into(),
        json!("No saved reference covers this lesson topic"),
    );
    let cleaned: Vec<&str> = planned_queries
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .collect();
    if !cleaned.is_empty() {
        gap.insert("plannedQueries".into(), json!(cleaned));
    }
    Value::Object(gap)
}

fn owner_origin_hints(trust: &FindingTrust) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut push = |hint: &str| {
        let trimmed = hint.trim().trim_end_matches('.');
        if !trimmed.is_empty() && seen.insert(trimmed.to_ascii_lowercase()) {
            out.push(trimmed.to_owned());
        }
    };
    for support in &trust.supports {
        for hint in &support.owner_hints {
            push(hint);
        }
    }
    if let Decision::Insufficient { next, .. } = &trust.decision {
        for step in next {
            if let NextStep::SearchOwner { origin_hints } = step {
                for hint in origin_hints {
                    push(hint);
                }
            }
        }
    }
    out
}

fn support_hosts(trust: &FindingTrust, sources: &[LearningSourceDto]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut hosts = Vec::new();
    for support in &trust.supports {
        let Some(url) = sources
            .iter()
            .find(|source| source.id == support.source_version_id)
            .and_then(|source| source.url.as_deref())
        else {
            continue;
        };
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                let lower = host.to_ascii_lowercase();
                if seen.insert(lower.clone()) {
                    hosts.push(lower);
                }
            }
        }
    }
    hosts
}

fn newer_than_day(trust: &FindingTrust) -> Option<String> {
    let timestamp = trust
        .supports
        .iter()
        .filter_map(|support| support.dates.published.or(support.dates.modified))
        .max()
        .or(match trust.profile.temporal {
            TemporalScope::Since { date: Some(ts), .. } | TemporalScope::AsOf { date: ts } => {
                Some(ts)
            }
            _ => None,
        })?;
    chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
}

/// Drops search candidates whose URL host belongs to `independentOf` when the
/// served gap explicitly asked for an independent source.
pub(super) fn filter_independent(
    gap: Option<&Value>,
    candidates: Vec<web_research::Candidate>,
) -> (Vec<web_research::Candidate>, Vec<String>) {
    let needs_independent = gap
        .and_then(|g| g.get("needsIndependentSource"))
        .and_then(Value::as_bool)
        == Some(true);
    if !needs_independent {
        return (candidates, Vec::new());
    }
    let excluded: Vec<String> = gap
        .and_then(|g| g.get("independentOf"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if excluded.is_empty() {
        return (candidates, Vec::new());
    }
    let mut kept = Vec::with_capacity(candidates.len());
    let mut dropped_urls = Vec::new();
    for candidate in candidates {
        if super::super::provenance_gate::owner_hinted(&candidate.result.url, &excluded) {
            dropped_urls.push(candidate.result.url);
        } else {
            kept.push(candidate);
        }
    }
    (kept, dropped_urls)
}

/// Outcome of attempting `need` and any fallback alternative queries.
pub(super) struct NeedSelection {
    pub reached: bool,
    pub failed: bool,
    pub owner: Vec<String>,
    pub selected: Vec<(WebSearchResult, String)>,
    pub completed_lines: Vec<String>,
}

enum SearchOutcome {
    AlreadySearched,
    Unavailable,
    Found(Vec<web_research::Candidate>),
}

/// Runs `need` and, if that yields zero usable selections and alternative
/// queries exist within budget, tries those rewordings (falling back from
/// specialized registry/body kinds to general web search when empty) until
/// one yields selected candidates.
#[allow(clippy::too_many_arguments)]
pub(super) async fn select_need_candidates(
    llm: &dyn LLMPort,
    context: &Context,
    gaps: &[Value],
    urls: &HashSet<String>,
    searched: &mut Searched,
    need: &web_research::PlannedNeed,
    gap: Option<usize>,
    looking: &str,
    pages_left: bool,
) -> Result<NeedSelection> {
    let mut reached = false;
    let mut failed = false;
    let mut selected = Vec::new();
    let mut owner = Vec::new();
    let mut completed_lines = Vec::new();

    match search_once(llm, context, gaps, searched, need, looking).await {
        SearchOutcome::AlreadySearched => {}
        SearchOutcome::Unavailable => failed = true,
        SearchOutcome::Found(results) => {
            reached = true;
            completed_lines.push(search_dedup_key(need));
            (owner, selected) =
                filter_and_select(llm, context, gaps, urls, gap, &need.query, results).await?;
        }
    }

    if selected.is_empty() && pages_left && !need.alternatives.is_empty() {
        'alts: for alt in &need.alternatives {
            let mut variants = Vec::new();
            let mut primary_alt = need.clone();
            primary_alt.query = alt.clone();
            primary_alt.alternatives = Vec::new();
            variants.push(primary_alt);
            if !matches!(
                need.kind,
                lattice_core::domain::sourcing::NeedKind::Web
                    | lattice_core::domain::sourcing::NeedKind::OfficialDocs
            ) {
                let mut web_alt = need.clone();
                web_alt.query = alt.clone();
                web_alt.kind = lattice_core::domain::sourcing::NeedKind::Web;
                web_alt.identifier = None;
                web_alt.ecosystem = None;
                web_alt.alternatives = Vec::new();
                variants.push(web_alt);
            }
            for variant in variants {
                match search_once(llm, context, gaps, searched, &variant, looking).await {
                    SearchOutcome::AlreadySearched => continue,
                    SearchOutcome::Unavailable => {
                        failed = true;
                        continue;
                    }
                    SearchOutcome::Found(results) => {
                        reached = true;
                        completed_lines.push(search_dedup_key(&variant));
                        let (ov, sel) = filter_and_select(
                            llm,
                            context,
                            gaps,
                            urls,
                            gap,
                            &variant.query,
                            results,
                        )
                        .await?;
                        if !sel.is_empty() {
                            owner = ov;
                            selected = sel;
                            break 'alts;
                        }
                    }
                }
            }
        }
    }

    Ok(NeedSelection {
        reached,
        failed,
        owner,
        selected,
        completed_lines,
    })
}

fn search_dedup_key(need: &web_research::PlannedNeed) -> String {
    if need.kind == lattice_core::domain::sourcing::NeedKind::Web {
        need.line()
    } else {
        format!("[{}] {}", need.kind.as_str(), need.line())
    }
}

async fn search_once(
    llm: &dyn LLMPort,
    context: &Context,
    gaps: &[Value],
    searched: &mut Searched,
    need: &web_research::PlannedNeed,
    looking: &str,
) -> SearchOutcome {
    let line = need.line();
    let key = search_dedup_key(need);
    if !context
        .searched
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key.clone())
    {
        return SearchOutcome::AlreadySearched;
    }
    searched.queries.push(line.clone());
    crate::lesson_progress::stage(looking.to_owned());
    let cost = selection_cost(llm, gaps);
    let room = web_research::Room {
        tokens: selection_room(llm, &context.topic, &need.query, gaps),
        cost: &cost,
    };
    let results = match web_research::search_need(context.web.as_ref(), need, room).await {
        Ok(results) => results,
        Err(error) => {
            tracing::warn!(%error, "Lesson reference search unavailable");
            trace::decided(
                Stage::Research,
                &format!("search {line}"),
                "search unavailable",
                &error.to_string(),
            );
            context
                .searched
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&key);
            searched.queries.pop();
            return SearchOutcome::Unavailable;
        }
    };
    trace::decided(
        Stage::Research,
        &format!("search {line}"),
        "searched",
        &format!("{} results", results.len()),
    );
    SearchOutcome::Found(results)
}

async fn filter_and_select(
    llm: &dyn LLMPort,
    context: &Context,
    gaps: &[Value],
    urls: &HashSet<String>,
    gap: Option<usize>,
    query: &str,
    results: Vec<web_research::Candidate>,
) -> Result<(Vec<String>, Vec<(WebSearchResult, String)>)> {
    let (candidates, held): (Vec<web_research::Candidate>, Vec<web_research::Candidate>) = results
        .into_iter()
        .partition(|candidate| !urls.contains(&identity(&candidate.result.url)));
    for candidate in &held {
        trace::decided(
            Stage::Research,
            &format!("page {}", candidate.result.url),
            "dropped",
            "the course already holds this page",
        );
    }
    let served_gap = gap.and_then(|at| gaps.get(at));
    let (candidates, same_site) = filter_independent(served_gap, candidates);
    for url in &same_site {
        trace::decided(
            Stage::Research,
            &format!("page {url}"),
            "dropped",
            "from an already-held site for an independent source gap",
        );
    }
    let offered: Vec<String> = candidates
        .iter()
        .map(|candidate| candidate.result.url.clone())
        .collect();
    let owner: Vec<String> = candidates
        .iter()
        .filter_map(|candidate| candidate.owner.as_ref())
        .map(|owner| owner.domain.clone())
        .collect();
    let selected = select_noted(llm, &context.topic, query, gaps, candidates).await?;
    for url in offered
        .iter()
        .filter(|url| !selected.iter().any(|(result, _)| &result.url == *url))
    {
        trace::decided(
            Stage::Research,
            &format!("page {url}"),
            "dropped",
            "the selector did not choose it for the gap",
        );
    }
    Ok((owner, selected))
}
