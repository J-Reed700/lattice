//! Grading a review: the findings a learner made, a judge's assessment of
//! them against the key, whether that assessment is consistent, and the score
//! it gives.

use serde::{Deserialize, Serialize};

use super::drill::{weight, KeyIssue, LineSpan, Severity};
use super::edits::overlaps;

/// One comment of a learner's review: the lines it is on, when it selects
/// any, what it says, and the severity the learner gave it, when they gave
/// one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub span: Option<LineSpan>,
    pub text: String,
    pub severity: Option<Severity>,
}

/// What came of one key issue. A finding is named by its position in the
/// review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueOutcome {
    /// The finding names the issue: the right place and the right reason.
    Found {
        finding: usize,
    },
    /// The finding is at the right place with the wrong reason, or none.
    HalfFound {
        finding: usize,
    },
    Missed,
}

/// What came of one finding. An issue is named by its position in the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingOutcome {
    /// The finding is about these key issues.
    Matches { issues: Vec<usize> },
    /// A real problem the key does not hold.
    ValidExtra,
    /// Not a problem.
    FalsePositive,
}

/// A judge's verdict on a review: an outcome for every key issue and every
/// finding, each with the reason the judge gave for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assessment {
    pub issues: Vec<(IssueOutcome, String)>,
    pub findings: Vec<(FindingOutcome, String)>,
}

/// How an assessment is not a consistent assignment. Issues and findings are
/// named by position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssessmentFault {
    /// The assessment has `got` outcomes for a key of `expected` issues.
    IssueCount { expected: usize, got: usize },
    /// The assessment has `got` outcomes for a review of `expected` findings.
    FindingCount { expected: usize, got: usize },
    /// The issue is found or half found by a finding there is not.
    FindingOutOfRange { issue: usize, finding: usize },
    /// The finding matches an issue there is not.
    IssueOutOfRange { finding: usize, issue: usize },
    /// The issue is found or half found by a finding that does not match it.
    CreditedWithoutMatch { issue: usize, finding: usize },
    /// The finding matches no issue at all.
    MatchesNothing { finding: usize },
    /// The finding matches an issue that the assessment calls missed.
    MatchedButMissed { finding: usize, issue: usize },
}

/// A judge's assessment is a consistent assignment: one outcome per key issue
/// and per finding, every index in range, an issue found or half found by
/// finding `f` only if `f` matches it, and every issue a finding matches found
/// or half found, by that finding or another.
///
/// The first fault is reported, in this order: the counts, the indexes in
/// range (issues, then findings), the issues credited to a finding that does
/// not match them, then the findings, each in turn.
pub fn validate(
    key_len: usize,
    findings_len: usize,
    assessment: &Assessment,
) -> Result<(), AssessmentFault> {
    if assessment.issues.len() != key_len {
        return Err(AssessmentFault::IssueCount {
            expected: key_len,
            got: assessment.issues.len(),
        });
    }
    if assessment.findings.len() != findings_len {
        return Err(AssessmentFault::FindingCount {
            expected: findings_len,
            got: assessment.findings.len(),
        });
    }
    for (issue, (outcome, _)) in assessment.issues.iter().enumerate() {
        if let Some(finding) = credited(outcome).filter(|&finding| finding >= findings_len) {
            return Err(AssessmentFault::FindingOutOfRange { issue, finding });
        }
    }
    for (finding, (outcome, _)) in assessment.findings.iter().enumerate() {
        if let FindingOutcome::Matches { issues } = outcome {
            if let Some(&issue) = issues.iter().find(|&&issue| issue >= key_len) {
                return Err(AssessmentFault::IssueOutOfRange { finding, issue });
            }
        }
    }
    for (issue, (outcome, _)) in assessment.issues.iter().enumerate() {
        let Some(finding) = credited(outcome) else {
            continue;
        };
        if !matches_issue(assessment, finding, issue) {
            return Err(AssessmentFault::CreditedWithoutMatch { issue, finding });
        }
    }
    for (finding, (outcome, _)) in assessment.findings.iter().enumerate() {
        let FindingOutcome::Matches { issues } = outcome else {
            continue;
        };
        if issues.is_empty() {
            return Err(AssessmentFault::MatchesNothing { finding });
        }
        for &issue in issues {
            let credited_to_someone = assessment
                .issues
                .get(issue)
                .and_then(|(outcome, _)| credited(outcome))
                .is_some();
            if !credited_to_someone {
                return Err(AssessmentFault::MatchedButMissed { finding, issue });
            }
        }
    }
    Ok(())
}

/// The finding that found or half found an issue, when one did.
fn credited(outcome: &IssueOutcome) -> Option<usize> {
    match outcome {
        IssueOutcome::Found { finding } | IssueOutcome::HalfFound { finding } => Some(*finding),
        IssueOutcome::Missed => None,
    }
}

/// Whether finding number `finding` matches issue number `issue`.
fn matches_issue(assessment: &Assessment, finding: usize, issue: usize) -> bool {
    matches!(
        assessment.findings.get(finding),
        Some((FindingOutcome::Matches { issues }, _)) if issues.contains(&issue)
    )
}

/// The pairs of a finding and a key issue, by position, whose spans overlap:
/// a hint the judge is shown, never the verdict. A finding with no span
/// overlaps nothing.
pub fn overlapping(findings: &[Finding], key: &[KeyIssue]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for (finding, found) in findings.iter().enumerate() {
        let Some(span) = &found.span else {
            continue;
        };
        for (issue, known) in key.iter().enumerate() {
            if overlaps(span, &known.span) {
                pairs.push((finding, issue));
            }
        }
    }
    pairs
}

/// How a review fared, as its parts: no single number stands for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub found: usize,
    pub half_found: usize,
    pub missed: usize,
    pub valid_extras: usize,
    pub false_positives: usize,
    /// One tally per severity present in the key, the most severe first.
    pub by_severity: Vec<SeverityTally>,
    /// What the review found of the key, each issue weighing what its severity
    /// does and a half found one half as much: 1.0 for an empty key.
    pub weighted_recall: f64,
    /// The share of findings that match an issue or are a valid extra: none
    /// when there are no findings.
    pub precision: Option<f64>,
    /// The first finding found an issue of the key's highest severity: a half
    /// find, or a find credited to a later finding, is not leading with it.
    pub led_with_the_most_severe: bool,
}

/// The key issues of one severity and how many of them were found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeverityTally {
    pub severity: Severity,
    pub found: usize,
    pub half_found: usize,
    pub total: usize,
}

/// The score of a review under an assessment. The assessment should pass
/// `validate`; for one that does not, an issue without an outcome counts as
/// missed and a finding without one as a false positive.
pub fn score(key: &[KeyIssue], findings: &[Finding], assessment: &Assessment) -> Score {
    let outcomes: Vec<IssueOutcome> = (0..key.len())
        .map(|issue| {
            assessment
                .issues
                .get(issue)
                .map_or(IssueOutcome::Missed, |&(outcome, _)| outcome)
        })
        .collect();
    let mut found = 0;
    let mut half_found = 0;
    let mut missed = 0;
    let mut earned = 0.0;
    let mut possible = 0.0;
    for (issue, outcome) in key.iter().zip(&outcomes) {
        let worth = f64::from(weight(issue.severity));
        possible += worth;
        match outcome {
            IssueOutcome::Found { .. } => {
                found += 1;
                earned += worth;
            }
            IssueOutcome::HalfFound { .. } => {
                half_found += 1;
                earned += worth / 2.0;
            }
            IssueOutcome::Missed => missed += 1,
        }
    }
    let by_severity = Severity::ALL
        .iter()
        .filter_map(|&severity| tally(severity, key, &outcomes))
        .collect();
    let mut matching = 0;
    let mut valid_extras = 0;
    let mut false_positives = 0;
    for finding in 0..findings.len() {
        match assessment.findings.get(finding).map(|(outcome, _)| outcome) {
            Some(FindingOutcome::Matches { .. }) => matching += 1,
            Some(FindingOutcome::ValidExtra) => valid_extras += 1,
            Some(FindingOutcome::FalsePositive) | None => false_positives += 1,
        }
    }
    Score {
        found,
        half_found,
        missed,
        valid_extras,
        false_positives,
        by_severity,
        weighted_recall: if key.is_empty() {
            1.0
        } else {
            earned / possible
        },
        precision: if findings.is_empty() {
            None
        } else {
            Some((matching + valid_extras) as f64 / findings.len() as f64)
        },
        led_with_the_most_severe: led_with_the_most_severe(key, findings, assessment),
    }
}

/// How the key's issues of one severity fared, when the key has any.
fn tally(severity: Severity, key: &[KeyIssue], outcomes: &[IssueOutcome]) -> Option<SeverityTally> {
    let mut counts = SeverityTally {
        severity,
        found: 0,
        half_found: 0,
        total: 0,
    };
    for (_, outcome) in key
        .iter()
        .zip(outcomes)
        .filter(|(issue, _)| issue.severity == severity)
    {
        counts.total += 1;
        match outcome {
            IssueOutcome::Found { .. } => counts.found += 1,
            IssueOutcome::HalfFound { .. } => counts.half_found += 1,
            IssueOutcome::Missed => {}
        }
    }
    (counts.total > 0).then_some(counts)
}

/// Whether an issue of the key's highest severity was found by the first
/// finding.
fn led_with_the_most_severe(
    key: &[KeyIssue],
    findings: &[Finding],
    assessment: &Assessment,
) -> bool {
    let Some(highest) = key.iter().map(|issue| issue.severity).max() else {
        return false;
    };
    if findings.is_empty() {
        return false;
    }
    key.iter()
        .zip(&assessment.issues)
        .any(|(issue, (outcome, _))| {
            issue.severity == highest && matches!(outcome, IssueOutcome::Found { finding: 0 })
        })
}
