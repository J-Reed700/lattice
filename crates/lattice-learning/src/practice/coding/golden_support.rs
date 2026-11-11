#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

//! What the golden request tests share: the text a request renders to, a model that keeps what it is sent, a
//! fixture observation, and the comparison.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::Mutex;

use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};
use serde_json::Value;

use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::dto::{
    stages_of, CodeHintDto, CodeHintRung, CodingExampleDto, CodingSessionDto, CodingStage,
    ExampleCoverageDto, ExampleVerdict,
};
use crate::practice::coding::observation::test_support::{failing_run, problem, session};
use crate::practice::coding::problem::{CodingSetting, CodingTask, Problem};
use crate::practice::practical_dto::{LearningSimulationSpeaker, LearningSimulationTurnDto};

/// Compares `actual` with the golden text `expected` (from `include_str!`). A mismatch panics with the whole
/// actual text between the marker lines `<<<GOLDEN name` and `GOLDEN>>>`.
pub(crate) fn assert_golden(name: &str, expected: &str, actual: &str) {
    let expected = expected.replace("\r\n", "\n");
    if expected != actual {
        let same = expected
            .lines()
            .zip(actual.lines())
            .take_while(|(golden, seen)| golden == seen)
            .count();
        let golden = expected.lines().nth(same).unwrap_or("<end>");
        let seen = actual.lines().nth(same).unwrap_or("<end>");
        panic!(
            "golden {name} differs\nfirst difference at line {}:\n  golden: {golden}\n  actual: {seen}\n<<<GOLDEN {name}\n{actual}GOLDEN>>>\n",
            same + 1
        );
    }
}

/// The variants of one golden file, in the order added.
#[derive(Default)]
pub(crate) struct Golden {
    text: String,
}

impl Golden {
    pub(crate) fn add(&mut self, name: &str, body: &str) {
        if !self.text.is_empty() {
            self.text.push('\n');
        }
        let _ = writeln!(self.text, "=== {name} ===");
        self.text.push_str(body.trim_end_matches('\n'));
        self.text.push('\n');
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).expect("a value writes out as JSON")
}

/// A request as the model server receives it: each message under its role, the schema, and every other field.
pub(crate) fn render_completion(request: &CompletionRequest) -> String {
    let mut out = String::new();
    for input in &request.input {
        match input {
            CompletionInput::Message { role, content } => {
                let _ = writeln!(out, "--- {role} ---\n{content}");
            }
            other => {
                let value = serde_json::to_value(other).expect("an input writes out as JSON");
                let _ = writeln!(out, "--- other input ---\n{}", pretty(&value));
            }
        }
    }
    let schema = request
        .json_schema
        .as_ref()
        .map_or_else(|| "none".to_owned(), pretty);
    let _ = writeln!(out, "--- schema ---\n{schema}");
    let mut rest = serde_json::to_value(request).expect("a request writes out as JSON");
    if let Some(fields) = rest.as_object_mut() {
        fields.remove("input");
        fields.remove("json_schema");
    }
    let _ = writeln!(out, "--- options ---\n{}", pretty(&rest));
    out
}

/// Every request a recorder was sent, one variant each, named `name` and the request's place in the call.
pub(crate) fn add_requests(golden: &mut Golden, name: &str, model: &Recorder) {
    let sent = model.requests();
    let total = sent.len();
    for (at, request) in sent.iter().enumerate() {
        golden.add(
            &format!("{name}, request {} of {total}", at + 1),
            &render_completion(request),
        );
    }
}

/// A model that answers with the next canned text and keeps every request it was sent.
pub(crate) struct Recorder {
    answers: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl Recorder {
    pub(crate) fn new(answers: impl IntoIterator<Item = String>) -> Self {
        Self {
            answers: Mutex::new(answers.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn requests(&self) -> Vec<CompletionRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl LLMPort for Recorder {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.requests.lock().unwrap().push(request.clone());
        let next = self.answers.lock().unwrap().pop_front();
        next.map(CompletionResponse::from_text)
            .ok_or_else(|| AppError::InternalError("Unexpected extra model call".into()))
    }

    fn model_name(&self) -> &str {
        "golden-model"
    }

    fn max_context_tokens(&self) -> usize {
        128_000
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn turn(
    ordinal: i64,
    speaker: LearningSimulationSpeaker,
    content: &str,
) -> LearningSimulationTurnDto {
    LearningSimulationTurnDto {
        id: format!("turn-{ordinal}"),
        ordinal,
        speaker,
        content: content.to_owned(),
        citations: Vec::new(),
        model_name: None,
        created_at: ordinal,
    }
}

/// The fixture problem, a session of `task` in `setting` that holds a failing run, changes since the draft was last
/// seen, learner examples, a hint and a transcript, and the draft last seen. The stage is the third of the task's.
pub(crate) fn rich_session(
    task: CodingTask,
    setting: CodingSetting,
) -> (Problem, CodingSessionDto, Vec<LearningLabFile>) {
    let problem = problem();
    let stages = stages_of(task);
    let stage: CodingStage = stages.get(2).copied().unwrap_or(CodingStage::Code);
    let mut session = session(&problem, setting, stage);
    session.task = task;
    session.stages = stages.to_vec();
    session.runs.push(failing_run());
    session.examples = vec![
        CodingExampleDto {
            ordinal: 0,
            args: vec!["[]".to_owned()],
            expected: "0".to_owned(),
            verdict: ExampleVerdict::Right,
            expects: None,
            why_invalid: None,
        },
        CodingExampleDto {
            ordinal: 1,
            args: vec!["[1, -1]".to_owned()],
            expected: "2".to_owned(),
            verdict: ExampleVerdict::Wrong,
            expects: None,
            why_invalid: None,
        },
    ];
    session.coverage = ExampleCoverageDto {
        caught: 1,
        total: 2,
        uncaught: Some("Ignores negative values".to_owned()),
    };
    session.hints = vec![CodeHintDto {
        ordinal: 0,
        rung: CodeHintRung::Nudge,
        text: "What should the loop do with each value?".to_owned(),
        reason: "The draft never reads the values.".to_owned(),
        lines: Vec::new(),
        path: None,
        stage,
        run_id: None,
        in_role: false,
        at: 20,
    }];
    session.turns = vec![
        turn(
            0,
            LearningSimulationSpeaker::Counterpart,
            "Welcome. Read the problem first.",
        ),
        turn(
            1,
            LearningSimulationSpeaker::Learner,
            "Can the list be empty?",
        ),
        turn(
            2,
            LearningSimulationSpeaker::Counterpart,
            "Yes, and its total is 0.",
        ),
    ];
    let seen = vec![LearningLabFile {
        path: "solution.py".to_owned(),
        content: "def total(values):\n    pass\n".to_owned(),
    }];
    (problem, session, seen)
}
