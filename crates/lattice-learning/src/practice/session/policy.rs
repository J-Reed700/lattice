//! Who speaks in a session, and when: the cue table of each setting, for every kind of work.

use crate::practice::session::kind::{Kind, ReplyGuard, Wording};
use crate::practice::session::observation::Observation;
use crate::practice::simulation_policy::TurnPolicy;
use lattice_core::domain::learning::practice::work::{CheckScope, Form, Setting, Spacing, Stage};
use lattice_core::shared::error::Result;
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
        0
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

macro_rules! policy {
    ($policy:ident, $setting:expr) => {
        impl TurnPolicy<Observation> for $policy {
            type Event = Event;
            type Cue = Cue;

            fn cue(&self, _event: &Event, _state: &Observation) -> Option<Cue> {
                None
            }

            fn system(&self, _cue: &Cue, _state: &Observation) -> String {
                String::new()
            }

            fn check(&self, _content: &str, _cue: &Cue, _state: &Observation) -> Result<()> {
                Ok(())
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
