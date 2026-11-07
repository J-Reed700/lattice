//! What each stage of a drill build hands to the next, and what the finished drill keeps as its evidence. Every
//! stage output is saved as a job checkpoint, so each is plain data.

use crate::features::learning::drills::runner::RunOutcome;
use lattice_core::domain::learning::drills::{
    Assessment, DrillFile, Edit, IssueOutcome, KeyIssue, LessonBrief, LineSpan, Origin, Proof,
    Severity,
};
use serde::{Deserialize, Serialize};

/// A model call the build made, and the model that answered it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCall {
    pub purpose: String,
    pub model: String,
}

/// A run of code the build made, and how it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    pub what: String,
    pub outcome: RunOutcome,
}

/// The calls and the runs that led to a result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trail {
    pub calls: Vec<ModelCall>,
    pub runs: Vec<RunRecord>,
}

impl Trail {
    pub fn call(&mut self, purpose: &str, model: &str) {
        self.calls.push(ModelCall {
            purpose: purpose.to_owned(),
            model: model.to_owned(),
        });
    }

    pub fn run(&mut self, what: &str, outcome: &RunOutcome) {
        self.runs.push(RunRecord {
            what: what.to_owned(),
            outcome: outcome.clone(),
        });
    }
}

/// A passage of one of a course's sources: the bytes `start_byte..end_byte` of the source version `version_id`,
/// which never changes, so `text` is always exactly what the version holds there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePassage {
    pub version_id: String,
    pub start_byte: usize,
    pub end_byte: usize,
    /// The source's title.
    pub title: String,
    pub text: String,
}

/// Stage 1: the clean material. For code, the files and the suite that passes on them where the language has a
/// runner; for a passage, the explanation and the source passage it explains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanCode {
    pub drill_id: String,
    pub title: String,
    pub domain: String,
    pub files: Vec<DrillFile>,
    /// The paths of the files that are tests: part of the code under review.
    pub tests: Vec<String>,
    pub suite: Option<String>,
    /// The passage an explanation was written from. A checkpoint saved before passages has none.
    #[serde(default)]
    pub source: Option<SourcePassage>,
    /// The brief of the lesson the writer was shown; `None` for a passage, and in checkpoints saved before drills
    /// were built from lessons.
    #[serde(default)]
    pub brief: Option<LessonBrief>,
    /// What the lesson's fit for the material rests on, read before the brief was; `None` in checkpoints saved
    /// before drills were built from lessons.
    #[serde(default)]
    pub basis: Option<String>,
    pub trail: Trail,
}

/// A problem the clean review found, and the edit that repairs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repair {
    pub description: String,
    pub severity: Severity,
    pub edit: Edit,
}

/// Stage 2: the repaired material, which every later stage compares against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanReview {
    pub files: Vec<DrillFile>,
    pub repairs: Vec<Repair>,
    pub trail: Trail,
}

/// The material every later stage compares against, with what was written beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    pub drill_id: String,
    pub title: String,
    pub domain: String,
    pub files: Vec<DrillFile>,
    pub tests: Vec<String>,
    pub suite: Option<String>,
    pub source: Option<SourcePassage>,
    pub brief: Option<LessonBrief>,
}

impl Baseline {
    pub fn new(code: &CleanCode, review: &CleanReview) -> Self {
        Self {
            drill_id: code.drill_id.clone(),
            title: code.title.clone(),
            domain: code.domain.clone(),
            files: review.files.clone(),
            tests: code.tests.clone(),
            suite: code.suite.clone(),
            source: code.source.clone(),
            brief: code.brief.clone(),
        }
    }
}

/// How a mutation can be shown to change behaviour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verification {
    /// The source of a test that passes on the clean code and fails with the mutation.
    KillingTest(String),
    /// Why no deterministic test can observe the defect.
    Unobservable(String),
    /// Nothing was asked: the pattern or the language does not call for a test.
    NotAsked,
}

/// One planted defect as the model proposed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mutation {
    pub pattern: String,
    pub edit: Edit,
    pub severity: Severity,
    pub consequence: String,
    pub explanation: String,
    pub verification: Verification,
}

impl Mutation {
    /// The edit that undoes this one.
    pub fn fix(&self) -> Edit {
        Edit {
            path: self.edit.path.clone(),
            before: self.edit.after.clone(),
            after: self.edit.before.clone(),
        }
    }
}

/// Stage 3: one mutation for each target pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Planted {
    pub mutations: Vec<Mutation>,
    pub trail: Trail,
}

/// A mutation that was proven, and how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenMutation {
    pub mutation: Mutation,
    pub proof: Proof,
    pub trail: Trail,
}

/// A mutation that was dropped, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedMutation {
    pub mutation: Mutation,
    pub why: String,
    pub trail: Trail,
}

/// Stage 4: the mutations that stand, and the drill's files and spans with only those applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proved {
    pub proven: Vec<ProvenMutation>,
    pub dropped: Vec<DroppedMutation>,
    pub files: Vec<DrillFile>,
    /// The lines each proven mutation's new text occupies, in the order of `proven`.
    pub spans: Vec<LineSpan>,
}

impl Proved {
    /// The planted issues of the key, in the order of the targets.
    pub fn planted_key(&self) -> Vec<KeyIssue> {
        self.proven
            .iter()
            .zip(&self.spans)
            .enumerate()
            .map(|(position, (proven, span))| KeyIssue {
                id: format!("planted-{}", position + 1),
                pattern: proven.mutation.pattern.clone(),
                severity: proven.mutation.severity,
                span: span.clone(),
                consequence: proven.mutation.consequence.clone(),
                fix: proven.mutation.fix(),
                explanation: proven.mutation.explanation.clone(),
                proof: proven.proof.clone(),
                origin: Origin::Planted,
                lesson: None,
            })
            .collect()
    }
}

/// One finding of the blind review, with the lines its quote was found on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewedFinding {
    pub path: String,
    pub quote: String,
    pub comment: String,
    pub severity: Severity,
    /// `None` when the quote is not in the file, or is in it more than once.
    pub span: Option<LineSpan>,
}

/// How findable a planted issue was: whether the blind review found it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Findability {
    pub issue: String,
    pub pattern: String,
    pub outcome: IssueOutcome,
    pub reason: String,
}

/// Stage 5: what the blind review found, how it was judged, and the issues it added to the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlindReview {
    pub findings: Vec<ReviewedFinding>,
    /// `None` when there was no key to judge the review against.
    pub assessment: Option<Assessment>,
    pub findability: Vec<Findability>,
    pub extras: Vec<KeyIssue>,
    pub trail: Trail,
}

/// What the clean review found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanReviewEvidence {
    pub repairs: Vec<Repair>,
    pub trail: Trail,
}

/// How one planted issue was proven.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofEvidence {
    pub issue: String,
    pub pattern: String,
    pub proof: Proof,
    pub trail: Trail,
}

/// The blind review, as the drill keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlindReviewEvidence {
    pub findings: Vec<ReviewedFinding>,
    pub assessment: Option<Assessment>,
    pub findability: Vec<Findability>,
    /// The ids of the issues the review added to the key.
    pub extras: Vec<String>,
    pub trail: Trail,
}

/// Everything that shows how a drill was made and why its key can be trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub clean_code: Trail,
    pub clean_review: CleanReviewEvidence,
    pub plant: Trail,
    pub proofs: Vec<ProofEvidence>,
    pub dropped: Vec<DroppedMutation>,
    pub blind_review: BlindReviewEvidence,
    /// The brief of the lesson the drill was built from, when it was built from one for the writer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brief: Option<LessonBrief>,
}

impl Evidence {
    /// The evidence of a build whose stages produced these outputs.
    pub fn new(
        code: &CleanCode,
        review: &CleanReview,
        planted: &Planted,
        proved: &Proved,
        blind: &BlindReview,
    ) -> Self {
        let proofs = proved
            .proven
            .iter()
            .enumerate()
            .map(|(position, proven)| ProofEvidence {
                issue: format!("planted-{}", position + 1),
                pattern: proven.mutation.pattern.clone(),
                proof: proven.proof.clone(),
                trail: proven.trail.clone(),
            })
            .collect();
        Self {
            clean_code: code.trail.clone(),
            clean_review: CleanReviewEvidence {
                repairs: review.repairs.clone(),
                trail: review.trail.clone(),
            },
            plant: planted.trail.clone(),
            proofs,
            dropped: proved.dropped.clone(),
            blind_review: BlindReviewEvidence {
                findings: blind.findings.clone(),
                assessment: blind.assessment.clone(),
                findability: blind.findability.clone(),
                extras: blind.extras.iter().map(|issue| issue.id.clone()).collect(),
                trail: blind.trail.clone(),
            },
            brief: code.brief.clone(),
        }
    }
}
