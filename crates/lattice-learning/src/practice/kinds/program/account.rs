//! A program's session as the debrief's model reads and cites it.

use crate::lab_runtime::LearningLabFile;
use crate::practice::kinds::program::parts::part_of;
use crate::practice::kinds::program::shape::{
    coding_scope_of, files_of, problem_case_of, problem_of, run_result_of,
};
use crate::practice::session::kind::{Account, Citation};
use crate::practice::session::record::SessionCheck;
use lattice_core::domain::learning::practice::work::{CaseStatus, Exercise, Form, PartRef, Piece};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Map, Value};

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

    fn check(
        &self,
        exercise: &Exercise,
        check: &SessionCheck,
        before: Option<&SessionCheck>,
    ) -> Result<Value> {
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
        let files = match before {
            Some(before) if before.piece == check.piece => {
                Value::String(format!("the same files as run {}", before.id))
            }
            _ => files_json(&files_of(&check.piece)?),
        };
        Ok(json!({
            "runId": check.id,
            "scope": tag(&coding_scope_of(check.scope)),
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
            tag: "lines".to_owned(),
            sentence: "Lines: ref is a run id, path is a file of that run, start and end are the first and last line, counted from 1 as the files are numbered below.".to_owned(),
        }
    }

    fn cites_check(&self) -> Citation {
        Citation {
            tag: "run".to_owned(),
            sentence: "A run: ref is a run id.".to_owned(),
        }
    }

    fn fields(&self) -> Vec<(String, Value)> {
        vec![
            ("path".to_owned(), json!({"type": "string"})),
            ("start".to_owned(), json!({"type": "integer"})),
            ("end".to_owned(), json!({"type": "integer"})),
        ]
    }

    fn leave_empty(&self) -> String {
        "Leave the fields a kind does not use empty, or 0 for start and end.".to_owned()
    }

    fn parts(
        &self,
        _exercise: &Exercise,
        check: &SessionCheck,
        fields: &Map<String, Value>,
    ) -> Result<Vec<PartRef>> {
        let path = fields
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let number = |name: &str| fields.get(name).and_then(Value::as_i64).unwrap_or(0);
        let (start, end) = (number("start"), number("end"));
        let files = files_of(&check.piece)?;
        let file = files.iter().find(|file| file.path == path).ok_or_else(|| {
            AppError::InvalidInput(format!("lines: run {} has no file {path}.", check.id))
        })?;
        let total = file.content.lines().count();
        match (u32::try_from(start), u32::try_from(end)) {
            (Ok(first), Ok(last))
                if first >= 1
                    && first <= last
                    && usize::try_from(last).is_ok_and(|last| last <= total) =>
            {
                Ok((first..=last)
                    .map(|line| part_of(&file.path, line))
                    .collect())
            }
            _ => Err(AppError::InvalidInput(format!(
                "lines: {start} to {end} is not within the {total} lines of {} in run {}.",
                file.path, check.id
            ))),
        }
    }
}
