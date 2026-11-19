//! The outline review protocol: a verdict for every open finding, new
//! findings only at the paths this pass may touch, and nothing about
//! citations, which code has already settled. Findings have stable ids so a
//! verdict can close them and a repeat can be merged instead of re-raised.
use crate::lessons::structured_call::{Asking, CallKind, ReasonFirst};
use crate::outline_review_scope::ReviewScope;
use crate::{
    dto::*, outline_draft::*, outline_progress::OutlineProgress,
    reference_collection::ReferenceCollection,
};
use lattice_core::application::services::claim_verification::ClaimVerdict;
use lattice_core::application::services::model_contract::ask::{self, ask_new};
use lattice_core::application::services::model_contract::shown::{Internal, Scope, Told};
use lattice_core::{application::ports::LLMPort, shared::error::Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;

mod contract;
#[cfg(test)]
pub(crate) mod scripted;

use contract::{Category, Entry, Finding, Input, OutlineReview, Place, Status, REVIEW_DRAFT_BRIEF};

// Keep learner-facing numbering explicit in every review and repair prompt.
// JSON pointers and prerequisite indices are implementation positions, not the
// module numbers displayed by CourseSyllabus.
pub(crate) fn module_numbering(candidate: &Value) -> Value {
    json!({
        "rule": "Learner-facing module numbers start at 1. JSON pointers, moduleIndex and prerequisiteIndices start at 0. For example, /modules/1 is Module 2; a reference there to Module 1 points to the previous module, not itself. Prefer module titles in learner-facing references. Do not request renumbering based on JSON array indices.",
        "modules": candidate["modules"].as_array().into_iter().flatten().enumerate().map(|(index, module)| json!({"path":format!("/modules/{index}"),"moduleIndex":index,"moduleNumber":index+1,"title":module["title"]})).collect::<Vec<_>>()
    })
}

pub(crate) fn kind_name(kind: LearningOutlineIssueKind) -> &'static str {
    match kind {
        LearningOutlineIssueKind::Factual => "factual",
        LearningOutlineIssueKind::Plan => "plan",
        LearningOutlineIssueKind::References => "references",
    }
}

/// The text an item-level path names: a module summary, an outcome or a
/// lesson objective. Module and course paths name no single item.
pub(crate) fn item_text(candidate: &Value, path: &str) -> Option<String> {
    let item = candidate.pointer(path)?;
    let mut parts = path.trim_start_matches("/modules/").split('/').skip(1);
    let field = match parts.next()? {
        "summary" | "outcomes" => "text",
        "lessons" => "objective",
        _ => return None,
    };
    item.get(field)?.as_str().map(str::to_owned)
}

/// The text a finding at `path` is about: the item's text, or at a module its
/// title and project.
fn claim_of(candidate: &Value, path: &str) -> Option<String> {
    module_index(path)?;
    item_text(candidate, path).or_else(|| path_text(candidate, path))
}

/// Where a factual finding can be researched and evidenced: a module summary
/// (plan prose that may carry factual framing) or the module itself (its title
/// and project). Outcomes and lesson objectives are learning-plan items, so
/// findings there are never factual.
pub(crate) fn factual_path(path: &str) -> bool {
    module_index(path).is_some() && !path.contains("/outcomes/") && !path.contains("/lessons/")
}

/// The text a finding at `path` is about, for locating a quoted phrase:
/// summary and outcome text, a lesson's title and objective, or a module's
/// title and project.
fn path_text(candidate: &Value, path: &str) -> Option<String> {
    let item = candidate.pointer(path)?;
    if path.contains("/lessons/") {
        return Some(format!(
            "{} {}",
            item.get("title")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            item.get("objective")
                .and_then(Value::as_str)
                .unwrap_or_default()
        ));
    }
    if path.ends_with("/summary") || path.contains("/outcomes/") {
        return item.get("text").and_then(Value::as_str).map(str::to_owned);
    }
    Some(format!(
        "{} {}",
        item.get("title")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        item.get("project")
            .map(Value::to_string)
            .unwrap_or_default()
    ))
}

/// The phrases a reason quotes: text between matching double, single or
/// curly quotes or backticks that start and end at word boundaries, so an
/// apostrophe inside a word is not a quote.
fn quoted_phrases(reason: &str) -> Vec<String> {
    let pairs = [
        ('"', '"'),
        ('\'', '\''),
        ('`', '`'),
        ('\u{201c}', '\u{201d}'),
        ('\u{2018}', '\u{2019}'),
    ];
    let chars: Vec<char> = reason.chars().collect();
    let boundary = |index: Option<&char>| index.is_none_or(|c| !c.is_alphanumeric());
    let mut phrases = Vec::new();
    for (open, close) in pairs {
        let mut index = 0;
        while index < chars.len() {
            let starts = chars.get(index) == Some(&open)
                && boundary(index.checked_sub(1).and_then(|i| chars.get(i)));
            if !starts {
                index += 1;
                continue;
            }
            let end = (index + 1..chars.len())
                .find(|end| chars.get(*end) == Some(&close) && boundary(chars.get(end + 1)));
            let Some(end) = end else {
                break;
            };
            let phrase: String = chars
                .get(index + 1..end)
                .unwrap_or_default()
                .iter()
                .collect();
            let phrase = phrase.trim();
            if phrase.chars().count() >= 3 && !phrase.contains('\n') {
                phrases.push(phrase.to_owned());
            }
            index = end + 1;
        }
    }
    phrases
}

/// Where a finding about a quoted phrase belongs.
#[derive(Debug, PartialEq)]
enum Placement {
    /// At its own path, or nowhere better is known.
    Keep,
    /// The one other text of the module that contains the phrase.
    Move(String),
    /// Several other texts of the module contain the phrase.
    Several(Vec<String>),
}

/// A finding whose reason quotes a phrase its path's text does not contain
/// belongs where the phrase is: the one other text of the same module that
/// contains it (case and spacing aside). Several such texts are listed for the
/// repair; none leaves the finding where it is.
fn placement(candidate: &Value, path: &str, reason: &str) -> Placement {
    let Some(module) = module_index(path) else {
        return Placement::Keep;
    };
    let phrases: Vec<String> = quoted_phrases(reason)
        .iter()
        .map(|p| normalized(p))
        .collect();
    let here = path_text(candidate, path)
        .map(|text| normalized(&text))
        .unwrap_or_default();
    if phrases.is_empty() || phrases.iter().any(|phrase| here.contains(phrase)) {
        return Placement::Keep;
    }
    for phrase in &phrases {
        let elsewhere: Vec<String> = crate::outline_review_scope::module_paths(candidate, module)
            .into_iter()
            .filter(|other| other != path)
            .filter(|other| {
                path_text(candidate, other).is_some_and(|text| normalized(&text).contains(phrase))
            })
            .collect();
        match elsewhere.len() {
            0 => continue,
            1 => {
                return elsewhere
                    .into_iter()
                    .next()
                    .map_or(Placement::Keep, Placement::Move)
            }
            _ => return Placement::Several(elsewhere),
        }
    }
    Placement::Keep
}

/// Move `issue` to where the phrase its reason quotes is, or name every
/// place it is in its reason, which the repair reads.
fn place(candidate: &Value, issue: &mut LearningOutlineIssueDto) {
    match placement(candidate, &issue.path, &issue.message) {
        Placement::Keep => {}
        Placement::Move(path) => {
            tracing::info!(id = %issue.id, from = %issue.path, to = %path, "Moved an outline finding to the text its reason quotes");
            if issue.kind == LearningOutlineIssueKind::Factual && !factual_path(&path) {
                issue.kind = LearningOutlineIssueKind::Plan;
            }
            if let Some(text) = claim_of(candidate, &path) {
                issue.claim = text;
            }
            issue.path = path;
        }
        Placement::Several(paths) => {
            issue.message = format!(
                "{} The quoted phrase appears at {}; correct it wherever it appears.",
                issue.message,
                paths.join(", ")
            );
        }
    }
}

fn normalized(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_text(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(text) => text.contains(needle),
        Value::Array(items) => items.iter().any(|item| contains_text(item, needle)),
        Value::Object(object) => object.values().any(|item| contains_text(item, needle)),
        _ => false,
    }
}

/// Assertions a strict judge found supported whose text, and attached
/// evidence, are still in the candidate. The reviewer is told about them and
/// code drops a factual finding that names one again.
pub(crate) fn verified_assertions(
    draft: &OutlineDraft,
    sources: &[LearningSourceDto],
) -> Vec<String> {
    let mut verified: Vec<String> = draft
        .claim_receipts
        .values()
        .filter(|receipt| receipt.verdict == ClaimVerdict::Supported)
        .filter(|receipt| match item_text(&draft.candidate, &receipt.path) {
            Some(text) => {
                text == receipt.claim
                    && match &receipt.source_id {
                        None => true,
                        Some(id) => {
                            let field = draft.candidate.pointer(&receipt.path);
                            let cited = field
                                .and_then(|field| field["sourceIndex"].as_u64())
                                .and_then(|index| sources.get(index as usize))
                                .map(|source| &source.id);
                            cited == Some(id)
                                && field.and_then(|field| field["quote"].as_str())
                                    == receipt.supporting_quote.as_deref()
                        }
                    }
            }
            None => draft
                .candidate
                .pointer(&receipt.path)
                .is_some_and(|module| contains_text(module, &receipt.claim)),
        })
        .map(|receipt| receipt.claim.clone())
        .collect();
    verified.sort();
    verified.dedup();
    verified
}

/// Apply a reviewed answer to the draft: close resolved findings, refresh
/// unresolved ones, convert and dedupe new ones. An open finding without a
/// verdict stays open unchanged and is asked about on the next pass. An answer
/// not read to its end closes nothing.
fn apply(
    draft: &mut OutlineDraft,
    entries: Vec<Entry>,
    complete: bool,
    sources: &[LearningSourceDto],
) {
    let open = draft.open_findings().count();
    let mut verdicts: BTreeMap<String, (Status, String)> = BTreeMap::new();
    let mut raised = Vec::new();
    for entry in entries {
        match entry {
            Entry::Verdict { at, status, reason } => {
                verdicts.entry(at.reference).or_insert((status, reason));
            }
            Entry::Found {
                place,
                category,
                reason,
            } => raised.push((place.reference, category, reason)),
        }
    }
    if !complete {
        verdicts.retain(|_, (status, _)| *status == Status::Unresolved);
    }
    if verdicts.len() != open {
        tracing::warn!(
            missing = open.saturating_sub(verdicts.len()),
            "The outline review omitted verdicts; those findings stay open for the next pass"
        );
    }
    let candidate = draft.candidate.clone();
    let mut kept = Vec::new();
    for mut issue in std::mem::take(&mut draft.review.issues) {
        if issue.kind == LearningOutlineIssueKind::References {
            kept.push(issue);
            continue;
        }
        let Some((status, reason)) = verdicts.remove(&issue.id) else {
            kept.push(issue);
            continue;
        };
        match status {
            Status::Resolved => {
                tracing::info!(id = %issue.id, path = %issue.path, "Outline finding resolved");
            }
            Status::Unresolved => {
                issue.message = reason.trim().to_owned();
                if let Some(text) = claim_of(&candidate, &issue.path) {
                    issue.claim = text;
                }
                place(&candidate, &mut issue);
                kept.push(issue);
            }
        }
    }
    draft.review.issues = kept;
    let verified: Vec<String> = verified_assertions(draft, sources)
        .iter()
        .map(|claim| normalized(claim))
        .collect();
    for (path, category, reason) in raised {
        let kind = match category {
            Category::Plan => LearningOutlineIssueKind::Plan,
            Category::Factual if factual_path(&path) => LearningOutlineIssueKind::Factual,
            Category::Factual => {
                tracing::info!(path = %path, "A factual finding on a plan item was recorded as a plan finding");
                LearningOutlineIssueKind::Plan
            }
        };
        let Some(claim) = claim_of(&draft.candidate, &path) else {
            continue;
        };
        if kind == LearningOutlineIssueKind::Factual && verified.contains(&normalized(&claim)) {
            tracing::info!(path = %path, "Dropped a factual finding on an assertion already verified against saved references");
            continue;
        }
        if let Some(existing) = draft
            .review
            .issues
            .iter_mut()
            .find(|issue| issue.path == path && normalized(&issue.claim) == normalized(&claim))
        {
            existing.message = reason.trim().to_owned();
            continue;
        }
        let id = draft.next_finding_id();
        let mut issue = LearningOutlineIssueDto {
            id,
            path,
            kind,
            claim,
            message: reason.trim().to_owned(),
            attempts: 0,
            exhausted: false,
            quote: String::new(),
            source_id: None,
            searched: vec![],
        };
        place(&draft.candidate, &mut issue);
        if draft.review.issues.iter().any(|existing| {
            existing.path == issue.path && normalized(&existing.claim) == normalized(&issue.claim)
        }) {
            continue;
        }
        draft.review.issues.push(issue);
    }
}

/// The findings still open, as the reviewer is shown them.
fn open_of(draft: &OutlineDraft) -> Vec<Finding> {
    draft
        .open_findings()
        .map(|issue| Finding {
            id: Internal::of(issue.id.clone()),
            path: Told(issue.path.clone()),
            kind: Told(kind_name(issue.kind).to_owned()),
            claim: Told(issue.claim.clone()),
            reason: Told(issue.message.clone()),
            attempts: Told(issue.attempts),
        })
        .collect()
}

/// One review pass over `scope`. Open findings get verdicts, new findings are
/// recorded, and the scope's receipt is kept so unchanged modules are not
/// reviewed again while their inputs match.
pub(crate) async fn review(
    llm: &dyn LLMPort,
    draft: &mut OutlineDraft,
    sources: &[LearningSourceDto],
    progress: &OutlineProgress,
    scope: ReviewScope,
) -> Result<()> {
    // The reviewer judges each open finding against the current text at its
    // path, so that is the claim it is shown.
    let candidate = draft.candidate.clone();
    for issue in &mut draft.review.issues {
        if let Some(text) = claim_of(&candidate, &issue.path) {
            issue.claim = text;
        }
    }
    let references = ReferenceCollection::lexical(sources)?;
    let mut queries = crate::outline_evidence::lesson_queries(&scope.candidate(&draft.candidate)?);
    queries.extend(draft.open_findings().map(|issue| issue.claim.clone()));
    let passages = crate::outline_evidence::select(llm, &references, &queries).await?;
    let places: Vec<Place> = scope
        .allowed_paths(&draft.candidate)
        .into_iter()
        .map(|path| Place { path: Told(path) })
        .collect();
    let open = open_of(draft);
    let verified = verified_assertions(draft, sources);
    let total = draft
        .candidate
        .get("modules")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let mut payload = json!({"goal":draft.request.goal,"priorKnowledge":draft.request.prior_knowledge,"minutesPerSession":draft.request.minutes_per_session,"candidate":draft.candidate,"moduleNumbering":module_numbering(&draft.candidate),"sources":passages});
    if let (false, Some(object)) = (verified.is_empty(), payload.as_object_mut()) {
        object.insert("verifiedAssertions".into(), json!(verified));
    }
    scope.add_context(&mut payload, &draft.candidate)?;
    // Verified citations are settled by code; the reviewer never re-litigates them.
    for key in ["candidate", "modulesToReview", "courseSequence"] {
        if let Some(value) = payload.get_mut(key) {
            crate::outline_review_scope::strip_citations(value);
        }
    }
    progress.detail(format!(
        "Reviewing {} of {} modules with {} open {}",
        scope.indices.len(),
        total,
        open.len(),
        if open.len() == 1 {
            "finding"
        } else {
            "findings"
        }
    ));
    tracing::info!(
        incremental = scope.incremental,
        modules_to_review = scope.indices.len(),
        total_modules = total,
        open_findings = open.len(),
        verified_assertions = verified.len(),
        "Reviewing outline with completed checks retained where valid"
    );
    let found = ask_new::<OutlineReview>(
        llm,
        &Asking {
            progress: Some(progress),
            reasoning: Some(ReasonFirst {
                kind: CallKind::Authoring,
                subject: "the outline review",
                brief: REVIEW_DRAFT_BRIEF,
            }),
        },
        &mut Scope::for_question(),
        Input {
            context: Told(payload),
            open,
            places,
        },
        |_| None,
        &crate::model_call::answer_options(),
    )
    .await?;
    apply(draft, found.items, found.rest.is_none(), sources);
    if let Some(why) = &found.rest {
        if draft.open_findings().next().is_none() {
            return Err(ask::failed(why));
        }
        return Ok(());
    }
    draft.completed_review = Some(scope.receipt);
    Ok(())
}

#[cfg(test)]
mod placement_tests {
    use super::*;

    /// Module 2 of the accept4 run, where a finding about "index seeks" was
    /// filed against the B-tree outcome.
    fn candidate() -> Value {
        json!({"modules":[{},{},{
            "title":"PostgreSQL performance",
            "summary":{"text":"Tune Postgres queries for transactional and analytical workloads."},
            "outcomes":[
                {"text":"Write complex queries using CTEs, window functions, and recursive patterns."},
                {"text":"Analyze query execution plans to identify bottlenecks (full table scans vs. index seeks)."},
                {"text":"Design B-tree and partial indexes with covering columns for efficient Postgres queries."}
            ],
            "lessons":[{"title":"Reading plans","objective":"Read EXPLAIN output for a slow query."}]
        }]})
    }

    const REASON: &str = "The text at /modules/2/outcomes/2 still uses the term 'index seeks'. This is Microsoft SQL Server terminology. PostgreSQL execution plans use terms like 'Index Scan' or 'Bitmap Index Scan', not 'Index Seek'.";

    fn issue(path: &str, message: &str) -> LearningOutlineIssueDto {
        LearningOutlineIssueDto {
            id: "finding-2".into(),
            path: path.into(),
            kind: LearningOutlineIssueKind::Plan,
            claim: String::new(),
            message: message.into(),
            attempts: 1,
            exhausted: false,
            quote: String::new(),
            source_id: None,
            searched: vec![],
        }
    }

    #[test]
    fn a_finding_moves_to_the_one_text_that_holds_the_phrase_it_quotes() {
        let mut finding = issue("/modules/2/outcomes/2", REASON);
        place(&candidate(), &mut finding);
        assert_eq!(finding.path, "/modules/2/outcomes/1");
        assert!(finding.claim.contains("index seeks"));
    }

    #[test]
    fn every_text_that_holds_the_phrase_is_named_and_none_leaves_it_in_place() {
        let mut several = candidate();
        several["modules"][2]["lessons"][0]["objective"] =
            json!("Compare index seeks with sequential scans in EXPLAIN output.");
        let mut finding = issue("/modules/2/outcomes/2", REASON);
        place(&several, &mut finding);
        assert_eq!(finding.path, "/modules/2/outcomes/2");
        assert!(finding
            .message
            .ends_with("appears at /modules/2/outcomes/1, /modules/2/lessons/0; correct it wherever it appears."));

        let mut nowhere = issue(
            "/modules/2/outcomes/2",
            "The term 'clustered index' is SQL Server terminology.",
        );
        place(&candidate(), &mut nowhere);
        assert_eq!(nowhere.path, "/modules/2/outcomes/2");
        assert_eq!(
            nowhere.message,
            "The term 'clustered index' is SQL Server terminology."
        );
    }

    #[test]
    fn apostrophes_are_not_quotes() {
        assert_eq!(
            quoted_phrases("The module's outcome uses `index seeks` and Postgres' plans."),
            vec!["index seeks".to_owned()]
        );
    }
}

#[cfg(test)]
mod apply_tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::super::outline_reference_tests::candidate;
    use super::*;
    use crate::lessons::course_generation_tests::request;
    use lattice_core::application::services::model_contract::shape::{
        Ended, Found, Listed, New, Why, Written,
    };

    fn open_issue(id: &str, path: &str) -> LearningOutlineIssueDto {
        LearningOutlineIssueDto {
            id: id.into(),
            path: path.into(),
            kind: LearningOutlineIssueKind::Plan,
            claim: String::new(),
            message: "The earlier reason.".into(),
            attempts: 1,
            exhausted: false,
            quote: String::new(),
            source_id: None,
            searched: vec![],
        }
    }

    fn draft_with(paths: &[&str]) -> OutlineDraft {
        let mut draft = OutlineDraft::new(request(LearningCourseDepth::Course), candidate(), &[]);
        draft.review.issues = paths
            .iter()
            .enumerate()
            .map(|(at, path)| open_issue(&format!("finding-{}", at + 1), path))
            .collect();
        draft.findings_raised = paths.len() as u32;
        draft
    }

    fn places_of(draft: &OutlineDraft) -> Vec<Place> {
        crate::outline_review_scope::module_paths(&draft.candidate, 0)
            .into_iter()
            .chain(crate::outline_review_scope::module_paths(
                &draft.candidate,
                1,
            ))
            .map(|path| Place { path: Told(path) })
            .collect()
    }

    fn place_at(draft: &OutlineDraft, path: &str) -> String {
        let at = places_of(draft)
            .iter()
            .position(|place| place.path.0 == path)
            .unwrap();
        format!("p{}", at + 1)
    }

    fn verdict(at: &str, status: &str, reason: &str) -> Value {
        json!({"entry":{"kind":"verdict","at":at,"status":status,"reason":reason}})
    }

    fn found(place: &str, category: &str, reason: &str) -> Value {
        json!({"entry":{"kind":"finding","place":place,"category":category,"reason":reason}})
    }

    fn entries(draft: &OutlineDraft, items: Vec<Value>) -> Vec<Entry> {
        let open = open_of(draft);
        let places = places_of(draft);
        let mut scope = Scope::for_question();
        let mut showing = scope.request();
        showing.show(&open).unwrap();
        showing.show(&places).unwrap();
        let text = json!({ "items": items }).to_string();
        let read = <New<Entry> as Listed>::read(
            &Written {
                text: &text,
                ended: Ended::Complete,
            },
            &showing,
        );
        assert_eq!(read.rest, None);
        read.items
    }

    #[test]
    fn a_resolved_verdict_closes_a_finding_and_an_unresolved_one_refreshes_it() {
        let mut draft = draft_with(&["/modules/0/lessons/0", "/modules/0/lessons/1"]);
        let answer = entries(
            &draft,
            vec![
                verdict("f1", "resolved", "The objective now names the tide."),
                verdict(
                    "f2",
                    "unresolved",
                    "The objective still names no measurement.",
                ),
            ],
        );
        apply(&mut draft, answer, true, &[]);
        assert_eq!(draft.review.issues.len(), 1);
        let kept = &draft.review.issues[0];
        assert_eq!(kept.id, "finding-2");
        assert_eq!(kept.message, "The objective still names no measurement.");
        assert_eq!(
            Some(kept.claim.clone()),
            item_text(&draft.candidate, "/modules/0/lessons/1")
        );
    }

    #[test]
    fn an_open_finding_without_a_verdict_stays_open_unchanged() {
        let mut draft = draft_with(&["/modules/0/lessons/0"]);
        let answer = entries(&draft, vec![]);
        apply(&mut draft, answer, true, &[]);
        assert_eq!(draft.review.issues.len(), 1);
        assert_eq!(draft.review.issues[0].message, "The earlier reason.");
    }

    #[test]
    fn the_first_verdict_for_a_finding_counts() {
        let mut draft = draft_with(&["/modules/0/lessons/0"]);
        let answer = entries(
            &draft,
            vec![
                verdict("f1", "unresolved", "The first judgement."),
                verdict("f1", "resolved", "The second judgement."),
            ],
        );
        apply(&mut draft, answer, true, &[]);
        assert_eq!(draft.review.issues.len(), 1);
        assert_eq!(draft.review.issues[0].message, "The first judgement.");
    }

    #[test]
    fn a_new_finding_is_filed_with_the_text_of_its_place_without_the_model_writing_it() {
        let mut draft = draft_with(&[]);
        let lesson = place_at(&draft, "/modules/0/lessons/0");
        let module = place_at(&draft, "/modules/1");
        let answer = entries(
            &draft,
            vec![
                found(&lesson, "plan", "The objective names no measurement."),
                found(&module, "plan", "The project repeats the last one."),
            ],
        );
        apply(&mut draft, answer, true, &[]);
        assert_eq!(draft.review.issues.len(), 2);
        let first = &draft.review.issues[0];
        assert_eq!(first.id, "finding-1");
        assert_eq!(first.path, "/modules/0/lessons/0");
        assert_eq!(
            Some(first.claim.clone()),
            item_text(&draft.candidate, "/modules/0/lessons/0")
        );
        let second = &draft.review.issues[1];
        assert_eq!(second.path, "/modules/1");
        assert!(second
            .claim
            .contains(draft.candidate["modules"][1]["title"].as_str().unwrap()));
    }

    #[test]
    fn a_factual_finding_on_a_plan_item_is_recorded_as_a_plan_finding() {
        let mut draft = draft_with(&[]);
        let lesson = place_at(&draft, "/modules/0/lessons/0");
        let answer = entries(
            &draft,
            vec![found(
                &lesson,
                "factual",
                "No single design suits every question.",
            )],
        );
        apply(&mut draft, answer, true, &[]);
        assert_eq!(draft.review.issues.len(), 1);
        assert_eq!(draft.review.issues[0].kind, LearningOutlineIssueKind::Plan);
    }

    #[test]
    fn an_answer_not_read_to_its_end_closes_nothing_but_keeps_what_it_found() {
        let mut draft = draft_with(&["/modules/0/lessons/0"]);
        let summary = place_at(&draft, "/modules/0/summary");
        let cut = Found {
            items: entries(
                &draft,
                vec![
                    verdict("f1", "resolved", "The objective now names the tide."),
                    found(&summary, "plan", "The summary repeats the lesson."),
                ],
            ),
            rest: Some(Why::CutOff),
            left_out: 0,
            unread_why: None,
        };
        apply(&mut draft, cut.items, cut.rest.is_none(), &[]);
        let paths: Vec<&str> = draft
            .review
            .issues
            .iter()
            .map(|issue| issue.path.as_str())
            .collect();
        assert_eq!(paths, vec!["/modules/0/lessons/0", "/modules/0/summary"]);
    }
}
