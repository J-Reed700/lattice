//! Fakes of the model, the runner and the harness that the problem tests share, and a proven problem to start from.

#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::harness::{CaseHarness, RunEnd};
use crate::practice::coding::problem::{
    CaseCompare, CaseResult, CaseStatus, CaseVisibility, Misreading, Problem, ProblemCase,
    ProblemClarification, ProblemDifficulty, ProblemSignature, ProblemSpec, SignatureParam,
    SOLUTION_PATH,
};
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::DrillFile;
use lattice_core::shared::error::{AppError, Result};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// The file the fake harness lists its cases in, one `id|scale|digest` line each.
const LISTING: &str = "cases.txt";

/// A model that answers with the next canned text and keeps every request it was sent.
pub(crate) struct ScriptedModel {
    answers: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl ScriptedModel {
    pub(crate) fn new(answers: impl IntoIterator<Item = String>) -> Self {
        Self {
            answers: Mutex::new(answers.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    /// Every request's messages, joined, in the order sent.
    pub(crate) fn texts(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| {
                request
                    .input
                    .iter()
                    .filter_map(|input| match input {
                        CompletionInput::Message { content, .. } => Some(content.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect()
    }

    pub(crate) fn calls(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

#[async_trait]
impl LLMPort for ScriptedModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.requests.lock().unwrap().push(request.clone());
        let next = self.answers.lock().unwrap().pop_front();
        next.map(CompletionResponse::from_text)
            .ok_or_else(|| AppError::InternalError("Unexpected extra model call".into()))
    }

    fn model_name(&self) -> &str {
        "scripted-model"
    }

    fn max_context_tokens(&self) -> usize {
        128_000
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// A case as the fake runner sees it.
pub(crate) struct CaseRun {
    pub id: String,
    pub scale: bool,
    pub digest: bool,
}

/// What a fake solution does on a case.
pub(crate) enum Behaviour {
    /// Passes, taking `ms`; a scale case without a digest only runs.
    Passes { ms: u64 },
    Fails,
    Raises(&'static str),
}

type Judge = Box<dyn Fn(&str, &CaseRun) -> Behaviour + Send + Sync>;

/// A runner whose solutions behave as a judge says, by the text of the code.
pub(crate) struct FakeRunner {
    judge: Judge,
    codes: Mutex<Vec<String>>,
    unavailable: Option<String>,
}

impl FakeRunner {
    pub(crate) fn new(
        judge: impl Fn(&str, &CaseRun) -> Behaviour + Send + Sync + 'static,
    ) -> Self {
        Self {
            judge: Box::new(judge),
            codes: Mutex::new(Vec::new()),
            unavailable: None,
        }
    }

    pub(crate) fn unavailable(why: &str) -> Self {
        Self {
            unavailable: Some(why.to_owned()),
            ..Self::new(|_, _| Behaviour::Fails)
        }
    }

    /// The solutions it ran, in order.
    pub(crate) fn codes(&self) -> Vec<String> {
        self.codes.lock().unwrap().clone()
    }
}

#[async_trait]
impl CodeRunner for FakeRunner {
    fn name(&self) -> &str {
        "fake-runner"
    }

    fn entrypoint(&self) -> &str {
        "checks.py"
    }

    fn contract(&self) -> &str {
        "Programs run on a fake Python."
    }

    fn running(&self, _path: &str) -> String {
        String::new()
    }

    async fn run(
        &self,
        _files: &[DrillFile],
        _test: &str,
        _cancel: CancellationToken,
    ) -> Result<RunOutcome> {
        Ok(RunOutcome::Passed)
    }

    async fn run_capturing(
        &self,
        files: &[LearningLabFile],
        _read_only_paths: &[String],
        _cancel: CancellationToken,
    ) -> Result<(RunOutcome, String)> {
        if let Some(why) = &self.unavailable {
            return Ok((RunOutcome::Unavailable { why: why.clone() }, String::new()));
        }
        let content = |path: &str| {
            files
                .iter()
                .find(|file| file.path == path)
                .map(|file| file.content.clone())
                .unwrap_or_default()
        };
        let code = content(SOLUTION_PATH);
        self.codes.lock().unwrap().push(code.clone());
        let mut stdout = String::new();
        let mut all_passed = true;
        for line in content(LISTING).lines() {
            let mut parts = line.split('|');
            let run = CaseRun {
                id: parts.next().unwrap_or_default().to_owned(),
                scale: parts.next() == Some("1"),
                digest: parts.next() == Some("1"),
            };
            let (status, actual, ms, error) = match (self.judge)(&code, &run) {
                Behaviour::Passes { ms } if run.scale && !run.digest => {
                    ("ran", "'big'".to_owned(), ms, String::new())
                }
                Behaviour::Passes { ms } => ("passed", "'ok'".to_owned(), ms, String::new()),
                Behaviour::Fails => {
                    all_passed = false;
                    ("failed", "'wrong'".to_owned(), 1, String::new())
                }
                Behaviour::Raises(message) => {
                    all_passed = false;
                    ("error", String::new(), 1, message.to_owned())
                }
            };
            stdout.push_str(&format!("{}|{status}|{actual}|{ms}|{error}\n", run.id));
        }
        let outcome = if all_passed {
            RunOutcome::Passed
        } else {
            RunOutcome::Failed {
                output: String::new(),
            }
        };
        Ok((outcome, stdout))
    }
}

/// A harness that lists the cases for the fake runner and reads its lines back.
pub(crate) struct FakeHarness;

impl CaseHarness for FakeHarness {
    fn evaluator(
        &self,
        _spec: &ProblemSpec,
        cases: &[ProblemCase],
        _nonce: &str,
    ) -> Result<Vec<LearningLabFile>> {
        let listing = cases
            .iter()
            .map(|case| {
                format!(
                    "{}|{}|{}",
                    case.id,
                    u8::from(case.scale),
                    u8::from(case.digest.is_some())
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(vec![
            LearningLabFile {
                path: "checks.py".into(),
                content: "# evaluator".into(),
            },
            LearningLabFile {
                path: LISTING.into(),
                content: listing,
            },
        ])
    }

    fn results(
        &self,
        cases: &[ProblemCase],
        stdout: &str,
        _nonce: &str,
        _end: RunEnd,
    ) -> Vec<CaseResult> {
        cases
            .iter()
            .map(|case| {
                let line = stdout
                    .lines()
                    .map(|line| line.split('|').collect::<Vec<_>>())
                    .find(|parts| parts.first() == Some(&case.id.as_str()));
                match line {
                    Some(parts) if parts.len() == 5 => CaseResult {
                        case_id: case.id.clone(),
                        status: match parts[1] {
                            "passed" => CaseStatus::Passed,
                            "ran" => CaseStatus::Ran,
                            "failed" => CaseStatus::Failed,
                            _ => CaseStatus::Error,
                        },
                        actual: (!parts[2].is_empty()).then(|| parts[2].to_owned()),
                        printed: String::new(),
                        error: (!parts[4].is_empty()).then(|| parts[4].to_owned()),
                        elapsed_ms: parts[3].parse().ok(),
                    },
                    _ => CaseResult {
                        case_id: case.id.clone(),
                        status: CaseStatus::NotRun,
                        actual: None,
                        printed: String::new(),
                        error: None,
                        elapsed_ms: None,
                    },
                }
            })
            .collect()
    }

    fn program_output(&self, stdout: &str, _nonce: &str) -> String {
        stdout.to_owned()
    }
}

pub(crate) fn case(id: &str, category: &str, visibility: CaseVisibility, scale: bool) -> ProblemCase {
    ProblemCase {
        id: id.to_owned(),
        category: category.to_owned(),
        args: vec!["[1, 2]".into(), "3".into()],
        expected: (!scale).then(|| "[0, 1]".to_owned()),
        compare: CaseCompare::Exact,
        visibility,
        scale,
        digest: None,
    }
}

pub(crate) fn spec() -> ProblemSpec {
    ProblemSpec {
        title: "Pair with a given sum".into(),
        statement: "Return the positions of the two numbers that add up to the target.".into(),
        signature: ProblemSignature {
            function: "pair_positions".into(),
            params: vec![
                SignatureParam {
                    name: "numbers".into(),
                    annotation: "list[int]".into(),
                },
                SignatureParam {
                    name: "target".into(),
                    annotation: "int".into(),
                },
            ],
            returns: "list[int]".into(),
        },
        constraints: vec!["There is exactly one pair.".into()],
        clarifications: vec![ProblemClarification {
            question: "May a number be used twice?".into(),
            answer: "No, each position is used at most once.".into(),
        }],
        language: "python".into(),
        difficulty: ProblemDifficulty::Easy,
        target_complexity: "O(n) time, O(n) space".into(),
        patterns: vec!["hash_map_lookup".into()],
        starter: "def pair_positions(numbers: list[int], target: int) -> list[int]:\n    pass\n"
            .into(),
    }
}

/// The cases every fixture starts from: two examples, three hidden cases and one scale case.
pub(crate) fn cases() -> Vec<ProblemCase> {
    vec![
        case("example-1", "a pair at the start", CaseVisibility::Example, false),
        case("example-2", "a pair in the middle", CaseVisibility::Example, false),
        case("hidden-1", "duplicate numbers", CaseVisibility::Hidden, false),
        case("hidden-2", "negative numbers", CaseVisibility::Hidden, false),
        case("hidden-3", "the pair at the end", CaseVisibility::Hidden, false),
        case("scale-1", "a long list", CaseVisibility::Hidden, true),
    ]
}

/// The same cases once the proof has given the scale cases their digests.
pub(crate) fn proven_cases() -> Vec<ProblemCase> {
    let mut cases = cases();
    for case in cases.iter_mut().filter(|case| case.scale) {
        case.digest = Some("a".repeat(64));
    }
    cases
}

/// A proven problem with two killed misreadings, as the build stores it.
pub(crate) fn sample_problem() -> Problem {
    Problem {
        spec: spec(),
        reference: "def pair_positions(numbers, target):\n    seen = {}\n    for index, number in enumerate(numbers):\n        if target - number in seen:\n            return [seen[target - number], index]\n        seen[number] = index\n".into(),
        misreadings: vec![
            Misreading {
                id: "misreading-1".into(),
                description: "Allows a number to be used twice.".into(),
                code: "def pair_positions(numbers, target):\n    return [0, 0]\n".into(),
                killed_by: vec!["hidden-1".into()],
            },
            Misreading {
                id: "misreading-2".into(),
                description: "Returns the numbers, not their positions.".into(),
                code: "def pair_positions(numbers, target):\n    return numbers\n".into(),
                killed_by: vec!["example-1".into(), "hidden-2".into()],
            },
        ],
        cases: proven_cases(),
        reference_ms: BTreeMap::from([("scale-1".to_owned(), 12)]),
        bug: None,
    }
}
