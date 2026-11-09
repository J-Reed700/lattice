//! Checks on code a model wrote, run in the sandbox: whether a solution passes a problem's cases, whether a snippet
//! parses.

use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::harness::{run_cases, CaseHarness};
use crate::practice::coding::problem::{CaseStatus, Problem};
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

const PARSES_PY: &str = include_str!("solution_check/parses.py");

/// The fenced code blocks of a markdown text, in order. A block left open runs to the end of the text.
pub fn code_blocks(markdown: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut open: Option<(char, usize, Vec<&str>)> = None;
    for line in markdown.lines() {
        open = match open.take() {
            Some((marker, width, mut body)) => {
                if closes(line, marker, width) {
                    blocks.push(body.join("\n"));
                    None
                } else {
                    body.push(line);
                    Some((marker, width, body))
                }
            }
            None => opens(line).map(|(marker, width)| (marker, width, Vec::new())),
        };
    }
    if let Some((_, _, body)) = open {
        blocks.push(body.join("\n"));
    }
    blocks
}

/// A fence line: up to three spaces, then three or more of one marker. Returns the marker, how many, and the rest.
fn fence(line: &str) -> Option<(char, usize, &str)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let marker = trimmed.chars().next().filter(|c| matches!(*c, '`' | '~'))?;
    let width = trimmed.chars().take_while(|c| *c == marker).count();
    (width >= 3).then(|| (marker, width, trimmed.trim_start_matches(marker)))
}

fn opens(line: &str) -> Option<(char, usize)> {
    let (marker, width, info) = fence(line)?;
    (marker != '`' || !info.contains('`')).then_some((marker, width))
}

fn closes(line: &str, marker: char, width: usize) -> bool {
    fence(line).is_some_and(|(found, run, rest)| {
        found == marker && run >= width && rest.trim().is_empty()
    })
}

/// Whether `code`, as `solution.py`, passes every case of `problem`. A run that times out does not pass.
pub async fn passes_every_case(
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    problem: &Problem,
    code: &str,
    cancel: CancellationToken,
) -> Result<bool> {
    let (outcome, results) = run_cases(
        runner,
        harness,
        &problem.spec,
        code,
        &problem.cases,
        cancel,
    )
    .await?;
    if !matches!(outcome, RunOutcome::TimedOut { .. }) {
        if let Some(why) = outcome.says_nothing() {
            return Err(AppError::InvalidState(why));
        }
    }
    Ok(results
        .iter()
        .all(|result| result.status == CaseStatus::Passed))
}

/// Whether `code` parses as Python (run in the sandbox with `ast.parse`).
pub async fn parses_as_python(
    runner: &dyn CodeRunner,
    code: &str,
    cancel: CancellationToken,
) -> Result<bool> {
    let entrypoint = runner.entrypoint().to_owned();
    let files = [
        LearningLabFile {
            path: entrypoint.clone(),
            content: PARSES_PY.to_owned(),
        },
        LearningLabFile {
            path: "snippet.txt".to_owned(),
            content: code.to_owned(),
        },
    ];
    let (outcome, _) = runner
        .run_capturing(&files, &[entrypoint], cancel)
        .await?;
    match outcome {
        RunOutcome::Passed => Ok(true),
        RunOutcome::Failed { .. } | RunOutcome::TimedOut { .. } => Ok(false),
        RunOutcome::Cancelled | RunOutcome::Unavailable { .. } => Err(AppError::InvalidState(
            outcome
                .says_nothing()
                .unwrap_or_else(|| "The run did not finish.".to_owned()),
        )),
    }
}
