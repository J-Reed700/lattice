//! What help a surface allows at a help level.

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
    fn allows(&self, help: Help, level: HelpLevel) -> bool {
        match level {
            HelpLevel::FullHelp => true,
            HelpLevel::HintsOnly => help == Help::Hint,
            HelpLevel::NoHelp => false,
        }
    }
}

/// Written practice: every kind of help unless the level is no help.
pub struct WrittenAssistance;

impl AssistancePolicy for WrittenAssistance {
    fn allows(&self, _help: Help, level: HelpLevel) -> bool {
        level != HelpLevel::NoHelp
    }
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
    fn written_practice_gives_every_kind_of_help_unless_there_is_none() {
        for help in HELPS {
            assert!(WrittenAssistance.allows(help, HelpLevel::FullHelp), "{help:?}");
            assert!(WrittenAssistance.allows(help, HelpLevel::HintsOnly), "{help:?}");
            assert!(!WrittenAssistance.allows(help, HelpLevel::NoHelp), "{help:?}");
        }
    }
}
