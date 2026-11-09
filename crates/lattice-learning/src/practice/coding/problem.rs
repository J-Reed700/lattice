//! A coding problem: what the learner and the counterpart see, and the hidden parts that prove it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The file the learner's code lives in, beside the harness.
pub const SOLUTION_PATH: &str = "solution.py";

/// Where the proven problem is stored: a check file, so no public read returns it.
pub const PROBLEM_FILE_PATH: &str = "problem/problem.json";

/// What the learner is asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case
    /// The comparison learner examples use: the first example case's.
    pub fn default_compare(&self) -> CaseCompare {
        self.examples()
            .next()
            .map_or(CaseCompare::Exact, |case| case.compare.clone())
    }
")]
pub enum CodingTask {
    Problem,
    BugFix,
}

/// Who the counterpart is, and what the learner may see.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CodingSetting {
    Coached,
    Mock,
    Watched,
}

/// Something a setting may keep from the learner until the debrief.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hidden {
    /// A failing hidden case's input, expected output and actual output.
    FailingCase,
    /// The description of a misreading the learner's examples don't catch yet.
    UncaughtMisreading,
    /// The reference solution.
    WorkedSolution,
}

impl CodingSetting {
    /// Whether the learner may see `hidden` during the session, once they have `submitted` or not. The debrief
    /// shows everything.
    pub fn discloses(self, hidden: Hidden, submitted: bool) -> bool {
        match (self, hidden) {
            (Self::Coached, Hidden::FailingCase | Hidden::UncaughtMisreading) => true,
            (Self::Coached, Hidden::WorkedSolution) => submitted,
            (Self::Mock | Self::Watched, _) => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ProblemDifficulty {
    Easy,
    Medium,
    Hard,
}

/// How a case's result is compared with what it expects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CaseCompare {
    /// Python `==`.
    Exact,
    /// The same elements in any order, duplicates counted.
    Unordered,
    /// Numbers within `tolerance`, through nested lists and tuples; everything else `==`.
    FloatWithin { tolerance: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CaseVisibility {
    /// Shown in the statement.
    Example,
    /// Run on submit; a setting decides how much of a failure the learner sees.
    Hidden,
}

/// One call of the learner's function and what it should return.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProblemCase {
    /// Unique within the problem.
    pub id: String,
    /// What the case is about, in plain words: "an empty list". Shown when the case itself is hidden.
    pub category: String,
    /// One Python literal per parameter, in the signature's order.
    pub args: Vec<String>,
    /// A Python literal. `None` runs the call and reports what it returned, as "Try an input" does.
    pub expected: Option<String>,
    pub compare: CaseCompare,
    pub visibility: CaseVisibility,
    /// A large input that tests the complexity; runs after every other case. Its `args` are expressions, not
    /// literals, and `expected` is `None`.
    pub scale: bool,
    /// Scale cases only: the SHA-256 hex of the reference result's canonical form.
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SignatureParam {
    pub name: String,
    /// The Python annotation: `list[int]`.
    pub annotation: String,
}

/// The function the learner writes in `solution.py`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProblemSignature {
    pub function: String,
    pub params: Vec<SignatureParam>,
    /// The Python return annotation.
    pub returns: String,
}

/// A question the statement leaves open, and the problem's answer to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProblemClarification {
    pub question: String,
    pub answer: String,
}

/// A plausible wrong reading of the statement, as code, and the cases that tell it apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Misreading {
    pub id: String,
    /// What the misreading gets wrong, for Coached and the debrief.
    pub description: String,
    /// A complete `solution.py`.
    pub code: String,
    /// The ids of the cases it fails, recorded by the proof. Never empty in a proven problem.
    pub killed_by: Vec<String>,
}

/// Everything the counterpart may see. The learner's view is narrower and is cut by `CodingSetting::discloses`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProblemSpec {
    pub title: String,
    /// Markdown. Never names its pattern.
    pub statement: String,
    pub signature: ProblemSignature,
    pub constraints: Vec<String>,
    pub clarifications: Vec<ProblemClarification>,
    /// "python".
    pub language: String,
    pub difficulty: ProblemDifficulty,
    /// "O(n) time, O(n) space".
    pub target_complexity: String,
    /// Ids from the problem pattern catalogue. Not shown to the learner before the debrief.
    pub patterns: Vec<String>,
    /// The `solution.py` the learner starts from.
    pub starter: String,
}

/// A proven problem, as stored in the activity's `problem/problem.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub spec: ProblemSpec,
    /// A complete `solution.py` that passes every case.
    pub reference: String,
    pub misreadings: Vec<Misreading>,
    /// Every case, examples first, scale cases last.
    pub cases: Vec<ProblemCase>,
    /// How long the reference took on each scale case, by case id, in milliseconds.
    pub reference_ms: BTreeMap<String, u64>,
    /// Fix a bug: the id of the misreading whose code is the starter.
    #[serde(default)]
    pub bug: Option<String>,
}

impl Problem {
    /// The cases shown in the statement.
    pub fn examples(&self) -> impl Iterator<Item = &ProblemCase> {
        self.cases
            .iter()
            .filter(|case| case.visibility == CaseVisibility::Example)
    }

    /// The comparison learner examples use: the first example case's.
    pub fn default_compare(&self) -> CaseCompare {
        self.examples()
            .next()
            .map_or(CaseCompare::Exact, |case| case.compare.clone())
    }
}

/// How one case ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Passed,
    Failed,
    /// The call raised.
    Error,
    /// The case had no expected value; the call finished.
    Ran,
    /// The run stopped while this case was running.
    TimedOut,
    /// The run stopped before this case started.
    NotRun,
}

/// One case's result, as the harness reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CaseResult {
    pub case_id: String,
    pub status: CaseStatus,
    /// `repr` of what the call returned.
    pub actual: Option<String>,
    /// What the call printed.
    pub printed: String,
    /// The exception's type and message.
    pub error: Option<String>,
    pub elapsed_ms: Option<u64>,
}
