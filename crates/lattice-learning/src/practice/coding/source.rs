//! Where coding problems come from. `GeneratedProblems` has the model write a problem, proves it by running the
//! reference and the misreadings, and asks again with what failed.

use crate::drills::build::ask::{ask_request, refuse};
use crate::drills::runner::CodeRunner;
use crate::generation::parse_json;
use crate::lessons::structured_call::reask;
use crate::model_call;
use crate::practice::coding::harness::CaseHarness;
use crate::practice::coding::problem::{
    CaseCompare, CaseVisibility, CodingTask, Misreading, Problem, ProblemCase,
    ProblemClarification, ProblemDifficulty, ProblemSignature, ProblemSpec, SignatureParam,
};
use crate::practice::coding::proof::{prove, Proof};
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::problem_patterns::{
    named_in, problem_pattern, PROBLEM_PATTERNS,
};
use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

/// The three steps of a build, in the order the job reports them.
pub const STEP_WRITING: &str = "Writing the problem";
pub const STEP_PROVING: &str = "Proving it";
pub const STEP_SAVING: &str = "Saving";

/// How many times a draft that fails its proof is asked for again with what failed. Each attempt is a whole problem,
/// so this is a model-call budget: the design asks that 27 of 30 builds prove within it.
pub const MAX_PROOF_REASKS: usize = 2;

/// The fewest misreadings a draft offers. The proof drops those that pass every case and needs two that fail one.
pub const MIN_MISREADINGS: usize = 3;

/// The fewest cases the statement shows, enough to show the shape of the input and of the answer.
pub const MIN_EXAMPLE_CASES: usize = 2;

/// The fewest hidden cases, one for each edge the misreadings can be told apart by.
pub const MIN_HIDDEN_CASES: usize = 3;

/// The fewest scale cases: one large input is what tests the complexity the statement asks for.
pub const MIN_SCALE_CASES: usize = 1;

/// What an answer reserves: a statement, a reference, several misreadings and about a dozen cases. A reservation
/// sizes the request; an answer cut off at it is asked for again with twice the room.
const PROBLEM_ANSWER_TOKENS: usize = 8_000;

/// The lesson a problem is built to practise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonBrief {
    pub title: String,
    pub objective: String,
}

/// What a problem is built for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProblemRequest {
    pub program_id: String,
    pub lesson: Option<LessonBrief>,
    pub task: CodingTask,
    pub language: String,
    pub difficulty: ProblemDifficulty,
    /// Ids from the pattern catalogue that the problem must call for.
    pub patterns: Vec<String>,
    pub focus: Option<String>,
    /// What the learner has said about what they prepare for.
    pub details: Option<String>,
}

/// Reports a build's progress; false once the build is no longer wanted.
#[async_trait]
pub trait BuildSteps: Send + Sync {
    async fn step(&self, current: u32, message: &str) -> Result<bool>;
}

/// Where a problem comes from. The model writes one today; a pasted problem is the next source.
#[async_trait]
pub trait ProblemSource: Send + Sync {
    async fn problem(
        &self,
        request: &ProblemRequest,
        steps: &dyn BuildSteps,
        cancel: CancellationToken,
    ) -> Result<Problem>;
}

/// Problems the model writes and the app proves.
pub struct GeneratedProblems<'a> {
    pub llm: &'a dyn LLMPort,
    pub runner: &'a dyn CodeRunner,
    pub harness: &'a dyn CaseHarness,
}

#[async_trait]
impl ProblemSource for GeneratedProblems<'_> {
    async fn problem(
        &self,
        request: &ProblemRequest,
        steps: &dyn BuildSteps,
        cancel: CancellationToken,
    ) -> Result<Problem> {
        for id in &request.patterns {
            if problem_pattern(id).is_none() {
                return Err(AppError::InvalidInput(format!(
                    "{id} is not one of the problem patterns."
                )));
            }
        }
        let mut asking = model_call::structured(
            SYSTEM,
            prompt(request, self.runner.contract()),
            schema(),
            PROBLEM_ANSWER_TOKENS,
            "low",
        );
        let mut reasked = 0;
        loop {
            // Progress only moves forward, so a draft asked for again is written during the proving step.
            report(steps, u32::from(reasked > 0), STEP_WRITING).await?;
            let asked = ask_request(self.llm, asking.clone(), |raw| read(raw, request)).await?;
            report(steps, 1, STEP_PROVING).await?;
            let draft = asked.value;
            let proof = prove(
                self.runner,
                self.harness,
                &draft.spec,
                &draft.reference,
                &draft.misreadings,
                &draft.cases,
                cancel.clone(),
            )
            .await?;
            match proof {
                Proof::Proven(proven) => {
                    return assemble(request.task, draft.spec, draft.reference, proven)
                }
                Proof::Unproven(reason) => {
                    if reasked >= MAX_PROOF_REASKS {
                        return Err(AppError::ModelAnswerUnusable(reason));
                    }
                    reasked += 1;
                    asking = reask(asking, &asked.raw, &reason);
                }
            }
        }
    }
}

/// Tells the build's steps where it is; a build nobody waits for any more stops.
async fn report(steps: &dyn BuildSteps, current: u32, message: &str) -> Result<()> {
    if steps.step(current, message).await? {
        Ok(())
    } else {
        Err(AppError::InvalidState(
            "The coding build was stopped.".into(),
        ))
    }
}

/// A problem as the model drafted it, before the proof.
struct Draft {
    spec: ProblemSpec,
    reference: String,
    misreadings: Vec<Misreading>,
    cases: Vec<ProblemCase>,
}

/// How a draft compares a result with what a case expects.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CompareKind {
    Exact,
    Unordered,
    FloatWithin,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MisreadingAnswer {
    description: String,
    code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CaseAnswer {
    category: String,
    args: Vec<String>,
    expected: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScaleAnswer {
    category: String,
    args: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Answer {
    title: String,
    statement: String,
    function: String,
    params: Vec<SignatureParam>,
    returns: String,
    constraints: Vec<String>,
    clarifications: Vec<ProblemClarification>,
    target_complexity: String,
    patterns: Vec<String>,
    compare: CompareKind,
    tolerance: f64,
    reference: String,
    misreadings: Vec<MisreadingAnswer>,
    examples: Vec<CaseAnswer>,
    hidden: Vec<CaseAnswer>,
    scale: Vec<ScaleAnswer>,
}

const SYSTEM: &str = "You write coding problems for a learner to solve, and the checks that prove them. Everything you write is run: the reference solution against every case, and each misreading against every case.";

fn prompt(request: &ProblemRequest, runtime: &str) -> String {
    let difficulty = match request.difficulty {
        ProblemDifficulty::Easy => "easy",
        ProblemDifficulty::Medium => "medium",
        ProblemDifficulty::Hard => "hard",
    };
    let mut parts = vec![format!(
        "Write one {difficulty} {} problem. The learner writes a single function in solution.py. {runtime}",
        request.language
    )];
    if let Some(lesson) = &request.lesson {
        parts.push(format!(
            "It practises the lesson \"{}\", whose objective is: {}",
            lesson.title, lesson.objective
        ));
    }
    if let Some(focus) = request
        .focus
        .as_deref()
        .filter(|focus| !focus.trim().is_empty())
    {
        parts.push(format!("The learner asked to focus on: {focus}"));
    }
    if let Some(details) = request
        .details
        .as_deref()
        .filter(|details| !details.trim().is_empty())
    {
        parts.push(format!("The learner is preparing for: {details}"));
    }
    let required: Vec<String> = request
        .patterns
        .iter()
        .filter_map(|id| problem_pattern(id))
        .map(|pattern| format!("{} ({}): {}", pattern.id, pattern.name, pattern.cue))
        .collect();
    if !required.is_empty() {
        parts.push(format!(
            "The problem must call for these patterns, and its \"patterns\" must list them:\n{}",
            required.join("\n")
        ));
    }
    parts.push(
        "Rules:\n\
- The title and statement must never name a pattern or technique that solves the problem, or hint at one by name. The learner recognises the pattern.\n\
- Arguments and expected results are Python literals, one argument for each parameter in the signature's order. Use built-in types and annotations such as list[int]; import nothing.\n\
- The reference is a complete, correct solution.py that meets the target complexity.\n\
- A misreading is a complete solution.py that a careful learner could write by reading the statement wrongly, or by missing an edge. Write at least three, and make each fail a case you wrote. It must still run.\n\
- Write examples the statement shows, hidden cases that each probe an edge or a misreading, and at least one scale case.\n\
- A scale case gives an argument expression, not a literal, built only from calls of range, list, tuple, dict, set, str, int, len, sum, min, max, sorted, reversed, zip and enumerate, literals and arithmetic, such as list(range(50000)) or [3] * 100000. It has no expected result: the reference's answer is recorded. It is compared exactly, so the problem's answer for it must be one deterministic value, such as a number, a string or a sorted list.\n\
- \"clarifications\" are questions a candidate could ask that the statement leaves open, each with the answer the cases assume."
            .to_owned(),
    );
    if request.task == CodingTask::BugFix {
        parts.push(
            "The learner will be handed one misreading as buggy code to repair, so write each misreading as a small, realistic change to the reference (an off-by-one, a wrong comparison, a missed case) that keeps the function's shape."
                .to_owned(),
        );
    }
    parts.join("\n\n")
}

fn text() -> Value {
    json!({"type": "string", "minLength": 1})
}

fn list_of(items: Value) -> Value {
    json!({"type": "array", "items": items})
}

fn object(properties: Value) -> Value {
    let required: Vec<String> = match &properties {
        Value::Object(map) => map.keys().cloned().collect(),
        _ => Vec::new(),
    };
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": properties
    })
}

/// The schema of an answer. Every item has six properties or fewer: larger item grammars fail on Claude.
fn schema() -> Value {
    let ids: Vec<&str> = PROBLEM_PATTERNS.iter().map(|pattern| pattern.id).collect();
    let case = object(json!({
        "category": text(),
        "args": list_of(text()),
        "expected": text(),
    }));
    object(json!({
        "title": text(),
        "statement": text(),
        "function": text(),
        "params": list_of(object(json!({"name": text(), "annotation": text()}))),
        "returns": text(),
        "constraints": list_of(text()),
        "clarifications": list_of(object(json!({"question": text(), "answer": text()}))),
        "targetComplexity": text(),
        "patterns": {"type": "array", "minItems": 1, "items": {"type": "string", "enum": ids}},
        "compare": {"type": "string", "enum": ["exact", "unordered", "float_within"]},
        "tolerance": {"type": "number"},
        "reference": text(),
        "misreadings": list_of(object(json!({"description": text(), "code": text()}))),
        "examples": list_of(case.clone()),
        "hidden": list_of(case),
        "scale": list_of(object(json!({"category": text(), "args": list_of(text())})))
    }))
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Reads an answer, refusing what the app can see is wrong without running anything.
fn read(raw: &str, request: &ProblemRequest) -> Result<Draft> {
    let answer: Answer = parse_json(raw)?;
    for (value, what) in [
        (&answer.title, "title"),
        (&answer.statement, "statement"),
        (&answer.returns, "returns"),
        (&answer.target_complexity, "targetComplexity"),
        (&answer.reference, "reference"),
    ] {
        if value.trim().is_empty() {
            return Err(refuse(format!("The {what} is empty.")));
        }
    }
    if !is_identifier(&answer.function) {
        return Err(refuse(format!(
            "{} is not a Python function name.",
            answer.function
        )));
    }
    if answer.params.is_empty() {
        return Err(refuse(
            "The function takes no parameters; give it at least one.",
        ));
    }
    let mut names = HashSet::new();
    for param in &answer.params {
        if !is_identifier(&param.name) || !names.insert(param.name.as_str()) {
            return Err(refuse(format!(
                "{} is not a distinct Python parameter name.",
                param.name
            )));
        }
    }

    let mut patterns: Vec<String> = Vec::new();
    for id in &answer.patterns {
        if problem_pattern(id).is_none() {
            return Err(refuse(format!("{id} is not one of the problem patterns.")));
        }
        if !patterns.contains(id) {
            patterns.push(id.clone());
        }
    }
    if let Some(missing) = request.patterns.iter().find(|id| !patterns.contains(id)) {
        return Err(refuse(format!(
            "The problem must call for the pattern {missing}, and \"patterns\" must list it."
        )));
    }
    let shown = format!("{}\n{}", answer.title, answer.statement);
    if let Some(pattern) = named_in(&shown, &patterns) {
        return Err(refuse(format!(
            "The title or statement names the pattern \"{}\". Describe the problem without naming the technique that solves it.",
            pattern.name
        )));
    }

    let compare = match answer.compare {
        CompareKind::Exact => CaseCompare::Exact,
        CompareKind::Unordered => CaseCompare::Unordered,
        CompareKind::FloatWithin => {
            if !answer.tolerance.is_finite() || answer.tolerance <= 0.0 {
                return Err(refuse(
                    "compare float_within needs a tolerance greater than zero.",
                ));
            }
            CaseCompare::FloatWithin {
                tolerance: answer.tolerance,
            }
        }
    };

    for (count, least, what) in [
        (answer.examples.len(), MIN_EXAMPLE_CASES, "examples"),
        (answer.hidden.len(), MIN_HIDDEN_CASES, "hidden cases"),
        (answer.scale.len(), MIN_SCALE_CASES, "scale cases"),
        (answer.misreadings.len(), MIN_MISREADINGS, "misreadings"),
    ] {
        if count < least {
            return Err(refuse(format!(
                "Write at least {least} {what}; there are {count}."
            )));
        }
    }
    let arity = answer.params.len();
    let mut cases = Vec::new();
    for (index, example) in answer.examples.iter().enumerate() {
        cases.push(shown_case(
            format!("example-{}", index + 1),
            CaseVisibility::Example,
            &compare,
            example,
            arity,
        )?);
    }
    for (index, hidden) in answer.hidden.iter().enumerate() {
        cases.push(shown_case(
            format!("hidden-{}", index + 1),
            CaseVisibility::Hidden,
            &compare,
            hidden,
            arity,
        )?);
    }
    for (index, scale) in answer.scale.iter().enumerate() {
        let id = format!("scale-{}", index + 1);
        check_args(&id, &scale.category, &scale.args, arity)?;
        cases.push(ProblemCase {
            id,
            category: scale.category.clone(),
            args: scale.args.clone(),
            expected: None,
            compare: CaseCompare::Exact,
            visibility: CaseVisibility::Hidden,
            scale: true,
            digest: None,
        });
    }

    let mut misreadings = Vec::new();
    for (index, misreading) in answer.misreadings.iter().enumerate() {
        if misreading.description.trim().is_empty() || misreading.code.trim().is_empty() {
            return Err(refuse(format!(
                "Misreading {} needs a description and code.",
                index + 1
            )));
        }
        misreadings.push(Misreading {
            id: format!("misreading-{}", index + 1),
            description: misreading.description.clone(),
            code: misreading.code.clone(),
            killed_by: Vec::new(),
        });
    }

    let signature = ProblemSignature {
        function: answer.function,
        params: answer.params,
        returns: answer.returns,
    };
    let starter = starter_of(&signature);
    Ok(Draft {
        spec: ProblemSpec {
            title: answer.title.trim().to_owned(),
            statement: answer.statement.trim().to_owned(),
            signature,
            constraints: answer.constraints,
            clarifications: answer.clarifications,
            language: request.language.clone(),
            difficulty: request.difficulty,
            target_complexity: answer.target_complexity,
            patterns,
            starter,
        },
        reference: answer.reference,
        misreadings,
        cases,
    })
}

fn check_args(id: &str, category: &str, args: &[String], arity: usize) -> Result<()> {
    if category.trim().is_empty() {
        return Err(refuse(format!("The case {id} has no category.")));
    }
    if args.len() != arity {
        return Err(refuse(format!(
            "The case {id} has {} arguments; the signature has {arity} parameters.",
            args.len()
        )));
    }
    if args.iter().any(|arg| arg.trim().is_empty()) {
        return Err(refuse(format!("The case {id} has an empty argument.")));
    }
    Ok(())
}

fn shown_case(
    id: String,
    visibility: CaseVisibility,
    compare: &CaseCompare,
    answer: &CaseAnswer,
    arity: usize,
) -> Result<ProblemCase> {
    check_args(&id, &answer.category, &answer.args, arity)?;
    if answer.expected.trim().is_empty() {
        return Err(refuse(format!("The case {id} has no expected result.")));
    }
    Ok(ProblemCase {
        id,
        category: answer.category.clone(),
        args: answer.args.clone(),
        expected: Some(answer.expected.clone()),
        compare: compare.clone(),
        visibility,
        scale: false,
        digest: None,
    })
}

/// The `solution.py` a learner starts from: the signature and nothing else.
fn starter_of(signature: &ProblemSignature) -> String {
    let params: Vec<String> = signature
        .params
        .iter()
        .map(|param| {
            if param.annotation.trim().is_empty() {
                param.name.clone()
            } else {
                format!("{}: {}", param.name, param.annotation)
            }
        })
        .collect();
    format!(
        "def {}({}) -> {}:\n    pass\n",
        signature.function,
        params.join(", "),
        signature.returns
    )
}

/// The proven problem. For Fix a bug the starter is the code of the misreading killed by the most cases, the first
/// of those that tie.
fn assemble(
    task: CodingTask,
    mut spec: ProblemSpec,
    reference: String,
    proven: crate::practice::coding::proof::Proven,
) -> Result<Problem> {
    let mut bug = None;
    if task == CodingTask::BugFix {
        let mut chosen: Option<&Misreading> = None;
        for misreading in &proven.misreadings {
            if chosen.is_none_or(|best| misreading.killed_by.len() > best.killed_by.len()) {
                chosen = Some(misreading);
            }
        }
        let chosen = chosen.ok_or_else(|| {
            AppError::InvalidState("A proven problem keeps at least two misreadings.".into())
        })?;
        spec.starter = chosen.code.clone();
        bug = Some(chosen.id.clone());
    }
    Ok(Problem {
        spec,
        reference,
        misreadings: proven.misreadings,
        cases: proven.cases,
        reference_ms: proven.reference_ms,
        bug,
    })
}
