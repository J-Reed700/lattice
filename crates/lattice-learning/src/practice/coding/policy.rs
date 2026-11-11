//! Who speaks in a coding session, and when: the cue table of each setting.

use serde::{Deserialize, Serialize};

use crate::practice::coding::dto::{asked_for, CodingRunScope, CodingStage};
use crate::practice::coding::observation::WorkspaceObservation;
use crate::practice::coding::problem::{CodingSetting, CodingTask};
use crate::practice::simulation_policy::TurnPolicy;
use lattice_core::shared::error::{AppError, Result};

/// Something that happened in a session that may make the counterpart speak.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodingEvent {
    Started,
    LearnerMessage {
        spoken: bool,
    },
    StageMoved {
        from: CodingStage,
        to: CodingStage,
        skipped: Vec<CodingStage>,
    },
    RunFinished {
        scope: CodingRunScope,
    },
    DraftPaused {
        changed: bool,
    },
    Idle,
}

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

pub type CodingPolicy = dyn TurnPolicy<WorkspaceObservation, Event = CodingEvent, Cue = CodingCue>;

/// The one-line instruction a stage opens with.
pub fn stage_instruction(stage: CodingStage) -> &'static str {
    match stage {
        CodingStage::Understand => {
            "Say what the function takes and returns, and ask about anything the statement leaves open."
        }
        CodingStage::Examples => {
            "Write examples, edge cases included, and check each one against the problem."
        }
        CodingStage::Plan => "Say how you will solve it, and what that costs, before you write code.",
        CodingStage::Code => "Write the function, one small step at a time, and run it as you go.",
        CodingStage::Test => "Run your examples, then submit to run every case.",
        CodingStage::Review => "State the time and space cost, and look for anything to simplify.",
        CodingStage::Reproduce => "Run the code and see the failure for yourself.",
        CodingStage::Locate => "Find the line, or the idea, that is wrong.",
        CodingStage::Fix => "Change the code so that the failure goes away and nothing else does.",
        CodingStage::Verify => "Run every case and say why the code is right now.",
    }
}

/// The coach of the Coached setting: teaches, and speaks on every message, stage and run.
pub struct Coach {
    pub task: CodingTask,
}

/// The interviewer of the Mock setting: in role, speaks on messages, submits and skipped stages.
pub struct Interviewer {
    pub task: CodingTask,
}

/// The interviewer of the Watched setting: as the Mock one, and at checkpoints; it may stay silent.
pub struct WatchingInterviewer {
    pub task: CodingTask,
}

fn cue_for(setting: CodingSetting, task: CodingTask, event: &CodingEvent) -> Option<CodingCue> {
    let coached = setting == CodingSetting::Coached;
    let watched = setting == CodingSetting::Watched;
    match event {
        CodingEvent::Started => Some(CodingCue::Opening),
        CodingEvent::LearnerMessage { spoken } => Some(if *spoken && watched {
            CodingCue::Checkpoint
        } else {
            CodingCue::Reply
        }),
        CodingEvent::StageMoved { to, skipped, .. } => {
            if coached {
                return Some(CodingCue::StageOpened(*to));
            }
            let asked: Vec<CodingStage> = skipped
                .iter()
                .filter(|stage| asked_for(task).contains(stage))
                .copied()
                .collect();
            (!asked.is_empty()).then_some(CodingCue::AskForSkipped(asked))
        }
        CodingEvent::RunFinished { scope } => match (setting, scope) {
            (CodingSetting::Coached, scope) => Some(CodingCue::RunReview(*scope)),
            (_, CodingRunScope::All) => Some(CodingCue::Submitted),
            (CodingSetting::Watched, _) => Some(CodingCue::Checkpoint),
            (CodingSetting::Mock, _) => None,
        },
        CodingEvent::DraftPaused { changed } => {
            (watched && *changed).then_some(CodingCue::Checkpoint)
        }
        CodingEvent::Idle => coached.then_some(CodingCue::Idle),
    }
}

/// The text of `content` outside fenced code blocks.
fn outside_code(content: &str) -> String {
    let mut inside = false;
    let mut kept = Vec::new();
    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            inside = !inside;
        } else if !inside {
            kept.push(line);
        }
    }
    kept.join("\n")
}

fn check_for(setting: CodingSetting, content: &str) -> Result<()> {
    if setting != CodingSetting::Coached && outside_code(content).matches('?').count() > 1 {
        return Err(AppError::InvalidInput(
            "An interviewer asks one question at a time. Ask only one question, with one question mark outside any code."
                .into(),
        ));
    }
    Ok(())
}

const COACH_ROLE: &str = "You are the coach in a coding practice session. You teach: you explain, ask the question that moves the learner on, and say what they did well. You never write their solution for them.";

const INTERVIEWER_ROLE: &str = "You are the interviewer in a mock coding interview. Stay in role: you hold a problem for the candidate and are not a tutor. Do not teach, praise or grade; ask, listen and probe. Ask at most one question per turn, with one question mark outside any code.";

const WATCHING_ROLE: &str = " You are watching the candidate's code change as they work. Mostly stay quiet. Speak only when a question about what they are doing is worth asking.";

const RULES: &str = "Speak about the learner's code, runs and examples only from what you are given; never invent a result. Refer to code by line number. Do not show a complete solution: any code you write must leave the problem unsolved. Never name the technique or pattern that solves the problem; ask about the idea instead. Never write an id. Keep it short, and in the words a person would say aloud. When the learner asks a question the statement leaves open, answer it from the clarifications you are given, word for word, and set clarification to that clarification's number; when none covers it, write \"Your call: state your assumption.\" and set clarification to your_call; otherwise set clarification to none.";

const FORMAT: &str = "Answer as JSON: content is what you say; lines are the line numbers of the learner's code that you refer to, or none; path is the file those lines are in, or null; clarification is as described above.";

const SPEAK: &str =
    " Also set \"speak\" to false to stay silent, in which case content may be empty.";

fn stage_names(stages: &[CodingStage]) -> String {
    stages
        .iter()
        .map(|stage| format!("{stage:?}"))
        .collect::<Vec<_>>()
        .join(" and ")
}

fn cue_instruction(
    setting: CodingSetting,
    cue: &CodingCue,
    state: &WorkspaceObservation,
) -> String {
    let coached = setting == CodingSetting::Coached;
    match cue {
        CodingCue::Opening if coached => format!(
            "Open the session: greet the learner in a sentence, then give the instruction of the {:?} stage in your own words: {}",
            state.stage,
            stage_instruction(state.stage)
        ),
        CodingCue::Opening => {
            "Open the interview: introduce yourself in a sentence, say the problem is on screen, and ask the candidate to read it and say what they would like to clarify.".to_owned()
        }
        CodingCue::Reply => "Answer the learner's last message in the transcript.".to_owned(),
        CodingCue::StageOpened(stage) => format!(
            "The learner has moved to the {stage:?} stage. Open it with its instruction, in your own words: {}",
            stage_instruction(*stage)
        ),
        CodingCue::AskForSkipped(stages) => format!(
            "The candidate skipped {}. As an interviewer would, ask for it in one question: what they would have done there.",
            stage_names(stages)
        ),
        CodingCue::RunReview(scope) => format!(
            "The learner ran {scope:?}. Read the last run. Say what it shows, point at what to look at without fixing it, and ask what they make of it."
        ),
        CodingCue::Submitted if coached => {
            "The learner submitted every case. Say how it went from the last run, what to look at in a failing case, and what comes next.".to_owned()
        }
        CodingCue::Submitted => {
            "The candidate submitted. React as an interviewer: acknowledge the result without hinting at the cases you cannot see, and ask one question, about the cost of the code, an edge, or why they chose something.".to_owned()
        }
        CodingCue::Checkpoint => {
            "This is a checkpoint. Speak only if a question is worth asking now: why they chose something, what a line costs, or an edge they have not mentioned. If nothing is worth saying, stay silent.".to_owned()
        }
        CodingCue::Idle => {
            "The learner has been quiet for a while, with no edit, run or message. Check in kindly: ask what they are thinking about, and say they can ask for a hint.".to_owned()
        }
    }
}

fn system_for(setting: CodingSetting, cue: &CodingCue, state: &WorkspaceObservation) -> String {
    let role = match setting {
        CodingSetting::Coached => COACH_ROLE.to_owned(),
        CodingSetting::Mock => INTERVIEWER_ROLE.to_owned(),
        CodingSetting::Watched => format!("{INTERVIEWER_ROLE}{WATCHING_ROLE}"),
    };
    let speak = if setting == CodingSetting::Watched && *cue == CodingCue::Checkpoint {
        SPEAK
    } else {
        ""
    };
    format!(
        "{role}\n\n{RULES}\n\n{}\n\n{FORMAT}{speak}",
        cue_instruction(setting, cue, state)
    )
}

macro_rules! policy {
    ($policy:ident, $setting:expr) => {
        impl TurnPolicy<WorkspaceObservation> for $policy {
            type Event = CodingEvent;
            type Cue = CodingCue;

            fn cue(&self, event: &CodingEvent, _state: &WorkspaceObservation) -> Option<CodingCue> {
                cue_for($setting, self.task, event)
            }

            fn system(&self, cue: &CodingCue, state: &WorkspaceObservation) -> String {
                system_for($setting, cue, state)
            }

            fn check(
                &self,
                content: &str,
                _cue: &CodingCue,
                _state: &WorkspaceObservation,
            ) -> Result<()> {
                check_for($setting, content)
            }
        }
    };
}

policy!(Coach, CodingSetting::Coached);
policy!(Interviewer, CodingSetting::Mock);
policy!(WatchingInterviewer, CodingSetting::Watched);

/// The policy of a setting.
pub fn policy_for(setting: CodingSetting, task: CodingTask) -> Box<CodingPolicy> {
    match setting {
        CodingSetting::Coached => Box::new(Coach { task }),
        CodingSetting::Mock => Box::new(Interviewer { task }),
        CodingSetting::Watched => Box::new(WatchingInterviewer { task }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::practice::coding::observation::test_support::observation;

    use CodingCue as Cue;
    use CodingEvent as Event;
    use CodingStage as Stage;

    const SETTINGS: [CodingSetting; 3] = [
        CodingSetting::Coached,
        CodingSetting::Mock,
        CodingSetting::Watched,
    ];

    fn moved(from: Stage, to: Stage, skipped: &[Stage]) -> Event {
        Event::StageMoved {
            from,
            to,
            skipped: skipped.to_vec(),
        }
    }

    /// One row of the design's cue table: the event and the cue of Coached, Mock and Watched.
    fn rows() -> Vec<(&'static str, Event, [Option<Cue>; 3])> {
        vec![
            (
                "started",
                Event::Started,
                [Some(Cue::Opening), Some(Cue::Opening), Some(Cue::Opening)],
            ),
            (
                "typed message",
                Event::LearnerMessage { spoken: false },
                [Some(Cue::Reply), Some(Cue::Reply), Some(Cue::Reply)],
            ),
            (
                "spoken message",
                Event::LearnerMessage { spoken: true },
                [Some(Cue::Reply), Some(Cue::Reply), Some(Cue::Checkpoint)],
            ),
            (
                "stage moved, nothing skipped",
                moved(Stage::Understand, Stage::Examples, &[]),
                [Some(Cue::StageOpened(Stage::Examples)), None, None],
            ),
            (
                "stage moved, one asked-for stage skipped",
                moved(Stage::Understand, Stage::Plan, &[Stage::Examples]),
                [
                    Some(Cue::StageOpened(Stage::Plan)),
                    Some(Cue::AskForSkipped(vec![Stage::Examples])),
                    Some(Cue::AskForSkipped(vec![Stage::Examples])),
                ],
            ),
            (
                "stage moved, both asked-for stages skipped",
                moved(
                    Stage::Understand,
                    Stage::Code,
                    &[Stage::Examples, Stage::Plan],
                ),
                [
                    Some(Cue::StageOpened(Stage::Code)),
                    Some(Cue::AskForSkipped(vec![Stage::Examples, Stage::Plan])),
                    Some(Cue::AskForSkipped(vec![Stage::Examples, Stage::Plan])),
                ],
            ),
            (
                "stage moved, asked-for and other stages skipped",
                moved(Stage::Examples, Stage::Test, &[Stage::Plan, Stage::Code]),
                [
                    Some(Cue::StageOpened(Stage::Test)),
                    Some(Cue::AskForSkipped(vec![Stage::Plan])),
                    Some(Cue::AskForSkipped(vec![Stage::Plan])),
                ],
            ),
            (
                "stage moved, other stages skipped",
                moved(Stage::Plan, Stage::Review, &[Stage::Code, Stage::Test]),
                [Some(Cue::StageOpened(Stage::Review)), None, None],
            ),
            (
                "examples run",
                Event::RunFinished {
                    scope: CodingRunScope::Examples,
                },
                [
                    Some(Cue::RunReview(CodingRunScope::Examples)),
                    None,
                    Some(Cue::Checkpoint),
                ],
            ),
            (
                "input run",
                Event::RunFinished {
                    scope: CodingRunScope::Input,
                },
                [
                    Some(Cue::RunReview(CodingRunScope::Input)),
                    None,
                    Some(Cue::Checkpoint),
                ],
            ),
            (
                "all cases run",
                Event::RunFinished {
                    scope: CodingRunScope::All,
                },
                [
                    Some(Cue::RunReview(CodingRunScope::All)),
                    Some(Cue::Submitted),
                    Some(Cue::Submitted),
                ],
            ),
            (
                "draft paused, changed",
                Event::DraftPaused { changed: true },
                [None, None, Some(Cue::Checkpoint)],
            ),
            (
                "draft paused, unchanged",
                Event::DraftPaused { changed: false },
                [None, None, None],
            ),
            ("idle", Event::Idle, [Some(Cue::Idle), None, None]),
        ]
    }

    #[test]
    fn the_cue_table_holds_cell_by_cell() {
        for (name, event, expected) in rows() {
            for (setting, want) in SETTINGS.into_iter().zip(expected) {
                let (_, observation) = observation(setting, Stage::Understand);
                let policy = policy_for(setting, CodingTask::Problem);
                assert_eq!(
                    policy.cue(&event, &observation),
                    want,
                    "{name} in {setting:?}"
                );
            }
        }
    }

    #[test]
    fn what_is_asked_for_when_skipped_follows_the_task() {
        let event = moved(Stage::Reproduce, Stage::Fix, &[Stage::Locate]);
        let skipped_reproduce = moved(
            Stage::Locate,
            Stage::Verify,
            &[Stage::Reproduce, Stage::Fix],
        );
        for setting in [CodingSetting::Mock, CodingSetting::Watched] {
            let (_, observation) = observation(setting, Stage::Reproduce);
            let policy = policy_for(setting, CodingTask::BugFix);
            assert_eq!(policy.cue(&event, &observation), None, "{setting:?}");
            assert_eq!(
                policy.cue(&skipped_reproduce, &observation),
                Some(Cue::AskForSkipped(vec![Stage::Reproduce])),
                "{setting:?}"
            );
        }
    }

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

    #[test]
    fn interviewers_ask_one_question_and_the_coach_may_ask_two() {
        let two = "What does it return? And for an empty list?";
        for setting in SETTINGS {
            let (_, observation) = observation(setting, Stage::Plan);
            let policy = policy_for(setting, CodingTask::Problem);
            let verdict = policy.check(two, &Cue::Reply, &observation);
            match setting {
                CodingSetting::Coached => assert!(verdict.is_ok(), "{setting:?}"),
                _ => assert!(verdict.is_err(), "{setting:?}"),
            }
            assert!(
                policy
                    .check("What does it return?", &Cue::Reply, &observation)
                    .is_ok(),
                "{setting:?}"
            );
        }
    }

    #[test]
    fn a_question_mark_inside_a_code_block_is_not_a_question() {
        let (_, observation) = observation(CodingSetting::Mock, Stage::Code);
        let policy = policy_for(CodingSetting::Mock, CodingTask::Problem);
        let reply = "Look at this:\n```python\nx = a if b else c  # why?\nprint('?')\n```\nWhat does it print?";
        assert!(policy.check(reply, &Cue::Reply, &observation).is_ok());
    }

    #[test]
    fn a_stage_cue_tells_the_coach_the_stage_instruction() {
        let (_, observation) = observation(CodingSetting::Coached, Stage::Plan);
        let policy = policy_for(CodingSetting::Coached, CodingTask::Problem);
        let system = policy.system(&Cue::StageOpened(Stage::Plan), &observation);
        assert!(!stage_instruction(Stage::Plan).is_empty());
        assert!(system.contains(stage_instruction(Stage::Plan)));
    }

    #[test]
    fn every_stage_has_its_own_instruction() {
        let stages = [
            Stage::Understand,
            Stage::Examples,
            Stage::Plan,
            Stage::Code,
            Stage::Test,
            Stage::Review,
            Stage::Reproduce,
            Stage::Locate,
            Stage::Fix,
            Stage::Verify,
        ];
        let lines: std::collections::BTreeSet<&str> =
            stages.into_iter().map(stage_instruction).collect();
        assert_eq!(lines.len(), stages.len());
        assert!(!lines.contains(""));
    }

    #[test]
    fn only_the_watching_interviewer_is_told_it_may_stay_silent() {
        for setting in SETTINGS {
            let (_, observation) = observation(setting, Stage::Code);
            let policy = policy_for(setting, CodingTask::Problem);
            let system = policy.system(&Cue::Checkpoint, &observation);
            assert_eq!(
                system.contains("\"speak\""),
                setting == CodingSetting::Watched,
                "{setting:?}"
            );
        }
    }

    #[test]
    fn a_system_prompt_names_the_role_of_its_setting() {
        let roles = [
            (CodingSetting::Coached, "coach"),
            (CodingSetting::Mock, "interviewer"),
            (CodingSetting::Watched, "interviewer"),
        ];
        for (setting, role) in roles {
            let (_, observation) = observation(setting, Stage::Code);
            let policy = policy_for(setting, CodingTask::Problem);
            let system = policy.system(&Cue::Reply, &observation).to_lowercase();
            assert!(system.contains(role), "{setting:?}");
        }
    }
}

#[cfg(test)]
mod golden;
