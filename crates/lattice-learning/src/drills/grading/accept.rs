//! The checks a judge's answer passes before anyone sees it: one entry for every issue and every finding, a reason for
//! each, the reviewer's own words quoted exactly for a find, and an assignment `drills::validate` accepts. An answer
//! that fails says what to change, so that asking again fixes it.

use lattice_core::domain::learning::drills::{
    display_order, validate, Assessment, AssessmentFault, Finding, FindingOutcome, IssueOutcome,
};
use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;

use crate::drills::grader::Review;
use crate::grade_rationale::verbatim;

/// The judge's answer as it was written, before any check. Its numbers count from 1, as the learner's do.
#[derive(Deserialize)]
struct Answer {
    issues: Vec<IssueEntry>,
    findings: Vec<FindingEntry>,
}

#[derive(Deserialize)]
struct IssueEntry {
    issue: i64,
    reason: String,
    outcome: IssueVerdict,
    finding: Option<i64>,
    quote: Option<String>,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum IssueVerdict {
    Found,
    HalfFound,
    Missed,
}

#[derive(Deserialize)]
struct FindingEntry {
    finding: i64,
    reason: String,
    outcome: FindingVerdict,
    issues: Option<Vec<i64>>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum FindingVerdict {
    Matches,
    ValidExtra,
    FalsePositive,
}

/// How a message names one kind of thing the answer gives entries for.
struct Kind {
    title: &'static str,
    article: &'static str,
    singular: &'static str,
    plural: &'static str,
    owner: &'static str,
}

const ISSUE: Kind = Kind {
    title: "Issue",
    article: "an",
    singular: "issue",
    plural: "issues",
    owner: "the key",
};

const FINDING: Kind = Kind {
    title: "Finding",
    article: "a",
    singular: "finding",
    plural: "findings",
    owner: "the review",
};

impl Kind {
    /// Which of them there are: "issues 1 to 3", "issue 1 only", "no issues".
    fn numbered(&self, count: usize) -> String {
        match count {
            0 => format!("no {}", self.plural),
            1 => format!("{} 1 only", self.singular),
            count => format!("{} 1 to {count}", self.plural),
        }
    }
}

/// The judge's answer as an assessment of `review`, or the error that says what is wrong with it. The error is a
/// refusal (`structured_call::refusal`), so a caller that asks again once sends its text to the model.
pub(super) fn accept(raw: &str, review: &Review<'_>) -> Result<Assessment> {
    let answer: Answer = serde_json::from_str(raw.trim())
        .map_err(|error| refuse(format!("The answer could not be read: {error}.")))?;
    let order = display_order(&review.drill.key);
    let key_len = order.len();
    let findings = review.findings;

    let mut faults = Vec::new();
    let issues = by_position(
        &answer.issues,
        key_len,
        |entry| entry.issue,
        &ISSUE,
        &mut faults,
    );
    let matched = by_position(
        &answer.findings,
        findings.len(),
        |entry| entry.finding,
        &FINDING,
        &mut faults,
    );
    if !faults.is_empty() {
        return Err(refuse(faults.join(" ")));
    }

    let issue_outcomes: Vec<Option<(IssueOutcome, String)>> = issues
        .iter()
        .enumerate()
        .map(|(issue, entry)| issue_outcome(issue, entry, findings, &mut faults))
        .collect();
    let finding_outcomes: Vec<Option<(FindingOutcome, String)>> = matched
        .iter()
        .enumerate()
        .map(|(finding, entry)| finding_outcome(finding, entry, &order, &mut faults))
        .collect();
    let complete =
        issue_outcomes.iter().all(Option::is_some) && finding_outcomes.iter().all(Option::is_some);
    if !complete {
        return Err(refuse(faults.join(" ")));
    }
    let assessment = Assessment {
        issues: in_key_order(issue_outcomes.into_iter().flatten().collect(), &order),
        findings: finding_outcomes.into_iter().flatten().collect(),
    };
    if let Err(fault) = validate(key_len, findings.len(), &assessment) {
        faults.push(describe(fault, &assessment, &order));
    }
    if faults.is_empty() {
        Ok(assessment)
    } else {
        Err(refuse(faults.join(" ")))
    }
}

/// The outcomes of the issues, given in display order, in the key's own order.
fn in_key_order(
    shown: Vec<(IssueOutcome, String)>,
    order: &[usize],
) -> Vec<(IssueOutcome, String)> {
    let mut placed: Vec<(usize, (IssueOutcome, String))> =
        order.iter().copied().zip(shown).collect();
    placed.sort_by_key(|(position, _)| *position);
    placed.into_iter().map(|(_, outcome)| outcome).collect()
}

fn refuse(message: String) -> AppError {
    AppError::InvalidInput(message)
}

/// The entries by the number they carry, counted from 1: one for every number up to `count`, returned in position
/// order. A number below 1 or past `count`, a number with no entry and a number with several are faults; the first
/// of several entries stands in for them.
fn by_position<'a, E>(
    entries: &'a [E],
    count: usize,
    number_of: impl Fn(&E) -> i64,
    kind: &Kind,
    faults: &mut Vec<String>,
) -> Vec<&'a E> {
    let mut slots: Vec<Vec<&'a E>> = vec![Vec::new(); count];
    for entry in entries {
        let named = number_of(entry);
        match usize::try_from(named) {
            Ok(number) if (1..=count).contains(&number) => {
                if let Some(slot) = slots.get_mut(number - 1) {
                    slot.push(entry);
                }
            }
            _ => faults.push(format!(
                "{} {named} is not {} {} of {}, which has {}.",
                kind.title,
                kind.article,
                kind.singular,
                kind.owner,
                kind.numbered(count)
            )),
        }
    }
    let mut standing = Vec::with_capacity(count);
    for (position, slot) in slots.iter().enumerate() {
        let number = position + 1;
        match slot.as_slice() {
            [] => faults.push(format!(
                "{} {number} has no entry: every {} needs exactly one.",
                kind.title, kind.singular
            )),
            [only] => standing.push(*only),
            [first, ..] => {
                faults.push(format!(
                    "{} {number} has {} entries: every {} needs exactly one.",
                    kind.title,
                    slot.len(),
                    kind.singular
                ));
                standing.push(*first);
            }
        }
    }
    standing
}

/// What the entry says became of the issue at position `issue`, or `None` when it names no finding that can be
/// credited. A fault found in the entry is added to `faults`.
fn issue_outcome(
    issue: usize,
    entry: &IssueEntry,
    findings: &[Finding],
    faults: &mut Vec<String>,
) -> Option<(IssueOutcome, String)> {
    if entry.reason.trim().is_empty() {
        faults.push(no_reason(&ISSUE, issue));
    }
    let reason = entry.reason.trim().to_owned();
    let found = match entry.outcome {
        IssueVerdict::Missed => return Some((IssueOutcome::Missed, reason)),
        IssueVerdict::Found => true,
        IssueVerdict::HalfFound => false,
    };
    let how = if found { "found" } else { "half found" };
    let Some(named) = entry.finding else {
        faults.push(format!(
            "Issue {} is {how} but names no finding: give the finding that {}.",
            issue + 1,
            if found {
                "names its cause"
            } else {
                "points at it"
            }
        ));
        return None;
    };
    let credited = usize::try_from(named)
        .ok()
        .and_then(|number| number.checked_sub(1))
        .and_then(|position| findings.get(position).map(|finding| (position, finding)));
    let Some((position, finding)) = credited else {
        faults.push(format!(
            "Issue {} is {how} by finding {named}, but the review has {}.",
            issue + 1,
            FINDING.numbered(findings.len())
        ));
        return None;
    };
    let (issue, number) = (issue + 1, position + 1);
    let quote = entry
        .quote
        .as_deref()
        .map(str::trim)
        .filter(|quote| !quote.is_empty());
    match quote {
        None if found => faults.push(format!(
            "Issue {issue} is found by finding {number} but has no quote: quote the words of finding {number} that name its cause."
        )),
        Some(quote) if !verbatim(&finding.text, quote) => faults.push(format!(
            "The quote for issue {issue}, {quote:?}, is not in the text of finding {number}: copy the words that name the cause exactly as finding {number} wrote them."
        )),
        _ => {}
    }
    let outcome = if found {
        IssueOutcome::Found { finding: position }
    } else {
        IssueOutcome::HalfFound { finding: position }
    };
    Some((outcome, reason))
}

/// What the entry says became of the finding at position `finding`, or `None` when it matches an issue the key does
/// not have. The issues it names are numbers in display order; the outcome holds their key positions. A fault found
/// in the entry is added to `faults`.
fn finding_outcome(
    finding: usize,
    entry: &FindingEntry,
    order: &[usize],
    faults: &mut Vec<String>,
) -> Option<(FindingOutcome, String)> {
    if entry.reason.trim().is_empty() {
        faults.push(no_reason(&FINDING, finding));
    }
    let reason = entry.reason.trim().to_owned();
    let outcome = match entry.outcome {
        FindingVerdict::ValidExtra => FindingOutcome::ValidExtra,
        FindingVerdict::FalsePositive => FindingOutcome::FalsePositive,
        FindingVerdict::Matches => {
            let mut issues = Vec::new();
            let mut in_the_key = true;
            for &named in entry.issues.as_deref().unwrap_or_default() {
                match usize::try_from(named)
                    .ok()
                    .and_then(|number| number.checked_sub(1))
                    .and_then(|shown| order.get(shown))
                {
                    Some(&position) => issues.push(position),
                    None => {
                        in_the_key = false;
                        faults.push(format!(
                            "Finding {} matches issue {named}, but the key has {}.",
                            finding + 1,
                            ISSUE.numbered(order.len())
                        ));
                    }
                }
            }
            if !in_the_key {
                return None;
            }
            issues.sort_unstable();
            issues.dedup();
            FindingOutcome::Matches { issues }
        }
    };
    Some((outcome, reason))
}

fn no_reason(kind: &Kind, number: usize) -> String {
    format!(
        "{} {} has no reason: say why before giving its outcome.",
        kind.title,
        number + 1
    )
}

/// What `fault` says is wrong with `assessment`, as an instruction. The fault holds key positions; the message names
/// issues by the numbers the task gave them.
fn describe(fault: AssessmentFault, assessment: &Assessment, order: &[usize]) -> String {
    let shown = |position: usize| {
        order
            .iter()
            .position(|&candidate| candidate == position)
            .map_or(position + 1, |index| index + 1)
    };
    let half_found = |issue: usize| {
        matches!(
            assessment.issues.get(issue),
            Some((IssueOutcome::HalfFound { .. }, _))
        )
    };
    match fault {
        AssessmentFault::IssueCount { expected, got } => {
            format!("The answer has {got} issue entries for {expected} issues.")
        }
        AssessmentFault::FindingCount { expected, got } => {
            format!("The answer has {got} finding entries for {expected} findings.")
        }
        AssessmentFault::FindingOutOfRange { issue, finding } => {
            let (issue, finding) = (shown(issue), finding + 1);
            format!("Issue {issue} is credited to finding {finding}, which the review does not have.")
        }
        AssessmentFault::IssueOutOfRange { finding, issue } => {
            let (finding, issue) = (finding + 1, shown(issue));
            format!("Finding {finding} matches issue {issue}, which the key does not have.")
        }
        AssessmentFault::CreditedWithoutMatch { issue, finding } => {
            let how = if half_found(issue) { "half found" } else { "found" };
            let (issue, finding) = (shown(issue), finding + 1);
            format!(
                "Issue {issue} is {how} by finding {finding}, but finding {finding} does not match issue {issue}: list issue {issue} among the issues finding {finding} matches, or give issue {issue} another outcome."
            )
        }
        AssessmentFault::MatchesNothing { finding } => format!(
            "Finding {} matches no issue: list the issues it matches, or call it a valid extra or a false positive.",
            finding + 1
        ),
        AssessmentFault::MatchedButMissed { finding, issue } => {
            let (finding, issue) = (finding + 1, shown(issue));
            format!(
                "Finding {finding} matches issue {issue}, but issue {issue} is missed: credit issue {issue} to a finding that earns it, or take it out of finding {finding}'s issues."
            )
        }
    }
}
