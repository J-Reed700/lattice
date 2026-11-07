//! What an outcome means for a pattern's schedule, and which patterns the next
//! drill plants.

use serde::{Deserialize, Serialize};

use super::catalogue::{patterns_for, Family, Pattern};
use super::drill::{KeyIssue, Origin};
use super::grading::{Assessment, IssueOutcome};

/// How well a pattern was recognised, in the words recall's schedule takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PatternRating {
    Again,
    Hard,
    Good,
    Easy,
}

/// The rating an issue's outcome gives its pattern: found by the first finding
/// is Easy, found is Good, half found is Hard and missed is Again.
pub fn rating(outcome: &IssueOutcome) -> PatternRating {
    match outcome {
        IssueOutcome::Found { finding: 0 } => PatternRating::Easy,
        IssueOutcome::Found { .. } => PatternRating::Good,
        IssueOutcome::HalfFound { .. } => PatternRating::Hard,
        IssueOutcome::Missed => PatternRating::Again,
    }
}

/// The rating of one key issue's pattern in one review, in the form the schedule is given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueRating {
    /// The id of a catalogue pattern.
    pub pattern: String,
    pub rating: PatternRating,
}

/// The rating of each key issue's pattern under an assessment, in the key's order. An issue the assessment has no
/// outcome for counts as missed, as `score` counts it. A missed issue found in review gives no rating, because it was
/// never proven; one that was found or half found is rated like a planted issue.
pub fn ratings(key: &[KeyIssue], assessment: &Assessment) -> Vec<IssueRating> {
    key.iter()
        .enumerate()
        .filter_map(|(position, issue)| {
            let outcome = assessment
                .issues
                .get(position)
                .map_or(IssueOutcome::Missed, |&(outcome, _)| outcome);
            if issue.origin == Origin::FoundInReview && outcome == IssueOutcome::Missed {
                return None;
            }
            Some(IssueRating {
                pattern: issue.pattern.clone(),
                rating: rating(&outcome),
            })
        })
        .collect()
}

/// When a pattern is next due for one learner and language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatternDue {
    /// The id of a catalogue pattern.
    pub pattern: String,
    /// `None`: never practised.
    pub due_at: Option<i64>,
}

/// Where a pattern stands, in the order the next drill prefers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Standing {
    /// Practised and due: `due_at` is not after now.
    Due,
    /// Never practised.
    New,
    /// Practised and not yet due.
    Rested,
}

struct Candidate {
    pattern: &'static Pattern,
    standing: Standing,
    due_at: Option<i64>,
    /// The pattern's place among those that apply to the language.
    order: usize,
}

/// The patterns the next drill plants, `count` of them or every one that
/// applies when there are fewer.
///
/// Only patterns that apply to `language` are chosen. They are ranked patterns
/// due (`due_at <= now`), earliest first; then patterns never practised, in the
/// catalogue's order; then the rest, soonest due first. From that ranking each
/// pick is the first pattern whose family has not been picked yet, so a drill
/// interleaves families; when every family left has been picked, the families
/// start over and the pick is the first of what is left. A pattern with no
/// entry in `states` is never practised, and a pattern listed twice takes its
/// first entry. The same inputs give the same patterns in the same order.
pub fn targets(
    language: &str,
    states: &[PatternDue],
    now: i64,
    count: usize,
) -> Vec<&'static Pattern> {
    let mut candidates: Vec<Candidate> = patterns_for(language)
        .enumerate()
        .map(|(order, pattern)| {
            let due_at = states
                .iter()
                .find(|state| state.pattern == pattern.id)
                .and_then(|state| state.due_at);
            let standing = match due_at {
                None => Standing::New,
                Some(at) if at <= now => Standing::Due,
                Some(_) => Standing::Rested,
            };
            Candidate {
                pattern,
                standing,
                due_at,
                order,
            }
        })
        .collect();
    candidates.sort_by_key(|candidate| (candidate.standing, candidate.due_at, candidate.order));
    let mut chosen = Vec::new();
    let mut picked_families: Vec<Family> = Vec::new();
    while chosen.len() < count && !candidates.is_empty() {
        let next = candidates
            .iter()
            .position(|candidate| !picked_families.contains(&candidate.pattern.family));
        let at = match next {
            Some(at) => at,
            None => {
                picked_families.clear();
                0
            }
        };
        let picked = candidates.remove(at);
        picked_families.push(picked.pattern.family);
        chosen.push(picked.pattern);
    }
    chosen
}
