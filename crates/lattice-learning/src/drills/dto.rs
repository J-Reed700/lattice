//! What the review view sends and reads (docs/design/2026-11-04-code-review-drills.md section 1). The drill a learner
//! may see holds its title, language and files and nothing of its key, its clean code, its suite, its killing tests or
//! its evidence; the reveal after a grade holds the key's issues and the clean code beside the drill's.

use super::attempts::{AttemptMode, AttemptStatus};
use super::bank::{NextStatus, NextUp};
use lattice_core::domain::learning::drills::{
    DiffLine, DrillFile, DrillStanding, Finding, IssueFix, LineSpan, Material, PatternRating,
    Score, Severity, SeverityTally,
};
use serde::{Deserialize, Serialize};

/// How serious an issue is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CodeReviewSeverity {
    Critical,
    Major,
    Minor,
    Nit,
}

impl From<Severity> for CodeReviewSeverity {
    fn from(severity: Severity) -> Self {
        match severity {
            Severity::Critical => Self::Critical,
            Severity::Major => Self::Major,
            Severity::Minor => Self::Minor,
            Severity::Nit => Self::Nit,
        }
    }
}

impl From<CodeReviewSeverity> for Severity {
    fn from(severity: CodeReviewSeverity) -> Self {
        match severity {
            CodeReviewSeverity::Critical => Self::Critical,
            CodeReviewSeverity::Major => Self::Major,
            CodeReviewSeverity::Minor => Self::Minor,
            CodeReviewSeverity::Nit => Self::Nit,
        }
    }
}

/// One file of a drill, as the learner reads it or leaves it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodeReviewFileDto {
    pub path: String,
    pub text: String,
}

impl From<&DrillFile> for CodeReviewFileDto {
    fn from(file: &DrillFile) -> Self {
        Self {
            path: file.path.clone(),
            text: file.text.clone(),
        }
    }
}

impl From<CodeReviewFileDto> for DrillFile {
    fn from(file: CodeReviewFileDto) -> Self {
        Self {
            path: file.path,
            text: file.text,
        }
    }
}

/// Whole lines of one file: 1-based, both ends included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodeReviewSpanDto {
    pub path: String,
    pub start: u32,
    pub end: u32,
}

impl From<&LineSpan> for CodeReviewSpanDto {
    fn from(span: &LineSpan) -> Self {
        Self {
            path: span.path.clone(),
            start: span.start,
            end: span.end,
        }
    }
}

impl From<CodeReviewSpanDto> for LineSpan {
    fn from(span: CodeReviewSpanDto) -> Self {
        Self {
            path: span.path,
            start: span.start,
            end: span.end,
        }
    }
}

/// One comment of a learner's review: the lines it is on, when it selects any, what it says, and the severity the
/// learner gave it, when they gave one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodeReviewFindingDto {
    pub span: Option<CodeReviewSpanDto>,
    pub text: String,
    pub severity: Option<CodeReviewSeverity>,
}

impl From<&Finding> for CodeReviewFindingDto {
    fn from(finding: &Finding) -> Self {
        Self {
            span: finding.span.as_ref().map(CodeReviewSpanDto::from),
            text: finding.text.clone(),
            severity: finding.severity.map(CodeReviewSeverity::from),
        }
    }
}

impl From<CodeReviewFindingDto> for Finding {
    fn from(finding: CodeReviewFindingDto) -> Self {
        Self {
            span: finding.span.map(LineSpan::from),
            text: finding.text,
            severity: finding.severity.map(Severity::from),
        }
    }
}

/// What a drill of a course is on: code in a language, or an explanation of a passage of the course's sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DrillMaterialDto {
    Code { language: String },
    Passage,
}

impl From<DrillMaterialDto> for Material {
    fn from(material: DrillMaterialDto) -> Self {
        match material {
            DrillMaterialDto::Code { language } => Self::Code { language },
            DrillMaterialDto::Passage => Self::Passage,
        }
    }
}

impl From<&Material> for DrillMaterialDto {
    fn from(material: &Material) -> Self {
        match material {
            Material::Code { language } => Self::Code {
                language: language.clone(),
            },
            Material::Passage => Self::Passage,
        }
    }
}

/// What a drill is made of, as the learner may see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CodeReviewMaterialDto {
    Code {
        language: String,
    },
    /// An explanation of a passage; the passage itself is in the reveal only.
    Passage {
        #[serde(rename = "sourceTitle")]
        source_title: String,
    },
}

/// The lesson of a course a drill was built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DrillLessonRefDto {
    pub program_id: String,
    pub lesson_id: String,
    /// The lesson's title now.
    pub title: String,
}

/// The drill as the learner may see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodeReviewDrillDto {
    pub title: String,
    pub material: CodeReviewMaterialDto,
    pub files: Vec<CodeReviewFileDto>,
    /// The lesson the drill was built from, while it exists; none for a drill from before drills were built from
    /// lessons.
    pub lesson: Option<DrillLessonRefDto>,
    /// How many issues were planted: the number the learner is asked to find, and the one the score counts against.
    pub issues: usize,
}

/// A course's review drills, as its Practice view reads them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CourseDrillsDto {
    /// The language code drills are offered in: the one asked for, or the one the course suggests.
    pub language: String,
    /// What a code drill in `language` with no lesson asked for comes to; none when no lesson can be drilled in code.
    pub next_code: Option<NextDrillDto>,
    /// What an explanation with no lesson asked for comes to; none when no lesson can be drilled on a passage.
    pub next_passage: Option<NextDrillDto>,
    /// The course has a source with text, which an explanation's passage is taken from.
    pub has_sources: bool,
    /// The course's modules in order, each with its lessons a drill can be built from.
    pub modules: Vec<DrillModuleDto>,
    /// The course's latest attempts, newest first.
    pub recent: Vec<CourseDrillAttemptDto>,
}

/// The lesson a start with no lesson asked for draws on, and what it comes to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NextDrillDto {
    pub lesson_id: String,
    pub status: NextDrillStatus,
}

/// Whether the lesson a start with no lesson draws on has a drill ready, one being built, or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum NextDrillStatus {
    /// A drill of it is ready, and the start opens an attempt on it.
    Ready,
    /// A build of it is under way, and the start waits for it.
    Building,
    /// The start queues its build.
    ToBuild,
}

impl From<NextStatus> for NextDrillStatus {
    fn from(status: NextStatus) -> Self {
        match status {
            NextStatus::Ready => Self::Ready,
            NextStatus::Building => Self::Building,
            NextStatus::ToBuild => Self::ToBuild,
        }
    }
}

impl From<NextUp> for NextDrillDto {
    fn from(next: NextUp) -> Self {
        Self {
            lesson_id: next.lesson_id,
            status: next.status.into(),
        }
    }
}

/// A module of the course, and its lessons a drill can be built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DrillModuleDto {
    pub module_id: String,
    pub title: String,
    pub lessons: Vec<DrillLessonDto>,
}

/// A lesson a drill can be built from, and where its drills stand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DrillLessonDto {
    pub lesson_id: String,
    pub title: String,
    pub standing: DrillStandingDto,
    /// When its drills are next due; none while no drill of it has been rated.
    pub due_at: Option<i64>,
    /// A code drill can be built from it: false once one kept none of the lesson's ideas, until the lesson or the
    /// course's sources change.
    pub fits_code: bool,
    /// An explanation can be built from it: false once the course's sources held no passage for it, until the lesson
    /// or the sources change.
    pub fits_passage: bool,
    /// The course's current lesson.
    pub current: bool,
    /// The learner marked it done.
    pub completed: bool,
}

/// Where a lesson's drills stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DrillStandingDto {
    /// Rated, and due again.
    Due,
    /// No drill of it has been rated.
    New,
    /// Rated, and not due yet.
    Rested,
    /// A drill of it is out and not yet graded, or ready to be taken.
    Pending,
}

impl From<DrillStanding> for DrillStandingDto {
    fn from(standing: DrillStanding) -> Self {
        match standing {
            DrillStanding::Due => Self::Due,
            DrillStanding::New => Self::New,
            DrillStanding::Rested => Self::Rested,
            DrillStanding::Pending => Self::Pending,
        }
    }
}

/// One of a course's latest attempts, as its drills view lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CourseDrillAttemptDto {
    pub attempt_id: String,
    pub status: AttemptStatus,
    pub mode: AttemptMode,
    pub started_at: i64,
    /// The drill's title.
    pub title: String,
    pub material: DrillMaterialDto,
    /// The lesson the drill was built from, while it exists.
    pub lesson: Option<DrillLessonRefDto>,
    /// How the review fared, once it is graded.
    pub score: Option<CodeReviewScoreDto>,
}

/// What starting a drill came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StartedCodeReviewDto {
    /// A drill was ready: this attempt is open on it.
    Started {
        #[serde(rename = "attemptId")]
        attempt_id: String,
        mode: AttemptMode,
        #[serde(rename = "startedAt")]
        started_at: i64,
        drill: CodeReviewDrillDto,
    },
    /// None was ready: this build will make one.
    Building {
        #[serde(rename = "jobId")]
        job_id: String,
    },
}

/// An attempt: where it stands, the learner's own review and, once it is graded, the reveal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodeReviewResultDto {
    pub attempt_id: String,
    pub mode: AttemptMode,
    pub status: AttemptStatus,
    pub started_at: i64,
    pub submitted_at: Option<i64>,
    /// As the client measured it.
    pub time_spent_ms: Option<i64>,
    pub drill: CodeReviewDrillDto,
    /// The learner's own comments, as they handed them in.
    pub findings: Vec<CodeReviewFindingDto>,
    /// The drill's files as the learner left them, when they wrote a fix.
    pub fix_files: Option<Vec<CodeReviewFileDto>>,
    /// Why the review could not be graded, when it could not.
    pub error: Option<String>,
    /// The model that graded the review, or tried to, as the server names it.
    pub grader_model: Option<String>,
    pub graded_at: Option<i64>,
    /// Everything the key knows, once the review is graded.
    pub reveal: Option<CodeReviewRevealDto>,
}

/// What a graded review shows the learner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodeReviewRevealDto {
    /// Every key issue, in the key's order.
    pub issues: Vec<RevealedIssueDto>,
    /// Every finding of the learner's, in the order they made them.
    pub findings: Vec<RevealedFindingDto>,
    pub score: CodeReviewScoreDto,
    /// How well the patterns of the key's issues were recognised. A review-found issue that was not raised has none.
    pub ratings: Vec<RatedPatternDto>,
    /// What came of the learner's fix for each key issue, when they wrote one.
    pub fix: Option<Vec<IssueFix>>,
    /// The code before any issue was planted, to read beside the drill's.
    pub clean_files: Vec<CodeReviewFileDto>,
    /// The source passage an explanation was written from; none for code.
    pub source: Option<RevealedSourceDto>,
}

/// The source passage of a drill on a passage, as the reveal shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RevealedSourceDto {
    pub title: String,
    pub text: String,
    /// The version of the source the passage is the exact text of.
    pub version_id: String,
}

/// What came of an issue of the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum IssueResultKind {
    Found,
    /// Right lines, wrong reason.
    HalfFound,
    Missed,
}

/// What came of a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FindingResultKind {
    /// It is about issues of the key.
    Matches,
    /// A real problem the key does not hold.
    ValidExtra,
    FalsePositive,
}

/// The fix of an issue as the lines it removes and the lines it adds, in the file the issue is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FixDiffDto {
    pub path: String,
    pub lines: Vec<DiffLine>,
}

/// One issue of the key, with what came of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RevealedIssueDto {
    pub id: String,
    /// The id of the catalogue pattern.
    pub pattern: String,
    pub pattern_name: String,
    pub severity: CodeReviewSeverity,
    /// Where the issue is, in the drill's files.
    pub span: CodeReviewSpanDto,
    pub consequence: String,
    /// How to say it in an interview.
    pub explanation: String,
    pub fix: FixDiffDto,
    /// The words of the source passage that settle the issue, when a source proved it.
    pub source_quote: Option<String>,
    pub outcome: IssueResultKind,
    /// The position of the finding that earned the credit, when one did.
    pub finding: Option<usize>,
    /// The judge's reason for the outcome.
    pub reason: String,
    /// Not planted: a blind review raised it. Raising it counts for the learner and leaving it costs nothing.
    pub also_valid: bool,
    /// What the issue breaks of the lesson the drill was built from; none for an issue of the catalogue alone.
    pub lesson: Option<RevealedLessonLinkDto>,
}

/// The lesson's words a key issue goes against, as the reveal shows them under "From the lesson".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RevealedLessonLinkDto {
    pub lesson_id: String,
    /// In the order the key cites them: a sentence, or a whole block of the lesson's code, each.
    pub sentences: Vec<RevealedLessonSentenceDto>,
}

/// One unit of a lesson's words, and where it comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RevealedLessonSentenceDto {
    pub text: String,
    /// The title of the source it comes from; none for the lesson's own teaching.
    pub source: Option<String>,
}

/// One finding of the learner's, with what came of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RevealedFindingDto {
    pub span: Option<CodeReviewSpanDto>,
    pub text: String,
    pub severity: Option<CodeReviewSeverity>,
    pub outcome: FindingResultKind,
    /// The positions of the key's issues the finding is about; none unless it matches.
    pub issues: Vec<usize>,
    /// The judge's reason for the outcome.
    pub reason: String,
}

/// How a review fared, as its parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodeReviewScoreDto {
    pub found: usize,
    pub half_found: usize,
    pub missed: usize,
    pub valid_extras: usize,
    pub false_positives: usize,
    /// One tally for each severity the key has, the most severe first.
    pub by_severity: Vec<SeverityTallyDto>,
    pub weighted_recall: f64,
    /// The share of findings that match an issue or are a valid extra; none without findings.
    pub precision: Option<f64>,
    pub led_with_the_most_severe: bool,
}

/// The key's issues of one severity and how many of them were found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SeverityTallyDto {
    pub severity: CodeReviewSeverity,
    pub found: usize,
    pub half_found: usize,
    pub total: usize,
}

impl From<&SeverityTally> for SeverityTallyDto {
    fn from(tally: &SeverityTally) -> Self {
        Self {
            severity: tally.severity.into(),
            found: tally.found,
            half_found: tally.half_found,
            total: tally.total,
        }
    }
}

impl From<&Score> for CodeReviewScoreDto {
    fn from(score: &Score) -> Self {
        Self {
            found: score.found,
            half_found: score.half_found,
            missed: score.missed,
            valid_extras: score.valid_extras,
            false_positives: score.false_positives,
            by_severity: score
                .by_severity
                .iter()
                .map(SeverityTallyDto::from)
                .collect(),
            weighted_recall: score.weighted_recall,
            precision: score.precision,
            led_with_the_most_severe: score.led_with_the_most_severe,
        }
    }
}

/// How well one key issue's pattern was recognised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RatedPatternDto {
    pub pattern: String,
    pub pattern_name: String,
    pub rating: PatternRating,
}
