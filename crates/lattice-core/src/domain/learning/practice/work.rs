//! The vocabulary of a practice session, in values only: nothing here knows what a piece of work is made of.

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// Names a kind of work.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(transparent)]
pub struct KindId(String);

impl KindId {
    pub fn new(id: &str) -> Self {
        Self(id.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How the starting state is derived from proven material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Form {
    Build,
    Repair,
    Complete,
    Arrange,
}

/// Who the counterpart is, and what the learner may see.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Setting {
    Coached,
    Mock,
    Watched,
}

/// Something a setting may keep from the learner until the debrief.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hidden {
    /// A failing hidden case's detail.
    FailingCase,
    /// The description of a misreading the learner's own cases don't catch yet.
    UncaughtMisreading,
    /// The reference piece.
    WorkedSolution,
}

impl Setting {
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

/// Where a session is in its form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Understand,
    Examples,
    Plan,
    Make,
    Check,
    Review,
    Reproduce,
    Locate,
    Fix,
    Verify,
}

impl Form {
    /// The stages of the form in order. Examples is there only when the kind takes learner cases.
    pub fn stages(self, learner_cases: bool) -> Vec<Stage> {
        match self {
            Self::Build if learner_cases => vec![
                Stage::Understand,
                Stage::Examples,
                Stage::Plan,
                Stage::Make,
                Stage::Check,
                Stage::Review,
            ],
            Self::Build => vec![
                Stage::Understand,
                Stage::Plan,
                Stage::Make,
                Stage::Check,
                Stage::Review,
            ],
            Self::Repair => vec![Stage::Reproduce, Stage::Locate, Stage::Fix, Stage::Verify],
            Self::Complete | Self::Arrange => {
                vec![Stage::Understand, Stage::Make, Stage::Check, Stage::Review]
            }
        }
    }

    /// The misreadings the form itself needs: Repair starts from one, the others need none.
    pub fn misreadings_needed(self) -> usize {
        match self {
            Self::Repair => 1,
            Self::Build | Self::Complete | Self::Arrange => 0,
        }
    }
}

/// How much of the answer a hint gives, in the order they are offered.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "snake_case")]
pub enum Rung {
    Nudge,
    Idea,
    Plan,
    Point,
}

impl Rung {
    /// The rung after the highest one already given.
    pub fn after(highest_given: Option<Rung>) -> Rung {
        match highest_given {
            None => Self::Nudge,
            Some(Self::Nudge) => Self::Idea,
            Some(Self::Idea) => Self::Plan,
            Some(Self::Plan | Self::Point) => Self::Point,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

/// What a kind's practice is scheduled by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Spacing {
    /// The technique it exercises; a due technique gets a new exercise.
    Technique,
    /// The exercise itself; a due exercise comes back.
    Item,
}

/// Which cases a check runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CheckScope {
    Shown,
    All,
    /// One case the learner supplied; it asks for no verdict.
    Own,
}

/// How a check as a whole stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pending,
    Running,
    Passed,
    Failed,
    TimedOut,
    Cancelled,
    Interrupted,
    Unavailable,
}

/// What the reference said about a case the learner wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LearnerCaseVerdict {
    Right,
    Wrong,
    Invalid,
}

/// One state of the work: a draft, a reference, a misreading. The engine stores it and never reads inside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct Piece(serde_json::Value);

impl Piece {
    pub fn new(value: serde_json::Value) -> Self {
        Self(value)
    }

    pub fn value(&self) -> &serde_json::Value {
        &self.0
    }
}

/// Names an addressable unit of a piece.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(transparent)]
pub struct PartRef(String);

impl PartRef {
    pub fn new(part: &str) -> Self {
        Self(part.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CaseVisibility {
    /// Shown with the brief.
    Shown,
    /// Checked on submit; a setting decides how much of a failure the learner sees.
    Hidden,
}

/// One check on a piece. The kind owns `body`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Case {
    /// Unique within the exercise.
    pub id: String,
    pub title: String,
    pub visibility: CaseVisibility,
    pub body: serde_json::Value,
}

/// How one case ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Passed,
    Failed,
    /// The check itself raised.
    Error,
    /// The case asked for no verdict; the check finished.
    Ran,
    /// The check stopped while this case was running.
    TimedOut,
    /// The check stopped before this case started.
    NotRun,
}

/// One case's result. The kind owns `detail`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CaseResult {
    pub case_id: String,
    pub status: CaseStatus,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum MarkState {
    Wrong,
    Empty,
}

/// The kind's diagnosis of one part.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PartMark {
    pub part: PartRef,
    pub state: MarkState,
}

/// One check of a piece against cases. The verdict never depends on `marks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckReport {
    pub results: Vec<CaseResult>,
    pub marks: Vec<PartMark>,
    /// What the kind says about the check beyond its cases; `Null` when it has nothing.
    pub detail: serde_json::Value,
}

impl CheckReport {
    /// Whether there is at least one result and every one passed.
    pub fn all_passed(&self) -> bool {
        !self.results.is_empty()
            && self
                .results
                .iter()
                .all(|result| result.status == CaseStatus::Passed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
}

/// One change between two drafts, in the kind's own units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub part: PartRef,
    pub kind: ChangeKind,
    pub text: String,
}

/// A plausible wrong reading of the brief, as a piece, and the cases that tell it apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Misreading {
    pub id: String,
    /// What the misreading gets wrong, for Coached and the debrief.
    pub description: String,
    pub piece: Piece,
    /// The ids of the cases it fails, recorded by the proof.
    pub killed_by: Vec<String>,
}

/// What the learner is asked to do. The kind owns `spec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Brief {
    pub title: String,
    pub difficulty: Difficulty,
    pub spec: serde_json::Value,
}

/// What an author hands the proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub kind: KindId,
    pub form: Form,
    pub brief: Brief,
    pub reference: Piece,
    pub misreadings: Vec<Misreading>,
    pub cases: Vec<Case>,
    /// Build's starting piece.
    pub starter: Option<Piece>,
    /// Complete's parts to empty.
    pub asks: Vec<PartRef>,
}

/// Proven material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Exercise {
    pub kind: KindId,
    pub form: Form,
    pub brief: Brief,
    pub reference: Piece,
    pub misreadings: Vec<Misreading>,
    pub cases: Vec<Case>,
    /// The piece the learner starts from.
    pub start: Piece,
    /// Repair: the id of the misreading the start is.
    pub planted: Option<String>,
    /// The kind's proof record.
    pub proof: serde_json::Value,
}

macro_rules! labels {
    ($($ty:ident),* $(,)?) => {
        $(impl $ty {
            /// The name text uses for this value.
            pub fn label(self) -> &'static str {
                ""
            }
        })*
    };
}

labels!(
    Setting,
    Form,
    Stage,
    CheckScope,
    Rung,
    CheckStatus,
    CaseStatus,
    LearnerCaseVerdict
);
