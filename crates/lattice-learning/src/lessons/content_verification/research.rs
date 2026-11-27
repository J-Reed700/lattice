//! Supplement pinned evidence when a lesson's claim checks expose gaps, or
//! when no saved reference covers a lesson topic before it is written.
use super::*;
use crate::lessons::trace::{self, Stage};
use crate::references::{admission, web_research};
#[cfg(test)]
use lattice_core::application::contracts::web::WebSearchInput;
use lattice_core::application::contracts::web::WebSearchResult;
use lattice_core::application::ports::WebServiceTrait;
use lattice_core::shared::http::url_identity::identity;
use lattice_core::shared::persistence::Database;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "research_tests.rs"]
pub(crate) mod tests;

const RESEARCH_KEYS: Protocol = lattice_core::protocol!("research", 1);

mod gaps;
mod query;
mod saving;
use gaps::{claim_gap as claim_gap_value, select_need_candidates, topic_gap};
pub(super) use query::select_noted;
pub use query::select_references;
#[cfg(test)]
use query::SELECT_SYSTEM;
pub(crate) use query::{selection_cost, selection_room, Hit, Input, Keep, ResearchQuery};
use saving::Saving;

#[derive(Clone)]
struct Context {
    database: Database,
    program_id: String,
    topic: String,
    web: Arc<dyn WebServiceTrait>,
    searched: Arc<Mutex<HashSet<String>>>,
}

tokio::task_local! { static CURRENT:Context; }

pub(crate) async fn run<T>(
    database: Database,
    program_id: String,
    topic: String,
    web: Option<Arc<dyn WebServiceTrait>>,
    future: impl std::future::Future<Output = T>,
) -> T {
    match web {
        Some(web) => {
            CURRENT
                .scope(
                    Context {
                        database,
                        program_id,
                        topic,
                        web,
                        searched: Default::default(),
                    },
                    future,
                )
                .await
        }
        None => future.await,
    }
}

/// Gaps the research planner judged no public search can help: `claims`
/// unresolved claims after `searched` searches already tried.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Exhausted {
    pub(super) claims: usize,
    pub(super) searched: usize,
}

impl std::fmt::Display for Exhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "No further public search can help {} unresolved statement{} ({} search{} already tried)",
            self.claims,
            if self.claims == 1 { "" } else { "s" },
            self.searched,
            if self.searched == 1 { "" } else { "es" },
        )
    }
}

/// What a research round searched and saved, and what it cost: the claim
/// ledger records it for every claim the round researched (§3 `Researched`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Searched {
    /// The queries this round sent, the user's hints first.
    pub(super) queries: Vec<String>,
    /// Source version ids this round saved.
    pub(super) sources_added: Vec<String>,
    /// Each version this round saved for a claim's gap, with the gap whose
    /// search chose it. A page a user's hint found is in no gap.
    pub(super) chosen: Vec<(usize, String)>,
    /// Pages this round fetched, whether it saved them or not.
    pub(super) fetched: usize,
    /// A claim research round's time from its start to its result over the
    /// claims it researched: what researching one claim on the web costs.
    /// `None` when no round ran.
    pub(super) per_claim: Option<Duration>,
    /// Why the round could not research, if it could not. That is not
    /// research: recorded as one, it used up each claim's one round and sent
    /// unsupported claims to be rewritten. A round with nothing to search, or
    /// no web to search it on, is research that found nothing, as before.
    pub(super) unresearched: Option<Unresearched>,
}

/// Why a research round could not research.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unresearched {
    /// Every search and page read it tried failed.
    Unreached,
    /// Its planner gave no plan it could use.
    Unplanned,
}

/// What a research round did for the lesson's gaps.
pub(super) enum Expansion<'a> {
    /// New references were saved; the gaps are checked again against them.
    Saved(Box<ReferenceCollection<'a>>, Searched),
    /// The planner found nothing public left to search for these gaps.
    Exhausted(Exhausted, Searched),
    /// No research ran, or its searches saved nothing new.
    Unchanged(Searched),
}

impl<'a> Expansion<'a> {
    /// The expanded references, when this round saved any.
    pub(super) fn saved(self) -> Option<ReferenceCollection<'a>> {
        match self {
            Self::Saved(references, _) => Some(*references),
            Self::Exhausted(..) | Self::Unchanged(_) => None,
        }
    }

    /// The round with what it cost per claim it researched.
    fn timed(mut self, per_claim: Option<Duration>) -> Self {
        match &mut self {
            Self::Saved(_, searched) | Self::Exhausted(_, searched) | Self::Unchanged(searched) => {
                searched.per_claim = per_claim;
            }
        }
        self
    }
}

impl Finding {
    /// Whether research should look for evidence for this finding: the
    /// checker found none or found it contradicted.
    pub(super) fn wants_research(&self) -> bool {
        matches!(
            self.verdict,
            ClaimVerdict::Unsupported | ClaimVerdict::Contradicted
        )
    }
}

/// Research for `findings`, the claims the ledger allows to be researched
/// (once per claim, for its life), with the user's `hints` searched verbatim
/// before the planned queries (§5.1 "Research again with a hint"). Each hint
/// comes with the finding of the claim it names, which is looked for whatever
/// its latest check found: the user asked for a source. A round whose
/// planning or selection gives no usable answer, or does not fit the window,
/// is [`Expansion::Unchanged`]: the claims stay held for review.
pub(super) async fn expand<'a>(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'a>,
    findings: &[Finding],
    hints: &[(usize, String)],
    checks: &ClaimChecks,
) -> Result<Expansion<'a>> {
    let Ok(context) = CURRENT.try_with(Clone::clone) else {
        return Ok(Expansion::Unchanged(Searched::default()));
    };
    // The round times itself from here to its result, whatever it is: the
    // searches that failed and the pages that could not be read are part of
    // what researching a claim costs.
    let started = Instant::now();
    let looked_for: Vec<Finding> = findings
        .iter()
        .enumerate()
        .filter(|(index, f)| f.wants_research() || hints.iter().any(|(at, _)| at == index))
        .map(|(_, f)| f.clone())
        .collect();
    let gaps: Vec<_> = looked_for
        .iter()
        .map(|f| {
            let missing = if f.wants_research() {
                f.reason.as_str()
            } else {
                HINTED
            };
            claim_gap_value(f, missing, &references.sources)
        })
        .collect();
    if gaps.is_empty() {
        return Ok(Expansion::Unchanged(Searched::default()));
    }
    let queries: Vec<String> = hints.iter().map(|(_, hint)| hint.clone()).collect();
    let round = Round {
        gaps: &gaps,
        hints: &queries,
        kind: Gaps::Claims,
    };
    let claims = u32::try_from(findings.len()).unwrap_or(u32::MAX);
    // Research adds evidence; it never stops the lesson. A round with no
    // usable answer leaves its claims held, as one that found nothing does:
    // a planner answer that failed validation twice, or a planning prompt
    // the window cannot hold, failed the whole lesson job before.
    let (expanded, searched) = match research(llm, references, &context, round).await {
        Ok(Expansion::Saved(expanded, searched)) => (*expanded, searched),
        Ok(unresearched) => return Ok(unresearched.timed(started.elapsed().checked_div(claims))),
        Err(error) if leaves_held(&error) => {
            tracing::warn!(%error, claims, "Claim research gave no usable answer; the claims stay held");
            return Ok(Expansion::Unchanged(Searched {
                unresearched: Some(Unresearched::Unplanned),
                ..Searched::default()
            })
            .timed(started.elapsed().checked_div(claims)));
        }
        Err(error) => return Err(error),
    };
    // Persist the claim-to-source scope before returning. If the app exits
    // between research and rechecking, recovery still reopens only the failed
    // comparisons that requested these automatically captured references.
    evidence_selection::record_research_scope(&expanded, &looked_for, &searched.chosen, checks)
        .await?;
    Ok(Expansion::Saved(Box::new(expanded), searched).timed(started.elapsed().checked_div(claims)))
}

/// Whether a failed research round only leaves its gaps open: its answer
/// was unusable or its prompt does not fit; an outage or a fault fails the job.
fn leaves_held(error: &AppError) -> bool {
    matches!(
        model_call::ErrorClass::of(error),
        model_call::ErrorClass::Unusable | model_call::ErrorClass::DoesNotFit
    )
}

/// What a claim a hint names lacks when its latest check found it supported.
const HINTED: &str = "The user asked for a source that states it.";

/// Before a lesson is written, search for references on its topics that no
/// saved reference covers. Without web research the topics simply stay
/// uncovered, and an unusable planning or selection answer leaves them so
/// too: this step adds evidence, it never stops the lesson.
pub(crate) async fn research_topics<'a>(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'a>,
    topics: &[(&str, &[String])],
) -> Result<Option<ReferenceCollection<'a>>> {
    let Ok(context) = CURRENT.try_with(Clone::clone) else {
        return Ok(None);
    };
    let gaps: Vec<_> = topics
        .iter()
        .map(|(topic, queries)| topic_gap(topic, queries))
        .collect();
    if gaps.is_empty() {
        return Ok(None);
    }
    let round = Round {
        gaps: &gaps,
        hints: &[],
        kind: Gaps::Topics,
    };
    match research(llm, references, &context, round).await {
        Err(error) if leaves_held(&error) => {
            tracing::warn!(%error, topics = gaps.len(), "Lesson topic research gave no usable answer; the topics stay uncovered");
            Ok(None)
        }
        result => result.map(Expansion::saved),
    }
}

/// The pages a round of research may fetch (claim classes design §3.6): for
/// the statements it researches, at most two each, the selector's first choice
/// among the results for the statement and, when the statement names an owner,
/// one result on the owner's site; and in all at most twice the number of
/// statements, a count known before the first request. A hint's searches are
/// held to the total alone, and a lesson's topics, which no statement stands
/// behind, to nothing.
struct Pages {
    left: usize,
    /// Per statement: whether its general page and its owner-site page are
    /// fetched.
    taken: Vec<(bool, bool)>,
}

impl Pages {
    fn of(kind: Gaps, statements: usize) -> Self {
        match kind {
            Gaps::Claims => Self {
                left: statements.saturating_mul(2),
                taken: vec![(false, false); statements],
            },
            Gaps::Topics => Self {
                left: usize::MAX,
                taken: Vec::new(),
            },
        }
    }

    /// Whether a page may still be fetched for the search of `gap`, as an
    /// owner-site page or not.
    fn allows(&self, gap: Option<usize>, owned: bool) -> bool {
        self.left > 0
            && gap
                .and_then(|gap| self.taken.get(gap))
                .is_none_or(|(general, owner)| !(if owned { *owner } else { *general }))
    }

    /// One page fetched for the search of `gap`.
    fn take(&mut self, gap: Option<usize>, owned: bool) {
        self.left = self.left.saturating_sub(1);
        if let Some((general, owner)) = gap.and_then(|gap| self.taken.get_mut(gap)) {
            *(if owned { owner } else { general }) = true;
        }
    }

    /// What the progress says before the first request.
    fn announced(&self, statements: usize) -> String {
        format!(
            "Looking up {statements} statement{} on the web, at most {} pages",
            if statements == 1 { "" } else { "s" },
            self.left
        )
    }
}

/// What a research round is asked to fill.
#[derive(Clone, Copy, PartialEq)]
enum Gaps {
    /// Claims the evidence checks could not support.
    Claims,
    /// Lesson topics no saved reference covers, before writing.
    Topics,
}

/// What one research round is asked for.
struct Round<'r> {
    gaps: &'r [Value],
    /// The user's hints, searched verbatim before the planned queries.
    hints: &'r [String],
    kind: Gaps,
}

/// Plan searches for the round's gaps, choose and capture credible results,
/// save them to the program, and reload the collection.
async fn research<'a>(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'a>,
    context: &Context,
    round: Round<'_>,
) -> Result<Expansion<'a>> {
    let Round { gaps, hints, kind } = round;
    crate::lesson_progress::phase(
        crate::lesson_progress::Phase::Research,
        match kind {
            Gaps::Claims => "Finding authoritative references for unresolved statements",
            Gaps::Topics => "Finding references for lesson topics no saved reference covers",
        },
    );
    let search_key = RESEARCH_KEYS.key(sha256_hex(
        json!({"topic":context.topic,
        "model":llm.model_name(),"policy":POLICY})
        .to_string(),
    ));
    let mut completed_queries: HashSet<String> = crate::lesson_drafts::checkpoint(&search_key)
        .await?
        .and_then(|saved| serde_json::from_value(saved).ok())
        .unwrap_or_default();
    context
        .searched
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .extend(completed_queries.iter().cloned());
    let already_searched: Vec<_> = context
        .searched
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .cloned()
        .collect();
    let planned = web_research::plan_needs(
        llm,
        &context.topic,
        gaps,
        references.catalog(llm),
        &already_searched,
    )
    .await?;
    // The user's hints are searches like the planner's, their site written
    // where they wrote it.
    let mut needs: Vec<(web_research::PlannedNeed, Option<usize>)> = Vec::new();
    for hint in hints {
        match web_research::PlannedNeed::web(hint.clone()).searches() {
            Ok(searches) => needs.extend(searches.into_iter().map(|need| (need, None))),
            Err(_) => tracing::warn!(
                chars = hint.chars().count(),
                "A research hint is not a search: no terms are left once any site: operators are lifted out"
            ),
        }
    }
    for web_research::GapNeed { gap, need } in planned {
        if !needs
            .iter()
            .any(|(existing, _)| existing.kind == need.kind && existing.line() == need.line())
        {
            needs.push((need, Some(gap)));
        }
    }
    // An empty plan is the planner's answer that public search cannot help
    // these gaps, not a failure: topics stay uncovered, and claims go on to
    // the repair that follows research, or to the user.
    trace::decided(
        Stage::Research,
        "research round",
        "planned searches",
        &format!(
            "{:?}",
            needs
                .iter()
                .map(|(need, _)| need.line())
                .collect::<Vec<_>>()
        ),
    );
    if needs.is_empty() {
        let exhausted = Exhausted {
            claims: gaps.len(),
            searched: already_searched.len(),
        };
        if kind == Gaps::Claims {
            tracing::info!(
                claims = exhausted.claims,
                already_searched = exhausted.searched,
                "Reference research planner found nothing new to search; continuing with saved references"
            );
            crate::lesson_progress::stage(format!(
                "{exhausted}; continuing with the saved references"
            ));
        }
        return Ok(Expansion::Exhausted(exhausted, Searched::default()));
    }
    let program_repo = crate::repository::LearningRepository::new(context.database.clone());
    let library =
        crate::source_library::LearningSourceLibraryRepository::new(context.database.clone());
    let mut sources = program_repo
        .verification_sources(&context.program_id)
        .await?;
    // Pages already held, without fragments: a link to a place in a saved
    // page is that page.
    let mut urls: HashSet<_> = sources
        .iter()
        .filter_map(|s| s.url.as_deref().map(identity))
        .collect();
    let mut added = 0;
    let mut searched = Searched::default();
    // Whether anything this round tried answered, and whether anything failed.
    let (mut reached, mut failed) = (false, false);
    let saving = Saving {
        context,
        program_repo: &program_repo,
        library: &library,
    };
    let mut pages = Pages::of(kind, gaps.len());
    // Said before the first request and kept on the line while the round
    // searches, so the count is what the user reads.
    let looking = match kind {
        Gaps::Claims => pages.announced(gaps.len()),
        Gaps::Topics => "Searching the web for missing lesson evidence".to_owned(),
    };
    crate::lesson_progress::stage(looking.clone());
    for (need, gap) in needs {
        let selection = select_need_candidates(
            llm,
            context,
            gaps,
            &urls,
            &mut searched,
            &need,
            gap,
            &looking,
            pages.allows(gap, false) || pages.allows(gap, true),
        )
        .await?;
        if selection.failed {
            failed = true;
        }
        if selection.reached {
            reached = true;
        }
        if !selection.reached {
            continue;
        }
        let mut capture_failed = false;
        let owner = selection.owner;
        for (result, note) in selection.selected {
            let owned =
                need.site.is_some() || super::provenance_gate::owner_hinted(&result.url, &owner);
            if !pages.allows(gap, owned) {
                trace::decided(
                    Stage::Research,
                    &format!("page {}", result.url),
                    "dropped",
                    "the round already took as many pages as it allows for this gap",
                );
                continue;
            }
            if !urls.insert(identity(&result.url)) {
                trace::decided(
                    Stage::Research,
                    &format!("page {}", result.url),
                    "dropped",
                    "the page was already taken",
                );
                continue;
            }
            pages.take(gap, owned);
            searched.fetched += 1;
            crate::lesson_progress::stage("Saving an additional reference for lesson verification");
            let captured = match web_research::capture(context.web.as_ref(), &result).await {
                Ok(Some(captured)) => captured,
                Ok(None) => {
                    trace::decided(
                        Stage::Research,
                        &format!("page {}", result.url),
                        "dropped",
                        "no readable page was captured",
                    );
                    continue;
                }
                Err(error) => {
                    tracing::warn!(%error, url = %result.url, "Lesson research page unavailable");
                    trace::decided(
                        Stage::Research,
                        &format!("page {}", result.url),
                        "dropped",
                        &format!("the page could not be fetched: {error}"),
                    );
                    capture_failed = true;
                    continue;
                }
            };
            // The page read is judged as the library judges every page it
            // holds: a login, home or listing page, a directory of links, a
            // page too short to say anything and an abstract are not sources.
            let address = captured.resolved_url.as_deref().unwrap_or(&result.url);
            if let Err(refusal) = admission::admit(address, &captured.title, &captured.text) {
                tracing::info!(url = %result.url, %refusal, "A fetched page is not a source; it is not saved");
                trace::decided(
                    Stage::Research,
                    &format!("page {}", result.url),
                    "dropped",
                    &format!("it is not a source: {refusal}"),
                );
                continue;
            }
            if let Some(version_id) = saving
                .save(&mut sources, &mut urls, &result.url, captured, Some(&note))
                .await?
            {
                let searched_for = gap
                    .and_then(|gap| gaps.get(gap))
                    .and_then(|gap| gap.get("claim").or_else(|| gap.get("topic")))
                    .and_then(Value::as_str)
                    .map_or_else(|| "a hint".to_owned(), |what| format!("\"{what}\""));
                trace::decided(
                    Stage::Research,
                    &format!("page {}", result.url),
                    "kept",
                    &format!(
                        "saved as source version {version_id} for {searched_for}; chosen because: {note}"
                    ),
                );
                if let Some(gap) = gap {
                    searched.chosen.push((gap, version_id.clone()));
                }
                searched.sources_added.push(version_id);
                added += 1;
            }
        }
        // Never checkpoint an in-flight search or a transient fetch failure as
        // completed. Captured references are already durable and deduplicated.
        if !capture_failed && !selection.completed_lines.is_empty() {
            completed_queries.extend(selection.completed_lines);
            crate::lesson_drafts::record_checkpoint(
                &search_key,
                serde_json::to_value(&completed_queries)?,
            )
            .await?;
        }
    }
    searched.unresearched = (failed && !reached).then_some(Unresearched::Unreached);
    if added == 0 && sources.len() == references.sources.len() {
        return Ok(Expansion::Unchanged(searched));
    }
    crate::lesson_progress::stage(format!(
        "Indexing {added} additional references; checking which saved comparisons can be reused"
    ));
    references
        .reopen(&sources)
        .await
        .map(|references| Expansion::Saved(Box::new(references), searched))
}
