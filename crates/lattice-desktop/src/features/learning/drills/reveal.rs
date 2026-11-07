//! An attempt as the review view reads it: the drill as the learner may see it, and, once the attempt is graded, the
//! reveal of everything the key knows (docs/design/2026-11-04-code-review-drills.md section 1). The key, the clean
//! files, the suite, the killing tests and the evidence of a drill reach the learner only through what is built here.

use super::attempts::{Attempt, AttemptStatus};
use super::dto::{
    CodeReviewDrillDto, CodeReviewFileDto, CodeReviewFindingDto, CodeReviewResultDto,
    CodeReviewRevealDto, CodeReviewScoreDto, FindingResultKind, FixDiffDto, IssueResultKind,
    RatedPatternDto, RevealedFindingDto, RevealedIssueDto,
};
use super::repository::StoredDrill;
use lattice_core::domain::learning::drills::{
    line_diff, pattern, FindingOutcome, IssueOutcome, KeyIssue, Origin,
};
use lattice_core::shared::error::{AppError, Result};

/// The drill as the learner may see it: its title, its language and its files.
pub fn learner_drill(stored: &StoredDrill) -> CodeReviewDrillDto {
    CodeReviewDrillDto {
        title: stored.drill.title.clone(),
        language: stored.drill.language.clone(),
        files: stored
            .drill
            .files
            .iter()
            .map(CodeReviewFileDto::from)
            .collect(),
    }
}

/// The attempt on `stored`, as the review view reads it. A graded attempt carries the reveal; any other carries none.
pub fn result_of(attempt: &Attempt, stored: &StoredDrill) -> Result<CodeReviewResultDto> {
    let reveal = match attempt.status {
        AttemptStatus::Graded => Some(reveal_of(attempt, stored)?),
        _ => None,
    };
    Ok(CodeReviewResultDto {
        attempt_id: attempt.id.clone(),
        mode: attempt.mode,
        status: attempt.status,
        started_at: attempt.started_at,
        submitted_at: attempt.submitted_at,
        time_spent_ms: attempt.time_spent_ms,
        drill: learner_drill(stored),
        findings: attempt
            .findings
            .iter()
            .flatten()
            .map(CodeReviewFindingDto::from)
            .collect(),
        fix_files: attempt
            .fix_files
            .as_ref()
            .map(|files| files.iter().map(CodeReviewFileDto::from).collect()),
        error: attempt.error.clone(),
        grader_model: attempt.grader_model.clone(),
        graded_at: attempt.graded_at,
        reveal,
    })
}

/// Everything a graded attempt shows. A graded attempt has its assessment, score and ratings; one that does not is
/// corrupt.
fn reveal_of(attempt: &Attempt, stored: &StoredDrill) -> Result<CodeReviewRevealDto> {
    let missing = |what: &str| AppError::InvalidData(format!("A graded attempt has no {what}."));
    let assessment = attempt
        .assessment
        .as_ref()
        .ok_or_else(|| missing("assessment"))?;
    let score = attempt.score.as_ref().ok_or_else(|| missing("score"))?;
    let ratings = attempt.ratings.as_ref().ok_or_else(|| missing("ratings"))?;
    let issues = stored
        .drill
        .key
        .iter()
        .enumerate()
        .map(|(position, issue)| {
            let (outcome, reason) = assessment.issues.get(position).map_or(
                (IssueOutcome::Missed, String::new()),
                |(outcome, reason)| (*outcome, reason.clone()),
            );
            revealed_issue(issue, outcome, reason)
        })
        .collect();
    let findings = attempt
        .findings
        .iter()
        .flatten()
        .enumerate()
        .map(|(position, finding)| {
            let (outcome, reason) = assessment.findings.get(position).map_or(
                (FindingOutcome::FalsePositive, String::new()),
                |(outcome, reason)| (outcome.clone(), reason.clone()),
            );
            let (kind, about) = match outcome {
                FindingOutcome::Matches { issues } => (FindingResultKind::Matches, issues),
                FindingOutcome::ValidExtra => (FindingResultKind::ValidExtra, Vec::new()),
                FindingOutcome::FalsePositive => (FindingResultKind::FalsePositive, Vec::new()),
            };
            let shown = CodeReviewFindingDto::from(finding);
            RevealedFindingDto {
                span: shown.span,
                text: shown.text,
                severity: shown.severity,
                outcome: kind,
                issues: about,
                reason,
            }
        })
        .collect();
    let ratings = ratings
        .iter()
        .map(|rated| RatedPatternDto {
            pattern: rated.pattern.clone(),
            pattern_name: name_of(&rated.pattern),
            rating: rated.rating,
        })
        .collect();
    Ok(CodeReviewRevealDto {
        issues,
        findings,
        score: CodeReviewScoreDto::from(score),
        ratings,
        fix: attempt.fix.clone(),
        clean_files: stored
            .clean_files
            .iter()
            .map(CodeReviewFileDto::from)
            .collect(),
    })
}

/// One issue of the key, with its outcome and the judge's reason for it.
fn revealed_issue(issue: &KeyIssue, outcome: IssueOutcome, reason: String) -> RevealedIssueDto {
    let (kind, finding) = match outcome {
        IssueOutcome::Found { finding } => (IssueResultKind::Found, Some(finding)),
        IssueOutcome::HalfFound { finding } => (IssueResultKind::HalfFound, Some(finding)),
        IssueOutcome::Missed => (IssueResultKind::Missed, None),
    };
    RevealedIssueDto {
        id: issue.id.clone(),
        pattern: issue.pattern.clone(),
        pattern_name: name_of(&issue.pattern),
        severity: issue.severity.into(),
        span: (&issue.span).into(),
        consequence: issue.consequence.clone(),
        explanation: issue.explanation.clone(),
        fix: FixDiffDto {
            path: issue.fix.path.clone(),
            lines: line_diff(&issue.fix.before, &issue.fix.after),
        },
        outcome: kind,
        finding,
        reason,
        also_valid: issue.origin == Origin::FoundInReview,
    }
}

/// A pattern's name, or its id when the catalogue no longer has it.
fn name_of(id: &str) -> String {
    pattern(id).map_or_else(|| id.to_owned(), |known| known.name.to_owned())
}
