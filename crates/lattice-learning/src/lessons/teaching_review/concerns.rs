//! The concerns a lesson's reviewer reports, and the section kinds each applies to.
use crate::dto::LearningBlockKind;
use serde::{Deserialize, Serialize};

/// A defect a lesson's reviewer owns; every other check is left to other checks.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LessonConcern {
    WorkedExample,
    Exercise,
    AnswerGivenAway,
    CheckAnswer,
    NearMiss,
    Promise,
    LaterLesson,
    Workload,
}

impl LessonConcern {
    /// Every concern, in the order the reviewer is told them.
    pub(crate) const ALL: [Self; 8] = [
        Self::WorkedExample,
        Self::Exercise,
        Self::AnswerGivenAway,
        Self::CheckAnswer,
        Self::NearMiss,
        Self::Promise,
        Self::LaterLesson,
        Self::Workload,
    ];

    /// The ids of `concerns`, comma-separated.
    pub(crate) fn list(concerns: impl IntoIterator<Item = Self>) -> String {
        concerns
            .into_iter()
            .map(Self::id)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The ids of the concerns `keep` accepts, in the order of `ALL`.
    pub(crate) fn ids_where(keep: fn(Self) -> bool) -> Vec<&'static str> {
        Self::ALL
            .into_iter()
            .filter(|concern| keep(*concern))
            .map(Self::id)
            .collect()
    }

    /// The concern's serialized name.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::WorkedExample => "worked_example",
            Self::Exercise => "exercise",
            Self::AnswerGivenAway => "answer_given_away",
            Self::CheckAnswer => "check_answer",
            Self::NearMiss => "near_miss",
            Self::Promise => "promise",
            Self::LaterLesson => "later_lesson",
            Self::Workload => "workload",
        }
    }

    /// Whether a section of `kind` can fail this concern.
    pub(crate) fn applies_to(self, kind: &LearningBlockKind) -> bool {
        use LearningBlockKind as Kind;
        match self {
            Self::WorkedExample => matches!(kind, Kind::WorkedExample),
            Self::Exercise => matches!(kind, Kind::GuidedPractice | Kind::IndependentPractice),
            Self::AnswerGivenAway => matches!(kind, Kind::Check | Kind::GuidedPractice),
            Self::CheckAnswer => matches!(kind, Kind::Check),
            Self::NearMiss => matches!(kind, Kind::Contrast),
            Self::Promise | Self::LaterLesson => true,
            Self::Workload => false,
        }
    }

    /// Whether a section whose kind is `kind` (none when unreadable) takes this concern.
    pub(crate) fn takes(self, kind: Option<&LearningBlockKind>) -> bool {
        match kind {
            Some(kind) => self.applies_to(kind),
            None => LearningBlockKind::ALL
                .iter()
                .all(|kind| self.applies_to(kind)),
        }
    }

    /// Whether a section's check can name this concern.
    pub(crate) fn in_checks(self) -> bool {
        LearningBlockKind::ALL
            .iter()
            .any(|kind| self.applies_to(kind))
    }

    /// Whether the concern is reported as an issue, outside every section.
    pub(crate) fn in_issues(self) -> bool {
        matches!(self, Self::Promise | Self::LaterLesson | Self::Workload)
    }

    /// What the reviewer is asked about this concern.
    pub(crate) fn asks(self) -> &'static str {
        match self {
            Self::WorkedExample => "A worked example does not follow from its stated premises to its stated result: work it through yourself, inspecting its stated conditions, quantities, procedure and claimed result.",
            Self::Exercise => "An exercise cannot be done as written, or its rubric does not evaluate the work it asks for: solve it yourself.",
            Self::AnswerGivenAway => "A check gives its answer away above its 'Check your thinking' line, which reveals the answer below it by design, or guided practice leaks its answer.",
            Self::CheckAnswer => "A check's answer, or its reasoning below its 'Check your thinking' line, is wrong.",
            Self::NearMiss => "A contrast's near-miss does not actually differ from its case in the feature it names.",
            Self::Promise => "The lesson promises automatic test execution, external peer review or expert certification. Written practice and tutor feedback review text only; code runs only in a separate lab runtime the learner configures and starts.",
            Self::LaterLesson => "The lesson says a later lesson teaches something that no later lesson in the course plan teaches. This rests on the course plan, not on a source.",
            Self::Workload => "A specific objective cannot be scoped to the stated session, with every preceding lesson taken as known; a topic title alone is not a workload defect.",
        }
    }
}
