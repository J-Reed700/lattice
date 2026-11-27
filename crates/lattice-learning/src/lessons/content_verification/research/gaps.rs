//! Gap metadata formatting, independent-source filtering, and alternative-query
//! fallback for lesson reference research.
use super::*;
use crate::lessons::company_framing;
use crate::lessons::content_verification::provenance_gate::FindingTrust;
use lattice_core::domain::provenance::{ClaimKind, Decision, Missing, NextStep, TemporalScope};
use std::collections::BTreeSet;

/// Builds the planner/selector JSON gap object for `finding`, including any
/// trust-profile metadata (claim kind, subject, owner hints, independence, and
/// staleness) so `web_research::plan_needs` can target primary sources directly.
/// When `finding.trust` is absent (`no_source` claim gaps), infers `kind` and
/// a public `subject` without leaking ungrounded company names.
pub(super) fn claim_gap(
    finding: &Finding,
    missing_reason: &str,
    sources: &[LearningSourceDto],
    topic: &str,
    companies: &[String],
) -> Value {
    let mut gap = serde_json::Map::new();
    gap.insert("claim".into(), json!(finding.statement));
    gap.insert("missingEvidence".into(), json!(missing_reason));

    let Some(trust) = &finding.trust else {
        let (kind, subject) = infer_gap_metadata(&finding.statement, topic, companies);
        gap.insert("kind".into(), json!(kind.as_str()));
        if let Some(subject) = subject {
            gap.insert("subject".into(), json!(subject));
        }
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

/// Returns true when a `no_source` finding (`trust: None`) only expresses an
/// invented scenario narrative or proprietary claim about an ungrounded external
/// company without making a checkable public protocol, API, or specification
/// assertion.
pub(super) fn is_scenario_local_gap(finding: &Finding, companies: &[String]) -> bool {
    if finding.trust.is_some() {
        return false;
    }
    let statement = finding.statement.trim();
    if has_checkable_spec_or_code(statement) {
        return false;
    }
    if !company_framing::company_framing_faults(statement, companies).is_empty() {
        return true;
    }
    let lower = statement.replace('\u{2019}', "'").to_ascii_lowercase();
    const SCENARIO_CUES: &[&str] = &[
        "for this example",
        "in this scenario",
        "fictionalized",
        "fictional",
        "our delivery service",
        "our connector",
        "we receive ",
        "the clinic's",
        "the example clinic",
        "suppose ",
    ];
    SCENARIO_CUES.iter().any(|cue| lower.contains(cue))
}

fn has_checkable_spec_or_code(statement: &str) -> bool {
    if statement.contains('`') || statement.contains("://") {
        return true;
    }
    let has_snake_or_dot = statement.split_whitespace().any(|raw| {
        let token = raw.trim_matches(|ch: char| {
            !ch.is_ascii_alphanumeric() && !matches!(ch, '_' | '.' | '/' | '-')
        });
        (token.contains('_') && token.chars().any(|ch| ch.is_ascii_alphabetic()))
            || (token.contains('.')
                && !token.ends_with('.')
                && token.split('.').all(|part| {
                    !part.is_empty()
                        && part
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
                }))
            || (token.starts_with('/') && token.len() > 1)
            || token.starts_with("system/")
            || token.starts_with("patient/")
            || token.starts_with("user/")
    });
    if has_snake_or_dot {
        return true;
    }
    let lower = statement.to_ascii_lowercase();
    const SPEC_TERMS: &[&str] = &[
        "rfc ",
        "ietf",
        "w3c",
        "hl7",
        "smart",
        "oauth",
        "oidc",
        "jwt",
        "jwk",
        "jwks",
        "jws",
        "jwe",
        "mllp",
        "rs256",
        "es384",
        "tls",
        "https",
        "client_assertion",
        "grant_type",
        "expires_in",
        "token_endpoint",
        "well-known",
        "specification",
        "standard",
    ];
    SPEC_TERMS.iter().any(|term| {
        if term.contains(' ') {
            lower.contains(term)
        } else {
            company_framing::contains_whole_phrase(&lower, term)
        }
    })
}

fn infer_gap_metadata(
    statement: &str,
    topic: &str,
    companies: &[String],
) -> (ClaimKind, Option<String>) {
    let lower = statement.to_ascii_lowercase();
    let kind = if contains_version_signal(&lower) {
        ClaimKind::VersionBound
    } else if has_checkable_spec_or_code(statement)
        || [
            "scope",
            "scopes",
            "endpoint",
            "header",
            "parameter",
            "payload",
            "token",
            "signature",
            "key",
            "algorithm",
            "status code",
            "protocol",
            "schema",
            "api",
            "method",
            "function",
        ]
        .iter()
        .any(|term| company_framing::contains_whole_phrase(&lower, term))
    {
        ClaimKind::Specification
    } else if contains_measurement_signal(&lower) {
        ClaimKind::Measurement
    } else if [" is defined as ", " means ", " refers to "]
        .iter()
        .any(|cue| lower.contains(cue))
    {
        ClaimKind::Definition
    } else {
        ClaimKind::General
    };
    let subject = extract_public_subject(statement, companies)
        .or_else(|| extract_public_subject(topic, companies));
    (kind, subject)
}

fn contains_version_signal(lower: &str) -> bool {
    [
        "version ",
        "since v",
        "deprecated",
        "introduced in",
        "added in",
    ]
    .iter()
    .any(|cue| lower.contains(cue))
        || lower.split_whitespace().any(|word| {
            let trimmed = word.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.');
            let body = trimmed.strip_prefix('v').unwrap_or(trimmed);
            body.contains('.')
                && body.chars().any(|ch| ch.is_ascii_digit())
                && body.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
        })
}

fn contains_measurement_signal(lower: &str) -> bool {
    const UNITS: &[&str] = &[
        "%", "ms", "second", "seconds", "minute", "minutes", "hour", "hours", "byte", "bytes",
        "kb", "mb", "gb",
    ];
    lower.chars().any(|ch| ch.is_ascii_digit())
        && UNITS.iter().any(|unit| {
            if *unit == "%" {
                lower.contains('%')
            } else {
                company_framing::contains_whole_phrase(lower, unit)
            }
        })
}

fn extract_public_subject(text: &str, companies: &[String]) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if let Some(at) = lower.find("rfc ") {
        let digits: String = text[at + 4..]
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect();
        if !digits.is_empty() {
            return Some(format!("RFC {digits}"));
        }
    }
    const STOP_CAPS: &[&str] = &[
        "the",
        "this",
        "that",
        "these",
        "those",
        "before",
        "after",
        "when",
        "where",
        "why",
        "how",
        "if",
        "now",
        "more",
        "notice",
        "here",
        "for",
        "in",
        "at",
        "on",
        "to",
        "with",
        "by",
        "as",
        "and",
        "or",
        "but",
        "we",
        "you",
        "your",
        "our",
        "its",
        "step",
        "section",
        "module",
        "lesson",
        "scenario",
        "example",
        "prepare",
        "preparing",
        "role",
        "backend",
        "engineer",
    ];
    let is_company = |phrase: &str| {
        companies
            .iter()
            .any(|company| company_framing::contains_whole_phrase(phrase, company))
    };
    let tokens: Vec<&str> = text.split_whitespace().collect();
    for index in 0..tokens.len() {
        let clean = |raw: &str| {
            raw.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '.')
        };
        let word = clean(tokens[index]);
        let word_lower = word.to_ascii_lowercase();
        let is_tech_cap = word
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
            && !STOP_CAPS.contains(&word_lower.as_str())
            && !is_company(word);
        if !is_tech_cap {
            continue;
        }
        let mut span = vec![word];
        for next_raw in tokens.iter().skip(index + 1).take(2) {
            let next = clean(next_raw);
            let next_lower = next.to_ascii_lowercase();
            let cont = next
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_uppercase())
                && !STOP_CAPS.contains(&next_lower.as_str())
                && !is_company(next);
            if cont {
                span.push(next);
            } else {
                break;
            }
        }
        let joined = span.join(" ");
        if joined.len() >= 2 && !is_company(&joined) {
            return Some(joined);
        }
    }
    for raw in tokens {
        let trimmed = raw
            .trim_matches(|ch: char| ch == '`' || ch == '(' || ch == ')' || ch == ',' || ch == ';');
        if let Some((pkg, member)) = trimmed.split_once('.') {
            if !pkg.is_empty()
                && !member.is_empty()
                && pkg
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
                && !is_company(pkg)
            {
                return Some(pkg.to_owned());
            }
        }
    }
    None
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
