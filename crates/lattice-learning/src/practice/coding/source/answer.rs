//! The Program author's contract: what the model is asked for a problem, and the form its answer takes.

use super::{drafted, PROBLEM_ANSWER_TOKENS};
use crate::practice::coding::problem::ProblemDifficulty;
use lattice_core::application::services::grounded_generation::HistoryItem;
use lattice_core::application::services::model_contract::ask::{Alone, Answered, Refusal, Says};
use lattice_core::application::services::model_contract::shape::{
    Authored, Fields, Form, Value, Vocabulary,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Showing, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId, CORRECTION_LEAD};
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};

/// The model call that writes one coding problem.
pub(crate) struct AuthorProblem;

impl Contract for AuthorProblem {
    const ID: ContractId = ContractId::new("program-author");
    type Input = Input;
}

/// The lesson a problem is written to practise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Brief {
    pub(super) title: Told<String>,
    pub(super) objective: Told<String>,
}

/// A pattern the app asks the problem to call for. The model is told it and does not write it back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Wanted {
    pub(super) id: Internal<String>,
    pub(super) name: Told<String>,
    pub(super) cue: Told<String>,
}

/// A pattern of the catalogue the problem may also call for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Pattern {
    pub(super) id: String,
    pub(super) name: Told<String>,
    pub(super) cue: Told<String>,
}

impl Kind for Pattern {
    const PREFIX: &'static str = "t";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.clone()
    }
}

/// A draft the proof refused: what the model wrote, and why it cannot stand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Earlier {
    pub(super) answer: Told<String>,
    pub(super) reason: Told<String>,
}

/// What one request says.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Input {
    pub(super) difficulty: Told<ProblemDifficulty>,
    pub(super) language: Told<String>,
    pub(super) runtime: Told<String>,
    pub(super) lesson: Option<Brief>,
    pub(super) focus: Option<Told<String>>,
    pub(super) details: Option<Told<String>>,
    pub(super) wanted: Vec<Wanted>,
    pub(super) others: Vec<Pattern>,
    pub(super) repair: bool,
    pub(super) earlier: Vec<Earlier>,
}

/// One written line of a list: a constraint or an argument.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Line {
    pub(super) text: String,
}

impl Authored for Line {
    fn form() -> Form {
        Form::new().text("text")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            text: fields.text("text")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParamAnswer {
    pub(super) name: String,
    pub(super) annotation: String,
}

impl Authored for ParamAnswer {
    fn form() -> Form {
        Form::new().text("name").text("annotation")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            name: fields.text("name")?,
            annotation: fields.text("annotation")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ClarificationAnswer {
    pub(super) question: String,
    pub(super) answer: String,
}

impl Authored for ClarificationAnswer {
    fn form() -> Form {
        Form::new().text("question").text("answer")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            question: fields.text("question")?,
            answer: fields.text("answer")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MisreadingAnswer {
    pub(super) description: String,
    pub(super) code: String,
}

impl Authored for MisreadingAnswer {
    fn form() -> Form {
        Form::new().text("description").text("code")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            description: fields.text("description")?,
            code: fields.text("code")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CaseAnswer {
    pub(super) category: String,
    pub(super) args: Vec<Line>,
    pub(super) expected: String,
}

impl Authored for CaseAnswer {
    fn form() -> Form {
        Form::new()
            .text("category")
            .list::<Line>("args")
            .text("expected")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            category: fields.text("category")?,
            args: fields.list("args")?,
            expected: fields.text("expected")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScaleAnswer {
    pub(super) category: String,
    pub(super) args: Vec<Line>,
}

impl Authored for ScaleAnswer {
    fn form() -> Form {
        Form::new().text("category").list::<Line>("args")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            category: fields.text("category")?,
            args: fields.list("args")?,
        })
    }
}

/// How a draft compares a result with what a case expects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum CompareWord {
    Exact,
    Unordered,
    FloatWithin,
}

impl Vocabulary for CompareWord {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("exact", Self::Exact),
        ("unordered", Self::Unordered),
        ("float_within", Self::FloatWithin),
    ];
}

impl CompareWord {
    /// The word the model writes for it.
    pub(super) fn written(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Unordered => "unordered",
            Self::FloatWithin => "float_within",
        }
    }
}

/// The problem the model wrote.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProblemAnswer {
    pub(super) title: String,
    pub(super) statement: String,
    pub(super) function_name: String,
    pub(super) params: Vec<ParamAnswer>,
    pub(super) returns: String,
    pub(super) constraints: Vec<Line>,
    pub(super) clarifications: Vec<ClarificationAnswer>,
    pub(super) target_complexity: String,
    pub(super) patterns: Vec<Picked<Pattern>>,
    pub(super) compare: CompareWord,
    pub(super) tolerance: String,
    pub(super) reference: String,
    pub(super) misreadings: Vec<MisreadingAnswer>,
    pub(super) examples: Vec<CaseAnswer>,
    pub(super) hidden: Vec<CaseAnswer>,
    pub(super) scale: Vec<ScaleAnswer>,
}

impl Authored for ProblemAnswer {
    fn form() -> Form {
        Form::new()
            .text("title")
            .text("statement")
            .text("functionName")
            .list::<ParamAnswer>("params")
            .text("returns")
            .list::<Line>("constraints")
            .list::<ClarificationAnswer>("clarifications")
            .text("targetComplexity")
            .cite::<Pattern>("patterns")
            .word::<CompareWord>("compare")
            .text("tolerance")
            .text("reference")
            .list::<MisreadingAnswer>("misreadings")
            .list::<CaseAnswer>("examples")
            .list::<CaseAnswer>("hidden")
            .list::<ScaleAnswer>("scale")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            title: fields.text("title")?,
            statement: fields.text("statement")?,
            function_name: fields.text("functionName")?,
            params: fields.list("params")?,
            returns: fields.text("returns")?,
            constraints: fields.list("constraints")?,
            clarifications: fields.list("clarifications")?,
            target_complexity: fields.text("targetComplexity")?,
            patterns: fields.cite("patterns"),
            compare: fields.word("compare")?,
            tolerance: fields.text("tolerance")?,
            reference: fields.text("reference")?,
            misreadings: fields.list("misreadings")?,
            examples: fields.list("examples")?,
            hidden: fields.list("hidden")?,
            scale: fields.list("scale")?,
        })
    }
}

const SYSTEM: &str = "You write coding problems for a learner to solve, and the checks that prove them. Everything you write is run: the reference solution against every case, and each misreading against every case.";

const FORM: &str = " Write title, statement, functionName, params (each with a name and an annotation), returns, constraints (each with a text), clarifications (each with a question and an answer), targetComplexity, patterns, compare, tolerance, reference, misreadings (each with a description and code), examples and hidden (each with a category, args and expected) and scale (each with a category and args). Each of args holds one object with a text for each argument. In patterns write the handles of the patterns the problem calls for, from the list shown. Write tolerance as a decimal number, 0 unless compare is float_within.";

const RULES: &str = "Rules:\n\
- The title and statement must never name a pattern or technique that solves the problem, or hint at one by name. The learner recognises the pattern.\n\
- Arguments and expected results are Python literals, one argument for each parameter in the signature's order. Use built-in types and annotations such as list[int]; import nothing.\n\
- The reference is a complete, correct solution.py that meets the target complexity.\n\
- A misreading is a complete solution.py that a careful learner could write by reading the statement wrongly, or by missing an edge. Write at least three, and make each fail a case you wrote. It must still run.\n\
- Write examples the statement shows, hidden cases that each probe an edge or a misreading, and at least one scale case.\n\
- A scale case gives an argument expression, not a literal, built only from calls of range, list, tuple, dict, set, str, int, len, sum, min, max, sorted, reversed, zip and enumerate, literals, arithmetic, comparisons, conditional expressions, indexing and comprehensions, such as list(range(50000)), [3] * 100000 or [i * 7919 % 100003 for i in range(100000)]. It has no expected result: the reference's answer is recorded. It is compared exactly, so the problem's answer for it must be one deterministic value, such as a number, a string or a sorted list.\n\
- \"clarifications\" are questions a candidate could ask that the statement leaves open, each with the answer the cases assume.";

const REPAIR: &str = "The learner will be handed one misreading as buggy code to repair, so write each misreading as a small, realistic change to the reference (an off-by-one, a wrong comparison, a missed case) that keeps the function's shape.";

impl Answered for AuthorProblem {
    type Answer = Value<ProblemAnswer>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let difficulty = match input.difficulty.0 {
            ProblemDifficulty::Easy => "easy",
            ProblemDifficulty::Medium => "medium",
            ProblemDifficulty::Hard => "hard",
        };
        let mut parts = vec![format!(
            "Write one {difficulty} {} problem. The learner writes a single function in solution.py. {}",
            input.language.0, input.runtime.0
        )];
        if let Some(lesson) = &input.lesson {
            parts.push(format!(
                "It practises the lesson \"{}\", whose objective is: {}",
                lesson.title.0, lesson.objective.0
            ));
        }
        if let Some(focus) = &input.focus {
            parts.push(format!("The learner asked to focus on: {}", focus.0));
        }
        if let Some(details) = &input.details {
            parts.push(format!("The learner is preparing for: {}", details.0));
        }
        if !input.wanted.is_empty() {
            let required: Vec<String> = input
                .wanted
                .iter()
                .map(|pattern| format!("{}: {}", pattern.name.0, pattern.cue.0))
                .collect();
            parts.push(format!(
                "The problem must call for these patterns:\n{}",
                required.join("\n")
            ));
        }
        if !input.others.is_empty() {
            let shown = showing.show(&input.others)?;
            let listed: Vec<String> = shown
                .iter()
                .map(|(handle, pattern)| {
                    format!(
                        "{} {}: {}",
                        handle.bracketed(),
                        pattern.name.0,
                        pattern.cue.0
                    )
                })
                .collect();
            let lead = if input.wanted.is_empty() {
                "The problem must call for at least one of these patterns, each named by the handle written before it:"
            } else {
                "The problem may also call for other patterns, each named by the handle written before it:"
            };
            parts.push(format!("{lead}\n{}", listed.join("\n")));
        }
        parts.push(RULES.to_owned());
        if input.repair {
            parts.push(REPAIR.to_owned());
        }
        let (history, task) = conversation(parts.join("\n\n"), &input.earlier);
        let mut says = Says::new(format!("{SYSTEM}{FORM}"), task);
        says.history = history;
        says.output_tokens = PROBLEM_ANSWER_TOKENS;
        Ok(says)
    }

    fn accept(input: &Input, answer: &ProblemAnswer) -> std::result::Result<(), Refusal> {
        drafted(input, answer).map(|_| ())
    }
}

/// The task, with each draft the proof refused answered in the conversation: the history holds the task, then
/// each refused draft and the correction that followed all but the last, which is the task asked again.
fn conversation(task: String, earlier: &[Earlier]) -> (Vec<HistoryItem>, String) {
    let mut history = Vec::new();
    let mut asking = task;
    for refused in earlier {
        history.push(HistoryItem {
            role: "user".into(),
            content: asking,
        });
        history.push(HistoryItem {
            role: "assistant".into(),
            content: refused.answer.0.clone(),
        });
        asking = format!(
            "{CORRECTION_LEAD} {} Answer the same task again, fixing that.",
            refused.reason.0
        );
    }
    (history, asking)
}
