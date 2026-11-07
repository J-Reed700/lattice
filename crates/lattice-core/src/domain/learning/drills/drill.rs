//! The types of a drill and its answer key: the files a learner reviews, the
//! defects planted in them as exact edits, and how each was proven.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// How serious a known issue is. `Critical` is the greatest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Major,
    Minor,
    Nit,
}

impl Severity {
    /// Every severity, the most severe first.
    pub const ALL: [Severity; 4] = [
        Severity::Critical,
        Severity::Major,
        Severity::Minor,
        Severity::Nit,
    ];
}

impl Ord for Severity {
    fn cmp(&self, other: &Self) -> Ordering {
        weight(*self).cmp(&weight(*other))
    }
}

impl PartialOrd for Severity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// What an issue of this severity counts for in a score: the more severe, the
/// more it weighs.
pub fn weight(severity: Severity) -> u32 {
    match severity {
        Severity::Critical => 4,
        Severity::Major => 3,
        Severity::Minor => 2,
        Severity::Nit => 1,
    }
}

/// One file of a drill, as the learner reads it: line endings are `\n`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrillFile {
    pub path: String,
    pub text: String,
}

/// Whole lines of one file: 1-based, both ends included, `start <= end`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LineSpan {
    pub path: String,
    pub start: u32,
    pub end: u32,
}

/// A text replacement in one file: the exact `before` text becomes `after`.
/// The edit that plants an issue is the clean code's `before` becoming the
/// flawed `after`; its fix is the reverse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    pub path: String,
    pub before: String,
    pub after: String,
}

/// What a drill asks a learner to review.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Material {
    /// Code in one language of the catalogue.
    Code { language: String },
    /// An explanation of a passage of a learner's source, one sentence to a
    /// line.
    Passage,
}

impl Material {
    /// Code in `language`.
    pub fn code(language: impl Into<String>) -> Self {
        Self::Code {
            language: language.into(),
        }
    }

    /// The language of code, `None` for a passage.
    pub fn language(&self) -> Option<&str> {
        match self {
            Self::Code { language } => Some(language),
            Self::Passage => None,
        }
    }
}

/// What a passage's source says of a planted sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceVerdict {
    /// The source states the opposite.
    Contradicted,
    /// The source does not support it.
    Unsupported,
}

/// What shows a planted issue is real.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Proof {
    /// A test that fails on the flawed code and passes on the clean code.
    Killed { test: String },
    /// An independent judge, shown both versions, confirmed that the edit
    /// introduces exactly this problem and that the clean code lacks it.
    Judged { reason: String },
    /// The source passage supports the clean sentence with an exact quote, and
    /// the checker found the planted one contradicted or unsupported by it.
    Sourced {
        quote: String,
        planted: SourceVerdict,
        reason: String,
    },
}

/// Where a key issue came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Planted as an edit to the clean code.
    Planted,
    /// Reported by a blind review of the finished code and judged real
    /// against the clean code.
    FoundInReview,
}

/// One known issue of a drill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyIssue {
    pub id: String,
    /// The id of the catalogue pattern this issue is an instance of.
    pub pattern: String,
    pub severity: Severity,
    /// Where the issue is, in the drill's files: computed from the applied
    /// edit, never stated by a model.
    pub span: LineSpan,
    /// What goes wrong because of it.
    pub consequence: String,
    /// The edit that repairs it.
    pub fix: Edit,
    /// How to say it in an interview.
    pub explanation: String,
    pub proof: Proof,
    pub origin: Origin,
    /// What the issue breaks of its drill's lesson: `None` for an issue of the catalogue alone, and in keys stored
    /// before drills were built from lessons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lesson: Option<LessonLink>,
}

/// What a key issue goes against in the lesson its drill was built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LessonLink {
    pub lesson_id: String,
    /// The lesson's sentences the defect breaks, as code copied them from the lesson's brief.
    pub sentences: Vec<String>,
    /// The title of the source the sentences come from, when they come from one.
    pub source: Option<String>,
}

/// Material to review and what is known to be wrong with it. A drill stored
/// with a `language` and no `material` is read as code in that language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "StoredDrill")]
pub struct Drill {
    pub id: String,
    pub material: Material,
    pub title: String,
    pub files: Vec<DrillFile>,
    pub key: Vec<KeyIssue>,
}

/// A drill as stored: new rows hold a `material`, rows from before passages
/// hold the language of their code.
#[derive(Deserialize)]
struct StoredDrill {
    id: String,
    material: Option<Material>,
    language: Option<String>,
    title: String,
    files: Vec<DrillFile>,
    key: Vec<KeyIssue>,
}

impl TryFrom<StoredDrill> for Drill {
    type Error = String;

    fn try_from(stored: StoredDrill) -> Result<Self, String> {
        let material = match (stored.material, stored.language) {
            (Some(material), _) => material,
            (None, Some(language)) => Material::Code { language },
            (None, None) => return Err("a drill has neither a material nor a language".to_owned()),
        };
        Ok(Self {
            id: stored.id,
            material,
            title: stored.title,
            files: stored.files,
            key: stored.key,
        })
    }
}
