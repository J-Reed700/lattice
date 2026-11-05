//! The task the judge reads: the code as the reviewer saw it, the code before any issue was planted, the key and the
//! review, numbered so that the answer can name them.

use lattice_core::domain::learning::drills::{
    overlapping, pattern, DrillFile, Finding, KeyIssue, LineSpan, Severity,
};

use crate::features::learning::drills::grader::Review;

/// The user message of the judge's request.
pub(super) fn task(review: &Review<'_>) -> String {
    let drill = review.drill;
    let findings = review.findings;
    let mut parts = vec![
        format!("Drill: {} ({})", drill.title, drill.language),
        format!(
            "The code as the reviewer saw it, with line numbers:\n\n{}",
            listing(&drill.files)
        ),
        format!(
            "The same code before any issue was planted, with its own line numbers:\n\n{}",
            listing(review.clean)
        ),
        format!("The key: {}.", counted(drill.key.len(), "issue")),
    ];
    parts.extend(
        drill
            .key
            .iter()
            .enumerate()
            .map(|(index, issue)| issue_block(index, issue, &drill.files)),
    );
    parts.push(format!(
        "The review: {}, in the order they were made.",
        counted(findings.len(), "finding")
    ));
    parts.extend(
        findings
            .iter()
            .enumerate()
            .map(|(index, finding)| finding_block(index, finding, &drill.files)),
    );
    parts.push(overlap_block(findings, &drill.key));
    parts.push(format!(
        "Give an entry for every issue ({}) and every finding ({}), the reason first.",
        numbers(drill.key.len()),
        numbers(findings.len())
    ));
    parts.join("\n\n")
}

/// Each file under its path, a numbered line for every line of its text.
fn listing(files: &[DrillFile]) -> String {
    files
        .iter()
        .map(|file| {
            format!(
                "File {}\n{}",
                file.path,
                numbered(&file.text, 1, usize::MAX).join("\n")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The lines of `text` from `first` to `last`, each as its number, a bar and its text. Numbers are padded to the
/// width of the last line of `text`, so lines quoted from a file line up with the file's listing.
fn numbered(text: &str, first: usize, last: usize) -> Vec<String> {
    let width = text.lines().count().to_string().len();
    text.lines()
        .enumerate()
        .map(|(position, line)| (position + 1, line))
        .filter(|(number, _)| (first..=last).contains(number))
        .map(|(number, line)| format!("{number:>width$} | {line}"))
        .collect()
}

fn issue_block(index: usize, issue: &KeyIssue, files: &[DrillFile]) -> String {
    let known = pattern(&issue.pattern);
    let name = known.map_or(issue.pattern.as_str(), |known| known.name);
    let mut lines = vec![format!(
        "Issue {index}: {name} ({}), {}",
        severity_word(issue.severity),
        place(&issue.span)
    )];
    if let Some(known) = known {
        lines.push(format!("Pattern: {}", known.definition));
    }
    lines.push(format!("Consequence: {}", issue.consequence));
    lines.push(quoted(&issue.span, files));
    lines.join("\n")
}

fn finding_block(index: usize, finding: &Finding, files: &[DrillFile]) -> String {
    let mut head = format!(
        "Finding {index}: {}",
        finding
            .span
            .as_ref()
            .map_or_else(|| "no lines selected".to_owned(), place)
    );
    if let Some(severity) = finding.severity {
        head.push_str(&format!(", severity given: {}", severity_word(severity)));
    }
    let mut lines = vec![head];
    if let Some(span) = &finding.span {
        lines.push(quoted(span, files));
    }
    lines.push(text_block(&finding.text));
    lines.join("\n")
}

/// The pairs of a finding and an issue whose lines overlap, the hint the judge is given.
fn overlap_block(findings: &[Finding], key: &[KeyIssue]) -> String {
    let pairs = overlapping(findings, key);
    if pairs.is_empty() {
        return "No finding's lines overlap an issue's lines.".to_owned();
    }
    let lines: Vec<String> = pairs
        .iter()
        .map(|(finding, issue)| format!("finding {finding} and issue {issue}"))
        .collect();
    format!(
        "Findings and issues whose lines overlap, a hint and not a verdict:\n{}",
        lines.join("\n")
    )
}

/// The lines a span covers, quoted from the drill's files.
fn quoted(span: &LineSpan, files: &[DrillFile]) -> String {
    let Some(file) = files.iter().find(|file| file.path == span.path) else {
        return format!("Lines: {} is not one of the drill's files", span.path);
    };
    let first = usize::try_from(span.start).unwrap_or(usize::MAX);
    let last = usize::try_from(span.end).unwrap_or(usize::MAX);
    let lines = numbered(&file.text, first, last);
    if lines.is_empty() {
        return "Lines: none of them is in the file".to_owned();
    }
    let indented: Vec<String> = lines.iter().map(|line| format!("  {line}")).collect();
    format!("Lines:\n{}", indented.join("\n"))
}

/// A finding's text, set apart from the labels around it.
fn text_block(text: &str) -> String {
    if text.trim().is_empty() {
        return "Text: (empty)".to_owned();
    }
    let indented: Vec<String> = text.lines().map(|line| format!("  {line}")).collect();
    format!("Text:\n{}", indented.join("\n"))
}

fn place(span: &LineSpan) -> String {
    if span.start == span.end {
        format!("{} line {}", span.path, span.start)
    } else {
        format!("{} lines {} to {}", span.path, span.start, span.end)
    }
}

fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "critical",
        Severity::Major => "major",
        Severity::Minor => "minor",
        Severity::Nit => "nit",
    }
}

/// "3 issues, numbered 0 to 2".
fn counted(count: usize, noun: &str) -> String {
    match count {
        0 => format!("no {noun}s"),
        1 => format!("1 {noun}, numbered 0"),
        count => format!("{count} {noun}s, numbered 0 to {}", count - 1),
    }
}

/// "0 to 2".
fn numbers(count: usize) -> String {
    match count {
        0 => "none".to_owned(),
        1 => "0".to_owned(),
        count => format!("0 to {}", count - 1),
    }
}
