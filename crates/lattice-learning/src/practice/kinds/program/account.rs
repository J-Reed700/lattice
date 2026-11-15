//! A program's session as the debrief's model reads and cites it.

use crate::lab_runtime::LearningLabFile;
use crate::practice::kinds::program::parts::part_of;
use crate::practice::kinds::program::shape::{
    files_of, problem_case_of, problem_of, run_result_of,
};
use crate::practice::session::debrief::Names;
use crate::practice::session::kind::{Account, Citation};
use crate::practice::session::record::SessionCheck;
use lattice_core::domain::learning::practice::work::{
    CaseStatus, CheckScope, Exercise, Form, Piece,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};

/// The word the debrief's model reads for the cases a run covered.
fn scope_word(scope: CheckScope) -> &'static str {
    match scope {
        CheckScope::Shown => "examples",
        CheckScope::All => "all",
        CheckScope::Own => "input",
    }
}

pub struct ProgramAccount;

fn tag<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn numbered(content: &str) -> String {
    content
        .lines()
        .enumerate()
        .map(|(at, line)| format!("{}: {line}", at + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn files_json(files: &[LearningLabFile]) -> Value {
    Value::Array(
        files
            .iter()
            .map(|file| json!({"path": file.path, "numbered": numbered(&file.content)}))
            .collect(),
    )
}

/// The files of a run, one line to a row: the name of the line, its number in the file, then the line.
fn named_files_json(files: &[LearningLabFile], names: &Names) -> Value {
    Value::Array(
        files
            .iter()
            .map(|file| {
                let lines = file
                    .content
                    .lines()
                    .enumerate()
                    .filter_map(|(at, line)| {
                        let number = u32::try_from(at + 1).ok()?;
                        Some(format!(
                            "{} {number}: {line}",
                            names.part(&part_of(&file.path, number))
                        ))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                json!({"path": file.path, "lines": lines})
            })
            .collect(),
    )
}

impl Account for ProgramAccount {
    fn exercise_key(&self) -> String {
        "problem".to_owned()
    }

    fn exercise(&self, exercise: &Exercise) -> Result<Value> {
        let problem = problem_of(exercise)?;
        let spec = &problem.spec;
        Ok(json!({
            "title": spec.title,
            "statement": spec.statement,
            "function": spec.signature.function,
            "parameters": spec.signature.params.iter().map(|param| json!({"name": param.name, "annotation": param.annotation})).collect::<Vec<_>>(),
            "returns": spec.signature.returns,
            "constraints": spec.constraints,
            "clarifications": spec.clarifications.iter().map(|c| json!({"question": c.question, "answer": c.answer})).collect::<Vec<_>>(),
            "targetComplexity": spec.target_complexity,
            "referenceSolution": problem.reference,
        }))
    }

    fn form_key(&self) -> String {
        "task".to_owned()
    }

    fn form(&self, form: Form) -> Result<Value> {
        match form {
            Form::Build => Ok(Value::String("problem".to_owned())),
            Form::Repair => Ok(Value::String("bug_fix".to_owned())),
            Form::Complete | Form::Arrange => Err(AppError::InvalidState(format!(
                "A program has no {} exercises.",
                form.label()
            ))),
        }
    }

    fn checks_key(&self) -> String {
        "runs".to_owned()
    }

    fn check(&self, exercise: &Exercise, check: &SessionCheck, names: &Names) -> Result<Value> {
        let mut failing = Vec::new();
        for result in &check.report.results {
            if matches!(result.status, CaseStatus::Passed | CaseStatus::Ran) {
                continue;
            }
            let case = exercise
                .cases
                .iter()
                .find(|case| case.id == result.case_id)
                .map(problem_case_of)
                .transpose()?;
            let ran = run_result_of(result)?;
            let shown = case.as_ref().filter(|case| !case.scale);
            failing.push(json!({
                "category": case.as_ref().map_or(result.case_id.as_str(), |case| case.category.as_str()),
                "status": tag(&ran.status),
                "expected": shown.and_then(|case| case.expected.as_deref()),
                "actual": shown.and(ran.actual.as_deref()),
                "error": ran.error,
            }));
        }
        let files = match names.same_as() {
            Some(before) => Value::String(format!("the same files as run {before}")),
            None => named_files_json(&files_of(&check.piece)?, names),
        };
        Ok(json!({
            "runId": names.check(),
            "scope": scope_word(check.scope),
            "passed": check.report.results.iter().filter(|result| result.status == CaseStatus::Passed).count(),
            "total": check.report.results.len(),
            "failing": failing,
            "files": files,
        }))
    }

    fn piece(&self, _exercise: &Exercise, piece: &Piece) -> Result<Value> {
        Ok(files_json(&files_of(piece)?))
    }

    fn cites_parts(&self) -> Citation {
        Citation {
            sentence: "Lines: parts holds the names of the lines of a run's files it cites."
                .to_owned(),
        }
    }

    fn cites_check(&self) -> Citation {
        Citation {
            sentence: "A run: checks holds the names of the runs it cites.".to_owned(),
        }
    }
}
