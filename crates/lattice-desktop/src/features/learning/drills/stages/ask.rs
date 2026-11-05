//! The model calls and the checks every stage of a drill build makes the same way.

use super::outputs::Trail;
use crate::features::learning::drills::runner::{CodeRunner, RunOutcome};
use crate::features::learning::lessons::structured_call::{complete_checked_request, reask};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::GroundedRequest;
use lattice_core::domain::learning::drills::{DrillFile, Edit, EditFault};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

/// An answer the caller accepted, and the text it was read from.
pub(in crate::features::learning) struct Asked<V> {
    pub raw: String,
    pub value: V,
}

/// Asks for `request` and reads the answer with `accept`. An answer `accept` refuses is asked for once more in the
/// same conversation, with the reason; a second refusal fails with the second reason.
pub(in crate::features::learning) async fn ask_request<V>(
    llm: &dyn LLMPort,
    request: GroundedRequest,
    accept: impl Fn(&str) -> Result<V>,
) -> Result<Asked<V>> {
    let (raw, value) = complete_checked_request(llm, request, None, None, |raw| {
        accept(raw).map(|value| (raw.to_owned(), value))
    })
    .await?;
    Ok(Asked { raw, value })
}

/// Asks for `request`, reads the answer with `accept`, and where the language has a runner and `to_run` names
/// something to run, runs it. A run that fails sends the answer back once, in the same conversation, with the
/// runner's output; a second failure fails the call. A run that ends without saying anything about the code fails
/// the call as an unavailable service, never as a bad answer.
pub(in crate::features::learning) async fn ask_until_suite_passes<V>(
    llm: &dyn LLMPort,
    runner: Option<&dyn CodeRunner>,
    cancel: &CancellationToken,
    request: GroundedRequest,
    purpose: &str,
    what_runs: &str,
    accept: impl Fn(&str) -> Result<V>,
    to_run: impl Fn(&V) -> Option<(Vec<DrillFile>, String)>,
    trail: &mut Trail,
) -> Result<V> {
    let model = llm.model_name().to_owned();
    let mut request = request;
    let mut again = false;
    loop {
        let asked = ask_request(llm, request.clone(), &accept).await?;
        if again {
            trail.call(&format!("{purpose} (asked again)"), &model);
        } else {
            trail.call(purpose, &model);
        }
        let Some((runner, (files, test))) = runner.zip(to_run(&asked.value)) else {
            return Ok(asked.value);
        };
        let outcome = runner.run(&files, &test, cancel.clone()).await?;
        trail.run(what_runs, &outcome);
        let why = match outcome {
            RunOutcome::Passed => return Ok(asked.value),
            RunOutcome::Failed { output } => {
                format!("{what_runs} failed. The runner's output follows.\n{output}")
            }
            RunOutcome::Inconclusive { why } => {
                return Err(AppError::ServiceNotAvailable(format!(
                    "{what_runs} could not be run to an end: {why}"
                )))
            }
        };
        if again {
            return Err(AppError::ModelAnswerUnusable(why));
        }
        request = reask(request, &asked.raw, &why);
        again = true;
    }
}

/// A refusal the model is asked again with.
pub(in crate::features::learning) fn refuse(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}

/// `text` with every line ending turned into `\n`.
pub(in crate::features::learning) fn normalise(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// The files as a prompt shows them, each between its own tags.
pub(in crate::features::learning) fn render_files(files: &[DrillFile]) -> String {
    files
        .iter()
        .map(|file| format!("<file path=\"{}\">\n{}\n</file>", file.path, file.text))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// What to tell the model about an edit that does not apply: which one, and the change that fixes it. `labels` and
/// `edits` are in the order the edits were applied in.
pub(in crate::features::learning) fn describe_fault(
    fault: EditFault,
    edits: &[Edit],
    labels: &[String],
    paths: &[String],
) -> String {
    let label = |index: usize| labels.get(index).map_or("An edit", String::as_str);
    let path = |index: usize| edits.get(index).map_or("", |edit| edit.path.as_str());
    match fault {
        EditFault::UnknownFile { edit } => format!(
            "{}: {} is not one of the files. Use the path of one of these: {}.",
            label(edit),
            path(edit),
            paths.join(", ")
        ),
        EditFault::Empty { edit } => format!(
            "{}: `before` is empty. Copy the lines being changed into `before`, and keep a neighbouring line in both `before` and `after`.",
            label(edit)
        ),
        EditFault::NotFound { edit } => format!(
            "{}: `before` is not in {}. Copy it from the code exactly, character for character, indentation included.",
            label(edit),
            path(edit)
        ),
        EditFault::Ambiguous { edit, count } => format!(
            "{}: `before` is in {} {count} times. Take in more of the lines around it so that it occurs once.",
            label(edit),
            path(edit)
        ),
        EditFault::Overlaps { first, second } => format!(
            "{} and {} overlap, touch, or end up on one line. Change different parts of the code, with an unchanged line between them.",
            label(first),
            label(second)
        ),
    }
}

/// The schema of a severity.
pub(in crate::features::learning) fn severity_schema() -> Value {
    json!({"type": "string", "enum": ["critical", "major", "minor", "nit"]})
}

/// The schema of an edit's path: one of the files.
pub(in crate::features::learning) fn path_schema(files: &[DrillFile]) -> Value {
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    json!({"type": "string", "enum": paths})
}

/// The schema of a text that must say something.
pub(in crate::features::learning) fn text_schema() -> Value {
    json!({"type": "string", "minLength": 1})
}
