//! Where a program's problems come from: the author the Program kind drafts with, and the build the evals run.

use crate::lessons::structured_call::Asking;
use crate::model_call;
use crate::practice::coding::problem::{
    CaseCompare, CaseVisibility, Misreading, Problem, ProblemCase, ProblemClarification,
    ProblemDifficulty, ProblemSignature, ProblemSpec, SignatureParam,
};
use crate::practice::kinds::program::shape::{
    brief_of, built_problem_of, case_of, misreading_of, program_form, solution_piece,
};
use crate::practice::kinds::program::toolchain::Toolchains;
use crate::practice::kinds::program::KIND;
use crate::practice::session::author::{Author, Authored, Commission, Lesson};
use crate::practice::session::builder::{build, BuildSteps, Built, Making, NothingStore, Target};
use crate::practice::session::kind::Kind;
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::ask::{self, ask, Refusal, Settled};
use lattice_core::application::services::model_contract::shown::{Internal, Scope, Told};
use lattice_core::domain::learning::practice::problem_patterns::{
    named_in, problem_pattern, PROBLEM_PATTERNS,
};
use lattice_core::domain::learning::practice::work::{Draft as ExerciseDraft, Form, KindId};
use lattice_core::shared::error::{AppError, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

mod answer;
use answer::{
    AuthorProblem, Brief, CaseAnswer, CompareWord, Earlier, Input, Line, Pattern, ProblemAnswer,
    Wanted,
};

#[cfg(test)]
mod tests;

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
    pub form: Form,
    pub language: String,
    pub difficulty: ProblemDifficulty,
    /// Ids from the pattern catalogue that the problem must call for.
    pub patterns: Vec<String>,
    pub focus: Option<String>,
    /// What the learner has said about what they prepare for.
    pub details: Option<String>,
}

/// What a program's build request carries beyond the lesson and the form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramOptions {
    pub language: String,
    pub difficulty: ProblemDifficulty,
    pub focus: Option<String>,
}

/// What a program's store tells its author: the language the problem is written in, the patterns it must call for
/// and what the learner prepares for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramFacts {
    pub language: String,
    pub patterns: Vec<String>,
    pub details: Option<String>,
}

/// The Program kind's author: the model writes a problem for the lesson, and `drafted` refuses what the app can see is
/// wrong without running anything.
pub struct ProgramAuthor {
    pub tools: Arc<dyn Toolchains>,
}

#[async_trait]
impl Author for ProgramAuthor {
    async fn draft(&self, commission: &Commission<'_>) -> Result<Authored> {
        let request = request_of(commission)?;
        let mut wanted = Vec::new();
        for id in &request.patterns {
            let Some(pattern) = problem_pattern(id) else {
                return Err(AppError::InvalidInput(format!(
                    "{id} is not one of the problem patterns."
                )));
            };
            wanted.push(Wanted {
                id: Internal::of(pattern.id.to_owned()),
                name: Told(pattern.name.to_owned()),
                cue: Told(pattern.cue.to_owned()),
            });
        }
        let runner = self.tools.runner(&request.language)?;
        let input = Input {
            difficulty: Told(request.difficulty),
            language: Told(request.language.clone()),
            runtime: Told(runner.contract().to_owned()),
            lesson: request.lesson.as_ref().map(|lesson| Brief {
                title: Told(lesson.title.clone()),
                objective: Told(lesson.objective.clone()),
            }),
            focus: request
                .focus
                .iter()
                .find(|focus| !focus.trim().is_empty())
                .map(|focus| Told(focus.clone())),
            details: request
                .details
                .iter()
                .find(|details| !details.trim().is_empty())
                .map(|details| Told(details.clone())),
            wanted,
            others: PROBLEM_PATTERNS
                .iter()
                .filter(|pattern| !request.patterns.iter().any(|id| id == pattern.id))
                .map(|pattern| Pattern {
                    id: pattern.id.to_owned(),
                    name: Told(pattern.name.to_owned()),
                    cue: Told(pattern.cue.to_owned()),
                })
                .collect(),
            repair: request.form == Form::Repair,
            earlier: commission
                .failed
                .iter()
                .map(|failed| Earlier {
                    answer: Told(failed.raw.clone()),
                    reason: Told(failed.reason.clone()),
                })
                .collect(),
        };
        let settled = ask::<AuthorProblem>(
            commission.llm,
            &Asking {
                progress: None,
                reasoning: None,
            },
            &mut Scope::for_question(),
            &input,
            &model_call::answer_options(),
        )
        .await?;
        let answer = match settled {
            Settled::Answered(answer) => answer,
            Settled::Unanswered(why) => return Err(ask::failed(&why)),
        };
        let problem = drafted(&input, &answer)
            .map_err(|Refusal(reason)| AppError::ModelAnswerUnusable(reason))?;
        Ok(Authored::Draft {
            draft: Box::new(exercise_draft_of(problem, commission.form)?),
            raw: answer_text(&answer),
        })
    }
}

fn parsed<T: DeserializeOwned>(value: &Value, what: &str) -> Result<T> {
    serde_json::from_value(value.clone()).map_err(|error| {
        AppError::InvalidInput(format!(
            "The program's build {what} are not usable: {error}."
        ))
    })
}

/// The problem request a commission stands for.
fn request_of(commission: &Commission<'_>) -> Result<ProblemRequest> {
    let options: ProgramOptions = parsed(commission.options, "options")?;
    let facts: ProgramFacts = parsed(commission.facts, "facts")?;
    Ok(ProblemRequest {
        program_id: commission.program_id.to_owned(),
        lesson: commission.lesson.map(|lesson| LessonBrief {
            title: lesson.title.clone(),
            objective: lesson.objective.clone(),
        }),
        form: program_form(commission.form)?,
        language: facts.language,
        difficulty: options.difficulty,
        patterns: facts.patterns,
        focus: options.focus,
        details: facts.details,
    })
}

/// A drafted problem in the core vocabulary. Fix a bug starts from a misreading the proof picks, so it has no starter.
fn exercise_draft_of(drafted: Draft, form: Form) -> Result<ExerciseDraft> {
    let starter = (form == Form::Build)
        .then(|| solution_piece(&drafted.spec.starter))
        .transpose()?;
    Ok(ExerciseDraft {
        kind: KindId::new(KIND),
        form,
        brief: brief_of(&drafted.spec)?,
        reference: solution_piece(&drafted.reference)?,
        misreadings: drafted
            .misreadings
            .iter()
            .map(misreading_of)
            .collect::<Result<Vec<_>>>()?,
        cases: drafted
            .cases
            .iter()
            .map(case_of)
            .collect::<Result<Vec<_>>>()?,
        starter,
        asks: Vec::new(),
    })
}

/// A problem request written as the build template takes it.
struct RequestParts {
    options: Value,
    facts: Value,
    lesson: Option<Lesson>,
}

fn written(value: &impl Serialize) -> Result<Value> {
    serde_json::to_value(value).map_err(|error| AppError::Serialization(error.to_string()))
}

fn parts_of(request: &ProblemRequest) -> Result<RequestParts> {
    Ok(RequestParts {
        options: written(&ProgramOptions {
            language: request.language.clone(),
            difficulty: request.difficulty,
            focus: request.focus.clone(),
        })?,
        facts: written(&ProgramFacts {
            language: request.language.clone(),
            patterns: request.patterns.clone(),
            details: request.details.clone(),
        })?,
        lesson: request.lesson.as_ref().map(|lesson| Lesson {
            id: String::new(),
            title: lesson.title.clone(),
            objective: lesson.objective.clone(),
        }),
    })
}

/// A store that remembers nothing, for a build whose lesson is not one of the program's.
struct Unremembered;

#[async_trait]
impl NothingStore for Unremembered {
    async fn nothing_known(&self, _target: &Target<'_>) -> Result<Option<String>> {
        Ok(None)
    }

    async fn record_nothing(&self, _target: &Target<'_>, _reason: &str) -> Result<()> {
        Ok(())
    }
}

/// Builds one problem of `request` with the Program kind, without saving it: what the build evals run.
pub async fn build_problem(
    kind: &Kind,
    request: &ProblemRequest,
    llm: &dyn LLMPort,
    steps: &dyn BuildSteps,
    cancel: CancellationToken,
) -> Result<Problem> {
    let parts = parts_of(request)?;
    let making = Making {
        program_id: &request.program_id,
        lesson: parts.lesson.as_ref(),
        form: request.form,
        options: &parts.options,
        facts: &parts.facts,
    };
    match build(kind, &making, llm, &Unremembered, steps, cancel).await? {
        Built::Exercise(exercise) => built_problem_of(&exercise),
        Built::NothingToMake(reason) => Err(AppError::ModelAnswerUnusable(reason)),
    }
}

/// A problem as the model drafted it, before the proof.
struct Draft {
    spec: ProblemSpec,
    reference: String,
    misreadings: Vec<Misreading>,
    cases: Vec<ProblemCase>,
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn refuse(reason: impl Into<String>) -> Refusal {
    Refusal(reason.into())
}

/// The decimal a `tolerance` text holds, when it is greater than zero.
fn tolerance_of(text: &str) -> Option<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|tolerance| tolerance.is_finite() && *tolerance > 0.0)
}

/// The patterns a problem calls for: the ones asked for, then the others the model named.
fn patterns_of(input: &Input, answer: &ProblemAnswer) -> Vec<String> {
    let mut patterns: Vec<String> = input
        .wanted
        .iter()
        .map(|pattern| pattern.id.value().clone())
        .collect();
    for pick in &answer.patterns {
        if !patterns.contains(&pick.reference) {
            patterns.push(pick.reference.clone());
        }
    }
    patterns
}

fn lines_of(lines: &[Line]) -> Vec<String> {
    lines.iter().map(|line| line.text.clone()).collect()
}

/// The draft an answer makes, refusing what the app can see is wrong without running anything.
fn drafted(input: &Input, answer: &ProblemAnswer) -> std::result::Result<Draft, Refusal> {
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
    if !is_identifier(&answer.function_name) {
        return Err(refuse(format!(
            "{} is not a Python function name.",
            answer.function_name
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

    let patterns = patterns_of(input, answer);
    if patterns.is_empty() {
        return Err(refuse(
            "The problem must call for at least one pattern; name it in \"patterns\" by its handle.",
        ));
    }
    let shown = format!("{}\n{}", answer.title, answer.statement);
    if let Some(pattern) = named_in(&shown, &patterns) {
        return Err(refuse(format!(
            "The title or statement names the pattern \"{}\". Describe the problem without naming the technique that solves it.",
            pattern.name
        )));
    }

    let compare = match answer.compare {
        CompareWord::Exact => CaseCompare::Exact,
        CompareWord::Unordered => CaseCompare::Unordered,
        CompareWord::FloatWithin => match tolerance_of(&answer.tolerance) {
            Some(tolerance) => CaseCompare::FloatWithin { tolerance },
            None => {
                return Err(refuse(
                    "compare float_within needs a tolerance greater than zero.",
                ))
            }
        },
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
        let args = lines_of(&scale.args);
        check_args(&id, &scale.category, &args, arity)?;
        cases.push(ProblemCase {
            id,
            category: scale.category.clone(),
            args,
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
        function: answer.function_name.clone(),
        params: answer
            .params
            .iter()
            .map(|param| SignatureParam {
                name: param.name.clone(),
                annotation: param.annotation.clone(),
            })
            .collect(),
        returns: answer.returns.clone(),
    };
    let starter = starter_of(&signature);
    Ok(Draft {
        spec: ProblemSpec {
            title: answer.title.trim().to_owned(),
            statement: answer.statement.trim().to_owned(),
            signature,
            constraints: lines_of(&answer.constraints),
            clarifications: answer
                .clarifications
                .iter()
                .map(|clarification| ProblemClarification {
                    question: clarification.question.clone(),
                    answer: clarification.answer.clone(),
                })
                .collect(),
            language: input.language.0.clone(),
            difficulty: input.difficulty.0,
            target_complexity: answer.target_complexity.clone(),
            patterns,
            starter,
        },
        reference: answer.reference.clone(),
        misreadings,
        cases,
    })
}

/// The answer as the model wrote it, to hand back when the proof refuses it.
fn answer_text(answer: &ProblemAnswer) -> String {
    let lines = |lines: &[Line]| -> Vec<Value> {
        lines
            .iter()
            .map(|line| json!({"text": line.text}))
            .collect()
    };
    let case = |case: &CaseAnswer| json!({"category": case.category, "args": lines(&case.args), "expected": case.expected});
    json!({
        "title": answer.title,
        "statement": answer.statement,
        "functionName": answer.function_name,
        "params": answer.params.iter().map(|param| json!({"name": param.name, "annotation": param.annotation})).collect::<Vec<_>>(),
        "returns": answer.returns,
        "constraints": lines(&answer.constraints),
        "clarifications": answer.clarifications.iter().map(|clarification| json!({"question": clarification.question, "answer": clarification.answer})).collect::<Vec<_>>(),
        "targetComplexity": answer.target_complexity,
        "patterns": answer.patterns.iter().map(|pick| pick.handle.written()).collect::<Vec<_>>(),
        "compare": answer.compare.written(),
        "tolerance": answer.tolerance,
        "reference": answer.reference,
        "misreadings": answer.misreadings.iter().map(|misreading| json!({"description": misreading.description, "code": misreading.code})).collect::<Vec<_>>(),
        "examples": answer.examples.iter().map(case).collect::<Vec<_>>(),
        "hidden": answer.hidden.iter().map(case).collect::<Vec<_>>(),
        "scale": answer.scale.iter().map(|scale| json!({"category": scale.category, "args": lines(&scale.args)})).collect::<Vec<_>>(),
    })
    .to_string()
}

fn check_args(
    id: &str,
    category: &str,
    args: &[String],
    arity: usize,
) -> std::result::Result<(), Refusal> {
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
) -> std::result::Result<ProblemCase, Refusal> {
    let args = lines_of(&answer.args);
    check_args(&id, &answer.category, &args, arity)?;
    if answer.expected.trim().is_empty() {
        return Err(refuse(format!("The case {id} has no expected result.")));
    }
    Ok(ProblemCase {
        id,
        category: answer.category.clone(),
        args,
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
