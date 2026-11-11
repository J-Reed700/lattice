//! Who speaks in a session, and when: the cue table of each setting, for every kind of work.

use crate::practice::session::kind::{Kind, ReplyGuard, Wording};
use crate::practice::session::observation::Observation;
use crate::practice::simulation_policy::TurnPolicy;
use lattice_core::domain::learning::practice::work::{CheckScope, Form, Setting, Spacing, Stage};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[cfg(test)]
mod tests;

/// Something that happened in a session that may make the counterpart speak.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Started,
    LearnerMessage {
        spoken: bool,
    },
    StageMoved {
        from: Stage,
        to: Stage,
        skipped: Vec<Stage>,
    },
    Checked {
        scope: CheckScope,
    },
    DraftPaused {
        changed: bool,
    },
    Idle,
}

/// Why the counterpart speaks. Stored while pending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cue {
    Opening,
    Reply,
    StageOpened(Stage),
    AskForSkipped(Vec<Stage>),
    CheckReview(CheckScope),
    Submitted,
    Checkpoint,
    Idle,
}

impl Cue {
    /// Which of two pending cues to keep: true when `self` ranks strictly above `other`.
    pub fn outranks(&self, other: &Cue) -> bool {
        self.rank() > other.rank()
    }

    fn rank(&self) -> u8 {
        match self {
            Self::Opening => 7,
            Self::Reply => 6,
            Self::Submitted => 5,
            Self::AskForSkipped(_) => 4,
            Self::StageOpened(_) => 3,
            Self::CheckReview(_) => 2,
            Self::Checkpoint => 1,
            Self::Idle => 0,
        }
    }
}

pub type Policy = dyn TurnPolicy<Observation, Event = Event, Cue = Cue>;

/// The roles of a kind that a counterpart's policy uses.
struct Roles {
    form: Form,
    spacing: Spacing,
    wording: Arc<dyn Wording>,
    reply_guard: Arc<dyn ReplyGuard>,
}

/// The coach of the Coached setting: teaches, and speaks on every message, stage and check.
pub struct Coach(Roles);

/// The interviewer of the Mock setting: in role, speaks on messages, submits and skipped stages.
pub struct Interviewer(Roles);

/// The interviewer of the Watched setting: as the Mock one, and at checkpoints; it may stay silent.
pub struct WatchingInterviewer(Roles);

/// The stages an interviewer asks for when they are skipped: the thinking stages before the work starts.
fn asked_for(form: Form) -> &'static [Stage] {
    match form {
        Form::Build => &[Stage::Examples, Stage::Plan],
        Form::Repair => &[Stage::Reproduce],
        Form::Complete | Form::Arrange => &[],
    }
}

fn cue_for(setting: Setting, form: Form, event: &Event) -> Option<Cue> {
    let coached = setting == Setting::Coached;
    let watched = setting == Setting::Watched;
    match event {
        Event::Started => Some(Cue::Opening),
        Event::LearnerMessage { spoken } => Some(if *spoken && watched {
            Cue::Checkpoint
        } else {
            Cue::Reply
        }),
        Event::StageMoved { to, skipped, .. } => {
            if coached {
                return Some(Cue::StageOpened(*to));
            }
            let asked: Vec<Stage> = skipped
                .iter()
                .filter(|stage| asked_for(form).contains(stage))
                .copied()
                .collect();
            (!asked.is_empty()).then_some(Cue::AskForSkipped(asked))
        }
        Event::Checked { scope } => match (setting, scope) {
            (Setting::Coached, scope) => Some(Cue::CheckReview(*scope)),
            (_, CheckScope::All) => Some(Cue::Submitted),
            (Setting::Watched, _) => Some(Cue::Checkpoint),
            (Setting::Mock, _) => None,
        },
        Event::DraftPaused { changed } => (watched && *changed).then_some(Cue::Checkpoint),
        Event::Idle => coached.then_some(Cue::Idle),
    }
}

/// The text of `content` outside fenced blocks.
fn outside_fences(content: &str) -> String {
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

/// `build` over `slot`, or nothing when the kind words the slot as empty.
fn sentence(slot: &str, build: impl FnOnce(&str) -> String) -> Option<String> {
    (!slot.is_empty()).then(|| build(slot))
}

fn fixed(text: &str) -> Option<String> {
    Some(text.to_owned())
}

fn join(sentences: impl IntoIterator<Item = Option<String>>) -> String {
    sentences
        .into_iter()
        .flatten()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

impl Roles {
    fn question_clause(&self) -> String {
        match self.wording.quoted_work() {
            Some(quoted) => format!("with one question mark outside any {quoted}"),
            None => "with one question mark".to_owned(),
        }
    }

    fn check(&self, setting: Setting, content: &str) -> Result<()> {
        if setting != Setting::Coached && outside_fences(content).matches('?').count() > 1 {
            return Err(AppError::InvalidInput(format!(
                "An interviewer asks one question at a time. Ask only one question, {}.",
                self.question_clause()
            )));
        }
        Ok(())
    }

    fn role(&self, setting: Setting) -> String {
        let wording = &self.wording;
        if setting == Setting::Coached {
            return join([
                sentence(&wording.practice_scene(), |scene| {
                    format!("You are the coach in {scene}.")
                }),
                fixed("You teach: you explain, ask the question that moves the learner on, and say what they did well."),
                fixed("You never write their solution for them."),
            ]);
        }
        let interviewer = join([
            sentence(&wording.interview_scene(), |scene| {
                format!("You are the interviewer in {scene}.")
            }),
            sentence(&wording.task_noun(), |task| {
                format!("Stay in role: you hold a {task} for the candidate and are not a tutor.")
            }),
            fixed("Do not teach, praise or grade; ask, listen and probe."),
            Some(format!(
                "Ask at most one question per turn, {}.",
                self.question_clause()
            )),
        ]);
        if setting == Setting::Mock {
            return interviewer;
        }
        let watching = join([
            sentence(&wording.piece_noun(), |piece| {
                format!("You are watching the candidate's {piece} change as they work.")
            }),
            fixed("Mostly stay quiet."),
            fixed("Speak only when a question about what they are doing is worth asking."),
        ]);
        format!("{interviewer} {watching}")
    }

    fn spacing_rule(&self) -> Option<String> {
        match self.spacing {
            Spacing::Technique => sentence(&self.wording.task_noun(), |task| {
                format!("Never name the technique or pattern that solves the {task}; ask about the idea instead.")
            }),
            Spacing::Item => fixed("Never state what belongs in a part; ask the learner why a part belongs where they put it."),
        }
    }

    fn rules(&self, state: &Observation) -> String {
        let wording = &self.wording;
        join([
            sentence(&wording.evidence(), |evidence| {
                format!(
                    "Speak about {evidence} only from what you are given; never invent a result."
                )
            }),
            Some(wording.refer_rule()),
            sentence(&wording.unsolved_rule(), |rule| {
                format!("Do not show a complete solution: {rule}.")
            }),
            self.spacing_rule(),
            fixed("Never write an id. Keep it short, and in the words a person would say aloud."),
            Some(self.reply_guard.rules(&state.brief)),
        ])
    }

    fn format_line(&self) -> String {
        let fields = self.reply_guard.format();
        if fields.is_empty() {
            "Answer as JSON: content is what you say.".to_owned()
        } else {
            format!("Answer as JSON: content is what you say; {fields}.")
        }
    }

    fn instruction(&self, setting: Setting, cue: &Cue, state: &Observation) -> String {
        let wording = &self.wording;
        let coached = setting == Setting::Coached;
        let stage_line = |stage: Stage| wording.stage_instruction(&state.brief, self.form, stage);
        let stage_label = |stage: Stage| wording.stage_label(&state.brief, self.form, stage);
        let noun = wording.check_noun();
        match cue {
            Cue::Opening if coached => format!(
                "Open the session: greet the learner in a sentence, then give the instruction of the {} stage in your own words: {}",
                stage_label(state.stage),
                stage_line(state.stage)
            ),
            Cue::Opening => format!(
                "Open the interview: introduce yourself in a sentence, say the {} is on screen, and ask the candidate to read it and say what they would like to clarify.",
                wording.task_noun()
            ),
            Cue::Reply => "Answer the learner's last message in the transcript.".to_owned(),
            Cue::StageOpened(stage) => format!(
                "The learner has moved to the {} stage. Open it with its instruction, in your own words: {}",
                stage_label(*stage),
                stage_line(*stage)
            ),
            Cue::AskForSkipped(stages) => format!(
                "The candidate skipped {}. As an interviewer would, ask for it in one question: what they would have done there.",
                stages
                    .iter()
                    .map(|stage| stage_label(*stage))
                    .collect::<Vec<_>>()
                    .join(" and ")
            ),
            Cue::CheckReview(scope) => format!(
                "The learner {} {}. Read the last {noun}. Say what it shows, point at what to look at without fixing it, and ask what they make of it.",
                wording.checked_verb(),
                wording.scope_label(*scope)
            ),
            Cue::Submitted if coached => format!(
                "The learner submitted every case. Say how it went from the last {noun}, what to look at in a failing case, and what comes next."
            ),
            Cue::Submitted => format!(
                "The candidate submitted. React as an interviewer: acknowledge the result without hinting at the cases you cannot see, and ask one question, about {}.",
                wording.submit_probes()
            ),
            Cue::Checkpoint => format!(
                "This is a checkpoint. Speak only if a question is worth asking now: {}. If nothing is worth saying, stay silent.",
                wording.checkpoint_probes()
            ),
            Cue::Idle => format!(
                "The learner has been quiet for a while, with no edit, {noun} or message. Check in kindly: ask what they are thinking about, and say they can ask for a hint."
            ),
        }
    }

    fn system(&self, setting: Setting, cue: &Cue, state: &Observation) -> String {
        let speak = if setting == Setting::Watched && *cue == Cue::Checkpoint {
            " Also set \"speak\" to false to stay silent, in which case content may be empty."
        } else {
            ""
        };
        format!(
            "{}\n\n{}\n\n{}\n\n{}{speak}",
            self.role(setting),
            self.rules(state),
            self.instruction(setting, cue, state),
            self.format_line()
        )
    }
}

macro_rules! policy {
    ($policy:ident, $setting:expr) => {
        impl TurnPolicy<Observation> for $policy {
            type Event = Event;
            type Cue = Cue;

            fn cue(&self, event: &Event, _state: &Observation) -> Option<Cue> {
                cue_for($setting, self.0.form, event)
            }

            fn system(&self, cue: &Cue, state: &Observation) -> String {
                self.0.system($setting, cue, state)
            }

            fn check(&self, content: &str, _cue: &Cue, _state: &Observation) -> Result<()> {
                self.0.check($setting, content)
            }
        }
    };
}

policy!(Coach, Setting::Coached);
policy!(Interviewer, Setting::Mock);
policy!(WatchingInterviewer, Setting::Watched);

/// The policy of a setting, speaking in the words of `kind`.
pub fn policy_for(setting: Setting, kind: &Kind, form: Form) -> Box<Policy> {
    let roles = Roles {
        form,
        spacing: kind.spacing,
        wording: kind.wording.clone(),
        reply_guard: kind.reply_guard.clone(),
    };
    match setting {
        Setting::Coached => Box::new(Coach(roles)),
        Setting::Mock => Box::new(Interviewer(roles)),
        Setting::Watched => Box::new(WatchingInterviewer(roles)),
    }
}
