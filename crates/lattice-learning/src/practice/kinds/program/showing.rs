//! A program as text for a model: the brief, the numbered draft, one line per case and the notes of a run.

use crate::lab_runtime::LearningLabFile;
use crate::practical_dto::CaseCheckDetailDto;
use crate::practice::coding::dto::CaseViewDto;
use crate::practice::coding::problem::{CaseStatus, ProblemClarification, ProblemSignature};
use crate::practice::coding::session::{cut_case, example_view};
use crate::practice::kinds::program::shape::{
    files_of, problem_case_of, run_output_of, run_result_of, spec_of,
};
use crate::practice::session::kind::Showing;
use lattice_core::domain::learning::practice::problem_patterns::problem_pattern;
use lattice_core::domain::learning::practice::work::{Brief, Case, CaseResult, Piece};
use lattice_core::shared::error::Result;

#[cfg(test)]
mod tests;

pub struct ProgramShowing;

/// What the brief block says.
pub(crate) struct BriefFacts<'a> {
    pub title: &'a str,
    pub statement: &'a str,
    pub signature: &'a ProblemSignature,
    pub constraints: &'a [String],
    pub examples: &'a [CaseViewDto],
    pub clarifications: &'a [ProblemClarification],
    pub target_complexity: &'a str,
    pub pattern_names: &'a [String],
}

/// The names of the patterns with these ids, in order; an id the catalogue lacks is left out.
pub(crate) fn pattern_names(ids: &[String]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| problem_pattern(id))
        .map(|pattern| pattern.name.to_owned())
        .collect()
}

/// The brief block: whole lines, each ending in a newline.
pub(crate) fn brief_text(facts: &BriefFacts<'_>) -> String {
    let mut out = String::new();
    let mut line = |text: String| {
        out.push_str(&text);
        out.push('\n');
    };
    line(format!("Problem: {}", facts.title));
    line(facts.statement.to_owned());
    let params = facts
        .signature
        .params
        .iter()
        .map(|param| format!("{}: {}", param.name, param.annotation))
        .collect::<Vec<_>>()
        .join(", ");
    line(format!(
        "Signature: def {}({}) -> {}",
        facts.signature.function, params, facts.signature.returns
    ));
    for constraint in facts.constraints {
        line(format!("Constraint: {constraint}"));
    }
    for case in facts.examples {
        line(format!("Example in the statement: {}", case_text(case)));
    }
    for (index, clarification) in facts.clarifications.iter().enumerate() {
        line(format!(
            "Clarification {index}: asked \"{}\", answered \"{}\"",
            clarification.question, clarification.answer
        ));
    }
    line(format!("Target complexity: {}", facts.target_complexity));
    if !facts.pattern_names.is_empty() {
        line(format!(
            "Techniques the problem is built around: {}",
            facts.pattern_names.join(", ")
        ));
    }
    out
}

/// The draft block: each file under its path, every line numbered.
pub(crate) fn draft_text(files: &[LearningLabFile]) -> String {
    let mut out = String::new();
    for file in files {
        out.push_str(&format!("=== {} ===\n", file.path));
        for (index, text) in file.content.lines().enumerate() {
            out.push_str(&format!("{} | {text}\n", index + 1));
        }
    }
    out
}

/// A case as the learner sees it: its category, then whatever the setting shows of its input and results.
pub(crate) fn case_text(case: &CaseViewDto) -> String {
    let mut parts = vec![case.category.clone()];
    if let Some(args) = &case.args {
        parts.push(format!("input ({})", args.join(", ")));
    }
    if let Some(expected) = &case.expected {
        parts.push(format!("expected {expected}"));
    }
    if let Some(actual) = &case.actual {
        parts.push(format!("got {actual}"));
    }
    if let Some(printed) = case.printed.as_ref().filter(|printed| !printed.is_empty()) {
        parts.push(format!("printed {printed}"));
    }
    if let Some(error) = &case.error {
        parts.push(format!("raised {error}"));
    }
    parts.join("; ")
}

/// What a run printed outside any case, one line each, for what it did print.
pub(crate) fn run_notes(stdout: &str, stderr: &str) -> Vec<String> {
    let mut notes = Vec::new();
    if !stdout.is_empty() {
        notes.push(format!("Printed outside any case: {stdout}"));
    }
    if !stderr.is_empty() {
        notes.push(format!("Error output: {stderr}"));
    }
    notes
}

impl Showing for ProgramShowing {
    fn brief(&self, brief: &Brief, shown: &[Case]) -> Result<String> {
        let spec = spec_of(brief)?;
        let examples = shown
            .iter()
            .map(|case| problem_case_of(case).map(|case| example_view(&case)))
            .collect::<Result<Vec<_>>>()?;
        let names = pattern_names(&spec.patterns);
        Ok(brief_text(&BriefFacts {
            title: &spec.title,
            statement: &spec.statement,
            signature: &spec.signature,
            constraints: &spec.constraints,
            examples: &examples,
            clarifications: &spec.clarifications,
            target_complexity: &spec.target_complexity,
            pattern_names: &names,
        }))
    }

    fn piece(&self, _brief: &Brief, piece: &Piece) -> Result<String> {
        Ok(draft_text(&files_of(piece)?))
    }

    fn case(
        &self,
        _brief: &Brief,
        case: &Case,
        result: Option<&CaseResult>,
        open: bool,
    ) -> Result<String> {
        let body = problem_case_of(case)?;
        let run = result.map(run_result_of).transpose()?;
        let detail = CaseCheckDetailDto {
            case_id: body.id,
            category: body.category,
            visibility: body.visibility,
            scale: body.scale,
            mine: false,
            status: run.as_ref().map_or(CaseStatus::NotRun, |run| run.status),
            args: Some(body.args),
            expected: body.expected,
            actual: run.as_ref().and_then(|run| run.actual.clone()),
            printed: run.as_ref().map(|run| run.printed.clone()),
            error: run.as_ref().and_then(|run| run.error.clone()),
            elapsed_ms: run.as_ref().and_then(|run| run.elapsed_ms),
        };
        Ok(case_text(&cut_case(&detail, open)))
    }

    fn check_notes(&self, _brief: &Brief, detail: &serde_json::Value) -> Result<Vec<String>> {
        let (stdout, stderr) = run_output_of(detail)?;
        Ok(run_notes(&stdout, &stderr))
    }
}
