//! What help a surface allows at a help level.

use crate::dto::{LearningPracticeHintLevel, LearningPracticeMode, LearningTutorRequestKind};

/// How much help a surface gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpLevel {
    FullHelp,
    HintsOnly,
    NoHelp,
}

/// A kind of help a learner can ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Help {
    Hint,
    WorkedSolution,
    Tutor,
}

/// Whether a surface gives `help` at `level`.
pub trait AssistancePolicy: Send + Sync {
    fn allows(&self, help: Help, level: HelpLevel) -> bool;
}

/// Coding sessions: full help gives everything, hints-only gives hints, no help gives nothing.
pub struct CodingAssistance;

impl AssistancePolicy for CodingAssistance {
    fn allows(&self, _help: Help, _level: HelpLevel) -> bool {
        false
    }
}

/// Written practice: every kind of help unless the level is no help.
pub struct WrittenAssistance;

impl AssistancePolicy for WrittenAssistance {
    fn allows(&self, _help: Help, _level: HelpLevel) -> bool {
        false
    }
}

/// The help level of a written-practice mode: Explore gives full help, Practice hints only, Demonstrate none.
pub fn written_level(_mode: &LearningPracticeMode) -> HelpLevel {
    HelpLevel::FullHelp
}

/// The kind of help a tutor request asks for: a worked explanation is a worked solution.
pub fn written_help(
    _kind: &LearningTutorRequestKind,
    _level: Option<&LearningPracticeHintLevel>,
) -> Help {
    Help::Tutor
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELPS: [Help; 3] = [Help::Hint, Help::WorkedSolution, Help::Tutor];

    #[test]
    fn coding_full_help_allows_every_kind() {
        for help in HELPS {
            assert!(CodingAssistance.allows(help, HelpLevel::FullHelp), "{help:?}");
        }
    }

    #[test]
    fn coding_hints_only_allows_hints_and_nothing_else() {
        assert!(CodingAssistance.allows(Help::Hint, HelpLevel::HintsOnly));
        assert!(!CodingAssistance.allows(Help::WorkedSolution, HelpLevel::HintsOnly));
        assert!(!CodingAssistance.allows(Help::Tutor, HelpLevel::HintsOnly));
    }

    #[test]
    fn coding_no_help_allows_nothing() {
        for help in HELPS {
            assert!(!CodingAssistance.allows(help, HelpLevel::NoHelp), "{help:?}");
        }
    }

    #[test]
    fn written_practice_table() {
        for help in HELPS {
            assert!(WrittenAssistance.allows(help, HelpLevel::FullHelp), "{help:?}");
            assert!(!WrittenAssistance.allows(help, HelpLevel::NoHelp), "{help:?}");
        }
        assert!(WrittenAssistance.allows(Help::Hint, HelpLevel::HintsOnly));
        assert!(WrittenAssistance.allows(Help::Tutor, HelpLevel::HintsOnly));
        assert!(!WrittenAssistance.allows(Help::WorkedSolution, HelpLevel::HintsOnly));
    }

    #[test]
    fn a_written_mode_has_its_level() {
        assert_eq!(written_level(&LearningPracticeMode::Explore), HelpLevel::FullHelp);
        assert_eq!(written_level(&LearningPracticeMode::Practice), HelpLevel::HintsOnly);
        assert_eq!(written_level(&LearningPracticeMode::Demonstrate), HelpLevel::NoHelp);
    }

    #[test]
    fn a_tutor_request_asks_for_the_help_it_gives() {
        use LearningPracticeHintLevel::*;
        let hint = LearningTutorRequestKind::Hint;
        assert_eq!(written_help(&hint, Some(&WorkedExplanation)), Help::WorkedSolution);
        for level in [OrientingQuestion, ConceptOrSource, PartialStrategy] {
            assert_eq!(written_help(&hint, Some(&level)), Help::Hint, "{level:?}");
        }
        assert_eq!(written_help(&hint, None), Help::Hint);
        for kind in [LearningTutorRequestKind::Question, LearningTutorRequestKind::Critique] {
            assert_eq!(written_help(&kind, None), Help::Tutor);
            assert_eq!(written_help(&kind, Some(&PartialStrategy)), Help::Tutor);
        }
    }
}
