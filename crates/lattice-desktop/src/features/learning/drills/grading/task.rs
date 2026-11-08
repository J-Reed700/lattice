//! The task the judge reads: the material as the reviewer saw it, the material before any issue was planted, the
//! source passage of an explanation, the key and the review, numbered so that the answer can name them. Issues are
//! numbered most severe first.

use lattice_core::domain::learning::drills::{
    display_order, overlapping, pattern, DrillFile, Finding, KeyIssue, LineSpan, Severity,
};

use crate::features::learning::drills::grader::Review;
use crate::features::learning::drills::material::JudgeWording;

/// The user message of the judge's request.
pub(super) fn task(review: &Review<'_>, wording: &JudgeWording) -> String {
    let drill = review.drill;
    let findings = review.findings;
    let order = display_order(&drill.key);
    let mut parts = vec![
        format!("Drill: {} ({})", drill.title, wording.kind),
        format!("{}\n\n{}", wording.reviewed, listing(&drill.files)),
        format!("{}\n\n{}", wording.original, listing(review.clean)),
    ];
    if let (Some(heading), Some(source)) = (wording.source, review.source) {
        parts.push(format!(
            "{heading} {}:\n<passage>\n{}\n</passage>",
            source.title, source.text
        ));
    }
    parts.push(format!("The key: {}.", counted(drill.key.len(), "issue")));
    parts.extend(order.iter().enumerate().filter_map(|(index, &position)| {
        let issue = drill.key.get(position)?;
        Some(issue_block(index + 1, issue, &drill.files, wording))
    }));
    parts.push(format!(
        "The review: {}, in the order they were made.",
        counted(findings.len(), "finding")
    ));
    parts.extend(
        findings
            .iter()
            .enumerate()
            .map(|(index, finding)| finding_block(index + 1, finding, &drill.files, wording)),
    );
    parts.push(overlap_block(findings, &drill.key, &order, wording));
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

fn issue_block(
    number: usize,
    issue: &KeyIssue,
    files: &[DrillFile],
    wording: &JudgeWording,
) -> String {
    let known = pattern(&issue.pattern);
    let name = known.map_or(issue.pattern.as_str(), |known| known.name);
    let mut lines = vec![format!(
        "Issue {number}: {name} ({}), {}",
        severity_word(issue.severity),
        place(&issue.span, wording)
    )];
    if let Some(known) = known {
        lines.push(format!("Pattern: {}", known.definition));
    }
    if let Some(link) = issue
        .lesson
        .as_ref()
        .filter(|link| !link.sentences.is_empty())
    {
        let mut block = String::from("From the lesson:");
        for sentence in &link.sentences {
            for line in sentence.text.lines() {
                block.push_str("\n  ");
                block.push_str(line);
            }
        }
        lines.push(block);
    }
    lines.push(format!("Consequence: {}", issue.consequence));
    lines.push(quoted(&issue.span, files, wording));
    lines.join("\n")
}

fn finding_block(
    number: usize,
    finding: &Finding,
    files: &[DrillFile],
    wording: &JudgeWording,
) -> String {
    let mut head = format!(
        "Finding {number}: {}",
        finding.span.as_ref().map_or_else(
            || format!("no {} selected", wording.lines),
            |span| place(span, wording)
        )
    );
    if let Some(severity) = finding.severity {
        head.push_str(&format!(", severity given: {}", severity_word(severity)));
    }
    let mut lines = vec![head];
    if let Some(span) = &finding.span {
        lines.push(quoted(span, files, wording));
    }
    lines.push(text_block(&finding.text));
    lines.join("\n")
}

/// The pairs of a finding and an issue, the issue by its number in the task, whose lines overlap: the hint the judge
/// is given.
fn overlap_block(
    findings: &[Finding],
    key: &[KeyIssue],
    order: &[usize],
    wording: &JudgeWording,
) -> String {
    let mut pairs: Vec<(usize, usize)> = overlapping(findings, key)
        .into_iter()
        .filter_map(|(finding, issue)| {
            let shown = order.iter().position(|&position| position == issue)?;
            Some((finding, shown))
        })
        .collect();
    let noun = wording.lines;
    if pairs.is_empty() {
        return format!("No finding's {noun} overlap an issue's {noun}.");
    }
    pairs.sort_unstable();
    let lines: Vec<String> = pairs
        .iter()
        .map(|(finding, issue)| format!("finding {} and issue {}", finding + 1, issue + 1))
        .collect();
    format!(
        "Findings and issues whose {noun} overlap, a hint and not a verdict:\n{}",
        lines.join("\n")
    )
}

/// The lines a span covers, quoted from the drill's files.
fn quoted(span: &LineSpan, files: &[DrillFile], wording: &JudgeWording) -> String {
    let label = capitalised(wording.lines);
    let Some(file) = files.iter().find(|file| file.path == span.path) else {
        return format!("{label}: {} is not one of the drill's files", span.path);
    };
    let first = usize::try_from(span.start).unwrap_or(usize::MAX);
    let last = usize::try_from(span.end).unwrap_or(usize::MAX);
    let lines = numbered(&file.text, first, last);
    if lines.is_empty() {
        return format!("{label}: none of them is in the file");
    }
    let indented: Vec<String> = lines.iter().map(|line| format!("  {line}")).collect();
    format!("{label}:\n{}", indented.join("\n"))
}

/// `word` with its first letter in capitals.
fn capitalised(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// A finding's text, set apart from the labels around it.
fn text_block(text: &str) -> String {
    if text.trim().is_empty() {
        return "Text: (empty)".to_owned();
    }
    let indented: Vec<String> = text.lines().map(|line| format!("  {line}")).collect();
    format!("Text:\n{}", indented.join("\n"))
}

fn place(span: &LineSpan, wording: &JudgeWording) -> String {
    if span.start == span.end {
        format!("{} {} {}", span.path, wording.line, span.start)
    } else {
        format!(
            "{} {} {} to {}",
            span.path, wording.lines, span.start, span.end
        )
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

/// "3 issues, numbered 1 to 3".
fn counted(count: usize, noun: &str) -> String {
    match count {
        0 => format!("no {noun}s"),
        1 => format!("1 {noun}, numbered 1"),
        count => format!("{count} {noun}s, numbered 1 to {count}"),
    }
}

/// "1 to 3".
fn numbers(count: usize) -> String {
    match count {
        0 => "none".to_owned(),
        1 => "1".to_owned(),
        count => format!("1 to {count}"),
    }
}
