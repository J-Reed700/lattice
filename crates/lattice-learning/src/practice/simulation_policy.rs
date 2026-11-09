//! The seam between a counterpart's session and the turns it speaks.

use crate::practical_dto::{LearningInterviewPlanDto, LearningSimulationSessionDto};
use lattice_core::shared::error::{AppError, Result};

/// When a counterpart speaks, what it is told, and the rule its replies keep.
pub trait TurnPolicy<S>: Send + Sync {
    type Event;
    type Cue;
    /// Whether the counterpart speaks after `event`, and on what cue.
    fn cue(&self, event: &Self::Event, state: &S) -> Option<Self::Cue>;
    /// The system prompt for a reply on `cue`.
    fn system(&self, cue: &Self::Cue, state: &S) -> String;
    /// The policy's own rule on a reply's text. The checks every reply gets stay in the turn generator.
    fn check(&self, content: &str, cue: &Self::Cue, state: &S) -> Result<()>;
}

/// What a simulation policy reads.
pub struct SimulationState<'a> {
    pub session: &'a LearningSimulationSessionDto,
    pub interview_plan: Option<&'a LearningInterviewPlanDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationEvent {
    Turn,
    Finish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationCue {
    Turn,
    Closing,
}

/// A simulation that asks an interview plan's questions.
pub struct InterviewQuestionsPolicy;

/// A simulation that plays a scenario's counterpart.
pub struct ScenarioPolicy;

fn not_built() -> AppError {
    AppError::InvalidState {
        details: "not built yet".into(),
    }
}

impl<'a> TurnPolicy<SimulationState<'a>> for InterviewQuestionsPolicy {
    type Event = SimulationEvent;
    type Cue = SimulationCue;

    fn cue(&self, _event: &SimulationEvent, _state: &SimulationState<'a>) -> Option<SimulationCue> {
        None
    }

    fn system(&self, _cue: &SimulationCue, _state: &SimulationState<'a>) -> String {
        String::new()
    }

    fn check(&self, _content: &str, _cue: &SimulationCue, _state: &SimulationState<'a>) -> Result<()> {
        Err(not_built())
    }
}

impl<'a> TurnPolicy<SimulationState<'a>> for ScenarioPolicy {
    type Event = SimulationEvent;
    type Cue = SimulationCue;

    fn cue(&self, _event: &SimulationEvent, _state: &SimulationState<'a>) -> Option<SimulationCue> {
        None
    }

    fn system(&self, _cue: &SimulationCue, _state: &SimulationState<'a>) -> String {
        String::new()
    }

    fn check(&self, _content: &str, _cue: &SimulationCue, _state: &SimulationState<'a>) -> Result<()> {
        Err(not_built())
    }
}

#[cfg(test)]
#[path = "simulation_policy/tests.rs"]
mod tests;
