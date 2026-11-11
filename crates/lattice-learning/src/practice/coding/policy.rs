//! Why the counterpart speaks, as the coding tables store it: the stored form of the engine's cue.

use serde::{Deserialize, Serialize};

use crate::practice::coding::dto::{CodingRunScope, CodingStage};

/// Why the counterpart speaks. Stored while pending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodingCue {
    Opening,
    Reply,
    StageOpened(CodingStage),
    AskForSkipped(Vec<CodingStage>),
    RunReview(CodingRunScope),
    Submitted,
    Checkpoint,
    Idle,
}

impl CodingCue {
    /// Which of two pending cues to keep: true when `self` ranks strictly above `other`.
    pub fn outranks(&self, other: &CodingCue) -> bool {
        self.rank() > other.rank()
    }

    fn rank(&self) -> u8 {
        match self {
            Self::Opening => 7,
            Self::Reply => 6,
            Self::Submitted => 5,
            Self::AskForSkipped(_) => 4,
            Self::StageOpened(_) => 3,
            Self::RunReview(_) => 2,
            Self::Checkpoint => 1,
            Self::Idle => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use CodingCue as Cue;
    use CodingStage as Stage;

    #[test]
    fn a_cue_outranks_those_below_it_and_never_itself() {
        let ranked = [
            Cue::Opening,
            Cue::Reply,
            Cue::Submitted,
            Cue::AskForSkipped(vec![Stage::Examples]),
            Cue::StageOpened(Stage::Plan),
            Cue::RunReview(CodingRunScope::Examples),
            Cue::Checkpoint,
            Cue::Idle,
        ];
        for (index, higher) in ranked.iter().enumerate() {
            assert!(!higher.outranks(higher), "{higher:?} outranks itself");
            for lower in ranked.iter().skip(index + 1) {
                assert!(higher.outranks(lower), "{higher:?} must outrank {lower:?}");
                assert!(
                    !lower.outranks(higher),
                    "{lower:?} must not outrank {higher:?}"
                );
            }
        }
    }

    #[test]
    fn cues_of_one_kind_do_not_outrank_each_other() {
        assert!(!Cue::StageOpened(Stage::Plan).outranks(&Cue::StageOpened(Stage::Code)));
        assert!(
            !Cue::RunReview(CodingRunScope::All).outranks(&Cue::RunReview(CodingRunScope::Input))
        );
    }

    #[test]
    fn a_cue_survives_storage() {
        let cues = [
            Cue::Opening,
            Cue::Reply,
            Cue::StageOpened(Stage::Plan),
            Cue::AskForSkipped(vec![Stage::Examples, Stage::Plan]),
            Cue::RunReview(CodingRunScope::Input),
            Cue::Submitted,
            Cue::Checkpoint,
            Cue::Idle,
        ];
        for cue in cues {
            let json = serde_json::to_string(&cue).unwrap_or_default();
            assert!(!json.is_empty(), "{cue:?} does not serialize");
            assert_eq!(serde_json::from_str::<Cue>(&json).ok(), Some(cue));
        }
    }
}
