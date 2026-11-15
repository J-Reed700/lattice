//! What the judge reads: the material as the reviewer saw it, the material before any issue was planted, the source
//! passage of an explanation, the key and the review, each issue and finding in the words it is shown with. Issues are
//! shown most severe first.

use lattice_core::application::services::model_contract::shown::{Internal, Told};
use lattice_core::domain::learning::drills::{
    display_order, overlapping, pattern, DrillFile, Finding, KeyIssue, LineSpan, Severity,
};

use super::contract::{Context, FindingView, IssueView};
use crate::drills::grader::Review;
use crate::drills::material::JudgeWording;

/// What every request of `review` shows besides the things it asks about, under `instructions`.
pub(super) fn context(review: &Review<'_>, wording: &JudgeWording, instructions: &str) -> Context {
    let drill = review.drill;
    Context {
        instructions: Told(instructions.to_owned()),
        drill: Told(format!("Drill: {} ({})", drill.title, wording.kind)),
        reviewed: Told(format!("{}\n\n{}", wording.reviewed, listing(&drill.files))),
        original: Told(format!("{}\n\n{}", wording.original, listing(review.clean))),
        source: match (wording.source, review.source) {
            (Some(heading), Some(source)) => Some(Told(format!(
                "{heading} {}:\n<passage>\n{}\n</passage>",
                source.title, source.text
            ))),
            _ => None,
        },
    }
}

/// The issues of the key, most severe first, as the model is shown them.
pub(super) fn issue_views(review: &Review<'_>, wording: &JudgeWording) -> Vec<IssueView> {
    let drill = review.drill;
    display_order(&drill.key)
        .into_iter()
        .filter_map(|position| {
            let issue = drill.key.get(position)?;
            let (head, rest) = issue_parts(issue, &drill.files, wording);
            Some(IssueView {
                at: position,
                head: Told(head),
                rest: Told(rest),
            })
        })
        .collect()
}

/// The findings of the review, in the order they were made, each with the issues whose lines its lines overlap.
pub(super) fn finding_views(review: &Review<'_>, wording: &JudgeWording) -> Vec<FindingView> {
    let overlaps = overlapping(review.findings, &review.drill.key);
    review
        .findings
        .iter()
        .enumerate()
        .map(|(position, finding)| {
            let (head, rest) = finding_parts(finding, &review.drill.files, wording);
            FindingView {
                at: position,
                head: Told(head),
                rest: Told(rest),
                overlaps: Internal::of(
                    overlaps
                        .iter()
                        .filter(|(found, _)| *found == position)
                        .map(|(_, issue)| *issue)
                        .collect(),
                ),
            }
        })
        .collect()
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

/// The first line of an issue's block after its label, and the lines that follow it.
fn issue_parts(issue: &KeyIssue, files: &[DrillFile], wording: &JudgeWording) -> (String, String) {
    let known = pattern(&issue.pattern);
    let name = known.map_or(issue.pattern.as_str(), |known| known.name);
    let head = format!(
        "{name} ({}), {}",
        severity_word(issue.severity),
        place(&issue.span, wording)
    );
    let mut lines = Vec::new();
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
    (head, lines.join("\n"))
}

/// The first line of a finding's block after its label, and the lines that follow it.
fn finding_parts(
    finding: &Finding,
    files: &[DrillFile],
    wording: &JudgeWording,
) -> (String, String) {
    let mut head = finding.span.as_ref().map_or_else(
        || format!("no {} selected", wording.lines),
        |span| place(span, wording),
    );
    if let Some(severity) = finding.severity {
        head.push_str(&format!(", severity given: {}", severity_word(severity)));
    }
    let mut lines = Vec::new();
    if let Some(span) = &finding.span {
        lines.push(quoted(span, files, wording));
    }
    lines.push(text_block(&finding.text));
    (head, lines.join("\n"))
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
