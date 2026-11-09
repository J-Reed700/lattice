//! Coding hints: a ladder of four rungs, each written with the reference in view and checked before the learner
//! sees it.

use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::GroundedRequest;
use lattice_core::domain::learning::practice::named_in;
use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::drills::runner::CodeRunner;
use crate::generation::parse_json;
use crate::lab_runtime::LearningLabFile;
use crate::lessons::structured_call::{complete_checked_request, reask};
use crate::model_call;
use crate::practice::coding::dto::{CodeHintDto, CodeHintRung};
use crate::practice::coding::harness::CaseHarness;
use crate::practice::coding::observation::WorkspaceObservation;
use crate::practice::coding::problem::{CodingSetting, Problem};
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

/// A hint that passed its checks, ready to be stored as a `CodeHintDto`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeHint {
    pub rung: CodeHintRung,
    pub text: String,
    pub reason: String,
    pub lines: Vec<u32>,
    pub path: Option<String>,
}

#[derive(Deserialize)]
struct Written {
    text: String,
    reason: String,
    lines: Vec<u32>,
    path: Option<String>,
}

/// The rung after the highest one given: Nudge first, and Line stays Line.
pub fn next_rung(given: &[CodeHintDto]) -> CodeHintRung {
    match given.iter().map(|hint| hint.rung).max() {
        None => CodeHintRung::Nudge,
        Some(CodeHintRung::Nudge) => CodeHintRung::Idea,
        Some(CodeHintRung::Idea) => CodeHintRung::Plan,
        Some(CodeHintRung::Plan | CodeHintRung::Line) => CodeHintRung::Line,
    }
}

/// Writes the hint for `rung`, checks it, and asks once more with the reason when a check refuses it. A second
/// refusal fails with `AppError::ModelAnswerUnusable`.
#[allow(clippy::too_many_arguments)]
pub async fn write_hint(
    llm: &dyn LLMPort,
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    problem: &Problem,
    observation: &WorkspaceObservation,
    rung: CodeHintRung,
    in_role: bool,
    cancel: CancellationToken,
) -> Result<CodeHint> {
    let hinting = Hinting {
        reference: &problem.reference,
        starter: &problem.spec.starter,
        patterns: &problem.spec.patterns,
        setting: observation.setting,
        draft: &observation.files,
    };
    let proof = Sandbox {
        runner,
        harness,
        problem,
        cancel: &cancel,
    };
    let shown = serde_json::to_string_pretty(observation)
        .map_err(|error| AppError::InternalError(error.to_string()))?;
    let request = ask(&problem.reference, &shown, rung, in_role, observation.setting);
    write_checked(llm, &hinting, &proof, request, rung).await
}

/// What the checks read of the problem and the session.
struct Hinting<'a> {
    reference: &'a str,
    starter: &'a str,
    patterns: &'a [String],
    setting: CodingSetting,
    draft: &'a [LearningLabFile],
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

fn rung_name(rung: CodeHintRung) -> &'static str {
    match rung {
        CodeHintRung::Nudge => "Nudge",
        CodeHintRung::Idea => "Idea",
        CodeHintRung::Plan => "Plan",
        CodeHintRung::Line => "Line",
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
fn repeated_reference_line(hinting: &Hinting<'_>, shown: &[&str]) -> Option<usize> {
    hinting
        .reference
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, line)| says_something(line))
        .filter(|(_, line)| !hinting.starter.lines().any(|kept| kept.trim() == *line))
        .find(|(_, line)| shown.iter().any(|text| text.contains(line)))
        .map(|(number, _)| number)
}

/// What is wrong with the lines a Line hint names, if anything.
fn bad_lines(hinting: &Hinting<'_>, hint: &CodeHint) -> Option<String> {
    if hint.lines.is_empty() {
        return Some("A Line hint names at least one line of the candidate's code.".into());
    }
    let Some(path) = hint.path.as_deref() else {
        return Some("A Line hint names the file its lines are in, in `path`.".into());
    };
    let Some(file) = hinting.draft.iter().find(|file| file.path == path) else {
        return Some(format!("`path` {path} is not one of the candidate's files."));
    };
    let count = file.content.lines().count();
    hint.lines
        .iter()
        .find(|line| {
            usize::try_from(**line)
                .ok()
                .filter(|line| (1..=count).contains(line))
                .is_none()
        })
        .map(|line| format!("Line {line} does not exist in {path}, which has {count} lines."))
}

/// Why a check refuses `hint`, or None when every check accepts it. Errors are runs that could not be made.
async fn refusal(
    hinting: &Hinting<'_>,
    proof: &dyn Proof,
    hint: &CodeHint,
) -> Result<Option<String>> {
    if hint.text.trim().is_empty() || hint.reason.trim().is_empty() {
        return Ok(Some("The hint needs both its text and its reason.".into()));
    }
    let blocks = code_blocks(&hint.text);
    if matches!(hint.rung, CodeHintRung::Nudge | CodeHintRung::Idea) && !blocks.is_empty() {
        return Ok(Some(format!(
            "A {} hint holds no fenced code block. Say it in words.",
            rung_name(hint.rung)
        )));
    }
    if hint.rung == CodeHintRung::Line {
        if let Some(why) = bad_lines(hinting, hint) {
            return Ok(Some(why));
        }
    }
    let shown = [hint.text.as_str(), hint.reason.as_str()];
    if let Some(number) = repeated_reference_line(hinting, &shown) {
        return Ok(Some(format!(
            "The hint repeats line {number} of the reference solution word for word. Say it in other words."
        )));
    }
    let allowed = hinting.setting == CodingSetting::Coached && hint.rung == CodeHintRung::Idea;
    if !allowed {
        let named = shown
            .iter()
            .find_map(|text| named_in(text, hinting.patterns));
        if let Some(pattern) = named {
            return Ok(Some(format!(
                "The hint names the pattern \"{}\". Describe what to do without naming the technique.",
                pattern.name
            )));
        }
    }
    for block in &blocks {
        if hint.rung == CodeHintRung::Plan {
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

fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "text": {"type": "string", "minLength": 1},
            "reason": {"type": "string", "minLength": 1},
            "lines": {"type": "array", "items": {"type": "integer", "minimum": 1}},
            "path": {"type": ["string", "null"]}
        },
        "required": ["text", "reason", "lines", "path"]
    })
}

fn rung_instruction(rung: CodeHintRung, setting: CodingSetting) -> &'static str {
    match rung {
        CodeHintRung::Nudge => {
            "Ask one question that points at what to look at. Give no code and no fenced block."
        }
        CodeHintRung::Idea if setting == CodingSetting::Coached => {
            "Give the idea or structure that fits, and why. You may name the technique. Give no fenced block."
        }
        CodeHintRung::Idea => {
            "Give the idea or structure that fits, and why, without naming the technique. Give no fenced block."
        }
        CodeHintRung::Plan => {
            "Give numbered steps in plain language, each with its reason. A fenced block, if any, is pseudo-code that is not valid Python."
        }
        CodeHintRung::Line => {
            "Point at the lines of the candidate's code that are wrong, and say why. Set `path` to the file and `lines` to its 1-based line numbers. Never write the corrected line."
        }
    }
}

/// The request that asks for a hint of `rung`.
fn ask(
    reference: &str,
    observation: &str,
    rung: CodeHintRung,
    in_role: bool,
    setting: CodingSetting,
) -> GroundedRequest {
    let voice = if in_role {
        "You are the interviewer in a coding interview. Write the hint as the words you say to the candidate, in your own voice, staying in role."
    } else if setting == CodingSetting::Coached {
        "You are a coach helping a learner through a coding problem."
    } else {
        "You give a candidate one hint on a coding problem."
    };
    let system = format!(
        "{voice} The reference solution is private: it is there so that your hint is right, never to be quoted. Never write a line of it, and never write code that solves the problem. Speak about the candidate's code only from what they have written and run. Fill `path` and `lines` only for a Line hint; otherwise give `lines` as an empty list and `path` as null."
    );
    let prompt = format!(
        "Reference solution (private):\n{reference}\n\nWhat the candidate sees and has done:\n{observation}\n\nWrite a {} hint. {}",
        rung_name(rung),
        rung_instruction(rung, setting)
    );
    model_call::structured(&system, prompt, schema(), HINT_TOKENS, "low")
}

/// Asks for `request`, checks the answer, and asks once more with the reason when a check refuses it.
async fn write_checked(
    llm: &dyn LLMPort,
    hinting: &Hinting<'_>,
    proof: &dyn Proof,
    request: GroundedRequest,
    rung: CodeHintRung,
) -> Result<CodeHint> {
    let mut request = request;
    let mut again = false;
    loop {
        let (raw, written) = complete_checked_request(llm, request.clone(), None, None, |raw| {
            parse_json::<Written>(raw).map(|written| (raw.to_owned(), written))
        })
        .await?;
        let hint = if rung == CodeHintRung::Line {
            CodeHint {
                rung,
                text: written.text,
                reason: written.reason,
                lines: written.lines,
                path: written.path,
            }
        } else {
            CodeHint {
                rung,
                text: written.text,
                reason: written.reason,
                lines: Vec::new(),
                path: None,
            }
        };
        let Some(why) = refusal(hinting, proof, &hint).await? else {
            return Ok(hint);
        };
        if again {
            return Err(AppError::ModelAnswerUnusable(why));
        }
        request = reask(request, &raw, &why);
        again = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::practice::coding::dto::CodingStage;
    use lattice_core::application::ports::llm_port::{
        CompletionRequest, CompletionResponse,
    };
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

        fn hinting(&self, setting: CodingSetting) -> Hinting<'_> {
            Hinting {
                reference: &self.reference,
                starter: &self.starter,
                patterns: &self.patterns,
                setting,
                draft: &self.draft,
            }
        }

        fn pattern_name(&self) -> &'static str {
            PROBLEM_PATTERNS.first().map_or("", |pattern| pattern.name)
        }
    }

    fn hint(rung: CodeHintRung, text: &str) -> CodeHint {
        CodeHint {
            rung,
            text: text.into(),
            reason: "It points at what the code does not yet do.".into(),
            lines: Vec::new(),
            path: None,
        }
    }

    fn line_hint(lines: Vec<u32>, path: Option<&str>) -> CodeHint {
        CodeHint {
            lines,
            path: path.map(str::to_owned),
            ..hint(CodeHintRung::Line, "This return ends the loop on the first pass.")
        }
    }

    async fn refused(fixture: &Fixture, setting: CodingSetting, hint: &CodeHint) -> Option<String> {
        refusal(&fixture.hinting(setting), &Fake, hint)
            .await
            .expect("the checks run")
    }

    const CLEAN: &str = "Think about what you need to remember while you walk the list once.";

    #[tokio::test]
    async fn a_clean_hint_of_every_rung_is_accepted() {
        let fixture = Fixture::new();
        for rung in [CodeHintRung::Nudge, CodeHintRung::Idea, CodeHintRung::Plan] {
            assert_eq!(refused(&fixture, CodingSetting::Mock, &hint(rung, CLEAN)).await, None);
        }
        let line = line_hint(vec![3], Some("solution.py"));
        assert_eq!(refused(&fixture, CodingSetting::Mock, &line).await, None);
    }

    #[tokio::test]
    async fn blank_text_or_a_blank_reason_is_refused() {
        let fixture = Fixture::new();
        let blank_text = hint(CodeHintRung::Nudge, "  \n ");
        assert!(refused(&fixture, CodingSetting::Mock, &blank_text).await.is_some());
        let blank_reason = CodeHint {
            reason: " ".into(),
            ..hint(CodeHintRung::Nudge, CLEAN)
        };
        assert!(refused(&fixture, CodingSetting::Mock, &blank_reason).await.is_some());
    }

    #[tokio::test]
    async fn nudge_and_idea_hold_no_fenced_block() {
        let fixture = Fixture::new();
        let fenced = "Remember them:\n```python\nx\n```";
        for rung in [CodeHintRung::Nudge, CodeHintRung::Idea] {
            assert!(refused(&fixture, CodingSetting::Mock, &hint(rung, fenced)).await.is_some());
            assert_eq!(refused(&fixture, CodingSetting::Mock, &hint(rung, CLEAN)).await, None);
        }
    }

    #[tokio::test]
    async fn a_plan_block_that_parses_as_python_is_refused() {
        let fixture = Fixture::new();
        let runnable = hint(CodeHintRung::Plan, "Steps:\n```\nPARSES\n```");
        assert!(refused(&fixture, CodingSetting::Mock, &runnable).await.is_some());
        let steps = hint(CodeHintRung::Plan, "Steps:\n```\n1. remember each number\n```");
        assert_eq!(refused(&fixture, CodingSetting::Mock, &steps).await, None);
    }

    #[tokio::test]
    async fn a_line_hint_names_lines_that_exist_in_a_file_of_the_draft() {
        let fixture = Fixture::new();
        let cases = [
            line_hint(Vec::new(), Some("solution.py")),
            line_hint(vec![0], Some("solution.py")),
            line_hint(vec![5], Some("solution.py")),
            line_hint(vec![2, 9], Some("solution.py")),
            line_hint(vec![2], None),
            line_hint(vec![2], Some("other.py")),
        ];
        for refused_hint in cases {
            assert!(
                refused(&fixture, CodingSetting::Mock, &refused_hint).await.is_some(),
                "{refused_hint:?}"
            );
        }
        let last = line_hint(vec![1, 4], Some("solution.py"));
        assert_eq!(refused(&fixture, CodingSetting::Mock, &last).await, None);
    }

    #[tokio::test]
    async fn a_block_that_solves_the_problem_is_refused_on_any_rung_that_allows_blocks() {
        let fixture = Fixture::new();
        let solving = CodeHint {
            text: "Try this:\n```python\nPASSES\n```".into(),
            ..line_hint(vec![2], Some("solution.py"))
        };
        assert!(refused(&fixture, CodingSetting::Mock, &solving).await.is_some());
        let fragment = CodeHint {
            text: "Try this:\n```python\nfragment\n```".into(),
            ..line_hint(vec![2], Some("solution.py"))
        };
        assert_eq!(refused(&fixture, CodingSetting::Mock, &fragment).await, None);
    }

    #[tokio::test]
    async fn a_line_of_the_reference_is_refused_word_for_word() {
        let fixture = Fixture::new();
        let verbatim = hint(CodeHintRung::Plan, "Then store it with `seen[n] = i` each time.");
        let why = refused(&fixture, CodingSetting::Mock, &verbatim).await;
        assert!(why.is_some_and(|why| !why.contains("seen[n] = i")));
        let in_reason = CodeHint {
            reason: "Because `if target - n in seen:` is the test.".into(),
            ..hint(CodeHintRung::Nudge, CLEAN)
        };
        assert!(refused(&fixture, CodingSetting::Mock, &in_reason).await.is_some());
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
                refused(&fixture, CodingSetting::Mock, &hint(CodeHintRung::Plan, text)).await,
                None,
                "{text}"
            );
        }
    }

    #[tokio::test]
    async fn the_pattern_name_is_allowed_only_on_a_coached_idea() {
        let fixture = Fixture::new();
        let text = format!("This is a case for {}.", fixture.pattern_name());
        for setting in [CodingSetting::Coached, CodingSetting::Mock, CodingSetting::Watched] {
            for rung in [
                CodeHintRung::Nudge,
                CodeHintRung::Idea,
                CodeHintRung::Plan,
                CodeHintRung::Line,
            ] {
                let mut named = hint(rung, &text);
                if rung == CodeHintRung::Line {
                    named.lines = vec![2];
                    named.path = Some("solution.py".into());
                }
                let allowed = setting == CodingSetting::Coached && rung == CodeHintRung::Idea;
                assert_eq!(
                    refused(&fixture, setting, &named).await.is_none(),
                    allowed,
                    "{setting:?} {rung:?}"
                );
            }
        }
    }

    fn given(rungs: &[CodeHintRung]) -> Vec<CodeHintDto> {
        rungs
            .iter()
            .enumerate()
            .map(|(ordinal, rung)| CodeHintDto {
                ordinal: ordinal as u32,
                rung: *rung,
                text: String::new(),
                reason: String::new(),
                lines: Vec::new(),
                path: None,
                stage: CodingStage::Code,
                run_id: None,
                in_role: false,
                at: 0,
            })
            .collect()
    }

    #[test]
    fn the_rungs_come_in_order_and_the_top_one_stays() {
        assert_eq!(next_rung(&[]), CodeHintRung::Nudge);
        assert_eq!(next_rung(&given(&[CodeHintRung::Nudge])), CodeHintRung::Idea);
        assert_eq!(
            next_rung(&given(&[CodeHintRung::Nudge, CodeHintRung::Idea])),
            CodeHintRung::Plan
        );
        assert_eq!(next_rung(&given(&[CodeHintRung::Plan])), CodeHintRung::Line);
        assert_eq!(next_rung(&given(&[CodeHintRung::Line])), CodeHintRung::Line);
        assert_eq!(
            next_rung(&given(&[CodeHintRung::Idea, CodeHintRung::Nudge])),
            CodeHintRung::Plan
        );
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
    }

    fn answer(text: &str) -> String {
        serde_json::json!({
            "text": text,
            "reason": "It points at what the code does not yet do.",
            "lines": [],
            "path": null
        })
        .to_string()
    }

    const BAD: &str = "Then store it with `seen[n] = i` each time.";

    async fn written(llm: &Scripted, fixture: &Fixture, rung: CodeHintRung) -> Result<CodeHint> {
        let hinting = fixture.hinting(CodingSetting::Mock);
        let request = ask(&fixture.reference, "{}", rung, false, CodingSetting::Mock);
        write_checked(llm, &hinting, &Fake, request, rung).await
    }

    #[tokio::test]
    async fn a_refused_hint_is_asked_for_again_with_the_reason() {
        let fixture = Fixture::new();
        let llm = Scripted::new(&[&answer(BAD), &answer(CLEAN)]);
        let hint = written(&llm, &fixture, CodeHintRung::Plan).await.expect("second answer is clean");
        assert_eq!(hint.text, CLEAN);
        let why = refused(&fixture, CodingSetting::Mock, &self::hint(CodeHintRung::Plan, BAD))
            .await
            .expect("the first answer is refused");
        let requests = llm.requests();
        assert_eq!(requests.len(), 2);
        assert!(format!("{:?}", requests[1]).contains(&why));
    }

    #[tokio::test]
    async fn two_refused_answers_fail_as_an_unusable_model_answer() {
        let fixture = Fixture::new();
        let llm = Scripted::new(&[&answer(BAD), &answer(BAD), &answer(CLEAN)]);
        let error = written(&llm, &fixture, CodeHintRung::Plan).await.unwrap_err();
        assert!(matches!(error, AppError::ModelAnswerUnusable(_)), "{error:?}");
        assert_eq!(llm.requests().len(), 2);
    }

    #[tokio::test]
    async fn a_clean_first_answer_is_the_only_call() {
        let fixture = Fixture::new();
        let llm = Scripted::new(&[&answer(CLEAN)]);
        written(&llm, &fixture, CodeHintRung::Nudge).await.expect("clean");
        assert_eq!(llm.requests().len(), 1);
    }

    #[tokio::test]
    async fn only_a_line_hint_keeps_lines_and_a_path() {
        let fixture = Fixture::new();
        let with_lines = serde_json::json!({
            "text": CLEAN,
            "reason": "It points at what the code does not yet do.",
            "lines": [3],
            "path": "solution.py"
        })
        .to_string();
        let llm = Scripted::new(&[&with_lines, &with_lines]);
        let nudge = written(&llm, &fixture, CodeHintRung::Nudge).await.expect("clean");
        assert!(nudge.lines.is_empty() && nudge.path.is_none());
        let line = written(&llm, &fixture, CodeHintRung::Line).await.expect("clean");
        assert_eq!(line.lines, vec![3]);
        assert_eq!(line.path.as_deref(), Some("solution.py"));
    }
}
