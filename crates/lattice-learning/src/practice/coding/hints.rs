//! Coding hints: a ladder of four rungs, each written with the reference in view and checked before the learner
//! sees it.

use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::ask::{
    self, ask, Alone, Answered, Refusal, Says, Settled,
};
use lattice_core::application::services::model_contract::shape::{Authored, Fields, Form, Value};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Scope, Showing, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::domain::learning::practice::named_in;
use lattice_core::domain::learning::practice::work::{Rung, Setting};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use tokio_util::sync::CancellationToken;

use crate::drills::runner::CodeRunner;
use crate::lab_runtime::LearningLabFile;
use crate::lessons::structured_call::Asking;
use crate::model_call;
use crate::practice::coding::harness::CaseHarness;
use crate::practice::coding::problem::Problem;
use crate::practice::coding::solution_check::{code_blocks, parses_as_python, passes_every_case};

/// What a hint's answer reserves: a few paragraphs, a short fenced block, and the frame around them. A
/// reservation sizes the request; it never limits what the model writes.
const HINT_TOKENS: usize = 1_600;

/// Python's `keyword.kwlist`.
const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

/// A line of one of the candidate's files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    pub path: String,
    /// 1-based.
    pub line: u32,
}

/// A hint that passed its checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeHint {
    pub rung: Rung,
    pub text: String,
    pub reason: String,
    pub lines: Vec<Place>,
}

/// Writes the hint for `rung` on what the observation says, checks it, and asks once more with the reason when a
/// check refuses it. A second refusal fails with `AppError::ModelAnswerUnusable`. `observed` is the observation's
/// text; `setting` and `draft` are what the checks read of the session.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_hint_from(
    llm: &dyn LLMPort,
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    problem: &Problem,
    observed: &str,
    setting: Setting,
    draft: &[LearningLabFile],
    rung: Rung,
    in_role: bool,
    cancel: CancellationToken,
) -> Result<CodeHint> {
    let input = Input::of(
        &problem.reference,
        &problem.spec.starter,
        &problem.spec.patterns,
        observed,
        setting,
        draft,
        rung,
        in_role,
    );
    let proof = Sandbox {
        runner,
        harness,
        problem,
        cancel: &cancel,
    };
    write_checked(llm, input, &proof).await
}

/// The two things a check runs: code as a parse, and code as the solution.
#[async_trait]
trait Proof: Send + Sync {
    async fn parses(&self, code: &str) -> Result<bool>;
    async fn passes(&self, code: &str) -> Result<bool>;
}

struct Sandbox<'a> {
    runner: &'a dyn CodeRunner,
    harness: &'a dyn CaseHarness,
    problem: &'a Problem,
    cancel: &'a CancellationToken,
}

#[async_trait]
impl Proof for Sandbox<'_> {
    async fn parses(&self, code: &str) -> Result<bool> {
        parses_as_python(self.runner, code, self.cancel.clone()).await
    }

    async fn passes(&self, code: &str) -> Result<bool> {
        passes_every_case(
            self.runner,
            self.harness,
            self.problem,
            code,
            self.cancel.clone(),
        )
        .await
    }
}

fn rung_name(rung: Rung) -> &'static str {
    match rung {
        Rung::Nudge => "Nudge",
        Rung::Idea => "Idea",
        Rung::Plan => "Plan",
        Rung::Point => "Line",
    }
}

/// Whether a reference line says anything beyond Python's own words and punctuation.
fn says_something(line: &str) -> bool {
    line.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .any(|word| !PYTHON_KEYWORDS.contains(&word))
}

/// The first reference line the hint repeats word for word, as a 1-based line number. Lines also in the starter
/// and lines of Python keywords and punctuation alone do not count.
fn repeated_reference_line(input: &Input, shown: &[&str]) -> Option<usize> {
    input
        .reference
        .0
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, line)| says_something(line))
        .filter(|(_, line)| {
            !input
                .starter
                .value()
                .lines()
                .any(|kept| kept.trim() == *line)
        })
        .find(|(_, line)| shown.iter().any(|text| text.contains(line)))
        .map(|(number, _)| number)
}

/// Only a coach's Idea hint may name the technique.
fn may_name_technique(setting: Setting, rung: Rung) -> bool {
    setting == Setting::Coached && rung == Rung::Idea
}

/// Why a rule of what the model wrote refuses a hint of `text`, `reason` and `named` lines, or None when none
/// does.
fn refusal(input: &Input, text: &str, reason: &str, named: usize) -> Option<String> {
    if text.trim().is_empty() || reason.trim().is_empty() {
        return Some("The hint needs both its text and its reason.".into());
    }
    if matches!(input.rung, Rung::Nudge | Rung::Idea) && !code_blocks(text).is_empty() {
        return Some(format!(
            "A {} hint holds no fenced code block. Say it in words.",
            rung_name(input.rung)
        ));
    }
    if input.rung == Rung::Point && named == 0 {
        return Some("A Line hint names at least one line of the candidate's code.".into());
    }
    let shown = [text, reason];
    if let Some(number) = repeated_reference_line(input, &shown) {
        return Some(format!(
            "The hint repeats line {number} of the reference solution word for word. Say it in other words."
        ));
    }
    if !may_name_technique(input.setting, input.rung) {
        let named = shown
            .iter()
            .find_map(|text| named_in(text, input.patterns.value()));
        if let Some(pattern) = named {
            return Some(format!(
                "The hint names the pattern \"{}\". Describe what to do without naming the technique.",
                pattern.name
            ));
        }
    }
    None
}

/// Why running the code blocks of `text` refuses a hint of `rung`, or None when no block does. Errors are runs
/// that could not be made.
async fn unproven(proof: &dyn Proof, rung: Rung, text: &str) -> Result<Option<String>> {
    for block in &code_blocks(text) {
        if rung == Rung::Plan {
            if proof.parses(block).await? {
                return Ok(Some(
                    "A block in a Plan hint parses as Python. Write the steps as pseudo-code, or without a block."
                        .into(),
                ));
            }
        } else if proof.passes(block).await? {
            return Ok(Some(
                "A block in the hint solves the problem. Show a fragment at most, or none.".into(),
            ));
        }
    }
    Ok(None)
}

fn rung_instruction(rung: Rung, setting: Setting) -> &'static str {
    match rung {
        Rung::Nudge => {
            "Ask one question that points at what to look at. Give no code and no fenced block."
        }
        Rung::Idea if setting == Setting::Coached => {
            "Give the idea or structure that fits, and why. You may name the technique. Give no fenced block."
        }
        Rung::Idea => {
            "Give the idea or structure that fits, and why, without naming the technique. Give no fenced block."
        }
        Rung::Plan => {
            "Give numbered steps in plain language, each with its reason. A fenced block, if any, is pseudo-code that is not valid Python."
        }
        Rung::Point => {
            "Point at the lines of the candidate's code that are wrong, and say why. Set `lines` to their handles. Never write the corrected line."
        }
    }
}

struct HintAuthor;

impl Contract for HintAuthor {
    const ID: ContractId = ContractId::new("coding-hint");
    type Input = Input;
}

/// One line of the candidate's code, as a Line hint may point at it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Line {
    path: Told<String>,
    number: u32,
    text: Told<String>,
}

impl Kind for Line {
    const PREFIX: &'static str = "l";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = Place;

    fn reference(&self) -> Place {
        Place {
            path: self.path.0.clone(),
            line: self.number,
        }
    }
}

/// A hint a check refused, and why.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Earlier {
    hint: Told<String>,
    why: Told<String>,
}

/// What one request says about the hint to write.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Input {
    reference: Told<String>,
    observed: Told<String>,
    starter: Internal<String>,
    patterns: Internal<Vec<String>>,
    setting: Setting,
    rung: Rung,
    in_role: bool,
    /// The lines a Line hint may point at; none for another rung.
    lines: Vec<Line>,
    earlier: Option<Earlier>,
}

impl Input {
    #[allow(clippy::too_many_arguments)]
    fn of(
        reference: &str,
        starter: &str,
        patterns: &[String],
        observed: &str,
        setting: Setting,
        draft: &[LearningLabFile],
        rung: Rung,
        in_role: bool,
    ) -> Self {
        let lines = if rung == Rung::Point {
            draft
                .iter()
                .flat_map(|file| {
                    file.content
                        .lines()
                        .enumerate()
                        .filter_map(|(index, text)| {
                            Some(Line {
                                path: Told(file.path.clone()),
                                number: u32::try_from(index + 1).ok()?,
                                text: Told(text.to_owned()),
                            })
                        })
                })
                .collect()
        } else {
            Vec::new()
        };
        Self {
            reference: Told(reference.to_owned()),
            observed: Told(observed.to_owned()),
            starter: Internal::of(starter.to_owned()),
            patterns: Internal::of(patterns.to_vec()),
            setting,
            rung,
            in_role,
            lines,
            earlier: None,
        }
    }
}

/// What the model wrote for one hint.
struct Said {
    text: String,
    reason: String,
    lines: Vec<Picked<Line>>,
}

impl Authored for Said {
    fn form() -> Form {
        Form::new()
            .text("text")
            .text("reason")
            .cite::<Line>("lines")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            text: fields.text("text")?,
            reason: fields.text("reason")?,
            lines: fields.cite::<Line>("lines"),
        })
    }
}

impl Answered for HintAuthor {
    type Answer = Value<Said>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let lines = showing.show(&input.lines)?;
        let voice = if input.in_role {
            "You are the interviewer in a coding interview. Write the hint as the words you say to the candidate, in your own voice, staying in role."
        } else if input.setting == Setting::Coached {
            "You are a coach helping a learner through a coding problem."
        } else {
            "You give a candidate one hint on a coding problem."
        };
        let naming = if may_name_technique(input.setting, input.rung) {
            ""
        } else {
            " Never name the technique or pattern that solves the problem."
        };
        let system = format!(
            "{voice} The reference solution is private: it is there so that your hint is right, never to be quoted. Never write a line of it, and never write code that solves the problem.{naming} Speak about the candidate's code only from what they have written and run. Fill `lines` only for a Line hint; otherwise give `lines` as an empty list."
        );
        let prompt = format!(
            "Reference solution (private):\n{}\n\nWhat the candidate sees and has done:\n{}\n\nWrite a {} hint. {}",
            input.reference.0,
            input.observed.0,
            rung_name(input.rung),
            rung_instruction(input.rung, input.setting)
        );
        let mut closing = Vec::new();
        if !lines.is_empty() {
            let mut rows = String::from(
                "Lines of the candidate's code you may point at, each under its handle:",
            );
            let mut path: Option<&str> = None;
            for (handle, line) in lines.iter() {
                if path != Some(line.path.0.as_str()) {
                    let _ = write!(rows, "\n=== {} ===", line.path.0);
                    path = Some(line.path.0.as_str());
                }
                let _ = write!(rows, "\n{} {}", handle.bracketed(), line.text.0);
            }
            closing.push(rows);
        }
        if let Some(earlier) = &input.earlier {
            closing.push(format!(
                "Your earlier hint was refused.\nYour earlier hint: {}\nWhy it was refused: {}\nWrite the hint again.",
                earlier.hint.0, earlier.why.0
            ));
        }
        let mut says = Says::new(system, prompt);
        says.closing = closing.join("\n\n");
        says.output_tokens = HINT_TOKENS;
        Ok(says)
    }

    fn accept(input: &Input, said: &Said) -> std::result::Result<(), Refusal> {
        match refusal(input, &said.text, &said.reason, said.lines.len()) {
            Some(why) => Err(Refusal(why)),
            None => Ok(()),
        }
    }
}

/// Asks for a hint, and asks once more with the reason when running its code refuses it.
async fn write_checked(llm: &dyn LLMPort, mut input: Input, proof: &dyn Proof) -> Result<CodeHint> {
    let mut scope = Scope::for_question();
    let mut again = false;
    loop {
        let settled = ask::<HintAuthor>(
            llm,
            &Asking {
                progress: None,
                reasoning: None,
            },
            &mut scope,
            &input,
            &model_call::answer_options(),
        )
        .await?;
        let said = match settled {
            Settled::Answered(said) => said,
            Settled::Unanswered(why) => return Err(ask::failed(&why)),
        };
        let hint = CodeHint {
            rung: input.rung,
            text: said.text,
            reason: said.reason,
            lines: if input.rung == Rung::Point {
                said.lines.into_iter().map(|line| line.reference).collect()
            } else {
                Vec::new()
            },
        };
        let Some(why) = unproven(proof, hint.rung, &hint.text).await? else {
            return Ok(hint);
        };
        if again {
            return Err(AppError::ModelAnswerUnusable(why));
        }
        input.earlier = Some(Earlier {
            hint: Told(hint.text),
            why: Told(why),
        });
        again = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
    use lattice_core::domain::learning::practice::PROBLEM_PATTERNS;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct Fake;

    #[async_trait]
    impl Proof for Fake {
        async fn parses(&self, code: &str) -> Result<bool> {
            Ok(code.contains("PARSES"))
        }
        async fn passes(&self, code: &str) -> Result<bool> {
            Ok(code.contains("PASSES"))
        }
    }

    struct Fixture {
        reference: String,
        starter: String,
        patterns: Vec<String>,
        draft: Vec<LearningLabFile>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                reference: concat!(
                    "def two_sum(numbers, target):\n",
                    "    seen = {}\n",
                    "    for i, n in enumerate(numbers):\n",
                    "        if target - n in seen:\n",
                    "            return [seen[target - n], i]\n",
                    "        seen[n] = i\n",
                    "    return []\n"
                )
                .into(),
                starter: "def two_sum(numbers, target):\n    pass\n".into(),
                patterns: PROBLEM_PATTERNS
                    .first()
                    .map(|pattern| vec![pattern.id.to_owned()])
                    .unwrap_or_default(),
                draft: vec![LearningLabFile {
                    path: "solution.py".into(),
                    content: concat!(
                        "def two_sum(numbers, target):\n",
                        "    for i in numbers:\n",
                        "        return [0]\n",
                        "    return []\n"
                    )
                    .into(),
                }],
            }
        }

        fn input(&self, setting: Setting, rung: Rung) -> Input {
            Input::of(
                &self.reference,
                &self.starter,
                &self.patterns,
                "{}",
                setting,
                &self.draft,
                rung,
                false,
            )
        }

        fn pattern_name(&self) -> &'static str {
            PROBLEM_PATTERNS.first().map_or("", |pattern| pattern.name)
        }
    }

    fn hint(rung: Rung, text: &str) -> CodeHint {
        CodeHint {
            rung,
            text: text.into(),
            reason: "It points at what the code does not yet do.".into(),
            lines: Vec::new(),
        }
    }

    fn place(line: u32) -> Place {
        Place {
            path: "solution.py".into(),
            line,
        }
    }

    fn line_hint(lines: Vec<Place>) -> CodeHint {
        CodeHint {
            lines,
            ..hint(Rung::Point, "This return ends the loop on the first pass.")
        }
    }

    async fn refused(fixture: &Fixture, setting: Setting, hint: &CodeHint) -> Option<String> {
        let input = fixture.input(setting, hint.rung);
        if let Some(why) = refusal(&input, &hint.text, &hint.reason, hint.lines.len()) {
            return Some(why);
        }
        unproven(&Fake, hint.rung, &hint.text)
            .await
            .expect("the checks run")
    }

    const CLEAN: &str = "Think about what you need to remember while you walk the list once.";

    #[tokio::test]
    async fn a_clean_hint_of_every_rung_is_accepted() {
        let fixture = Fixture::new();
        for rung in [Rung::Nudge, Rung::Idea, Rung::Plan] {
            assert_eq!(
                refused(&fixture, Setting::Mock, &hint(rung, CLEAN)).await,
                None
            );
        }
        let line = line_hint(vec![place(3)]);
        assert_eq!(refused(&fixture, Setting::Mock, &line).await, None);
    }

    #[tokio::test]
    async fn blank_text_or_a_blank_reason_is_refused() {
        let fixture = Fixture::new();
        let blank_text = hint(Rung::Nudge, "  \n ");
        assert!(refused(&fixture, Setting::Mock, &blank_text)
            .await
            .is_some());
        let blank_reason = CodeHint {
            reason: " ".into(),
            ..hint(Rung::Nudge, CLEAN)
        };
        assert!(refused(&fixture, Setting::Mock, &blank_reason)
            .await
            .is_some());
    }

    #[tokio::test]
    async fn nudge_and_idea_hold_no_fenced_block() {
        let fixture = Fixture::new();
        let fenced = "Remember them:\n```python\nx\n```";
        for rung in [Rung::Nudge, Rung::Idea] {
            assert!(refused(&fixture, Setting::Mock, &hint(rung, fenced))
                .await
                .is_some());
            assert_eq!(
                refused(&fixture, Setting::Mock, &hint(rung, CLEAN)).await,
                None
            );
        }
    }

    #[tokio::test]
    async fn a_plan_block_that_parses_as_python_is_refused() {
        let fixture = Fixture::new();
        let runnable = hint(Rung::Plan, "Steps:\n```\nPARSES\n```");
        assert!(refused(&fixture, Setting::Mock, &runnable).await.is_some());
        let steps = hint(Rung::Plan, "Steps:\n```\n1. remember each number\n```");
        assert_eq!(refused(&fixture, Setting::Mock, &steps).await, None);
    }

    #[tokio::test]
    async fn a_line_hint_names_at_least_one_line() {
        let fixture = Fixture::new();
        assert_eq!(
            refused(&fixture, Setting::Mock, &line_hint(Vec::new())).await,
            Some("A Line hint names at least one line of the candidate's code.".into())
        );
        let last = line_hint(vec![place(1), place(4)]);
        assert_eq!(refused(&fixture, Setting::Mock, &last).await, None);
    }

    #[tokio::test]
    async fn a_block_that_solves_the_problem_is_refused_on_any_rung_that_allows_blocks() {
        let fixture = Fixture::new();
        let solving = CodeHint {
            text: "Try this:\n```python\nPASSES\n```".into(),
            ..line_hint(vec![place(2)])
        };
        assert!(refused(&fixture, Setting::Mock, &solving).await.is_some());
        let fragment = CodeHint {
            text: "Try this:\n```python\nfragment\n```".into(),
            ..line_hint(vec![place(2)])
        };
        assert_eq!(refused(&fixture, Setting::Mock, &fragment).await, None);
    }

    #[tokio::test]
    async fn a_line_of_the_reference_is_refused_word_for_word() {
        let fixture = Fixture::new();
        let verbatim = hint(Rung::Plan, "Then store it with `seen[n] = i` each time.");
        let why = refused(&fixture, Setting::Mock, &verbatim).await;
        assert!(why.is_some_and(|why| !why.contains("seen[n] = i")));
        let in_reason = CodeHint {
            reason: "Because `if target - n in seen:` is the test.".into(),
            ..hint(Rung::Nudge, CLEAN)
        };
        assert!(refused(&fixture, Setting::Mock, &in_reason).await.is_some());
    }

    #[tokio::test]
    async fn a_starter_line_or_keywords_alone_are_not_a_reference_line() {
        let fixture = Fixture::new();
        for text in [
            "Keep `def two_sum(numbers, target):` as it is.",
            "Think about what to return, and when.",
            "An empty case can return [] early.",
            "Does the loop need an else:?",
        ] {
            assert_eq!(
                refused(&fixture, Setting::Mock, &hint(Rung::Plan, text)).await,
                None,
                "{text}"
            );
        }
    }

    #[tokio::test]
    async fn the_pattern_name_is_allowed_only_on_a_coached_idea() {
        let fixture = Fixture::new();
        let text = format!("This is a case for {}.", fixture.pattern_name());
        for setting in [Setting::Coached, Setting::Mock, Setting::Watched] {
            for rung in [Rung::Nudge, Rung::Idea, Rung::Plan, Rung::Point] {
                let mut named = hint(rung, &text);
                if rung == Rung::Point {
                    named.lines = vec![place(2)];
                }
                let allowed = setting == Setting::Coached && rung == Rung::Idea;
                assert_eq!(
                    refused(&fixture, setting, &named).await.is_none(),
                    allowed,
                    "{setting:?} {rung:?}"
                );
            }
        }
    }

    struct Scripted {
        answers: Mutex<VecDeque<String>>,
        requests: Mutex<Vec<CompletionRequest>>,
    }

    impl Scripted {
        fn new(answers: &[&str]) -> Self {
            Self {
                answers: Mutex::new(answers.iter().map(|answer| (*answer).to_owned()).collect()),
                requests: Mutex::new(Vec::new()),
            }
        }

        fn requests(&self) -> Vec<CompletionRequest> {
            self.requests.lock().map(|r| r.clone()).unwrap_or_default()
        }
    }

    #[async_trait]
    impl LLMPort for Scripted {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            if let Ok(mut requests) = self.requests.lock() {
                requests.push(request.clone());
            }
            let next = self.answers.lock().ok().and_then(|mut a| a.pop_front());
            next.map(CompletionResponse::from_text)
                .ok_or_else(|| AppError::InternalError("Unexpected extra model call".into()))
        }
        fn model_name(&self) -> &str {
            "scripted"
        }
        fn max_context_tokens(&self) -> usize {
            128_000
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
        fn enforces(
            &self,
            _schema: &lattice_core::application::services::model_contract::shape::Schema,
        ) -> bool {
            false
        }
    }

    fn answer(text: &str) -> String {
        serde_json::json!({
            "text": text,
            "reason": "It points at what the code does not yet do.",
            "lines": [],
        })
        .to_string()
    }

    const BAD: &str = "Then store it with `seen[n] = i` each time.";

    async fn written(llm: &Scripted, fixture: &Fixture, rung: Rung) -> Result<CodeHint> {
        write_checked(llm, fixture.input(Setting::Mock, rung), &Fake).await
    }

    fn text_of(request: &CompletionRequest) -> String {
        format!("{request:?}")
    }

    #[tokio::test]
    async fn a_refused_hint_is_asked_for_again_with_the_reason() {
        let fixture = Fixture::new();
        let llm = Scripted::new(&[&answer(BAD), &answer(CLEAN)]);
        let hint = written(&llm, &fixture, Rung::Plan)
            .await
            .expect("second answer is clean");
        assert_eq!(hint.text, CLEAN);
        let why = refused(&fixture, Setting::Mock, &self::hint(Rung::Plan, BAD))
            .await
            .expect("the first answer is refused");
        let requests = llm.requests();
        assert_eq!(requests.len(), 2);
        assert!(text_of(&requests[1]).contains(&why));
    }

    #[tokio::test]
    async fn two_refused_answers_fail_as_an_unusable_model_answer() {
        let fixture = Fixture::new();
        let llm = Scripted::new(&[&answer(BAD), &answer(BAD), &answer(CLEAN)]);
        let error = written(&llm, &fixture, Rung::Plan).await.unwrap_err();
        assert!(
            matches!(error, AppError::ModelAnswerUnusable(_)),
            "{error:?}"
        );
        assert_eq!(llm.requests().len(), 2);
    }

    #[tokio::test]
    async fn a_clean_first_answer_is_the_only_call() {
        let fixture = Fixture::new();
        let llm = Scripted::new(&[&answer(CLEAN)]);
        written(&llm, &fixture, Rung::Nudge).await.expect("clean");
        assert_eq!(llm.requests().len(), 1);
    }

    #[tokio::test]
    async fn a_block_that_runs_as_the_solution_is_asked_for_again_once() {
        let fixture = Fixture::new();
        let pointing = |text: &str| {
            serde_json::json!({
                "text": text,
                "reason": "It points at what the code does not yet do.",
                "lines": ["l3"],
            })
            .to_string()
        };
        let llm = Scripted::new(&[
            &pointing("Try this:\n```python\nPASSES\n```"),
            &pointing(CLEAN),
        ]);
        let hint = written(&llm, &fixture, Rung::Point)
            .await
            .expect("second is clean");
        assert_eq!(hint.text, CLEAN);
        let requests = llm.requests();
        assert_eq!(requests.len(), 2);
        assert!(text_of(&requests[1]).contains("A block in the hint solves the problem."));

        let solving = answer("Steps:\n```\nPARSES\n```");
        let llm = Scripted::new(&[&solving, &answer(CLEAN)]);
        let hint = written(&llm, &fixture, Rung::Plan)
            .await
            .expect("second is clean");
        assert_eq!(hint.text, CLEAN);
        let requests = llm.requests();
        assert_eq!(requests.len(), 2);
        assert!(text_of(&requests[1]).contains("A block in a Plan hint parses as Python."));

        let llm = Scripted::new(&[&solving, &solving]);
        let error = written(&llm, &fixture, Rung::Plan).await.unwrap_err();
        assert!(
            matches!(error, AppError::ModelAnswerUnusable(_)),
            "{error:?}"
        );
        assert_eq!(llm.requests().len(), 2);
    }

    #[tokio::test]
    async fn only_a_line_hint_keeps_the_lines_it_points_at() {
        let fixture = Fixture::new();
        let with_lines = serde_json::json!({
            "text": CLEAN,
            "reason": "It points at what the code does not yet do.",
            "lines": ["l3"],
        })
        .to_string();
        let llm = Scripted::new(&[&with_lines, &with_lines]);
        let nudge = written(&llm, &fixture, Rung::Nudge).await.expect("clean");
        assert!(nudge.lines.is_empty());
        assert!(!text_of(&llm.requests()[0]).contains("[l3]"));
        let line = written(&llm, &fixture, Rung::Point).await.expect("clean");
        assert_eq!(line.lines, vec![place(3)]);
        assert!(text_of(&llm.requests()[1]).contains("[l3]         return [0]"));
    }

    #[tokio::test]
    async fn a_handle_that_names_no_line_costs_only_that_pointer() {
        let fixture = Fixture::new();
        let pointing = serde_json::json!({
            "text": CLEAN,
            "reason": "It points at what the code does not yet do.",
            "lines": ["l9", "l2"],
        })
        .to_string();
        let llm = Scripted::new(&[&pointing]);
        let line = written(&llm, &fixture, Rung::Point).await.expect("clean");
        assert_eq!(line.lines, vec![place(2)]);
        assert_eq!(llm.requests().len(), 1);
    }

    #[tokio::test]
    async fn a_line_hint_that_points_at_nothing_is_asked_for_again() {
        let fixture = Fixture::new();
        let nothing = answer(CLEAN);
        let pointing = serde_json::json!({
            "text": CLEAN,
            "reason": "It points at what the code does not yet do.",
            "lines": ["l1"],
        })
        .to_string();
        let llm = Scripted::new(&[&nothing, &pointing]);
        let line = written(&llm, &fixture, Rung::Point)
            .await
            .expect("second points");
        assert_eq!(line.lines, vec![place(1)]);
        assert!(text_of(&llm.requests()[1])
            .contains("A Line hint names at least one line of the candidate's code."));
    }

    #[test]
    fn the_request_tells_what_it_must_and_writes_no_internal_value() {
        for earlier in [false, true] {
            let mut marks = Marks::new();
            let input = Input {
                reference: marks.told("def two_sum(numbers, target):".to_owned()),
                observed: marks.told("The candidate ran the cases.".to_owned()),
                starter: marks.internal("def starter_kept_line(numbers):".to_owned()),
                patterns: marks.internal(vec!["pattern-key-0001".to_owned()]),
                setting: Setting::Coached,
                rung: Rung::Point,
                in_role: false,
                lines: vec![Line {
                    path: marks.told("solution.py file".to_owned()),
                    number: 1,
                    text: marks.told("def two_sum(numbers, target):".to_owned()),
                }],
                earlier: earlier.then(|| Earlier {
                    hint: marks.told("Then store it each time.".to_owned()),
                    why: marks.told("The hint repeats a line.".to_owned()),
                }),
            };
            holds::<HintAuthor>(&input, &marks);
        }
    }

    #[tokio::test]
    async fn prompt_snapshot_hint_author() {
        use lattice_core::application::services::model_contract::test_support::snapshot;
        let fixture = Fixture::new();
        let llm = Scripted::new(&[&answer(CLEAN)]);
        let observation = "Problem: find two numbers that add up to the target.
The candidate ran the cases: 1 of 3 passed.
Their file solution.py returns on the first pass of the loop.";
        let input = Input::of(
            &fixture.reference,
            &fixture.starter,
            &fixture.patterns,
            observation,
            Setting::Coached,
            &fixture.draft,
            Rung::Idea,
            true,
        );
        write_checked(&llm, input, &Fake).await.expect("clean");
        snapshot::assert_requests(env!("CARGO_MANIFEST_DIR"), "coding-hint", &llm.requests());
    }

    const NEVER_NAME: &str = "Never name the technique or pattern that solves the problem.";

    #[test]
    fn the_naming_rule_is_in_every_request_that_does_not_allow_a_name() {
        let fixture = Fixture::new();
        for setting in [Setting::Coached, Setting::Mock, Setting::Watched] {
            for rung in [Rung::Nudge, Rung::Idea, Rung::Plan, Rung::Point] {
                let input = fixture.input(setting, rung);
                let mut scope = Scope::for_question();
                let mut showing = scope.request();
                let says = HintAuthor::says(&input, &mut showing).expect("says");
                let allowed = setting == Setting::Coached && rung == Rung::Idea;
                assert_eq!(
                    says.instructions.contains(NEVER_NAME),
                    !allowed,
                    "{setting:?} {rung:?}"
                );
            }
        }
    }
}
