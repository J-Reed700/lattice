//! Shared builders for the drills' tests.

mod catalogue_tests;
mod drill_tests;
mod edits_tests;
mod grading_tests;
mod schedule_tests;

use super::*;

fn span(path: &str, start: u32, end: u32) -> LineSpan {
    LineSpan {
        path: path.to_owned(),
        start,
        end,
    }
}

fn file(path: &str, text: &str) -> DrillFile {
    DrillFile {
        path: path.to_owned(),
        text: text.to_owned(),
    }
}

fn edit(path: &str, before: &str, after: &str) -> Edit {
    Edit {
        path: path.to_owned(),
        before: before.to_owned(),
        after: after.to_owned(),
    }
}

/// A key issue of `severity` on lines `start` to `end` of `main.py`.
fn issue(severity: Severity, start: u32, end: u32) -> KeyIssue {
    KeyIssue {
        id: format!("issue-{start}"),
        pattern: "correctness.off_by_one".to_owned(),
        severity,
        span: span("main.py", start, end),
        consequence: "The total is wrong.".to_owned(),
        fix: edit("main.py", "<=", "<"),
        explanation: "The loop runs once too often.".to_owned(),
        proof: Proof::Judged {
            reason: "The edit changes the bound.".to_owned(),
        },
        origin: Origin::Planted,
    }
}

/// A finding on lines `start` to `end` of `main.py`.
fn finding_at(start: u32, end: u32) -> Finding {
    Finding {
        span: Some(span("main.py", start, end)),
        text: "This looks wrong.".to_owned(),
        severity: None,
    }
}

fn found(finding: usize) -> (IssueOutcome, String) {
    (IssueOutcome::Found { finding }, "Right.".to_owned())
}

fn half_found(finding: usize) -> (IssueOutcome, String) {
    (
        IssueOutcome::HalfFound { finding },
        "Right place.".to_owned(),
    )
}

fn missed() -> (IssueOutcome, String) {
    (IssueOutcome::Missed, "Not named.".to_owned())
}

fn matches(issues: &[usize]) -> (FindingOutcome, String) {
    (
        FindingOutcome::Matches {
            issues: issues.to_vec(),
        },
        "It names the issue.".to_owned(),
    )
}

fn valid_extra() -> (FindingOutcome, String) {
    (FindingOutcome::ValidExtra, "A real problem.".to_owned())
}

fn false_positive() -> (FindingOutcome, String) {
    (FindingOutcome::FalsePositive, "Not a problem.".to_owned())
}

fn assessment(
    issues: Vec<(IssueOutcome, String)>,
    findings: Vec<(FindingOutcome, String)>,
) -> Assessment {
    Assessment { issues, findings }
}
